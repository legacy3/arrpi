//! One no-retry client and bounded response collector for native adapters.
use crate::budget::{self, Budget};
use arrpi_core::{config::ConfigError, error::ApiError};
use bytes::Bytes;
use std::time::Duration;

/// Explicit collection intent preserves indeterminate native mutations.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Intent {
    Read,
    Write,
}

/// Nonsecret pool and timeout options chosen by the composition root.
#[derive(Clone, Copy, Debug)]
pub struct HttpOptions {
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub pool_idle_per_host: usize,
}

/// Shared no-proxy, no-redirect, no-retry transport without native credentials.
#[derive(Clone, Debug)]
pub struct Http {
    client: reqwest::Client,
}

impl Http {
    /// Create a transport without authorizing any native endpoint.
    ///
    /// # Errors
    /// Returns a redacted transport error when client initialization fails.
    pub fn new(options: HttpOptions) -> Result<Self, ConfigError> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .timeout(options.timeout)
            .connect_timeout(options.connect_timeout)
            .pool_max_idle_per_host(options.pool_idle_per_host)
            .build()
            .map_err(|error| {
                tracing::debug!(
                    event = "http_client_init_failed",
                    category = std::any::type_name_of_val(&error)
                );
                ConfigError::TRANSPORT
            })?;

        Ok(Self { client })
    }

    pub(crate) const fn client(&self) -> &reqwest::Client {
        &self.client
    }
}

impl Intent {
    pub(crate) const fn failure(self, error: ApiError) -> ApiError {
        match self {
            Self::Read => error,
            Self::Write => ApiError::UNKNOWN_OUTCOME,
        }
    }
}

pub(crate) fn transport_error(error: &reqwest::Error) -> ApiError {
    tracing::debug!(
        event = "http_transport_failure",
        category = std::any::type_name_of_val(error)
    );

    if error.is_timeout() {
        ApiError::TIMEOUT
    } else {
        ApiError::UNAVAILABLE
    }
}

pub(crate) async fn bounded(
    response: reqwest::Response,
    limit: usize,
    intent: Intent,
    budget: Option<&Budget>,
) -> Result<Bytes, ApiError> {
    announced(&response, limit, intent)?;

    let reservation = if let Some(budget) = budget {
        Some(
            budget
                .reserve(limit)
                .await
                .map_err(|error| intent.failure(error))?,
        )
    } else {
        None
    };

    collect(response, limit, intent, reservation).await
}

pub(crate) async fn collect(
    mut response: reqwest::Response,
    limit: usize,
    intent: Intent,
    reservation: Option<tokio::sync::OwnedSemaphorePermit>,
) -> Result<Bytes, ApiError> {
    let announced = announced(&response, limit, intent)?;

    let mut body = Vec::with_capacity(announced.unwrap_or(8192).min(limit));

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| intent.failure(transport_error(&error)))?
    {
        if chunk.len() > limit.saturating_sub(body.len()) {
            return Err(intent.failure(ApiError::RESPONSE_TOO_LARGE));
        }

        let required = body.len() + chunk.len();

        if required > body.capacity() {
            let capacity = required.max(body.capacity().saturating_mul(2)).min(limit);

            body.reserve_exact(capacity - body.len());
        }

        body.extend_from_slice(&chunk);
        tokio::task::yield_now().await;
    }

    body.shrink_to_fit();
    Ok(match reservation {
        Some(reservation) => {
            budget::leased(body, reservation).map_err(|error| intent.failure(error))?
        }

        None => Bytes::from(body),
    })
}

fn announced(
    response: &reqwest::Response,
    limit: usize,
    intent: Intent,
) -> Result<Option<usize>, ApiError> {
    if response
        .content_length()
        .is_some_and(|length| length > u64::try_from(limit).unwrap_or(u64::MAX))
    {
        return Err(intent.failure(ApiError::RESPONSE_TOO_LARGE));
    }

    Ok(response
        .content_length()
        .and_then(|length| usize::try_from(length).ok()))
}
