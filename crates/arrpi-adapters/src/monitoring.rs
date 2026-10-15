//! Explicit read-only monitoring protocols; no arbitrary native GET forwarding.
mod projection;
use crate::{
    budget::Budget,
    cache::Cache,
    http::{self, Http, Intent},
    metrics::Metrics,
};
use arrpi_core::{
    config::{CachePolicy, ConfigError, Source, SourceKind},
    contracts::Counters,
    error::ApiError,
    source::{ApiSource, Descriptor, Freshness, Health, Principal, ReadMode, SourceRead},
    summaries::{SummaryKind, SummarySource},
};
use async_trait::async_trait;
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Operational data expires quickly and never falls back to stale counters.
pub(crate) fn operational_policy(mut policy: CachePolicy) -> CachePolicy {
    policy.fresh_seconds = policy.fresh_seconds.min(15);
    policy.stale_seconds = 0;
    policy
}

pub(crate) struct Admission {
    pub(crate) budget: Budget,
    pub(crate) normalizations: arrpi_core::admission::Normalizations,
}

pub(crate) struct Monitor {
    descriptor: Descriptor,
    kind: SourceKind,
    base: url::Url,
    key: SecretString,
    http: Http,
    cache: Arc<Cache>,
    metrics: Metrics,
    budget: Budget,
    normalizations: arrpi_core::admission::Normalizations,
    limit: usize,
    upstream: Semaphore,
}

impl Monitor {
    pub(crate) fn new(
        name: &str,
        source: &Source,
        policy: CachePolicy,
        http: Http,
        admission: Admission,
        cache: Arc<Cache>,
    ) -> Result<Self, ConfigError> {
        let key = if source.kind == SourceKind::Maintainerr && source.api_key.value().is_empty() {
            SecretString::from(String::new())
        } else {
            crate::credential(source.api_key.value())?
        };

        Ok(Self {
            descriptor: Descriptor {
                name: name.to_owned(),
                kind: source.kind.name(),
                service_binding: None,
                endpoints: (&[][..]).into(),
            },
            kind: source.kind,
            base: source.base_url()?.into(),
            key,
            http,
            cache,
            metrics: Metrics::new(),
            budget: admission.budget,
            normalizations: admission.normalizations,
            limit: policy.max_body_bytes,
            upstream: Semaphore::new(policy.max_upstream_requests),
        })
    }

    async fn fetch(&self) -> Result<bytes::Bytes, ApiError> {
        let _permit = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            self.upstream.acquire(),
        )
        .await
        .map_err(|error| {
            tracing::debug!(
                event = "monitor_admission_timeout",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::CAPACITY
        })?
        .map_err(|error| {
            tracing::debug!(
                event = "monitor_admission_closed",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::UNAVAILABLE
        })?;
        let path = match self.kind {
            SourceKind::Sabnzbd => "api",
            SourceKind::Cleanuparr => "api/v2/stats",
            SourceKind::Bazarr => "api/badges",
            SourceKind::Maintainerr => "api/storage-metrics",
            _ => return Err(ApiError::NOT_FOUND),
        };
        let mut url = self.base.join(path).map_err(|error| {
            tracing::debug!(
                event = "monitor_route",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::NOT_FOUND
        })?;

        if self.kind == SourceKind::Cleanuparr {
            url.query_pairs_mut()
                .append_pair("hours", "168")
                .append_pair("includeDryRun", "false");
        }

        let mut request = self
            .http
            .client()
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json");

        if self.kind == SourceKind::Sabnzbd {
            let body = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("mode", "queue")
                .append_pair("output", "json")
                .append_pair("start", "0")
                .append_pair("limit", "1")
                .append_pair("apikey", self.key.expose_secret())
                .finish();
            let endpoint = self.base.join(path).map_err(|error| {
                tracing::debug!(
                    event = "monitor_route",
                    category = std::any::type_name_of_val(&error)
                );
                ApiError::NOT_FOUND
            })?;

            request = self
                .http
                .client()
                .post(endpoint)
                .header(reqwest::header::ACCEPT, "application/json")
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(body);
        }

        if self.kind != SourceKind::Sabnzbd && !self.key.expose_secret().is_empty() {
            let mut key = reqwest::header::HeaderValue::from_str(self.key.expose_secret())
                .map_err(|error| {
                    tracing::debug!(
                        event = "monitor_credential",
                        category = std::any::type_name_of_val(&error)
                    );
                    ApiError::REJECTED
                })?;

            key.set_sensitive(true);
            request = request.header("X-Api-Key", key);
        }

        let response = request
            .send()
            .await
            .map_err(|error| http::transport_error(&error))?;

        if response.status() != reqwest::StatusCode::OK {
            return Err(ApiError::upstream_read(response.status().as_u16()));
        }

        let body = http::bounded(response, self.limit, Intent::Read, Some(&self.budget)).await?;

        serde_json::from_slice::<serde::de::IgnoredAny>(&body)?;
        Ok(body)
    }
}

impl std::fmt::Debug for Monitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Monitor")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ApiSource for Monitor {
    fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    fn summaries(&self) -> Option<&dyn SummarySource> {
        Some(self)
    }

    fn counters(&self) -> Counters {
        self.metrics.snapshot()
    }

    async fn health(&self, _principal: &Principal, mode: ReadMode) -> Result<Health, ApiError> {
        let read = self.summary(SummaryKind::Status, mode).await?;

        Ok(Health {
            reachable: Some(true),
            version: read.data["version"].as_str().map(str::to_owned),
            health_issue_count: None,
            writes_supported: None,
            needs_account: false,
            freshness: Some(read.freshness),
        })
    }
}

#[async_trait]
impl SummarySource for Monitor {
    fn supports(&self, kind: SummaryKind) -> bool {
        kind == SummaryKind::Status
    }

    async fn summary(
        &self,
        kind: SummaryKind,
        mode: ReadMode,
    ) -> Result<SourceRead<Value>, ApiError> {
        if !self.supports(kind) {
            return Err(ApiError::NOT_FOUND);
        }

        let read = self
            .cache
            .get("status".to_owned(), &self.metrics, mode, self.fetch())
            .await?;

        let _lease = self.normalizations.try_acquire()?;

        Ok(SourceRead {
            data: projection::project(self.kind, &read.body)?,
            freshness: Freshness {
                source: self.descriptor.name.clone(),
                cache: read.state.into(),
                age_seconds: Some(read.age),
                checked_at: None,
                note: None,
            },
        })
    }
}
