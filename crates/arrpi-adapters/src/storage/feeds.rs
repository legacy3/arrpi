//! Transactional per-subject generations and hashed capability lookup.

use std::sync::{Arc, Mutex};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Days, NaiveDate};
use hmac::{Hmac, KeyInit as _, Mac as _};
use rusqlite::{Connection, OptionalExtension as _, TransactionBehavior, params};
use secrecy::{ExposeSecret as _, SecretString};
use sha2::{Digest as _, Sha256};
use tokio::sync::Semaphore;

use super::{Database, settings::sql_error};
use arrpi_core::{config::ConfigError, error::ApiError, secret::Secret, subscriptions::Policy};

/// Capability lifecycle operations preserve explicit revocation until rotation.
#[derive(Clone, Copy, Debug)]
pub enum FeedAction {
    Issue,
    Rotate,
    Revoke,
}

/// Private capability URLs; debug formatting deliberately excludes their tokens.
#[derive(serde::Serialize)]
pub struct FeedLinks {
    pub rss_path: String,
    pub ical_path: String,
}

impl std::fmt::Debug for FeedLinks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeedLinks").finish_non_exhaustive()
    }
}

/// Persistent subject grants with a private shared owner for blocking jobs.
#[derive(Clone)]
pub struct SubscriptionStore(Arc<SubscriptionState>);

struct SubscriptionState {
    database: Mutex<Connection>,
    master: Secret,
    policy: Policy,
    today: fn() -> Result<NaiveDate, ApiError>,
    namespace: [u8; 32],
    workers: Arc<Semaphore>,
}

impl SubscriptionStore {
    /// Open existing subscription tables and retain their persistent namespace.
    ///
    /// # Errors
    /// Returns a redacted error for invalid credentials, schema versions or storage failure.
    pub fn new(
        database: Database,
        master: Secret,
        policy: Policy,
        today: fn() -> Result<NaiveDate, ApiError>,
        new_namespace: [u8; 32],
    ) -> Result<Self, ConfigError> {
        let mut database = database.0;

        if !(32..=128).contains(&master.value().len()) {
            return Err(ConfigError::CREDENTIAL);
        }

        super::migrations::apply(&mut database).map_err(|error| {
            tracing::debug!(event = "feed_schema_migration", category = %error);
            ConfigError::invalid("cannot migrate application database")
        })?;
        database
            .execute(
                "INSERT OR IGNORE INTO feed_metadata(id, namespace) VALUES(1, ?1)",
                [new_namespace.as_slice()],
            )
            .map_err(|error| {
                tracing::debug!(event = "feed_namespace_init", code = ?error.sqlite_error_code());
                ConfigError::invalid("cannot initialize feed namespace")
            })?;
        let namespace: Vec<u8> = database
            .query_row(
                "SELECT namespace FROM feed_metadata WHERE id=1",
                [],
                |row| row.get(0),
            )
            .map_err(|error| {
                tracing::debug!(event = "feed_namespace_read", code = ?error.sqlite_error_code());
                ConfigError::invalid("cannot read feed namespace")
            })?;
        let namespace = namespace.try_into().map_err(|bytes: Vec<u8>| {
            tracing::debug!(event = "invalid_feed_namespace", length = bytes.len());
            ConfigError::invalid("invalid feed namespace")
        })?;

        Ok(Self(Arc::new(SubscriptionState {
            database: Mutex::new(database),
            master,
            policy,
            today,
            namespace,
            workers: Arc::new(Semaphore::new(4)),
        })))
    }

    /// Issue, rotate or revoke one authenticated subject's source grants.
    ///
    /// # Errors
    /// Returns a redacted error for invalid subjects, revoked grants, capacity or storage failure.
    pub async fn update(
        &self,
        subject: String,
        sources: Vec<String>,
        action: FeedAction,
    ) -> Result<Option<FeedLinks>, ApiError> {
        if subject.is_empty()
            || subject.len() > 128
            || !subject
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ApiError::BAD_QUERY);
        }

        self.run(move |store| store.update_sync(&subject, &sources, action))
            .await
    }

    /// Resolve a validated capability to its current persisted source grants.
    ///
    /// # Errors
    /// Returns an authentication error for invalid or revoked tokens, or a storage error.
    pub async fn sources(&self, token: SecretString) -> Result<Vec<String>, ApiError> {
        if token.expose_secret().len() != 43
            || !token
                .expose_secret()
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ApiError::UNAUTHORIZED);
        }

        self.run(move |store| store.sources_sync(&token)).await
    }

    /// Build a rolling date window from the injected clock and configured policy.
    ///
    /// # Errors
    /// Returns a redacted error if the clock or date arithmetic fails.
    pub fn query(&self) -> Result<String, ApiError> {
        let today = (self.0.today)()?;
        let start = today
            .checked_sub_days(Days::new(u64::from(self.0.policy.past_days)))
            .ok_or(ApiError::BAD_QUERY)?;
        let end = today
            .checked_add_days(Days::new(u64::from(self.0.policy.future_days)))
            .ok_or(ApiError::BAD_QUERY)?;

        Ok(format!("start={start}&end={end}"))
    }

    async fn run<T, F>(&self, work: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&SubscriptionState) -> Result<T, ApiError> + Send + 'static,
    {
        let permit = Arc::clone(&self.0.workers)
            .try_acquire_owned()
            .map_err(|error| {
                tracing::debug!(event="feed_worker_capacity",category=%error);
                ApiError::BUSY
            })?;
        let store = Arc::clone(&self.0);

        tokio::task::spawn_blocking(move || {
            let _permit = permit;

            work(&store)
        })
        .await
        .map_err(|error| join_error(&error))?
    }
}

impl SubscriptionState {
    fn update_sync(
        &self,
        subject: &str,
        sources: &[String],
        action: FeedAction,
    ) -> Result<Option<FeedLinks>, ApiError> {
        let mut database = self.database.lock().map_err(|error| {
            tracing::debug!(event = "feed_database_poisoned", category = %error);
            ApiError::INTERNAL
        })?;
        let transaction = database
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| sql_error(&error))?;
        let existing: Option<(i64, bool)> = transaction
            .query_row(
                "SELECT generation, enabled FROM feed_subjects WHERE subject = ?1",
                [subject],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| sql_error(&error))?;
        let (old_generation, enabled) = existing.unwrap_or((0, true));

        if !enabled && matches!(action, FeedAction::Issue) {
            return Err(ApiError::FORBIDDEN);
        }

        if existing.is_none() {
            let count: i64 = transaction
                .query_row("SELECT COUNT(*) FROM feed_subjects", [], |row| row.get(0))
                .map_err(|error| sql_error(&error))?;

            if count
                >= i64::try_from(self.policy.max_subjects).map_err(|error| {
                    tracing::debug!(event = "feed_limit", category = %error);
                    ApiError::INTERNAL
                })?
            {
                return Err(ApiError::CAPACITY);
            }
        }

        let generation = if old_generation == 0 || !matches!(action, FeedAction::Issue) {
            old_generation.checked_add(1).ok_or(ApiError::INTERNAL)?
        } else {
            old_generation
        };
        let token = self.token(subject, generation)?;
        let digest = Sha256::digest(token.expose_secret().as_bytes());
        let sources =
            serde_json::to_string(sources).map_err(|error| super::settings::json_error(&error))?;
        let enabled = !matches!(action, FeedAction::Revoke);

        transaction.execute("INSERT INTO feed_subjects(subject, generation, digest, sources, enabled) VALUES(?1, ?2, ?3, ?4, ?5) ON CONFLICT(subject) DO UPDATE SET generation=excluded.generation, digest=excluded.digest, sources=excluded.sources, enabled=excluded.enabled", params![subject, generation, digest.as_slice(), sources, enabled]).map_err(|error| sql_error(&error))?;
        transaction.commit().map_err(|error| sql_error(&error))?;

        Ok(enabled.then(|| FeedLinks {
            rss_path: format!("/feeds/{}/calendar.rss", token.expose_secret()),
            ical_path: format!("/feeds/{}/calendar.ics", token.expose_secret()),
        }))
    }

    fn sources_sync(&self, token: &SecretString) -> Result<Vec<String>, ApiError> {
        let database = self.database.lock().map_err(|error| {
            tracing::debug!(event = "feed_database_poisoned", category = %error);
            ApiError::INTERNAL
        })?;
        let digest = Sha256::digest(token.expose_secret().as_bytes());
        let record: Option<(String, i64, String)> = database.query_row("SELECT subject, generation, sources FROM feed_subjects WHERE digest=?1 AND enabled=1", [digest.as_slice()], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional().map_err(|error| sql_error(&error))?;
        let (subject, generation, sources) = record.ok_or(ApiError::UNAUTHORIZED)?;
        let presented = URL_SAFE_NO_PAD
            .decode(token.expose_secret())
            .map_err(|error| { tracing::debug!(event = "invalid_feed_encoding", category = ?std::mem::discriminant(&error)); ApiError::UNAUTHORIZED })?;

        self.mac(&subject, generation)?
            .verify_slice(&presented)
            .map_err(|error| {
                tracing::debug!(event = "invalid_feed_signature", category = %error);
                ApiError::UNAUTHORIZED
            })?;

        serde_json::from_str(&sources).map_err(|error| {
            tracing::debug!(event = "invalid_feed_grants", category = ?error.classify());
            ApiError::INTERNAL
        })
    }

    fn mac(&self, subject: &str, generation: i64) -> Result<Hmac<Sha256>, ApiError> {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.master.value().as_bytes()).map_err(|error| {
                tracing::debug!(event = "feed_key", category = %error);
                ApiError::INTERNAL
            })?;
        let length = u64::try_from(subject.len()).map_err(|error| {
            tracing::debug!(event = "feed_length", category = %error);
            ApiError::INTERNAL
        })?;

        mac.update(b"arrpi/feed-subscription/v1");
        mac.update(&self.namespace);
        mac.update(&length.to_be_bytes());
        mac.update(subject.as_bytes());
        mac.update(&generation.to_be_bytes());

        Ok(mac)
    }

    fn token(&self, subject: &str, generation: i64) -> Result<SecretString, ApiError> {
        Ok(URL_SAFE_NO_PAD
            .encode(self.mac(subject, generation)?.finalize().into_bytes())
            .into())
    }
}

impl std::fmt::Debug for SubscriptionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SubscriptionStore").finish_non_exhaustive()
    }
}

fn join_error(error: &tokio::task::JoinError) -> ApiError {
    tracing::debug!(
        event = "feed_worker",
        cancelled = error.is_cancelled(),
        panicked = error.is_panic()
    );
    ApiError::INTERNAL
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> Result<NaiveDate, ApiError> {
        NaiveDate::from_ymd_opt(2026, 10, 15).ok_or(ApiError::INTERNAL)
    }

    #[tokio::test]
    async fn anonymous_feed_lookup_rejects_work_before_spawning_when_capacity_is_full() {
        let policy = Policy::default();
        let store = SubscriptionStore::new(
            Connection::open_in_memory().expect("database").into(),
            "fake-feed-master-000000000000000000000".to_owned().into(),
            policy,
            today,
            [7; 32],
        )
        .expect("feed store");
        let held = Arc::clone(&store.0.workers)
            .acquire_many_owned(4)
            .await
            .expect("hold worker slots");

        assert_eq!(
            store
                .sources("x".repeat(43).into())
                .await
                .expect_err("capacity before random token lookup"),
            ApiError::BUSY
        );
        drop(held);
        assert_eq!(
            store
                .sources("x".repeat(43).into())
                .await
                .expect_err("invalid token after capacity release"),
            ApiError::UNAUTHORIZED
        );
    }
}
