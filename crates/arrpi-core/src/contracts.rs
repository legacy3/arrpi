//! Public response documents shared by HTTP serialization and frontend generation.

use crate::config::{CachePolicy, Config};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Revisioned settings; credential serialization always emits null.
#[derive(Debug, JsonSchema, Serialize)]
pub struct SettingsEnvelope {
    pub revision: u64,
    pub settings: Config,
}

/// Receipt for an applied revision; listener changes require a process restart.
#[derive(Debug, JsonSchema, Serialize)]
pub struct ConfigUpdateResult {
    pub revision: u64,
    pub restart_required: bool,
}

/// Operator-only connection diagnostics without native identities or error details.
#[derive(Debug, JsonSchema, Serialize)]
pub struct ConnectionHealth {
    pub source: String,
    pub status: ConnectionState,
    pub health_issue_count: Option<usize>,
}

/// Current probe outcome; configuration alone never establishes reachability.
#[derive(Clone, Copy, Debug, JsonSchema, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    Healthy,
    Attention,
    AuthenticationFailed,
    Unavailable,
    Unconfigured,
}

/// Aggregate source counters without native identities or query labels.
#[derive(Debug, Default, JsonSchema, Serialize)]
pub struct Counters {
    pub hits: u64,
    pub misses: u64,
    pub stale: u64,
    pub upstream_failures: u64,
}

/// Public native endpoints supported by the allowlisted gateway.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Endpoint {
    Calendar,
    Series,
    Movie,
    Queue,
    Health,
    Qualityprofile,
}

/// A visible source and its capabilities, without its connection credentials.
#[derive(Debug, JsonSchema, Serialize)]
pub struct SourceInfo {
    pub name: String,
    pub kind: String,
    pub endpoints: Vec<Endpoint>,
    pub calendar: bool,
    pub counters: Counters,
}

/// Source discovery for the authenticated principal.
#[derive(Debug, JsonSchema, Serialize)]
pub struct Status {
    pub config_manage: bool,
    pub version: &'static str,
    pub cache: CachePolicy,
    pub sources: Vec<SourceInfo>,
}

/// Cache provenance, distinct from current upstream reachability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheState {
    Hit,
    Miss,
    Stale,
}

/// Availability of one source in a combined result.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum SourceAvailability {
    Ok,
    Unavailable,
}

/// Calendar event type; cinema and home releases are separate events.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Episode,
    Cinema,
    Digital,
    Physical,
}

/// A calendar event contains only user-facing metadata and public identifiers.
#[derive(Debug, JsonSchema, Serialize)]
pub struct CalendarEvent {
    pub source: String,
    pub id: u64,
    pub title: String,
    pub kind: EventKind,
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmdb_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imdb_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tvdb_id: Option<u64>,
    pub has_file: bool,
}

/// Per-source calendar result and observation freshness.
#[derive(Debug, JsonSchema, Serialize)]
pub struct SourceStatus {
    pub source: String,
    pub status: SourceAvailability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache: Option<CacheState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<u64>,
}

/// Monitored releases combined across authorized calendar capabilities.
#[derive(Debug, JsonSchema, Serialize)]
pub struct Calendar {
    pub events: Vec<CalendarEvent>,
    pub sources: Vec<SourceStatus>,
}

/// Fresh per-account visibility of one registered command.
#[derive(Debug, JsonSchema, Serialize)]
pub struct ToolVisibility {
    pub name: String,
    pub description: String,
    pub available: bool,
}

/// MCP prerequisites and current bound-account tool permissions.
#[derive(Debug, JsonSchema, Serialize)]
pub struct McpStatus {
    pub configured: bool,
    pub connected: bool,
    pub writes_supported: bool,
    pub connection_error: Option<String>,
    pub tools: Vec<ToolVisibility>,
}

/// One aggregate counter row, without arguments or credentials.
#[derive(Debug, JsonSchema, Serialize)]
pub struct ToolStat {
    pub tool: String,
    pub outcome: String,
    pub calls: u64,
    pub duration_ms: u64,
    pub last_duration_ms: u64,
    pub last_used: String,
}

/// Account-scoped persistent command counters.
#[derive(Debug, JsonSchema, Serialize)]
pub struct ToolStatsPage {
    pub client: String,
    pub items: Vec<ToolStat>,
}

/// Whitelisted coordinates of a write attempt; free text is never recorded.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema, Serialize)]
pub struct AuditTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tmdb_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<crate::arguments::MediaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seasons: Option<Vec<u32>>,
}

/// A recorded attempt is observability, not an exactly-once mutation journal.
#[derive(Debug, JsonSchema, Serialize)]
pub struct AuditEntry {
    pub id: u64,
    pub actor_id: Option<u64>,
    pub tool: String,
    pub target: AuditTarget,
    pub outcome: String,
    pub created_at: String,
}

/// Complete pagination through one account's retained write history.
#[derive(Debug, JsonSchema, Serialize)]
pub struct AuditPage {
    pub client: String,
    pub items: Vec<AuditEntry>,
    pub total: u64,
    pub offset: usize,
    pub next_offset: Option<usize>,
}

impl Endpoint {
    /// Return the stable lowercase wire identity.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Calendar => "calendar",
            Self::Series => "series",
            Self::Movie => "movie",
            Self::Queue => "queue",
            Self::Health => "health",
            Self::Qualityprofile => "qualityprofile",
        }
    }
}

impl SourceAvailability {
    /// Return the stable lowercase wire identity.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Unavailable => "unavailable",
        }
    }
}

impl EventKind {
    /// Return the stable lowercase wire identity.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Episode => "episode",
            Self::Cinema => "cinema",
            Self::Digital => "digital",
            Self::Physical => "physical",
        }
    }
}
