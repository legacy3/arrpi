//! Operator-only probes reuse source transport and never impersonate requesters.
use super::{Runtime, lock_error};
use crate::{
    auth,
    error::{ApiError, HttpError},
};
use arrpi_core::{
    contracts::{ConnectionHealth, ConnectionState},
    source::ReadMode,
};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use std::{sync::Arc, time::Duration};

pub(super) async fn read(
    State(runtime): State<Arc<Runtime>>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Result<Json<ConnectionHealth>, HttpError> {
    let (gateway, principal) = {
        let live = runtime.live.read().map_err(|error| lock_error(&error))?;
        let access = auth::authenticate(&headers, &live.clients)?;

        if !access.allows("config", "manage") {
            return Err(ApiError::FORBIDDEN.into());
        }

        (Arc::clone(&live.gateway), (**access).clone())
    };
    let Some(source) = gateway.commands.sources.source(&name) else {
        if name == "seerr" {
            return Ok(Json(ConnectionHealth {
                source: name,
                status: ConnectionState::Unconfigured,
                health_issue_count: None,
            }));
        }

        return Err(ApiError::NOT_FOUND.into());
    };
    let _permit = Arc::clone(&gateway.requests)
        .try_acquire_owned()
        .map_err(|error| {
            tracing::debug!(
                event = "connection_health_admission",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::CAPACITY
        })?;
    let result = tokio::time::timeout(
        Duration::from_secs(gateway.policy.timeout_seconds.saturating_add(1)),
        source.connection_health(&principal, ReadMode::Fresh),
    )
    .await
    .unwrap_or_else(|error| {
        tracing::debug!(
            event = "connection_health_timeout",
            category = std::any::type_name_of_val(&error)
        );
        Err(ApiError::TIMEOUT)
    });
    let (status, health_issue_count) = match result {
        Ok(health) if health.reachable == Some(true) => {
            let state = if health.health_issue_count.is_some_and(|count| count > 0) {
                ConnectionState::Attention
            } else {
                ConnectionState::Healthy
            };

            (state, health.health_issue_count)
        }

        Err(error) if error == ApiError::FORBIDDEN || error == ApiError::UNAUTHORIZED => {
            (ConnectionState::AuthenticationFailed, None)
        }

        _ => (ConnectionState::Unavailable, None),
    };

    Ok(Json(ConnectionHealth {
        source: name,
        status,
        health_issue_count,
    }))
}
