//! Sonarr and Radarr adapters share bounded native mechanics behind distinct source types.

mod calendar;
mod profiles;
mod summaries;

use crate::{
    budget::Budget,
    cache::{Cache, Cached},
    http::{self, Http, Intent},
    metrics::Metrics,
};
use arrpi_core::{
    arguments::MediaType,
    config::{CachePolicy, ConfigError, Source, SourceKind},
    contracts::{CacheState, CalendarEvent, Counters},
    error::ApiError,
    source::{
        ApiSource, Body, CalendarSource, Descriptor, Freshness, Health, LibrarySource,
        NativeSource, Principal, QualityProfile, ReadMode, SourceRead,
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use reqwest::header::{self, HeaderValue};
use secrecy::{ExposeSecret as _, SecretString};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Clone, Debug)]
pub(crate) struct Pools {
    pub(crate) budget: Budget,
    pub(crate) normalizations: arrpi_core::admission::Normalizations,
    pub(crate) operational_cache: Arc<Cache>,
}

pub(crate) struct Native {
    pub(crate) descriptor: Descriptor,
    kind: SourceKind,
    media_type: MediaType,
    operational_cache: Arc<Cache>,
    base: url::Url,
    key: SecretString,
    cache: Arc<Cache>,
    upstream: Semaphore,
    metrics: Metrics,
    http: Http,
    limit: usize,
    budget: Budget,
    normalizations: arrpi_core::admission::Normalizations,
}

impl Native {
    pub(crate) fn new(
        name: &str,
        source: &Source,
        policy: CachePolicy,
        http: Http,
        pools: Pools,
        cache: Arc<Cache>,
    ) -> Result<Self, ConfigError> {
        let media_type = match source.kind {
            SourceKind::Sonarr => MediaType::Tv,
            SourceKind::Radarr => MediaType::Movie,
            _ => return Err(ConfigError::invalid("not a library provider")),
        };

        Ok(Self {
            media_type,
            operational_cache: pools.operational_cache,
            descriptor: Descriptor {
                name: name.to_owned(),
                kind: source.kind.name(),
                service_binding: source.seerr_server_id,
                endpoints: source.kind.endpoints().into(),
            },
            kind: source.kind,
            base: source.base_url()?.into(),
            key: super::credential(source.api_key.value())?,
            cache,
            upstream: Semaphore::new(policy.max_upstream_requests),
            metrics: Metrics::new(),
            http,
            limit: policy.max_body_bytes,
            budget: pools.budget,
            normalizations: pools.normalizations,
        })
    }

    async fn fetch(&self, endpoint: &str, query: &str) -> Result<Bytes, ApiError> {
        let _permit = tokio::time::timeout(Duration::from_millis(250), self.upstream.acquire())
            .await
            .map_err(|error| {
                tracing::debug!(
                    event = "native_validation",
                    category = std::any::type_name_of_val(&error)
                );
                ApiError::CAPACITY
            })?
            .map_err(|error| {
                tracing::debug!(
                    event = "native_validation",
                    category = std::any::type_name_of_val(&error)
                );
                ApiError::UNAVAILABLE
            })?;
        let mut url = self
            .base
            .join(&format!("api/v3/{endpoint}"))
            .map_err(|error| {
                tracing::debug!(
                    event = "native_validation",
                    category = std::any::type_name_of_val(&error)
                );
                ApiError::NOT_FOUND
            })?;

        url.set_query(Some(query));
        let mut key = HeaderValue::from_str(self.key.expose_secret()).map_err(|error| {
            tracing::debug!(
                event = "native_validation",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::REJECTED
        })?;

        key.set_sensitive(true);
        let response = self
            .http
            .client()
            .get(url)
            .header("X-Api-Key", key)
            .header(header::ACCEPT, "application/json")
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

    async fn cached(
        &self,
        endpoint: &str,
        query: &str,
        mode: ReadMode,
    ) -> Result<Cached, ApiError> {
        if !self
            .descriptor
            .endpoints
            .iter()
            .any(|native| native.name() == endpoint)
        {
            return Err(ApiError::NOT_FOUND);
        }

        let cache = if matches!(endpoint, "queue" | "health") {
            &self.operational_cache
        } else {
            &self.cache
        };

        cache
            .get(
                format!("{endpoint}?{query}"),
                &self.metrics,
                mode,
                self.fetch(endpoint, query),
            )
            .await
    }

    async fn health(&self, mode: ReadMode) -> Result<Health, ApiError> {
        let read = self.cached("health", "", mode).await?;
        let issues: Vec<serde::de::IgnoredAny> = serde_json::from_slice(&read.body)?;

        Ok(Health {
            reachable: (!matches!(read.state, CacheState::Stale)).then_some(true),
            version: None,
            health_issue_count: Some(issues.len()),
            writes_supported: None,
            needs_account: false,
            freshness: Some(self.freshness(&read)),
        })
    }

    fn freshness(&self, read: &Cached) -> Freshness {
        Freshness {
            source: self.descriptor.name.clone(),
            cache: read.state.into(),
            age_seconds: Some(read.age),
            checked_at: None,
            note: None,
        }
    }
}

impl std::fmt::Debug for Native {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Native")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

macro_rules! native_sources {
    ($($adapter:ident),+ $(,)?) => { $(
        #[derive(Debug)]
        pub(crate) struct $adapter(pub(crate) Native);

        #[async_trait]
        impl ApiSource for $adapter {
            fn descriptor(&self) -> &Descriptor { &self.0.descriptor }

            async fn health(&self, _principal: &Principal, mode: ReadMode) -> Result<Health, ApiError> { self.0.health(mode).await }

            fn summaries(&self) -> Option<&dyn arrpi_core::summaries::SummarySource> { Some(&self.0) }

            fn library(&self) -> Option<&dyn LibrarySource> { Some(&self.0) }

            fn calendar(&self) -> Option<&dyn CalendarSource> { Some(&self.0) }

            fn native(&self) -> Option<&dyn NativeSource> { Some(&self.0) }

            fn counters(&self) -> Counters { self.0.metrics.snapshot() }
        }
    )+ };
}

native_sources!(Sonarr, Radarr);

#[async_trait]
impl NativeSource for Native {
    async fn read(
        &self,
        endpoint: &str,
        query: &str,
        mode: ReadMode,
    ) -> Result<SourceRead<Body>, ApiError> {
        let cached = self.cached(endpoint, query, mode).await?;
        let freshness = self.freshness(&cached);

        Ok(SourceRead {
            data: cached.body.into(),
            freshness,
        })
    }
}

#[async_trait]
impl LibrarySource for Native {
    fn media_type(&self) -> MediaType {
        self.media_type
    }

    async fn snapshot(&self, mode: ReadMode) -> Result<SourceRead<Body>, ApiError> {
        self.read(
            if self.media_type() == MediaType::Movie {
                "movie"
            } else {
                "series"
            },
            "",
            mode,
        )
        .await
    }

    async fn profiles(&self) -> Result<Vec<QualityProfile>, ApiError> {
        let cached = self.cached("qualityprofile", "", ReadMode::Fresh).await?;
        let definitions: Value = serde_json::from_slice(&cached.body)?;
        let definitions = definitions
            .as_array()
            .filter(|rows| rows.len() <= 200)
            .ok_or(ApiError::INVALID_RESPONSE)?;

        Ok(definitions
            .iter()
            .filter_map(|row| {
                Some(QualityProfile {
                    id: row["id"].as_u64().filter(|id| *id > 0)?,
                    name: row["name"].as_str()?.to_owned(),
                    non_4k: profiles::safe_qualities(row),
                })
            })
            .collect())
    }
}

#[async_trait]
impl CalendarSource for Native {
    async fn calendar(
        &self,
        start: &str,
        end: &str,
        mode: ReadMode,
    ) -> Result<SourceRead<Vec<CalendarEvent>>, ApiError> {
        let window = (
            arrpi_core::arguments::date(start)?.into(),
            arrpi_core::arguments::date(end)?.into(),
        );

        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("start", start)
            .append_pair("end", &format!("{end}T23:59:59.9999999Z"))
            .append_pair("unmonitored", "false")
            .append_pair("includeSeries", "true")
            .finish();
        let cached = self.cached("calendar", &query, mode).await?;
        let _normalization = self.normalizations.try_acquire()?;

        let events = calendar::collect(&self.descriptor.name, self.kind, &cached.body, window)?;

        Ok(SourceRead {
            data: events,
            freshness: self.freshness(&cached),
        })
    }
}
