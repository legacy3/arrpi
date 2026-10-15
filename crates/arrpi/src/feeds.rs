//! Shared release identity, presentation and complete-source export policy.

use std::sync::Arc;

use axum::http::HeaderMap;
use chrono::{DateTime, Utc};

use crate::{calendar, error::ApiError, proxy::Gateway, query};

const IMDB_TITLE_ORIGIN: &str = "https://www.imdb.com/title/";

pub(crate) async fn load(
    gateway: &Arc<Gateway>,
    raw: Option<&str>,
    headers: &HeaderMap,
) -> Result<calendar::Calendar, ApiError> {
    if headers.contains_key("x-arrpi-feed-token")
        || crate::auth::authenticate(headers, &gateway.clients).is_err()
    {
        return crate::subscriptions::load(gateway, raw, headers).await;
    }

    complete(calendar::load(gateway, raw, headers).await?)
}

pub(crate) fn complete(data: calendar::Calendar) -> Result<calendar::Calendar, ApiError> {
    // A partial successful feed can look like removed items to subscribers.
    if data
        .sources
        .iter()
        .any(|source| source.status != arrpi_core::contracts::SourceAvailability::Ok)
    {
        return Err(ApiError::UNAVAILABLE);
    }

    Ok(data)
}

pub(crate) fn identity(release: &calendar::Event) -> String {
    format!(
        "{}-{}-{}@arrpi",
        release.source,
        release.id,
        release.kind.name()
    )
}

pub(crate) fn summary(release: &calendar::Event) -> String {
    let Some(series) = &release.series else {
        return format!("{} · {}", release.title, release.kind.name());
    };
    let Some((season, episode)) = release.season.zip(release.episode) else {
        return format!("{series} · {}", release.title);
    };

    format!("{series} · S{season:02}E{episode:02} · {}", release.title)
}

pub(crate) fn timestamp(release: &calendar::Event) -> Result<DateTime<Utc>, ApiError> {
    if release.kind == arrpi_core::contracts::EventKind::Episode {
        return DateTime::parse_from_rfc3339(&release.date)
            .map(|date| date.with_timezone(&Utc))
            .map_err(|error| {
                tracing::debug!(event = "invalid_feed_date", category = ?error.kind());
                ApiError::INVALID_RESPONSE
            });
    }

    query::date(&release.date)?
        .and_hms_opt(0, 0, 0)
        .map(|date| date.and_utc())
        .ok_or(ApiError::INVALID_RESPONSE)
}

pub(crate) fn public_link(release: &calendar::Event) -> Option<String> {
    release
        .imdb_id
        .as_ref()
        .map(|imdb| format!("{IMDB_TITLE_ORIGIN}{imdb}/"))
}
