//! Bounded native calls with a server-held integration credential.

use arrpi_core::{
    config::{CachePolicy, ConfigError, Source},
    error::ApiError,
};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Semaphore;

use crate::{
    budget::Budget,
    http::{self, Http, Intent},
};

pub(super) struct Client {
    base: url::Url,
    key: SecretString,
    http: Http,
    slots: Semaphore,
    budget: Budget,
    limit: usize,
    server_epoch: AtomicU64,
}

impl Client {
    pub(super) fn new(
        source: &Source,
        policy: CachePolicy,
        http: Http,
        budget: Budget,
    ) -> Result<Self, ConfigError> {
        Ok(Self {
            base: source.base_url()?.into(),
            key: crate::credential(source.api_key.value())?,
            http,
            slots: Semaphore::new(policy.max_upstream_requests),
            budget,
            limit: policy.max_body_bytes.min(4 * 1024 * 1024),
            server_epoch: AtomicU64::new(0),
        })
    }

    pub(super) fn now(&self) -> Option<u64> {
        let epoch = self.server_epoch.load(Ordering::Relaxed);

        (epoch > 0).then_some(epoch)
    }

    pub(super) async fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value, ApiError> {
        self.call(reqwest::Method::GET, path, query, None).await
    }

    pub(super) async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, &str)],
        payload: Option<&Value>,
    ) -> Result<Value, ApiError> {
        let intent = if method == reqwest::Method::GET {
            Intent::Read
        } else {
            Intent::Write
        };
        let _permit =
            tokio::time::timeout(std::time::Duration::from_millis(250), self.slots.acquire())
                .await
                .map_err(|error| crate::http::redacted_error(&error, ApiError::CAPACITY))?
                .map_err(|error| crate::http::redacted_error(&error, ApiError::INTERNAL))?;
        let url = self
            .base
            .join(path)
            .map_err(|error| crate::http::redacted_error(&error, ApiError::INTERNAL))?;
        let mut token = reqwest::header::HeaderValue::from_str(self.key.expose_secret())
            .map_err(|error| crate::http::redacted_error(&error, ApiError::INTERNAL))?;

        token.set_sensitive(true);
        let mut request = self
            .http
            .client()
            .request(method, url)
            .header("x-cw-token", token)
            .query(query);

        if let Some(payload) = payload {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(
                    serde_json::to_vec(payload)
                        .map_err(|error| crate::http::redacted_error(&error, ApiError::INTERNAL))?,
                );
        }

        let response = request
            .send()
            .await
            .map_err(|error| intent.failure(http::transport_error(&error)))?;
        let status = response.status();

        if let Some(epoch) = response
            .headers()
            .get(reqwest::header::DATE)
            .and_then(|date| date.to_str().ok())
            .and_then(|date| chrono::DateTime::parse_from_rfc2822(date).ok())
            .and_then(|date| u64::try_from(date.timestamp()).ok())
        {
            self.server_epoch.store(epoch, Ordering::Relaxed);
        }

        if !status.is_success() {
            return Err(match status.as_u16() {
                409 => ApiError::CONFLICT,
                429 => ApiError::UPSTREAM_RATE_LIMITED,
                400 | 422 => ApiError::REJECTED,
                404 => ApiError::NOT_FOUND,
                _ => intent.failure(ApiError::UNAVAILABLE),
            });
        }

        let bytes = http::bounded(response, self.limit, intent, Some(&self.budget)).await?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
            intent.failure(crate::http::redacted_error(
                &error,
                ApiError::INVALID_RESPONSE,
            ))
        })?;

        if value.get("ok").and_then(Value::as_bool) == Some(false) && !path.ends_with("/poll") {
            return Err(ApiError::REJECTED);
        }

        Ok(value)
    }
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrossWatchClient").finish_non_exhaustive()
    }
}
