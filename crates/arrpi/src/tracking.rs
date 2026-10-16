//! Authenticated tracking endpoints with server-resolved native account bindings.

use std::sync::Arc;

use arrpi_core::{
    source::Principal,
    tracking::{Binding, DestinationAction, Provider, TrackingSource},
};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, header},
    response::IntoResponse as _,
};
use serde_json::json;
use std::collections::BTreeMap;

use crate::{
    auth,
    error::{ApiError, HttpError},
    proxy::Gateway,
};

pub(crate) type Empty = BTreeMap<String, serde_json::Value>;

pub(crate) async fn status(
    State(gateway): State<Arc<Gateway>>,
    headers: HeaderMap,
) -> Result<impl axum::response::IntoResponse, HttpError> {
    let principal = auth::authenticate(&headers, &gateway.clients)?;
    let (source, binding) = resolve(&gateway, principal, "tracking-view")?;

    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(source.status(binding).await?),
    ))
}

pub(crate) async fn action(
    State(gateway): State<Arc<Gateway>>,
    Path(action): Path<String>,
    headers: HeaderMap,
    Json(input): Json<Empty>,
) -> Result<axum::response::Response, HttpError> {
    let principal = auth::authenticate(&headers, &gateway.clients)?;

    if !input.is_empty() {
        return Err(ApiError::BAD_QUERY.into());
    }

    let (source, binding) = resolve(&gateway, principal, "tracking-manage")?;
    let value = if let Ok(provider) = serde_json::from_value::<Provider>(json!(action)) {
        json!(source.authorize(binding, provider).await?)
    } else {
        match action.as_str() {
            "enable" => json!(source.set_enabled(binding, true).await?),

            "pause" => json!(source.set_enabled(binding, false).await?),

            "disconnect" => json!(source.disconnect(binding).await?),

            _ => {
                let (provider, operation) = action.split_once('-').ok_or(ApiError::NOT_FOUND)?;

                if operation == "poll" {
                    return Ok((
                        [(header::CACHE_CONTROL, "private, no-store")],
                        Json(source.poll(binding, provider).await?),
                    )
                        .into_response());
                }

                let operation: DestinationAction = serde_json::from_value(json!(operation))
                    .map_err(|error| {
                        tracing::debug!(
                            error_type = %std::any::type_name_of_val(&error),
                            "invalid tracking operation"
                        );
                        ApiError::NOT_FOUND
                    })?;

                json!(source.destination(binding, provider, operation).await?)
            }
        }
    };

    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(value)).into_response())
}

fn resolve<'a>(
    gateway: &'a Gateway,
    principal: &'a Principal,
    operation: &str,
) -> Result<(&'a dyn TrackingSource, &'a Binding), ApiError> {
    let binding = principal.tracking.as_ref().ok_or(ApiError::FORBIDDEN)?;

    if !principal.allows(&binding.source, operation) {
        return Err(ApiError::FORBIDDEN);
    }

    let source = gateway
        .commands
        .sources
        .source(&binding.source)
        .and_then(|source| source.tracking())
        .ok_or(ApiError::NOT_FOUND)?;

    Ok((source, binding))
}
