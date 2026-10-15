//! Typed native calendar decoding, monitoring and inclusive release dates.
use arrpi_core::{
    config::SourceKind,
    contracts::{CalendarEvent as Event, EventKind},
    error::ApiError,
};
use chrono::{DateTime, Datelike as _, NaiveDate, SecondsFormat, Utc};
use serde::{
    Deserialize,
    de::{Error as _, SeqAccess, Visitor},
};

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct NativeItem {
    id: Option<u64>,
    title: Option<String>,
    monitored: Option<bool>,
    air_date_utc: Option<String>,
    series: Option<NativeSeries>,
    season_number: Option<u64>,
    episode_number: Option<u64>,
    in_cinemas: Option<String>,
    digital_release: Option<String>,
    physical_release: Option<String>,
    tmdb_id: Option<u64>,
    imdb_id: Option<String>,
    has_file: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct NativeSeries {
    title: Option<String>,
    monitored: Option<bool>,
    imdb_id: Option<String>,
    tvdb_id: Option<u64>,
}

#[derive(Debug)]
struct Items(Vec<NativeItem>);

#[derive(Debug)]
struct ItemsVisitor;

impl<'de> Visitor<'de> for ItemsVisitor {
    type Value = Items;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an array containing at most 4096 calendar items")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut items = Vec::with_capacity(64);

        while let Some(item) = seq.next_element()? {
            if items.len() == 4096 {
                return Err(A::Error::custom("calendar item limit exceeded"));
            }

            items.push(item);
        }

        Ok(Items(items))
    }
}

impl<'de> Deserialize<'de> for Items {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(ItemsVisitor)
    }
}

pub(crate) fn collect(
    source: &str,
    kind: SourceKind,
    body: &[u8],
    window: (NaiveDate, NaiveDate),
) -> Result<Vec<Event>, ApiError> {
    let Items(items) = serde_json::from_slice(body).map_err(ApiError::from)?;
    let mut events = Vec::with_capacity(items.len());

    for item in items {
        if item.monitored == Some(false)
            || item
                .series
                .as_ref()
                .is_some_and(|series| series.monitored == Some(false))
        {
            continue;
        }

        match kind {
            SourceKind::Sonarr => episode(&mut events, source, &item),
            SourceKind::Radarr => movie(&mut events, source, &item),
            _ => return Err(ApiError::NOT_FOUND),
        }
    }

    events.retain(|event| {
        event
            .date
            .get(..10)
            .and_then(|date| arrpi_core::arguments::date(date).ok())
            .map(chrono::NaiveDate::from)
            .is_some_and(|date| date >= window.0 && date <= window.1)
    });

    if events.len() > 8192 {
        return Err(ApiError::INVALID_RESPONSE);
    }

    Ok(events)
}

fn episode(events: &mut Vec<Event>, source: &str, item: &NativeItem) {
    let Some(date) = item
        .air_date_utc
        .as_deref()
        .filter(|date| valid_timestamp(date))
    else {
        return;
    };
    let Some(id) = item.id else {
        return;
    };

    events.push(Event {
        source: source.into(),
        id,
        title: text(item.title.as_deref()),
        kind: EventKind::Episode,
        date: DateTime::parse_from_rfc3339(date)
            .map(|date| date.to_utc().to_rfc3339_opts(SecondsFormat::Secs, true))
            .unwrap_or_default(),
        series: item
            .series
            .as_ref()
            .map(|series| text(series.title.as_deref())),
        season: item.season_number,
        episode: item.episode_number,
        tmdb_id: None,
        imdb_id: item
            .series
            .as_ref()
            .and_then(|series| imdb(series.imdb_id.as_deref())),
        tvdb_id: item.series.as_ref().and_then(|series| series.tvdb_id),
        has_file: item.has_file,
    });
}

fn movie(events: &mut Vec<Event>, source: &str, item: &NativeItem) {
    let Some(id) = item.id else {
        return;
    };

    for (field, kind) in [
        (&item.in_cinemas, EventKind::Cinema),
        (&item.digital_release, EventKind::Digital),
        (&item.physical_release, EventKind::Physical),
    ] {
        let Some(date) = field
            .as_deref()
            .filter(|date| valid_timestamp(date) || arrpi_core::arguments::date(date).is_ok())
        else {
            continue;
        };

        events.push(Event {
            source: source.into(),
            id,
            title: text(item.title.as_deref()),
            kind,
            date: date.chars().take(10).collect(),
            series: None,
            season: None,
            episode: None,
            tmdb_id: item.tmdb_id,
            imdb_id: imdb(item.imdb_id.as_deref()),
            tvdb_id: None,
            has_file: item.has_file,
        });
    }
}

fn valid_timestamp(value: &str) -> bool {
    DateTime::parse_from_rfc3339(value).is_ok_and(|date| date.with_timezone(&Utc).year() >= 1900)
}

fn text(value: Option<&str>) -> String {
    value
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\t'))
        .take(512)
        .collect()
}

fn imdb(value: Option<&str>) -> Option<String> {
    let value = value?;
    let digits = value.strip_prefix("tt")?;

    ((7..=10).contains(&digits.len()) && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.to_owned())
}
