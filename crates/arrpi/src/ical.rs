//! Authenticated RFC 5545 export using the same bounded calendar and cache policy.

use std::sync::Arc;

use axum::{
    extract::{RawQuery, State},
    http::{HeaderMap, header},
    response::{IntoResponse as _, Response},
};
use icalendar::{Calendar, Component as _, Event, EventLike as _};

use crate::{
    calendar::{Calendar as Releases, Event as Release},
    error::{ApiError, HttpError},
    feeds,
    proxy::Gateway,
    query,
};

pub(crate) async fn export(
    State(gateway): State<Arc<Gateway>>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let data = feeds::load(&gateway, raw.as_deref(), &headers).await?;

    render(&data).map_err(HttpError)
}

pub(crate) fn render(data: &Releases) -> Result<Response, ApiError> {
    let mut calendar = Calendar::empty();

    for property in [
        ("VERSION", "2.0"),
        ("PRODID", "-//ArrPi//Release Calendar//EN"),
        ("CALSCALE", "GREGORIAN"),
    ] {
        calendar.append_property(property);
    }

    calendar.name("ArrPi");

    for release in &data.events {
        calendar.push(event(release)?);
    }

    let body = calendar.to_string();

    if body.len() > 8 * 1024 * 1024 {
        return Err(ApiError::INVALID_RESPONSE);
    }

    Ok((
        [
            (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"calendar.ics\"",
            ),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        body,
    )
        .into_response())
}

fn event(release: &Release) -> Result<Event, ApiError> {
    let mut event = Event::with_uid(&feeds::identity(release));
    let summary = feeds::summary(release);

    event
        .summary(&summary)
        .description("Release date; availability to watch may differ.");

    if release.kind == arrpi_core::contracts::EventKind::Episode {
        event.starts(feeds::timestamp(release)?);
    } else {
        let date = query::date(&release.date).map_err(|error| {
            tracing::debug!(event = "invalid_ical_day", category = %error);
            ApiError::INVALID_RESPONSE
        })?;

        event.all_day(date);
    }

    if let Some(link) = feeds::public_link(release) {
        event.url(&link);
    }

    Ok(event.done())
}
