//! Typed, source-scoped library maintenance; adding media remains a broker operation.

use crate::error::ApiError;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;
use std::fmt::Debug;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MediaTarget {
    /// Exact source name from `search_library`, `list_movies` or `list_series`.
    pub source: String,
    /// Native library ID, not a TMDB ID.
    #[schemars(range(min = 1, max = 2_147_483_647))]
    pub item_id: u32,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReleaseSearch {
    pub source: String,
    #[schemars(range(min = 1, max = 2_147_483_647))]
    pub item_id: u32,
    /// TV only. Omit to search the whole series. Cannot combine with `episode_ids`.
    #[schemars(range(min = 0, max = 2_147_483_647))]
    pub season: Option<u32>,
    /// Exact native episode IDs belonging to `item_id`. Cannot combine with season.
    #[serde(default)]
    #[schemars(length(max = 100), inner(range(min = 1, max = 2_147_483_647)))]
    pub episode_ids: Vec<u32>,
    /// Default: search only monitored, aired items without files. False allows profile upgrades.
    #[serde(default = "yes")]
    pub missing_only: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Episodes {
    pub source: String,
    #[schemars(range(min = 1, max = 2_147_483_647))]
    pub item_id: u32,
    #[schemars(range(min = 0, max = 2_147_483_647))]
    pub season: Option<u32>,
    #[serde(default)]
    pub missing_only: bool,
    #[serde(default)]
    pub aired_only: bool,
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "page_size")]
    #[schemars(range(min = 1, max = 500))]
    pub limit: usize,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandStatus {
    pub source: String,
    #[schemars(range(min = 1, max = 2_147_483_647))]
    pub command_id: u32,
}

const fn yes() -> bool {
    true
}

const fn page_size() -> usize {
    20
}

/// Only metadata refresh, profile-governed automatic searches and their safe reads.
#[async_trait]
pub trait MaintenanceSource: Debug + Send + Sync {
    async fn refresh(&self, item_id: u32) -> Result<Value, ApiError>;

    async fn search(&self, args: ReleaseSearch) -> Result<Value, ApiError>;

    async fn episodes(&self, args: Episodes) -> Result<Value, ApiError>;

    async fn command(&self, command_id: u32) -> Result<Value, ApiError>;
}

/// Native command IDs use positive signed 32-bit integers.
pub(crate) fn validate_id(id: u32) -> Result<(), ApiError> {
    if id == 0 || i32::try_from(id).is_err() {
        return Err(ApiError::BAD_QUERY);
    }

    Ok(())
}

impl ReleaseSearch {
    /// Validate coordinates before any upstream read or mutation.
    ///
    /// # Errors
    /// Rejects ambiguous scopes, duplicate IDs and configured batch overflow.
    pub fn validate(&self, maximum: usize) -> Result<(), ApiError> {
        validate_id(self.item_id)?;

        if self
            .season
            .is_some_and(|season| i32::try_from(season).is_err())
            || (self.season.is_some() && !self.episode_ids.is_empty())
            || self.episode_ids.len() > maximum.min(100)
        {
            return Err(ApiError::BAD_QUERY);
        }

        let mut seen = std::collections::BTreeSet::new();

        for id in &self.episode_ids {
            validate_id(*id)?;

            if !seen.insert(id) {
                return Err(ApiError::BAD_QUERY);
            }
        }

        Ok(())
    }
}
