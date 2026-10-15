//! One private `SQLite` file contains settings, credentials and feed state.

use std::sync::{Arc, Mutex};

use rusqlite::{Connection, OptionalExtension as _, params};
use serde_json::Value;

use super::Database;
use arrpi_core::{config::Config, error::ApiError};

/// Shared settings and observation storage; credentials stay private at serialization boundaries.
#[derive(Clone, Debug)]
pub struct Store {
    pub(super) connection: Arc<Mutex<Connection>>,
}

impl Store {
    /// Initialize the supported settings schema without replacing existing state.
    ///
    /// # Errors
    /// Returns a redacted error for an unsupported schema or database failure.
    pub fn new(database: Database) -> Result<Self, ApiError> {
        let mut connection = database.0;

        super::migrations::apply(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
        })
    }

    /// Read the current revision, or None before the initial seed.
    ///
    /// # Errors
    /// Returns a redacted database error.
    pub fn revision(&self) -> Result<Option<i64>, ApiError> {
        self.connection
            .lock()
            .map_err(|error| lock_error(&error))?
            .query_row("SELECT revision FROM app_settings WHERE id=1", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|error| sql_error(&error))
    }

    /// Insert the initial private configuration only when no document exists.
    ///
    /// # Errors
    /// Returns a redacted serialization or database error.
    pub fn seed(&self, config: &Config) -> Result<(), ApiError> {
        let document =
            serde_json::to_string(&private(config)?).map_err(|error| json_error(&error))?;

        self.connection
            .lock()
            .map_err(|error| lock_error(&error))?
            .execute(
                "INSERT OR IGNORE INTO app_settings VALUES(1, 1, ?1)",
                [document],
            )
            .map_err(|error| sql_error(&error))?;
        Ok(())
    }

    /// Read and validate the stored private configuration.
    ///
    /// # Errors
    /// Returns a redacted error for invalid stored data or database failure.
    pub fn load(&self) -> Result<Option<(i64, Config)>, ApiError> {
        let row: Option<(i64, String)> = self
            .connection
            .lock()
            .map_err(|error| lock_error(&error))?
            .query_row(
                "SELECT revision, document FROM app_settings WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| sql_error(&error))?;

        row.map(|(revision, document)| {
            let config: Config =
                serde_json::from_str(&document).map_err(|error| json_error(&error))?;

            config.validate().map_err(|error| {
                tracing::debug!(event="stored_configuration_invalid", category=%error);
                ApiError::INTERNAL
            })?;
            Ok((revision, config))
        })
        .transpose()
    }

    /// Replace a matching revision atomically while preserving private credential values.
    ///
    /// # Errors
    /// Returns a conflict if the revision changed, or a redacted persistence error.
    pub fn save(&self, revision: i64, config: &Config) -> Result<i64, ApiError> {
        let next = revision.checked_add(1).ok_or(ApiError::CAPACITY)?;
        let document =
            serde_json::to_string(&private(config)?).map_err(|error| json_error(&error))?;
        let changed = self
            .connection
            .lock()
            .map_err(|error| lock_error(&error))?
            .execute(
                "UPDATE app_settings SET revision=?1, document=?2 WHERE id=1 AND revision=?3",
                params![next, document, revision],
            )
            .map_err(|error| sql_error(&error))?;

        if changed != 1 {
            return Err(ApiError::CONFLICT);
        }

        Ok(next)
    }
}

/// Serialize a public configuration with every credential masked.
///
/// # Errors
/// Returns a redacted serialization error.
pub fn public(config: &Config) -> Result<Value, ApiError> {
    serde_json::to_value(config).map_err(|error| json_error(&error))
}

fn private(config: &Config) -> Result<Value, ApiError> {
    let mut value = public(config)?;

    for (name, source) in &config.sources {
        value["sources"][name]["api_key"] = credential(&source.api_key);
    }

    for (index, client) in config.clients.iter().enumerate() {
        value["clients"][index]["key"] = credential(&client.key);
    }

    if let Some(seerr) = &config.seerr {
        value["seerr"]["api_key"] = credential(&seerr.api_key);
    }

    value["subscriptions"]["master_key"] = credential(&config.subscriptions.master_key);
    Ok(value)
}

fn credential(secret: &arrpi_core::secret::Secret) -> Value {
    if secret.missing() {
        Value::Null
    } else {
        Value::String(secret.value().to_owned())
    }
}

/// Restore omitted credentials by stable source and account identity.
///
/// # Errors
/// Returns an invalid-query error when a new connection omitted its credential.
pub fn restore_secrets(config: &mut Config, current: &Config) -> Result<(), ApiError> {
    for (name, source) in &mut config.sources {
        if source.api_key.missing() {
            source.api_key = current
                .sources
                .get(name)
                .ok_or(ApiError::BAD_QUERY)?
                .api_key
                .clone();
        }
    }

    for client in &mut config.clients {
        if client.key.missing() {
            client.key = current
                .clients
                .iter()
                .find(|old| old.name == client.name)
                .ok_or(ApiError::BAD_QUERY)?
                .key
                .clone();
        }
    }

    if let Some(seerr) = &mut config.seerr
        && seerr.api_key.missing()
    {
        seerr.api_key = current
            .seerr
            .as_ref()
            .ok_or(ApiError::BAD_QUERY)?
            .api_key
            .clone();
    }

    if config.subscriptions.master_key.missing() {
        config.subscriptions.master_key = current.subscriptions.master_key.clone();
    }

    Ok(())
}

pub(super) fn sql_error(error: &rusqlite::Error) -> ApiError {
    tracing::debug!(event="settings_database", code=?error.sqlite_error_code());
    ApiError::INTERNAL
}

pub(super) fn json_error(error: &serde_json::Error) -> ApiError {
    tracing::debug!(event="settings_json", category=?error.classify());
    ApiError::INTERNAL
}

pub(super) fn lock_error<T>(error: &std::sync::PoisonError<T>) -> ApiError {
    tracing::debug!(event="settings_lock", poisoned=%error);
    ApiError::INTERNAL
}
