//! Owned summary documents used by adapters, HTTP serialization and TypeScript generation.
use crate::source::{Observation, PublicFreshness};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Independent source outcomes and explicit aggregate completeness.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct Summary {
    pub complete: bool,
    pub sources: Vec<SummaryRow>,
}

/// A failed source never discards another source's healthy counters.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SummaryRow {
    Ok {
        source: String,
        kind: String,
        data: Box<SummaryData>,
        freshness: PublicFreshness,
    },
    Unavailable {
        source: String,
        kind: String,
        code: String,
    },
}

impl SummaryRow {
    /// Stale or truncated observations cannot establish a complete snapshot.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Ok { data, freshness, .. } if data.is_complete() && freshness.cache != Observation::Stale)
    }
}

/// Normalized payloads have distinct required fields without changing their wire shape.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[serde(untagged)]
pub enum SummaryData {
    Library(LibrarySummary),
    Queue(QueueSummary),
    Sabnzbd(SabnzbdSummary),
    Cleanuparr(CleanuparrSummary),
    Bazarr(BazarrSummary),
    Maintainerr(MaintainerrSummary),
}

impl SummaryData {
    /// Preserve provider completeness independently of native source reachability.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        match self {
            Self::Library(data) => data.complete,
            Self::Queue(data) => data.complete,
            Self::Sabnzbd(data) => data.complete,
            Self::Cleanuparr(data) => data.complete,
            Self::Bazarr(data) => data.complete,
            Self::Maintainerr(data) => data.complete,
        }
    }
}

/// Null size means at least one item has no measurement; known bytes remain useful.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct LibrarySummary {
    pub complete: bool,
    pub items: u64,
    pub items_with_files: u64,
    pub monitored_items: u64,
    pub size_bytes: Option<u64>,
    pub known_size_bytes: u64,
    pub items_without_size: u64,
}

/// Byte counters deduplicate download identities while queue rows remain separate.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct QueueSummary {
    pub complete: bool,
    pub consistency: QueueConsistency,
    pub reason: Option<String>,
    pub items_observed: usize,
    pub total_reported: Option<usize>,
    pub downloads_observed: usize,
    pub rows_without_download_identity: usize,
    pub byte_counts_complete: bool,
    pub byte_counts_approximate: bool,
    pub size_bytes_observed: f64,
    pub remaining_bytes_observed: f64,
    pub statuses: BTreeMap<String, usize>,
    pub page_limit: usize,
    pub pages: Vec<PublicFreshness>,
}

/// Queue pagination is bounded but cannot provide an atomic upstream snapshot.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueConsistency {
    NonAtomicPages,
}

/// Active SAB counters exclude paused/checking jobs according to its native API.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct SabnzbdSummary {
    pub complete: bool,
    pub version: String,
    pub queued_jobs: u64,
    pub active_jobs: u64,
    pub paused: bool,
    pub speed_kib_per_second: f64,
    pub active_remaining_mebibytes: f64,
}

/// Seven-day counts exclude dry runs; successful replacement counts are unavailable.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct CleanuparrSummary {
    pub complete: bool,
    pub window_hours: u64,
    pub includes_dry_run: bool,
    pub removed_downloads: u64,
    pub searches: SearchCounts,
    pub replacement_count: Option<u64>,
    pub generated_at: String,
}

/// Search/grab counts do not imply a successful replacement download.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct SearchCounts {
    pub total: u64,
    pub completed: u64,
    pub failed: u64,
    pub grabbed: u64,
}

/// Missing language requirements are not distinct media-item counts.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct BazarrSummary {
    pub complete: bool,
    pub missing_subtitle_requirements: SubtitleRequirements,
    pub throttled_providers: u64,
    pub health_issues: u64,
}

/// An item can require more than one subtitle language.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct SubtitleRequirements {
    pub episodes: u64,
    pub movies: u64,
}

/// Collection estimates never represent physical filesystem capacity.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct MaintainerrSummary {
    pub complete: bool,
    pub generated_at: String,
    pub cleanup_scope: CleanupScope,
    pub cleanup_bytes_scope: CleanupBytesScope,
    pub cleanup: CleanupCounts,
    pub collections: CollectionCounts,
    pub reclaimable_uses_fallback: bool,
}

/// Counts accumulate across successful handled actions.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupScope {
    Cumulative,
}

/// Byte totals only include successful delete actions with a recorded size.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupBytesScope {
    SuccessfulDeleteActionsWithKnownSizes,
}

/// Native camelCase names are retained for the existing normalized contract.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupCounts {
    pub items_handled: u64,
    pub bytes_handled: u64,
    pub movies_handled: u64,
    pub shows_handled: u64,
    pub seasons_handled: u64,
    pub episodes_handled: u64,
}

/// Unknown sizes and overlapping fallback estimates remain explicit.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct CollectionCounts {
    #[serde(rename = "reclaimableCount")]
    pub reclaimable_count: u64,
    #[serde(rename = "reclaimableSizedCount")]
    pub reclaimable_sized_count: u64,
    #[serde(rename = "activeSizeBytes")]
    pub active_size_bytes: u64,
    pub unknown_size_collections: u64,
    pub reclaimable_size_complete: bool,
}
