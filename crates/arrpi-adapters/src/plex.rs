//! Bounded read-only Plex community metadata; personal ratings are never projected.
mod projection;
use crate::{
    budget::Budget,
    cache::Cache,
    http::{self, Http, Intent},
    metrics::Metrics,
};
use arrpi_core::{
    config::{CachePolicy, ConfigError, Source},
    contracts::Counters,
    error::ApiError,
    source::{
        ApiSource, Body, Descriptor, Freshness, Health, Principal, RatingsSource, ReadMode,
        SourceRead,
    },
};
use async_trait::async_trait;
use bytes::Bytes;
use secrecy::{ExposeSecret as _, SecretString};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

pub(crate) struct Plex {
    descriptor: Descriptor,
    base: url::Url,
    key: SecretString,
    http: Http,
    cache: Arc<Cache>,
    budget: Budget,
    metrics: Metrics,
    limit: usize,
    upstream: Semaphore,
    timeout: Duration,
}

impl Plex {
    pub(crate) fn new(
        name: &str,
        source: &Source,
        policy: CachePolicy,
        http: Http,
        budget: Budget,
        cache: Arc<Cache>,
    ) -> Result<Self, ConfigError> {
        Ok(Self {
            descriptor: Descriptor {
                name: name.to_owned(),
                kind: "plex",
                service_binding: None,
                endpoints: (&[][..]).into(),
            },
            base: source.base_url()?.into(),
            key: crate::credential(source.api_key.value())?,
            http,
            cache,
            budget,
            metrics: Metrics::new(),
            limit: policy.max_body_bytes,
            upstream: Semaphore::new(policy.max_upstream_requests),
            timeout: Duration::from_secs(policy.timeout_seconds),
        })
    }

    async fn request(&self, path: &str, query: &[(&str, String)]) -> Result<Bytes, ApiError> {
        let _slot = tokio::time::timeout(Duration::from_millis(250), self.upstream.acquire())
            .await
            .map_err(|error| http::redacted_error(&error, ApiError::CAPACITY))?
            .map_err(|error| http::redacted_error(&error, ApiError::UNAVAILABLE))?;
        let url = self
            .base
            .join(path)
            .map_err(|error| http::redacted_error(&error, ApiError::BAD_QUERY))?;
        let mut key = reqwest::header::HeaderValue::from_str(self.key.expose_secret())
            .map_err(|error| http::redacted_error(&error, ApiError::REJECTED))?;

        key.set_sensitive(true);
        let response = self
            .http
            .client()
            .get(url)
            .query(query)
            .header("X-Plex-Token", key)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|error| http::transport_error(&error))?;

        if response.status() != reqwest::StatusCode::OK {
            return Err(ApiError::upstream_read(response.status().as_u16()));
        }

        http::bounded(response, self.limit, Intent::Read, Some(&self.budget)).await
    }

    async fn fetch(&self) -> Result<Bytes, ApiError> {
        let reservation = self.budget.reserve(self.limit).await?;
        let mut output = Vec::with_capacity(self.limit.min(4096));

        output.push(b'[');

        for kind in [1, 2] {
            let mut offset = 0usize;

            loop {
                let body = self
                    .request(
                        "library/all",
                        &[
                            ("type", kind.to_string()),
                            ("includeGuids", "1".to_owned()),
                            ("includeRatings", "1".to_owned()),
                            ("X-Plex-Container-Start", offset.to_string()),
                            ("X-Plex-Container-Size", "100".to_owned()),
                        ],
                    )
                    .await?;
                let page = projection::page(&body, kind, offset)?;
                let count = page.count();
                let finished = page.finished;

                drop(body);

                if count > 0 {
                    let details = self
                        .request(
                            &page.metadata_path(),
                            &[
                                ("includeGuids", "1".to_owned()),
                                ("includeRatings", "1".to_owned()),
                            ],
                        )
                        .await?;

                    page.append(&details, kind, &mut output, self.limit)?;
                }

                offset += count;

                if finished {
                    break;
                }

                if offset > self.limit {
                    return Err(ApiError::RESPONSE_TOO_LARGE);
                }
            }
        }

        output.push(b']');
        output.shrink_to_fit();
        crate::budget::leased(output, reservation)
    }
}

impl std::fmt::Debug for Plex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plex")
            .field("descriptor", &self.descriptor)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl ApiSource for Plex {
    fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    fn ratings(&self) -> Option<&dyn RatingsSource> {
        Some(self)
    }

    fn counters(&self) -> Counters {
        self.metrics.snapshot()
    }

    async fn health(&self, _principal: &Principal, _mode: ReadMode) -> Result<Health, ApiError> {
        let body = self.request("library/sections", &[]).await?;
        let value: serde_json::Value = serde_json::from_slice(&body)?;

        if !value
            .get("MediaContainer")
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(ApiError::INVALID_RESPONSE);
        }

        Ok(Health {
            reachable: Some(true),
            version: None,
            health_issue_count: None,
            writes_supported: Some(false),
            needs_account: false,
            freshness: None,
        })
    }
}

#[async_trait]
impl RatingsSource for Plex {
    async fn snapshot(&self, mode: ReadMode) -> Result<SourceRead<Body>, ApiError> {
        let read = self
            .cache
            .get("community-ratings".to_owned(), &self.metrics, mode, async {
                tokio::time::timeout(self.timeout, self.fetch())
                    .await
                    .map_err(|error| http::redacted_error(&error, ApiError::TIMEOUT))?
            })
            .await?;

        Ok(SourceRead {
            data: read.body.into(),
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
