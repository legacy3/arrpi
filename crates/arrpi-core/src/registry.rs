//! One typed declaration of public commands, schemas and operation metadata.
use crate::{arguments as args, policy::Limits};
use schemars::{JsonSchema, schema_for};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{borrow::Cow, collections::BTreeMap, sync::Arc};

macro_rules! registry {
    ($id:ident { $($variant:ident => ($name:literal, $args:ty, $title:literal, $description:literal, $write:literal, $fresh:literal, $native:literal, $page:literal)),+ $(,)? }) => {
        /// Stable command identity shared by all protocol adapters.
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
        pub enum $id { $($variant),+ }

        impl $id {
            /// Public commands in stable name order.
            pub const ALL: &[Self] = &[$(Self::$variant),+];

            /// Resolve a wire name without constructing a schema or transport.
            #[must_use]
            pub fn parse(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$variant),)+ _ => None }
            }

            /// The stable wire name.
            #[must_use]
            pub const fn name(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }

            /// Whether executing the command may mutate upstream application state.
            #[must_use]
            pub const fn is_write(self) -> bool {
                match self { $(Self::$variant => $write),+ }
            }

            /// Whether the input accepts an explicit fresh-read request.
            #[must_use]
            pub const fn accepts_fresh(self) -> bool {
                match self { $(Self::$variant => $fresh),+ }
            }

            /// Whether the command can run without a verified Seerr actor.
            #[must_use]
            pub const fn is_native(self) -> bool {
                match self { $(Self::$variant => $native),+ }
            }

            /// Whether omitted page limits need a configured default.
            #[must_use]
            pub const fn is_paginated(self) -> bool {
                match self { $(Self::$variant => $page),+ }
            }

            /// Human-facing operation label, shared by HTTP, MCP and the UI.
            #[must_use]
            pub const fn title(self) -> &'static str {
                match self { $(Self::$variant => $title),+ }
            }

            fn description(self) -> &'static str {
                match self { $(Self::$variant => $description),+ }
            }

            fn schema(self) -> Map<String, Value> {
                match self { $(Self::$variant => schema::<$args>()),+ }
            }
        }
    };
}

registry!(ToolId {
    CancelRequest => ("cancel_request", args::Cancel, "Cancel request", "Cancel only this account's pending request. Does not delete library media.", true, false, false, false),
    CloseIssue => ("close_issue", args::IssueId, "Close issue", "Resolve an issue created by this account. Cannot close another user's issue.", true, false, false, false),
    CloseIssuesBatch => ("close_issues_batch", args::BatchIssues, "Close issues in batch", "Resolve multiple issues created by this account. Returns ordered outcomes and respects close_issue permissions/switches, batch size and write budget. Never closes another user's issue.", true, false, false, false),
    LibrarySummary => ("library_summary", args::Fresh, "Library totals", "Read minimal native library counts and on-disk bytes without titles. Missing byte measurements stay unknown.", false, true, true, false),
    ListIssues => ("list_issues", args::Page, "Browse issues", "List issues reported by this account.", false, true, false, true),
    ListMovies => ("list_movies", crate::library::LibraryFilter, "Browse movies", "List installed movies. Search/filter this library, not the request catalog.", false, true, true, true),
    ListSeries => ("list_series", crate::library::LibraryFilter, "Browse TV shows", "List installed TV series. Availability describes files already in the library.", false, true, true, true),
    MediaCommandStatus => ("media_command_status", crate::maintenance::CommandStatus, "Check command progress", "Read fresh progress of a metadata refresh or automatic release search using its source and command_id. completed means the Arr command finished, not that media downloaded or imported. After refresh_media completes, inspect series_episodes or search_releases. Do not poll continuously.", false, false, true, false),
    MediaDetails => ("media_details", args::Details, "Media details", "Read movie/TV metadata, genres, runtime, TMDB scores/votes and available IMDb/Rotten Tomatoes critic/audience ratings from Seerr, plus explicit availability and public links. Scores are normalized to 0-10 with origin/raw scale; missing is not zero. ratings_complete/source status report partial ratings failures. no_cache requests a fresh read; availability can lag until Seerr syncs Plex.", false, true, false, false),
    MediaDetailsBatch => ("media_details_batch", args::BatchDetails, "Media details in batch", "Read availability for multiple exact TMDB IDs with ordered per-item outcomes. Failed items do not hide later reads; rate limits, resource pressure or the total batch deadline stop the remainder. batch_timeout is safe to retry. no_cache refreshes Seerr reads, not its Plex sync.", false, true, false, false),
    MonitoringStatus => ("monitoring_status", args::Fresh, "App activity", "Read authorized SAB download, Cleanuparr cleanup/search, Bazarr subtitle requirement or Maintainerr cleanup counters. Each source reports its own freshness and metric semantics.", false, true, true, false),
    MyQuota => ("my_quota", args::Fresh, "Request allowance", "Read this account's native Seerr limits, usage, remaining allowance and rolling window. Movie units are movies; TV units are seasons. Always fresh.", false, true, false, false),
    MyRequests => ("my_requests", args::Page, "My requests", "List only this account's Seerr requests.", false, true, false, true),
    QueueSummary => ("queue_summary", args::Fresh, "Download queue", "Read bounded queue counters for explicitly authorized sources. complete=false means only part of a changing or excessive queue was observed. No job titles or paths.", false, true, true, false),
    RefreshMedia => ("refresh_media", crate::maintenance::MediaTarget, "Refresh metadata", "Refresh metadata and scan one existing movie or TV series in its native Arr source. Use source and native id from search_library, not a TMDB ID. Updates TV episode metadata but does not search indexers. Returns an asynchronous command_id; wait for completion with media_command_status before checking for new/missing episodes or calling search_releases. Does not add media or change monitoring/profiles/tags. Never automatically retry unknown_outcome.", true, false, true, false),
    ReportIssue => ("report_issue", args::Issue, "Report an issue", "Report a video/audio/subtitle/other issue for media tracked by Seerr as this account.", true, false, false, false),
    RequestMediaBatch => ("request_media_batch", args::BatchRequests, "Request media in batch", "Request multiple movies/TV seasons through Seerr as this account. Returns ordered per-item outcomes; never retry unknown_outcome automatically. Never requests 4K.", true, false, false, false),
    RequestMovie => ("request_movie", args::Movie, "Request a movie", "Request an exact movie TMDB ID through Seerr as this account. Omit profile/null keeps Seerr settings; otherwise use an exact request_profiles name. Checks duplicates fresh and never requests 4K. No automatic retries after unknown_outcome; inspect media details and my_requests first.", true, false, false, false),
    RequestProfiles => ("request_profiles", args::Profiles, "Request profiles", "List Seerr's current non-4K request profiles and configured ordinary/anime defaults where reported. Omit profile or use null for Seerr's configured default; any string is an exact profile name and requires the human advanced chooser permission.", false, true, false, false),
    RequestSeries => ("request_series", args::Series, "Request TV seasons", "Request explicit positive season numbers through Seerr as this account. Optional profile is an exact request_profiles name; omit/null keeps Seerr settings. Checks duplicates fresh and reports accepted/skipped seasons; never requests specials or 4K. Reconcile unknown_outcome before resubmitting.", true, false, false, false),
    RequestStatus => ("request_status", args::Request, "Check request progress", "Read the status of a request owned by this account. Approval is not download completion.", false, true, false, false),
    SearchLibrary => ("search_library", crate::library::LibraryFilter, "Search the library", "Search the installed library using structured genre, year, rating and availability filters. Joins authorized Plex community ratings before filtering/sorting. rating_provider selects IMDb/TMDB/Trakt/Sonarr/critics/audience, min_votes rejects unknown vote counts, max_runtime_minutes limits runtime. Returns all native ratings with provider, origin and raw scale; use media_details_batch for Seerr IMDb/Rotten Tomatoes/TMDB ratings and links. Returns plots, genres, ratings, runtime and file availability; use next_offset to paginate and tmdb_id for requests (id is native Arr). Check complete/source status before claiming absence. Not the request catalog. no_cache refreshes native and authorized Plex snapshots.", false, true, true, true),
    SearchMedia => ("search_media", args::Search, "Search for media", "Search Seerr's requestable movie/TV catalog with spaces or punctuation. Returns candidates with tmdb_id, title, year, media_type and availability; choose the exact match before requesting.", false, true, false, false),
    SearchReleases => ("search_releases", crate::maintenance::ReleaseSearch, "Search for releases", "Trigger an automatic indexer search for one existing movie/series, a TV season or explicit native episode_ids. Resolve source and native item_id with search_library. Defaults to monitored, aired missing content; missing_only=false also allows profile-governed upgrades. Movie profiles remain unchanged; profiles allowing 4K are refused. No monitoring changes or manual release grabs. Returns a command receipt, not download completion. Never automatically retry unknown_outcome; inspect media_command_status and the queue.", true, false, true, false),
    SeriesEpisodes => ("series_episodes", crate::maintenance::Episodes, "Browse series episodes", "List fresh structured episodes from one installed Sonarr series using source and native item_id. Supports season, missing_only, aired_only and offset/limit pagination; returns episode IDs, dates, monitoring and file availability. Aired missing episodes are candidates for search_releases. Refresh metadata first when checking for newly announced episodes; this read itself does not scan or download.", false, false, true, true),
    ServiceStatus => ("service_status", args::Fresh, "Service health", "Check authorized Seerr/Arr reachability and health cache age. no_cache probes upstreams fresh; source failures are independent.", false, true, true, false),
    UpcomingReleases => ("upcoming_releases", args::Dates, "Upcoming releases", "Read the monitored release calendar for an explicit date window, at most 93 days.", false, true, true, false)
});

/// Protocol-independent operation descriptor with a shared immutable input schema.
#[derive(Clone, Debug)]
pub struct ToolDefinition {
    pub name: Cow<'static, str>,
    pub title: Cow<'static, str>,
    pub description: Cow<'static, str>,
    input_schema: InputSchema,
    pub effects: Effects,
}

/// Shared schema storage, independent of a protocol adapter's allocation type.
#[derive(Clone, Debug, JsonSchema, Serialize)]
#[serde(transparent)]
pub struct InputSchema(Arc<Map<String, Value>>);

/// HTTP command discovery shares names, schemas and effects with protocol adapters.
#[derive(Debug, JsonSchema, Serialize)]
pub struct CommandCatalog {
    pub tools: Vec<CommandDescriptor>,
    pub account: Option<String>,
}

/// A command description retains the shared immutable schema allocation.
#[derive(Debug, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandDescriptor {
    pub name: Cow<'static, str>,
    pub title: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Sources permitted for commands that accept an explicit source argument.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    pub description: Cow<'static, str>,
    pub input_schema: InputSchema,
    pub annotations: BTreeMap<CommandAnnotation, bool>,
}

/// Operation effects are hints for clients; dispatch always rechecks authorization.
#[derive(Debug, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandAnnotation {
    ReadOnlyHint,
    DestructiveHint,
    IdempotentHint,
    OpenWorldHint,
}

impl From<ToolDefinition> for CommandDescriptor {
    fn from(definition: ToolDefinition) -> Self {
        let read = definition.effects.is_read_only();

        Self {
            name: definition.name,
            title: definition.title,
            source: None,
            sources: Vec::new(),
            description: definition.description,
            input_schema: definition.input_schema,
            annotations: [
                (CommandAnnotation::ReadOnlyHint, read),
                (
                    CommandAnnotation::DestructiveHint,
                    definition.effects.is_destructive(),
                ),
                (CommandAnnotation::IdempotentHint, read),
                (CommandAnnotation::OpenWorldHint, true),
            ]
            .into(),
        }
    }
}

impl AsRef<Map<String, Value>> for InputSchema {
    fn as_ref(&self) -> &Map<String, Value> {
        &self.0
    }
}

impl From<Map<String, Value>> for InputSchema {
    fn from(schema: Map<String, Value>) -> Self {
        Self(Arc::new(schema))
    }
}

impl From<InputSchema> for Arc<Map<String, Value>> {
    fn from(schema: InputSchema) -> Self {
        schema.0
    }
}

#[derive(Clone, Copy, Debug)]
enum Intent {
    Read,
    Write,
}

/// Native operation effects; authorization is still checked at dispatch.
#[derive(Clone, Copy, Debug)]
pub struct Effects {
    intent: Intent,
    destructive: bool,
}

impl Effects {
    const fn for_tool(id: ToolId) -> Self {
        Self {
            intent: if id.is_write() {
                Intent::Write
            } else {
                Intent::Read
            },
            destructive: matches!(id, ToolId::CancelRequest),
        }
    }

    /// Whether this command leaves native application state unchanged.
    #[must_use]
    pub const fn is_read_only(self) -> bool {
        matches!(self.intent, Intent::Read)
    }

    /// Whether this command removes an existing native request.
    #[must_use]
    pub const fn is_destructive(self) -> bool {
        self.destructive
    }
}

/// Configured schemas are built once for each immutable runtime revision.
#[derive(Debug)]
pub struct Registry {
    definitions: Vec<ToolDefinition>,
}

impl ToolDefinition {
    /// Retain remote schemas while deriving operation effects from the known command ID.
    #[must_use]
    pub fn remote(id: ToolId, description: Option<String>, input_schema: InputSchema) -> Self {
        Self {
            name: Cow::Borrowed(id.name()),
            title: Cow::Borrowed(id.title()),
            description: description.map_or_else(|| Cow::Borrowed(id.description()), Cow::Owned),
            input_schema,
            effects: Effects::for_tool(id),
        }
    }

    /// Read the configured JSON schema without cloning its backing allocation.
    #[must_use]
    pub fn input_schema(&self) -> &InputSchema {
        &self.input_schema
    }
}

impl Registry {
    /// Build all descriptors with the current batch and page limits.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            definitions: ToolId::ALL
                .iter()
                .map(|id| {
                    let mut input = id.schema();

                    configure(&mut input, limits);
                    ToolDefinition {
                        name: Cow::Borrowed(id.name()),
                        title: Cow::Borrowed(id.title()),
                        description: Cow::Borrowed(id.description()),
                        input_schema: input.into(),
                        effects: Effects::for_tool(*id),
                    }
                })
                .collect(),
        }
    }

    /// Find an immutable descriptor without rebuilding its schema.
    #[must_use]
    pub fn definition(&self, id: ToolId) -> &ToolDefinition {
        &self.definitions[id as usize]
    }
}

fn schema<T>() -> Map<String, Value>
where
    T: JsonSchema,
{
    schema_for!(T).as_object().cloned().unwrap_or_default()
}

fn configure(schema: &mut Map<String, Value>, limits: Limits) {
    if let Some(properties) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        for key in ["items", "issue_ids", "episode_ids"] {
            if let Some(items) = properties.get_mut(key) {
                items["maxItems"] = serde_json::json!(limits.max_batch_items);
            }
        }

        if let Some(limit) = properties.get_mut("limit") {
            limit["maximum"] = serde_json::json!(limits.max_page_items);
            limit["default"] = serde_json::json!(20.min(limits.max_page_items));
        }
    }
}
