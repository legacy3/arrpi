//! Consumed native wire fields; unknown fields are ignored for forward compatibility.
use arrpi_core::{
    error::ApiError,
    summaries::{CleanupCounts, SearchCounts},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct SabResponse {
    pub status: Option<bool>,
    pub queue: Option<SabQueue>,
}

/// `SABnzbd` 4/5 queue API: numeric speed/remaining values can be strings or numbers.
#[derive(Deserialize)]
pub(super) struct SabQueue {
    pub version: String,
    pub noofslots: u64,
    pub noofslots_total: u64,
    pub paused: bool,
    pub kbpersec: Number,
    pub mbleft: Number,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Number {
    Numeric(f64),
    Text(String),
}

impl Number {
    pub(super) fn value(self) -> Result<f64, ApiError> {
        let value = match self {
            Self::Numeric(value) => value,

            Self::Text(value) => value.parse().map_err(|error| {
                tracing::debug!(
                    event = "invalid_sab_counter",
                    category = std::any::type_name_of_val(&error)
                );
                ApiError::INVALID_RESPONSE
            })?,
        };

        if !value.is_finite() || value < 0.0 {
            return Err(ApiError::INVALID_RESPONSE);
        }

        Ok(value)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CleanupResponse {
    pub generated_at: String,
    pub timeframe_hours: u64,
    pub removals: Removals,
    pub searches: SearchCounts,
}

#[derive(Deserialize)]
pub(super) struct Removals {
    pub total: u64,
}

#[derive(Deserialize)]
pub(super) struct Badges {
    pub episodes: u64,
    pub movies: u64,
    pub providers: u64,
    pub status: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StorageMetrics {
    pub generated_at: String,
    pub cleanup_totals: CleanupCounts,
    pub collection_summary: Collections,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Collections {
    pub reclaimable_count: u64,
    pub reclaimable_sized_count: u64,
    pub active_size_bytes: u64,
    pub reclaimable_using_fallback: bool,
}
