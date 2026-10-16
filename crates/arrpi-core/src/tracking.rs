//! Account-bound playback tracking, independent of request brokers and transports.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{config::ConfigError, error::ApiError};

/// Native profile and verified Plex cloud identity selected by the operator.
#[derive(Clone, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub source: String,
    pub profile_id: String,
    pub plex_id: u64,
    #[serde(default)]
    pub plex_server_owner: bool,
}

impl Binding {
    /// Reject identifiers that could change native route/query structure.
    ///
    /// # Errors
    /// Returns a static validation failure without retaining rejected values.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.plex_id == 0 || !safe_id(&self.profile_id) {
            return Err(ConfigError::invalid("invalid tracking account binding"));
        }

        Ok(())
    }
}

/// Linked credentials are distinct from a running playback watcher.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, Serialize)]
pub struct Connection {
    pub authorizing: bool,
    pub linked: bool,
    pub needs_reconnect: bool,
    pub username: Option<String>,
}

/// Only explicit, native-confirmed delivery events are exposed to the account.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct Activity {
    pub title: String,
    pub destination: String,
    pub watched_at: i64,
    pub media_type: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
}

/// No native credentials, instance selectors or global activity leave the adapter.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[schemars(rename = "TrackingStatus")]
pub struct Status {
    pub plex: Connection,
    pub trakt: Connection,
    pub watcher: Watcher,
    pub activity_available: bool,
    pub plex_authorization_allowed: bool,
    pub watcher_ready: bool,
    pub recent: Vec<Activity>,
    pub destinations: Vec<DestinationStatus>,
}

/// One independently controlled playback destination in the native profile.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct DestinationStatus {
    pub id: String,
    pub label: String,
    pub connection: Connection,
    pub watcher: Watcher,
}

/// Destination operations always resolve instances from the bound profile.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DestinationAction {
    Enable,
    Pause,
    Disconnect,
}

/// Native device polling distinguishes approval from waiting and terminal expiry.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[schemars(rename = "TrackingPoll")]
pub struct Poll {
    pub state: PollState,
}

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PollState {
    Pending,
    Authorized,
    Expired,
    Denied,
}

/// Native PIN authorization is polled and refreshed by the upstream application.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[schemars(rename = "TrackingAuthorization")]
pub struct Authorization {
    pub code: String,
    pub verification_url: String,
    pub expires_in: u64,
}

/// Providers that can be linked by the current account.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Plex,
    Trakt,
    Simkl,
    Mdblist,
    Punchplay,
    Bingebase,
    Flicklist,
}

/// Sources own native identity checks and mutations; callers never supply profiles.
#[async_trait]
pub trait TrackingSource: std::fmt::Debug + Send + Sync {
    async fn status(&self, binding: &Binding) -> Result<Status, ApiError>;

    async fn authorize(
        &self,
        binding: &Binding,
        provider: Provider,
    ) -> Result<Authorization, ApiError>;

    async fn set_enabled(&self, binding: &Binding, enabled: bool) -> Result<Status, ApiError>;

    async fn disconnect(&self, binding: &Binding) -> Result<Status, ApiError>;

    async fn poll(&self, binding: &Binding, provider: &str) -> Result<Poll, ApiError>;

    async fn destination(
        &self,
        binding: &Binding,
        provider: &str,
        action: DestinationAction,
    ) -> Result<Status, ApiError>;
}

/// Native identifiers are opaque and never accepted as paths from end users.
#[must_use]
pub fn safe_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// A configured route may be paused or awaiting a native watcher restart.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub struct Watcher {
    pub enabled: bool,
    pub running: bool,
    pub managed_elsewhere: bool,
}
