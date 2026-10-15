//! Calendar HTTP decoding and response status; aggregation lives in the core.

use std::sync::Arc;

use crate::{
    auth,
    error::{ApiError, HttpError},
    proxy::Gateway,
    query,
};
use arrpi_core::{arguments::Dates, contracts::SourceAvailability};
use axum::{
    Json,
    extract::{RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse as _, Response},
};

#[rustfmt::skip]
pub(crate) use arrpi_core::contracts::{Calendar, CalendarEvent as Event};

pub(crate) async fn combined(
    State(gateway): State<Arc<Gateway>>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let calendar = load(&gateway, raw.as_deref(), &headers).await?;
    let status = if calendar
        .sources
        .iter()
        .any(|source| source.status == SourceAvailability::Ok)
    {
        StatusCode::OK
    } else {
        StatusCode::BAD_GATEWAY
    };

    Ok((
        status,
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(calendar),
    )
        .into_response())
}

pub(crate) async fn load(
    gateway: &Arc<Gateway>,
    raw: Option<&str>,
    headers: &HeaderMap,
) -> Result<Calendar, ApiError> {
    let principal = auth::authenticate(headers, &gateway.clients)?;

    gateway
        .commands
        .calendar(principal, dates(raw, headers)?)
        .await
}

pub(crate) async fn load_for(
    gateway: &Arc<Gateway>,
    raw: Option<&str>,
    headers: &HeaderMap,
    permitted: Vec<String>,
) -> Result<Calendar, ApiError> {
    gateway
        .commands
        .calendar_for(&permitted, dates(raw, headers)?)
        .await
}

fn dates(raw: Option<&str>, headers: &HeaderMap) -> Result<Dates, ApiError> {
    let query = query::canonical("calendar", raw)?;
    let pairs: std::collections::BTreeMap<_, _> =
        url::form_urlencoded::parse(query.as_bytes()).collect();

    Ok(Dates {
        start: pairs.get("start").ok_or(ApiError::BAD_QUERY)?.to_string(),
        end: pairs.get("end").ok_or(ApiError::BAD_QUERY)?.to_string(),
        no_cache: query::read_mode(headers) == arrpi_core::source::ReadMode::Fresh,
    })
}
