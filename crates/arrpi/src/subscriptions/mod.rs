//! User-scoped, revocable feed capabilities; issuance is an internal permission.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, header},
    response::{IntoResponse as _, Response},
};
use secrecy::SecretString;

use crate::{
    auth, calendar,
    error::{ApiError, HttpError},
    feeds,
    proxy::Gateway,
};

pub(crate) use arrpi_adapters::storage::{FeedAction as Action, SubscriptionStore as Store};

pub(crate) async fn issue(
    State(gateway): State<Arc<Gateway>>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    update(&gateway, raw.as_deref(), &headers, Action::Issue)
        .await
        .map_err(HttpError)
}

pub(crate) async fn rotate(
    State(gateway): State<Arc<Gateway>>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    update(&gateway, raw.as_deref(), &headers, Action::Rotate)
        .await
        .map_err(HttpError)
}

pub(crate) async fn revoke(
    State(gateway): State<Arc<Gateway>>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    update(&gateway, raw.as_deref(), &headers, Action::Revoke)
        .await
        .map_err(HttpError)
}

async fn update(
    gateway: &Arc<Gateway>,
    raw: Option<&str>,
    headers: &HeaderMap,
    action: Action,
) -> Result<Response, ApiError> {
    if raw.is_some_and(|query| !query.is_empty()) {
        return Err(ApiError::BAD_QUERY);
    }

    let access = auth::authenticate(headers, &gateway.clients)?;

    if !access.allows("feeds", "issue") {
        return Err(ApiError::FORBIDDEN);
    }

    let mut subjects = headers.get_all("x-arrpi-subject").iter();
    let subject = subjects
        .next()
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::BAD_QUERY)?;

    if subjects.next().is_some() {
        return Err(ApiError::BAD_QUERY);
    }

    let sources: Vec<String> = gateway
        .commands
        .sources
        .iter()
        .filter(|(name, source)| {
            source.calendar().is_some() && access.allows(name, "calendar-view")
        })
        .map(|(name, _)| name.to_owned())
        .collect();

    if sources.is_empty() {
        return Err(ApiError::FORBIDDEN);
    }

    let store = gateway.subscriptions.as_ref().ok_or(ApiError::NOT_FOUND)?;
    let result = store.update(subject.to_owned(), sources, action).await?;

    Ok(([(header::CACHE_CONTROL, "private, no-store")], Json(result)).into_response())
}

pub(crate) async fn grants(
    gateway: &Arc<Gateway>,
    headers: &HeaderMap,
) -> Result<Vec<String>, ApiError> {
    let mut values = headers.get_all("x-arrpi-feed-token").iter();
    let token = if let Some(value) = values.next() {
        if values.next().is_some() {
            return Err(ApiError::UNAUTHORIZED);
        }

        value.to_str().map_err(|error| {
            tracing::debug!(event = "invalid_feed_header", category = %error);
            ApiError::UNAUTHORIZED
        })?
    } else {
        auth::bearer(headers)?
    };

    sources(gateway, token.to_owned().into()).await
}

async fn sources(gateway: &Arc<Gateway>, token: SecretString) -> Result<Vec<String>, ApiError> {
    let store = gateway
        .subscriptions
        .as_ref()
        .ok_or(ApiError::UNAUTHORIZED)?;
    let sources = store.sources(token).await?;

    if sources.is_empty()
        || sources
            .iter()
            .any(|source| gateway.commands.sources.source(source).is_none())
    {
        return Err(ApiError::FORBIDDEN);
    }

    Ok(sources)
}

pub(crate) async fn load(
    gateway: &Arc<Gateway>,
    raw: Option<&str>,
    headers: &HeaderMap,
) -> Result<calendar::Calendar, ApiError> {
    let sources = grants(gateway, headers).await?;

    load_for(gateway, raw, sources).await
}

async fn load_for(
    gateway: &Arc<Gateway>,
    raw: Option<&str>,
    sources: Vec<String>,
) -> Result<calendar::Calendar, ApiError> {
    let query = if raw.is_none_or(str::is_empty) {
        gateway
            .subscriptions
            .as_ref()
            .ok_or(ApiError::UNAUTHORIZED)?
            .query()?
    } else {
        raw.unwrap_or_default().to_owned()
    };
    let data = calendar::load_for(gateway, Some(&query), &HeaderMap::new(), sources).await?;

    feeds::complete(data)
}

pub(crate) async fn public_feed(
    State(gateway): State<Arc<Gateway>>,
    Path((token, format)): Path<(String, String)>,
    RawQuery(raw): RawQuery,
) -> Result<Response, HttpError> {
    if !matches!(format.as_str(), "calendar.rss" | "calendar.ics") {
        return Err(ApiError::NOT_FOUND.into());
    }

    let sources = sources(&gateway, token.into()).await?;
    let data = load_for(&gateway, raw.as_deref(), sources).await?;

    if format == "calendar.rss" {
        crate::rss::render(&gateway.instance, &data).map_err(HttpError)
    } else {
        crate::ical::render(&data).map_err(HttpError)
    }
}
