//! Seerr owns requester attribution, quotas and native mutations.
mod accounts;
mod capabilities;
mod identity;
mod issues;
pub(crate) mod links;
pub(crate) mod model;
mod requests;
mod version;

use crate::{
    budget::Budget,
    http::{Http, Intent, bounded, collect, transport_error},
};
use arrpi_core::{
    config::{CachePolicy, ConfigError, SeerrConnection},
    error::ApiError,
    policy::{Binding, Limits},
    source::{Actor, ActorRights, Descriptor, Freshness, Observation, SourceRead},
};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

/// An uncached account-aware source with no generic public write endpoint.
pub struct Backend {
    descriptor: Descriptor,
    base: url::Url,
    key: secrecy::SecretString,
    http: Http,
    budget: Budget,
    limit: usize,
    admission: Arc<Semaphore>,
    writes: Arc<Semaphore>,
    queue_timeout: Duration,
    language: String,
}

impl Backend {
    /// Build an adapter using the runtime's shared transport and body budget.
    ///
    /// # Errors
    /// Rejects invalid connection details and unsafe native credentials.
    pub fn new(
        connection: &SeerrConnection,
        policy: CachePolicy,
        limits: Limits,
        http: Http,
        budget: Budget,
    ) -> Result<Self, ConfigError> {
        if policy.max_body_bytes == 0 || policy.max_body_bytes > budget.limit() {
            return Err(ConfigError::invalid("response bound exceeds body budget"));
        }

        Ok(Self {
            descriptor: Descriptor {
                name: "seerr".to_owned(),
                kind: "seerr",
                service_binding: None,
                endpoints: std::borrow::Cow::Borrowed(&[]),
            },
            base: connection.validate()?.into(),
            key: crate::credential(connection.api_key.value())?,
            http,
            budget,
            limit: policy.max_body_bytes,
            admission: Arc::new(Semaphore::new(policy.max_upstream_requests)),
            writes: Arc::new(Semaphore::new(limits.max_concurrent_writes)),
            queue_timeout: Duration::from_secs(limits.queue_timeout_seconds),
            language: connection.language.clone(),
        })
    }

    async fn write_slot(&self) -> Result<tokio::sync::OwnedSemaphorePermit, ApiError> {
        self.slot(&self.writes).await
    }

    async fn verified_actor(&self, binding: &Binding) -> Result<Actor, ApiError> {
        let value = self
            .call(binding, Method::GET, "auth/me", &[], None)
            .await?;
        let actor: identity::NativeActor = serde_json::from_value(value.data).map_err(|error| {
            tracing::debug!(
                event = "seerr_identity_invalid",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::INVALID_RESPONSE
        })?;

        if actor.id != binding.id || actor.plex_id != binding.plex_id {
            return Err(ApiError::FORBIDDEN);
        }

        Ok(actor.normalized())
    }

    async fn supports_writes(&self, binding: &Binding) -> Result<bool, ApiError> {
        let status = self
            .call(
                binding,
                Method::GET,
                "status",
                &[("checkUpdateAvailable", "false".to_owned())],
                None,
            )
            .await?;

        Ok(version::supports_writes(
            status.get("version").and_then(Value::as_str),
        ))
    }

    async fn write_identity(&self, binding: &Binding) -> Result<Actor, ApiError> {
        let actor = self.verified_actor(binding).await?;

        if !self.supports_writes(binding).await? {
            return Err(ApiError::UNSUPPORTED_VERSION);
        }

        Ok(actor)
    }

    async fn authorize_issue(&self, binding: &Binding) -> Result<(), ApiError> {
        if !self
            .write_identity(binding)
            .await?
            .rights
            .contains(ActorRights::REPORT_ISSUES)
        {
            return Err(ApiError::FORBIDDEN);
        }

        Ok(())
    }

    async fn call(
        &self,
        binding: &Binding,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<SourceRead<Value>, ApiError> {
        let _permit = self.slot(&self.admission).await?;
        let request = self.prepare(Some(binding), &method, path, query, body)?;
        let intent = if method == Method::GET {
            Intent::Read
        } else {
            Intent::Write
        };
        let reservation = if matches!(intent, Intent::Write) {
            Some(self.budget.reserve(self.limit).await?)
        } else {
            None
        };

        let response = request
            .send()
            .await
            .map_err(|error| intent.failure(transport_error(&error)))?;

        self.decode(response, &method, path, intent, reservation)
            .await
    }

    fn prepare(
        &self,
        binding: Option<&Binding>,
        method: &Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<reqwest::RequestBuilder, ApiError> {
        use secrecy::ExposeSecret as _;

        if binding.is_some_and(|binding| binding.id == 0 || binding.plex_id == 0) {
            return Err(ApiError::FORBIDDEN);
        }

        let mut url = self.base.join(&format!("api/v1/{path}")).map_err(|error| {
            tracing::debug!(
                event = "seerr_path_invalid",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::BAD_QUERY
        })?;
        let language = (path == "search" || path.starts_with("movie/") || path.starts_with("tv/"))
            .then_some(self.language.as_str());
        let query = query_string(query, language);

        url.set_query((!query.is_empty()).then_some(query.as_str()));
        let mut key =
            reqwest::header::HeaderValue::from_str(self.key.expose_secret()).map_err(|error| {
                tracing::debug!(
                    event = "seerr_key_invalid",
                    category = std::any::type_name_of_val(&error)
                );
                ApiError::INTERNAL
            })?;

        key.set_sensitive(true);
        let mut request = self
            .http
            .client()
            .request(method.clone(), url)
            .header("X-API-Key", key)
            .header("Accept", "application/json")
            .header("Cache-Control", "no-cache");

        if let Some(binding) = binding {
            request = request.header("X-API-User", binding.id.to_string());
        }

        if let Some(body) = body {
            request = request.header("Content-Type", "application/json").body(
                serde_json::to_vec(&body).map_err(|error| {
                    tracing::debug!(
                        event = "seerr_arguments_invalid",
                        category = std::any::type_name_of_val(&error)
                    );
                    ApiError::INTERNAL
                })?,
            );
        }

        Ok(request)
    }

    async fn decode(
        &self,
        response: reqwest::Response,
        method: &Method,
        path: &str,
        intent: Intent,
        reservation: Option<tokio::sync::OwnedSemaphorePermit>,
    ) -> Result<SourceRead<Value>, ApiError> {
        let status = response.status();
        let freshness = Freshness {
            source: self.descriptor.name.clone(),
            cache: Observation::NotCached,
            age_seconds: None,
            checked_at: response
                .headers()
                .get("date")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| chrono::DateTime::parse_from_rfc2822(v).ok())
                .map(|v| v.to_rfc3339()),
            note: Some(
                "Availability reflects Seerr's latest library sync; a fresh read does not trigger a scan.",
            ),
        };

        if status == StatusCode::NO_CONTENT {
            return Ok(SourceRead {
                data: Value::Null,
                freshness,
            });
        }

        if *method == Method::POST && path == "request" && status == StatusCode::CONFLICT {
            return Ok(SourceRead {
                data: json!({"_arrpi_noop":"already_requested"}),
                freshness,
            });
        }

        validate_status(status, method)?;
        let bytes = if matches!(intent, Intent::Write) {
            collect(response, self.limit, intent, reservation).await?
        } else {
            bounded(response, self.limit, intent, Some(&self.budget)).await?
        };
        let data = serde_json::from_slice(&bytes).map_err(|error| {
            tracing::debug!(
                event = "seerr_response_invalid",
                category = std::any::type_name_of_val(&error)
            );
            intent.failure(ApiError::INVALID_RESPONSE)
        })?;

        if *method == Method::POST && path == "request" && status == StatusCode::ACCEPTED {
            return Ok(SourceRead {
                data: json!({"_arrpi_noop":"nothing_to_request"}),
                freshness,
            });
        }

        if *method != Method::GET && !matches!(status, StatusCode::CREATED | StatusCode::OK) {
            return Err(ApiError::UNKNOWN_OUTCOME);
        }

        Ok(SourceRead { data, freshness })
    }

    async fn slot(
        &self,
        semaphore: &Arc<Semaphore>,
    ) -> Result<tokio::sync::OwnedSemaphorePermit, ApiError> {
        tokio::time::timeout(self.queue_timeout, Arc::clone(semaphore).acquire_owned())
            .await
            .map_err(|error| {
                tracing::debug!(
                    event = "seerr_queue_timeout",
                    category = std::any::type_name_of_val(&error)
                );
                ApiError::QUEUE_TIMEOUT
            })?
            .map_err(|error| {
                tracing::debug!(
                    event = "seerr_queue_closed",
                    category = std::any::type_name_of_val(&error)
                );
                ApiError::UNAVAILABLE
            })
    }
}

fn validate_status(status: StatusCode, method: &Method) -> Result<(), ApiError> {
    if status.is_success() {
        return Ok(());
    }

    if *method == Method::GET {
        return Err(ApiError::upstream_read(status.as_u16()));
    }

    Err(match status.as_u16() {
        401 | 403 => ApiError::FORBIDDEN,
        404 => ApiError::NOT_FOUND,
        409 => ApiError::CONFLICT,
        _ => ApiError::UNKNOWN_OUTCOME,
    })
}

fn query_string(query: &[(&str, String)], language: Option<&str>) -> String {
    const ENCODE: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    query
        .iter()
        .map(|(key, value)| (*key, value.as_str()))
        .chain(language.map(|value| ("language", value)))
        .map(|(key, value)| {
            format!(
                "{}={}",
                percent_encoding::utf8_percent_encode(key, ENCODE),
                percent_encoding::utf8_percent_encode(value, ENCODE)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Seerr")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
