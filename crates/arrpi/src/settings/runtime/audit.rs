//! Operator-only, account-scoped write history filtering.

use super::Runtime;
use crate::error::HttpError;
use arrpi_adapters::storage::AuditFilter;
use arrpi_core::{contracts::AuditPage, error::ApiError};
use axum::{
    Json,
    extract::{Query, State},
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AuditQuery {
    client: String,
    #[serde(default)]
    query: String,
    tool: Option<String>,
    outcome: Option<String>,
    since: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    limit: Option<usize>,
}

pub(super) async fn read(
    State(runtime): State<Arc<Runtime>>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<AuditPage>, HttpError> {
    let (_, config) = runtime.snapshot().await?;

    if !config.clients.iter().any(|c| c.name == query.client) {
        return Err(ApiError::NOT_FOUND.into());
    }

    let limit = query
        .limit
        .unwrap_or_else(|| 20.min(config.mcp.limits.max_page_items));

    if !(1..=config.mcp.limits.max_page_items).contains(&limit)
        || i64::try_from(query.offset).is_err()
    {
        return Err(ApiError::BAD_QUERY.into());
    }

    if query.query.chars().count() > 128
        || query
            .tool
            .as_ref()
            .is_some_and(|name| arrpi_core::registry::ToolId::parse(name).is_none())
        || query.outcome.as_ref().is_some_and(|value| value.len() > 64)
        || query
            .since
            .as_ref()
            .is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_err())
    {
        return Err(ApiError::BAD_QUERY.into());
    }

    let since = query
        .since
        .as_deref()
        .map(chrono::DateTime::parse_from_rfc3339)
        .transpose()
        .map_err(|error| {
            tracing::debug!(event="audit_timestamp",category=%error);
            ApiError::BAD_QUERY
        })?
        .map(|date| {
            date.with_timezone(&chrono::Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        });
    let store = Arc::clone(&runtime.store);

    Ok(Json(
        tokio::task::spawn_blocking(move || {
            store.search_audit(
                &query.client,
                query.offset,
                limit,
                &AuditFilter {
                    query: &query.query,
                    tool: query.tool.as_deref(),
                    outcome: query.outcome.as_deref(),
                    since: since.as_deref(),
                },
            )
        })
        .await
        .map_err(ApiError::from)??,
    ))
}
