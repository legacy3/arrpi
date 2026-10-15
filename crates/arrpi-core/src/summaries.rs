//! Minimal authorized counters, separate from full native library and queue access.
use crate::{
    error::ApiError,
    source::{ReadMode, SourceRead},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Each summary requires its own source grant.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SummaryKind {
    Queue,
    Library,
    Status,
}

impl SummaryKind {
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
    ) -> Result<SourceRead<Value>, ApiError>;
}
