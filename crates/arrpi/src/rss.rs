//! RSS 2.0 export of the scoped, bounded release calendar.

use std::sync::Arc;

use axum::{
    extract::{RawQuery, State},
    http::{HeaderMap, header},
    response::{IntoResponse as _, Response},
};
use rss::{Category, Channel, Guid, Item};

use crate::{
    calendar::{Calendar, Event},
    config::Instance,
    error::{ApiError, HttpError},
    feeds,
    proxy::Gateway,
};

pub(crate) async fn export(
    State(gateway): State<Arc<Gateway>>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
) -> Result<Response, HttpError> {
    let data = feeds::load(&gateway, raw.as_deref(), &headers).await?;

    render(&gateway.instance, &data).map_err(HttpError)
}

pub(crate) fn render(feed: &Instance, data: &Calendar) -> Result<Response, ApiError> {
    let mut channel = Channel::default();

    channel.set_title(feed.title.clone());
    channel.set_link(feed.link.clone());
    channel.set_description(
        "Monitored TV and movie release dates. Release dates do not imply playback availability.",
    );
    channel.set_generator(Some("ArrPi".to_owned()));
    channel.set_items(
        data.events
            .iter()
            .map(item)
            .collect::<Result<Vec<_>, _>>()?,
    );

    let body = channel.to_string();

    if body.len() > 8 * 1024 * 1024 {
        return Err(ApiError::INVALID_RESPONSE);
    }

    Ok((
        [
            (header::CONTENT_TYPE, "application/rss+xml; charset=utf-8"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        body,
    )
        .into_response())
}

fn item(release: &Event) -> Result<Item, ApiError> {
    let mut item = Item::default();
    let mut guid = Guid::default();
    let mut category = Category::default();

    guid.set_value(feeds::identity(release));
    guid.set_permalink(false);
    category.set_name(release.kind.name());
    item.set_guid(Some(guid));
    item.set_title(Some(
        feeds::summary(release)
            .chars()
            .filter(|character| {
                matches!(character,
                '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' |
                '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
            })
            .collect::<String>(),
    ));
    item.set_link(feeds::public_link(release));
    item.set_pub_date(Some(feeds::timestamp(release)?.to_rfc2822()));
    item.set_categories(vec![category]);
    item.set_description(Some(format!(
        "Release date: {}. {}",
        release.date,
        if release.has_file {
            "Available in the library."
        } else {
            "Not yet available in the library."
        }
    )));

    Ok(item)
}
