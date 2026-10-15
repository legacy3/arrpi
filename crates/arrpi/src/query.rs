//! Canonical, endpoint-specific queries without arbitrary upstream parameters.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use url::form_urlencoded;

use crate::error::ApiError;

pub(crate) fn canonical(endpoint: &str, raw: Option<&str>) -> Result<String, ApiError> {
    let raw = raw.unwrap_or_default();

    if raw.len() > 1024 {
        return Err(ApiError::BAD_QUERY);
    }

    let mut parameters = BTreeMap::new();

    for (key, value) in form_urlencoded::parse(raw.as_bytes()) {
        validate(endpoint, &key, &value)?;

        if parameters
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(ApiError::BAD_QUERY);
        }
    }

    if endpoint == "calendar" {
        validate_window(&parameters)?;
        parameters.insert("unmonitored".into(), "false".into());
        parameters.insert("includeSeries".into(), "true".into());
    }

    let mut serializer = form_urlencoded::Serializer::new(String::new());

    serializer.extend_pairs(parameters.iter());
    Ok(serializer.finish())
}

pub(crate) fn date(value: &str) -> Result<NaiveDate, ApiError> {
    arrpi_core::arguments::date(value).map(Into::into)
}

fn validate(endpoint: &str, key: &str, value: &str) -> Result<(), ApiError> {
    match (endpoint, key) {
        ("calendar", "start" | "end") => {
            date(value)?;
        }

        ("queue", "page" | "pageSize")
            if value
                .parse::<u16>()
                .is_ok_and(|number| (1..=100).contains(&number)) => {}

        ("queue", "includeUnknownSeriesItems" | "includeUnknownMovieItems")
            if matches!(value, "true" | "false") => {}

        _ => return Err(ApiError::BAD_QUERY),
    }

    Ok(())
}

fn validate_window(parameters: &BTreeMap<String, String>) -> Result<(), ApiError> {
    let start = parameters.get("start").ok_or(ApiError::BAD_QUERY)?;
    let end = parameters.get("end").ok_or(ApiError::BAD_QUERY)?;
    let span = date(end)?.signed_duration_since(date(start)?).num_days();

    if !(0..=93).contains(&span) {
        return Err(ApiError::BAD_QUERY);
    }

    Ok(())
}

pub(crate) fn read_mode(headers: &axum::http::HeaderMap) -> arrpi_core::source::ReadMode {
    if headers
        .get_all(axum::http::header::CACHE_CONTROL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|value| value.trim().eq_ignore_ascii_case("no-cache"))
    {
        arrpi_core::source::ReadMode::Fresh
    } else {
        arrpi_core::source::ReadMode::Cached
    }
}
