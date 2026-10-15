//! Minimal authorized counters, separate from full native library and queue access.
mod contracts;
use crate::{
    error::ApiError,
    source::{ReadMode, SourceRead},
};
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[rustfmt::skip]
pub use contracts::{
    BazarrSummary, CleanupBytesScope, CleanupCounts, CleanupScope, CleanuparrSummary,
    CollectionCounts, LibrarySummary, MaintainerrSummary, QueueConsistency, QueueSummary,
    SabnzbdSummary, SearchCounts, SubtitleRequirements, Summary, SummaryData, SummaryRow,
};

/// Each summary requires its own source grant.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryKind {
    Queue,
    Library,
    Status,
}

impl SummaryKind {
    /// Stable discovery order shared by transports and capability consumers.
    pub const ALL: &[Self] = &[Self::Queue, Self::Library, Self::Status];

    /// Exact permission suffix; metadata access never implies operational access.
    #[must_use]
    pub const fn grant(self) -> &'static str {
        match self {
            Self::Queue => "queue-summary",
            Self::Library => "library-summary",
            Self::Status => "status-view",
        }
    }
}

/// Provider-owned projections contain counters, never credentials, paths or job titles.
#[async_trait]
pub trait SummarySource: std::fmt::Debug + Send + Sync {
    fn supports(&self, kind: SummaryKind) -> bool;

    async fn summary(
        &self,
        kind: SummaryKind,
        mode: ReadMode,
    ) -> Result<SourceRead<SummaryData>, ApiError>;
}
