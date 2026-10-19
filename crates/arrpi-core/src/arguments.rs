//! Strict shared tool input schemas for HTTP, CLI and MCP adapters.
use crate::error::ApiError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Page {
    #[serde(default)]
    #[schemars(range(max = 10_000))]
    pub offset: usize,
    #[serde(default = "limit")]
    #[schemars(range(min = 1, max = 500))]
    pub limit: usize,
    #[serde(default)]
    pub no_cache: bool,
}
fn limit() -> usize {
    20
}
impl Page {
    /// Validate supported request/issue pagination.
    ///
    /// # Errors
    /// Returns invalid arguments for an excessive offset or page size.
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.offset > 10_000 || !(1..=500).contains(&self.limit) {
            Err(ApiError::BAD_QUERY)
        } else {
            Ok(())
        }
    }
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Search {
    #[serde(default)]
    pub no_cache: bool,
    #[schemars(length(min = 1, max = 256))]
    pub query: String,
    #[serde(default = "first")]
    #[schemars(range(min = 1, max = 100))]
    pub page: u32,
}
fn first() -> u32 {
    1
}
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Movie,
    Tv,
}
impl MediaType {
    /// Return the native media discriminator.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Movie => "movie",
            Self::Tv => "tv",
        }
    }
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Details {
    #[serde(default)]
    pub no_cache: bool,
    pub media_type: MediaType,
    #[schemars(range(min = 1, max = 4_294_967_295_u64))]
    pub tmdb_id: u64,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Movie {
    #[serde(default)]
    #[schemars(length(min = 1, max = 256))]
    pub profile: Option<String>,
    #[schemars(range(min = 1, max = 4_294_967_295_u64))]
    pub tmdb_id: u64,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Series {
    #[serde(default)]
    #[schemars(length(min = 1, max = 256))]
    pub profile: Option<String>,
    #[schemars(range(min = 1, max = 4_294_967_295_u64))]
    pub tmdb_id: u64,
    #[schemars(length(min = 1, max = 50), inner(range(min = 1, max = 1000)))]
    pub seasons: Vec<u32>,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    #[serde(default)]
    pub no_cache: bool,
    #[schemars(range(min = 1, max = 4_294_967_295_u64))]
    pub request_id: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cancel {
    #[schemars(range(min = 1, max = 4_294_967_295_u64))]
    pub request_id: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueId {
    #[schemars(range(min = 1, max = 4_294_967_295_u64))]
    pub issue_id: u64,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaRequest {
    #[serde(default)]
    #[schemars(length(min = 1, max = 256))]
    pub profile: Option<String>,
    pub media_type: MediaType,
    #[schemars(range(min = 1, max = 4_294_967_295_u64))]
    pub tmdb_id: u64,
    /// TV requires distinct positive seasons; movies omit this field or use an empty list.
    #[serde(default)]
    #[schemars(length(max = 50), inner(range(min = 1, max = 1000)))]
    pub seasons: Vec<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchRequests {
    #[schemars(length(min = 1, max = 100))]
    pub items: Vec<MediaRequest>,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchDetails {
    #[schemars(length(min = 1, max = 100))]
    pub items: Vec<Details>,
    #[serde(default)]
    pub no_cache: bool,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Dates {
    pub start: String,
    pub end: String,
    #[serde(default)]
    pub no_cache: bool,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum IssueType {
    Video,
    Audio,
    Subtitles,
    Other,
}
impl IssueType {
    /// Return the native Seerr issue category ID.
    #[must_use]
    pub const fn number(&self) -> u32 {
        match self {
            Self::Video => 1,
            Self::Audio => 2,
            Self::Subtitles => 3,
            Self::Other => 4,
        }
    }
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Issue {
    pub media_type: MediaType,
    #[schemars(range(min = 1, max = 4_294_967_295_u64))]
    pub tmdb_id: u64,
    pub category: IssueType,
    #[schemars(length(min = 1, max = 2000))]
    pub message: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}
/// Decode a strict typed command input without exposing rejected values.
///
/// # Errors
/// Returns invalid arguments for fields or types rejected by the input schema.
pub fn decode<T>(value: impl Into<JsonInput>) -> Result<T, ApiError>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(value.into().0).map_err(|error| {
        tracing::debug!(
            event = "command_failure",
            category = ?error.classify()
        );
        ApiError::BAD_QUERY
    })
}

/// Validate a native identifier accepted by Seerr.
///
/// # Errors
/// Returns invalid arguments for zero or an identifier wider than uint32.
pub fn positive(id: u64) -> Result<(), ApiError> {
    if id == 0 || id > u64::from(u32::MAX) {
        Err(ApiError::BAD_QUERY)
    } else {
        Ok(())
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profiles {
    pub media_type: MediaType,
    #[serde(default)]
    pub no_cache: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Fresh {
    #[serde(default)]
    pub no_cache: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchIssues {
    #[schemars(
        length(min = 1, max = 100),
        inner(range(min = 1, max = 4_294_967_295_u64))
    )]
    pub issue_ids: Vec<u64>,
}

/// Owned command JSON, redacted when used in diagnostics.
pub struct JsonInput(Value);

impl From<Value> for JsonInput {
    fn from(value: Value) -> Self {
        Self(value)
    }
}

impl std::fmt::Debug for JsonInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JsonInput { [REDACTED] }")
    }
}

/// Validate explicit seasons before issuing any request.
///
/// # Errors
/// Rejects episodes, duplicate/out-of-range seasons, or seasons on a movie.
pub fn validate_seasons(kind: MediaType, seasons: &[u32]) -> Result<(), ApiError> {
    if kind == MediaType::Movie && !seasons.is_empty()
        || kind == MediaType::Tv
            && (seasons.is_empty()
                || seasons.len() > 50
                || seasons.iter().any(|season| !(1..=1000).contains(season))
                || seasons
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != seasons.len())
    {
        return Err(ApiError::BAD_QUERY);
    }

    Ok(())
}

/// An exact wire date validated before entering calendar or native queries.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CalendarDate(chrono::NaiveDate);

impl CalendarDate {
    /// Signed difference between two validated dates, in whole days.
    #[must_use]
    pub fn days_since(self, earlier: Self) -> i64 {
        self.0.signed_duration_since(earlier.0).num_days()
    }
}

impl From<CalendarDate> for chrono::NaiveDate {
    fn from(date: CalendarDate) -> Self {
        date.0
    }
}

/// Parse the exact calendar wire date shared by queries, providers and feeds.
///
/// # Errors
/// Rejects malformed or impossible YYYY-MM-DD dates.
pub fn date(value: &str) -> Result<CalendarDate, ApiError> {
    if value.len() != 10
        || !value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 4 | 7) {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return Err(ApiError::BAD_QUERY);
    }

    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(CalendarDate)
        .map_err(|error| {
            tracing::debug!(event="invalid_calendar_date",category=?error.kind());
            ApiError::BAD_QUERY
        })
}
