//! A fourth provider exercises discovery and use cases without provider branches.

use arrpi_core::{
    Commands,
    arguments::{Dates, MediaRequest, MediaType, Page},
    config::Config,
    contracts::{CalendarEvent, EventKind},
    error::ApiError,
    executor::{AuditEvent, EventSink, Events},
    policy::Binding,
    registry::ToolId,
    source::{
        AccountSource, Actor, ActorRights, ApiSource, Body, BrokerProfile, BrokerProfiles,
        CalendarSource, Descriptor, Freshness, Health, LibrarySource, Observation, Principal,
        ProfileSelection, QualityProfile, RatingsSource, ReadMode, RequestSource, SourceRead,
        Sources,
    },
};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

fn descriptor(name: &str) -> Descriptor {
    Descriptor {
        name: name.to_owned(),
        kind: "fourth",
        service_binding: (name == "fourth").then_some(4),
        endpoints: std::borrow::Cow::Borrowed(&[]),
    }
}

fn freshness(name: &str) -> Freshness {
    Freshness {
        source: name.to_owned(),
        cache: Observation::NotCached,
        age_seconds: None,
        checked_at: None,
        note: None,
    }
}

fn healthy() -> Health {
    Health {
        reachable: Some(true),
        version: None,
        health_issue_count: None,
        writes_supported: None,
        needs_account: false,
        freshness: None,
    }
}

#[derive(Debug)]
struct RatingsProvider {
    descriptor: Descriptor,
    probes: Arc<AtomicUsize>,
}

#[async_trait]
impl ApiSource for RatingsProvider {
    fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    fn ratings(&self) -> Option<&dyn RatingsSource> {
        Some(self)
    }

    async fn health(&self, _: &Principal, _: ReadMode) -> Result<Health, ApiError> {
        self.probes.fetch_add(1, Ordering::SeqCst);
        Ok(healthy())
    }
}

#[async_trait]
impl RatingsSource for RatingsProvider {
    async fn snapshot(&self, _: ReadMode) -> Result<SourceRead<Body>, ApiError> {
        panic!("a health request must not read the ratings inventory")
    }
}

#[tokio::test]
async fn ratings_only_provider_exposes_health_only_with_its_status_grant() {
    let (mut commands, mut principal) = fixture(None);
    let probes = Arc::new(AtomicUsize::new(0));

    commands.sources = Sources::new();
    commands
        .sources
        .insert(Broker {
            descriptor: descriptor("catalog"),
            requests_enabled: false,
        })
        .expect("required account broker");
    commands
        .sources
        .insert(RatingsProvider {
            descriptor: descriptor("ratings"),
            probes: Arc::clone(&probes),
        })
        .expect("ratings-only provider");
    principal.permissions.clear();
    assert_eq!(
        commands
            .execute(&principal, ToolId::ServiceStatus, json!({}))
            .await,
        Err(ApiError::FORBIDDEN)
    );
    assert_eq!(probes.load(Ordering::SeqCst), 0);
    principal.permissions.insert("ratings:status-view".into());
    assert!(
        commands
            .list(&principal)
            .await
            .expect("discovery")
            .iter()
            .any(|tool| tool.name == "service_status")
    );
    let status = commands
        .execute(&principal, ToolId::ServiceStatus, json!({}))
        .await
        .expect("authorized health");

    assert_eq!(status["complete"], true);
    assert_eq!(status["sources"][0]["source"], "ratings");
    assert_eq!(probes.load(Ordering::SeqCst), 1);
}

#[derive(Debug, Default)]
struct Probe {
    library: AtomicUsize,
    calendar: AtomicUsize,
    calendar_events: AtomicUsize,
    health: AtomicUsize,
    modes: Mutex<Vec<ReadMode>>,
    pause_fresh: AtomicBool,
    snapshot_started: tokio::sync::Notify,
    snapshot_released: tokio::sync::Notify,
}

impl Probe {
    fn calls(&self) -> usize {
        self.library.load(Ordering::SeqCst)
            + self.calendar.load(Ordering::SeqCst)
            + self.health.load(Ordering::SeqCst)
    }
}

#[derive(Debug)]
struct Provider {
    descriptor: Descriptor,
    error: Option<ApiError>,
    probe: Arc<Probe>,
}
#[async_trait]
impl ApiSource for Provider {
    fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    async fn health(&self, _principal: &Principal, _mode: ReadMode) -> Result<Health, ApiError> {
        self.probe.health.fetch_add(1, Ordering::SeqCst);
        self.error.map_or_else(|| Ok(healthy()), Err)
    }

    fn library(&self) -> Option<&dyn LibrarySource> {
        Some(self)
    }

    fn calendar(&self) -> Option<&dyn CalendarSource> {
        Some(self)
    }
}
#[async_trait]
impl LibrarySource for Provider {
    fn media_type(&self) -> MediaType {
        MediaType::Movie
    }

    async fn snapshot(&self, mode: ReadMode) -> Result<SourceRead<Body>, ApiError> {
        self.probe.library.fetch_add(1, Ordering::SeqCst);
        self.probe.modes.lock().expect("recorded modes").push(mode);

        if let Some(error) = self.error {
            return Err(error);
        }

        if mode == ReadMode::Fresh && self.probe.pause_fresh.load(Ordering::SeqCst) {
            self.probe.snapshot_started.notify_one();
            self.probe.snapshot_released.notified().await;
        }

        Ok(SourceRead {data:bytes::Bytes::from_static(br#"[{"id":7,"title":"A funny film","tmdbId":42,"genres":["Comedy"],"hasFile":true}]"#).into(),freshness:freshness(&self.descriptor.name)})
    }

    async fn profiles(&self) -> Result<Vec<QualityProfile>, ApiError> {
        Ok(vec![
            QualityProfile {
                id: 1,
                name: "1080p".to_owned(),
                non_4k: true,
            },
            QualityProfile {
                id: 2,
                name: "2160p".to_owned(),
                non_4k: false,
            },
            QualityProfile {
                id: 3,
                name: "Current definition name".to_owned(),
                non_4k: true,
            },
        ])
    }
}
#[async_trait]
impl CalendarSource for Provider {
    async fn calendar(
        &self,
        start: &str,
        _end: &str,
        mode: ReadMode,
    ) -> Result<SourceRead<Vec<CalendarEvent>>, ApiError> {
        self.probe.calendar.fetch_add(1, Ordering::SeqCst);
        self.probe.modes.lock().expect("recorded modes").push(mode);

        if let Some(error) = self.error {
            return Err(error);
        }

        Ok(SourceRead {
            data: (0..self.probe.calendar_events.load(Ordering::SeqCst).max(1))
                .map(|index| CalendarEvent {
                    source: self.descriptor.name.clone(),
                    id: 7 + u64::try_from(index).expect("bounded fixture"),
                    title: "A funny film".into(),
                    kind: EventKind::Digital,
                    date: start.to_owned(),
                    series: None,
                    season: None,
                    episode: None,
                    tmdb_id: Some(42),
                    imdb_id: None,
                    tvdb_id: None,
                    has_file: true,
                })
                .collect(),
            freshness: freshness(&self.descriptor.name),
        })
    }
}

#[derive(Debug)]
struct Broker {
    descriptor: Descriptor,
    requests_enabled: bool,
}
#[async_trait]
impl ApiSource for Broker {
    fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    async fn health(&self, _principal: &Principal, _mode: ReadMode) -> Result<Health, ApiError> {
        Ok(healthy())
    }

    fn accounts(&self) -> Option<&dyn AccountSource> {
        Some(self)
    }

    fn requests(&self) -> Option<&dyn RequestSource> {
        self.requests_enabled.then_some(self)
    }
}
#[async_trait]
impl AccountSource for Broker {
    async fn identity(&self, binding: &Binding) -> Result<Actor, ApiError> {
        Ok(Actor {
            id: binding.id,
            rights: ActorRights::REQUEST_MOVIES
                | ActorRights::REQUEST_SERIES
                | ActorRights::SELECT_PROFILE
                | ActorRights::CANCEL_OWN_PENDING,
        })
    }

    async fn compatible(&self, _binding: &Binding) -> Result<bool, ApiError> {
        Ok(true)
    }

    async fn quota(&self, _binding: &Binding, _bypass: bool) -> Result<Value, ApiError> {
        Ok(json!({"unlimited":true}))
    }

    async fn service_profiles(
        &self,
        _binding: &Binding,
        _kind: MediaType,
    ) -> Result<Option<BrokerProfiles>, ApiError> {
        Ok(Some(BrokerProfiles {
            server_id: 4,
            default_profile_id: Some(1),
            anime_profile_id: None,
            profiles: vec![
                BrokerProfile {
                    id: 1,
                    name: "1080p".to_owned(),
                },
                BrokerProfile {
                    id: 2,
                    name: "2160p".to_owned(),
                },
                BrokerProfile {
                    id: 3,
                    name: "Stale broker name".to_owned(),
                },
            ],
            freshness: freshness(&self.descriptor.name),
        }))
    }
}
#[async_trait]
impl RequestSource for Broker {
    async fn submit(
        &self,
        binding: &Binding,
        item: MediaRequest,
        selection: Option<ProfileSelection>,
    ) -> Result<Value, ApiError> {
        Ok(
            json!({"request_id":19,"tmdb_id":item.tmdb_id,"actor_id":binding.id,"status":"approved","selection":selection.map(|choice|json!({"server_id":choice.server_id,"profile_id":choice.profile_id,"name":choice.name}))}),
        )
    }

    async fn cancel(&self, _binding: &Binding, _id: u64) -> Result<Value, ApiError> {
        Err(ApiError::FORBIDDEN)
    }

    async fn requests(&self, _binding: &Binding, _page: Page) -> Result<Value, ApiError> {
        Ok(json!({"items":[]}))
    }

    async fn request(&self, _binding: &Binding, _id: u64) -> Result<Value, ApiError> {
        Ok(Value::Null)
    }
}

#[derive(Clone, Debug, Default)]
struct FailingSink {
    targets: Arc<Mutex<Vec<Value>>>,
}
#[async_trait]
impl EventSink for FailingSink {
    async fn audit(
        &self,
        _client: &str,
        _actor: Option<u64>,
        events: &[AuditEvent],
        _maximum: usize,
    ) -> Result<(), ApiError> {
        self.targets.lock().expect("test targets").extend(
            events
                .iter()
                .map(|event| serde_json::to_value(&event.target).expect("typed audit target")),
        );
        Err(ApiError::INTERNAL)
    }

    async fn statistics(
        &self,
        _client: &str,
        _tool: ToolId,
        _outcome: &str,
        _duration: u64,
    ) -> Result<(), ApiError> {
        Err(ApiError::INTERNAL)
    }
}

fn fixture(events: Option<Events>) -> (Commands, Principal) {
    let (commands, principal, _) = fixture_with_probe(events);

    (commands, principal)
}

fn fixture_with_probe(events: Option<Events>) -> (Commands, Principal, Arc<Probe>) {
    let mut config =
        Config::parse(include_str!("../../../tests/arrpi.json")).expect("fake settings");

    config.mcp.enabled = true;
    config.cache.max_normalizations = 1;
    let mut sources = Sources::new();
    let probe = Arc::new(Probe::default());

    sources
        .insert(Provider {
            descriptor: descriptor("fourth"),
            error: None,
            probe: Arc::clone(&probe),
        })
        .expect("new source");
    sources
        .insert(Provider {
            descriptor: descriptor("broken"),
            error: Some(ApiError::UNAVAILABLE),
            probe: Arc::clone(&probe),
        })
        .expect("isolated source");
    sources
        .insert(Broker {
            descriptor: descriptor("catalog"),
            requests_enabled: true,
        })
        .expect("account broker");
    let principal = Principal {
        tracking: None,
        name: "friend".into(),
        binding: Some(Binding {
            id: 13,
            plex_id: 17,
        }),
        permissions: [
            "fourth:library-view",
            "fourth:calendar-view",
            "broken:library-view",
            "broken:calendar-view",
            "requests:create",
            "requests:view",
        ]
        .into_iter()
        .map(Into::into)
        .collect(),
    };

    (
        Commands::new(
            &config,
            sources,
            arrpi_core::executor::Recording::new(events, tokio::time::Instant::now),
            arrpi_core::admission::Normalizations::new(config.cache.max_normalizations)
                .expect("bounded normalization pool"),
        ),
        principal,
        probe,
    )
}

#[tokio::test]
async fn fourth_provider_participates_in_discovery_library_calendar_and_health() {
    let (commands, principal) = fixture(None);
    let tools = commands
        .list(&principal)
        .await
        .expect("discover capabilities");

    assert!(
        tools.iter().any(|tool| tool.name == "list_movies"),
        "generic movie capability"
    );
    assert!(
        !tools.iter().any(|tool| tool.name == "search_media"),
        "missing catalog capability is not advertised"
    );
    let library = commands
        .execute(
            &principal,
            ToolId::SearchLibrary,
            json!({"query":"funny","limit":1}),
        )
        .await
        .expect("query fourth provider");

    assert_eq!(library["items"][0]["title"], "A funny film");
    assert_eq!(library["total"], 1);
    assert_eq!(
        library["complete"], false,
        "one source failed independently"
    );
    let calendar = commands
        .calendar(
            &principal,
            Dates {
                start: "2026-10-15".into(),
                end: "2026-10-16".into(),
                no_cache: true,
            },
        )
        .await
        .expect("generic calendar");

    assert_eq!(calendar.events.len(), 1);
    assert_eq!(calendar.events[0].source, "fourth");
    assert_eq!(calendar.sources.len(), 2);
    let health = commands
        .execute(&principal, ToolId::ServiceStatus, json!({}))
        .await
        .expect("generic health");

    assert_eq!(health["complete"], false);
    assert_eq!(
        health["sources"].as_array().expect("source health").len(),
        3
    );
}

#[tokio::test]
async fn absent_grant_prevents_library_access_before_provider_io() {
    let (commands, mut principal, probe) = fixture_with_probe(None);

    principal.permissions.clear();
    let result = commands
        .execute(&principal, ToolId::ListMovies, json!({}))
        .await;

    assert_eq!(result, Err(ApiError::FORBIDDEN));
    assert!(
        commands
            .list(&principal)
            .await
            .expect("empty discovery")
            .is_empty()
    );
    assert_eq!(
        probe.calls(),
        0,
        "authorization rejects before provider I/O"
    );
}

#[tokio::test]
async fn failed_observation_sink_preserves_known_write_receipt_and_whitelists_targets() {
    let sink = FailingSink::default();
    let (commands, principal) = fixture(Some(Events::new(sink.clone())));
    let receipt = commands
        .execute(&principal, ToolId::RequestMovie, json!({"tmdb_id":42}))
        .await
        .expect("upstream receipt is authoritative");

    assert_eq!(receipt["request_id"], 19);
    assert_eq!(receipt["actor_id"], 13, "bound identity reaches capability");
    assert_eq!(
        *sink.targets.lock().expect("captured target"),
        vec![json!({"tmdb_id":42,"request_id":19})]
    );
}

#[test]
fn registry_rejects_name_collisions_and_multiple_account_brokers() {
    let mut sources = Sources::new();

    sources
        .insert(Broker {
            descriptor: descriptor("one"),
            requests_enabled: true,
        })
        .expect("first account source");
    assert!(
        sources
            .insert(Broker {
                descriptor: descriptor("two"),
                requests_enabled: true,
            })
            .is_err()
    );
    assert!(
        sources
            .insert(Provider {
                descriptor: descriptor("one"),
                error: None,
                probe: Arc::new(Probe::default())
            })
            .is_err()
    );
    assert_eq!(sources.len(), 1, "failed insert never replaces source");
}

#[tokio::test]
async fn library_and_calendar_tools_propagate_fresh_mode_to_generic_capabilities() {
    let (commands, principal, probe) = fixture_with_probe(None);

    commands
        .execute(&principal, ToolId::ListMovies, json!({"no_cache":true}))
        .await
        .expect("fresh library snapshot");
    assert_eq!(
        *probe.modes.lock().expect("library modes"),
        vec![ReadMode::Fresh, ReadMode::Fresh]
    );
    probe.modes.lock().expect("clear modes").clear();
    let calendar = commands
        .execute(
            &principal,
            ToolId::UpcomingReleases,
            json!({"start":"2026-10-15","end":"2026-10-16","no_cache":true}),
        )
        .await
        .expect("tool calendar dispatcher");

    assert_eq!(calendar["events"][0]["source"], "fourth");
    assert_eq!(probe.calendar.load(Ordering::SeqCst), 2);
    assert_eq!(
        *probe.modes.lock().expect("calendar modes"),
        vec![ReadMode::Fresh, ReadMode::Fresh]
    );
    probe.modes.lock().expect("clear modes").clear();
    commands
        .execute(&principal, ToolId::ListMovies, json!({}))
        .await
        .expect("cached library read");
    assert_eq!(
        *probe.modes.lock().expect("cached modes"),
        vec![ReadMode::Cached, ReadMode::Cached]
    );
}

#[tokio::test]
async fn denied_calendar_tool_never_dispatches_a_provider_query() {
    let (commands, mut principal, probe) = fixture_with_probe(None);

    principal.permissions.clear();
    assert_eq!(
        commands
            .execute(
                &principal,
                ToolId::UpcomingReleases,
                json!({"start":"2026-10-15","end":"2026-10-16"})
            )
            .await
            .expect_err("no calendar grants"),
        ApiError::FORBIDDEN
    );
    assert_eq!(
        probe.calls(),
        0,
        "denied calendar does not reach healthy or failed source"
    );
}

#[tokio::test]
async fn named_request_profiles_require_linked_safe_matching_library_definitions() {
    let (commands, principal) = fixture(None);
    let profiles = commands
        .execute(
            &principal,
            ToolId::RequestProfiles,
            json!({"media_type":"movie"}),
        )
        .await
        .expect("generic broker profiles");

    assert_eq!(profiles["can_select_profile"], true);
    assert_eq!(profiles["default_profile"], json!({"id":1,"name":"1080p"}));
    assert_eq!(
        profiles["items"].as_array().expect("profile choices").len(),
        2
    );
    assert_eq!(profiles["items"][1]["profile"], "1080p");

    for name in ["2160p", "Stale broker name"] {
        assert_eq!(
            commands
                .execute(
                    &principal,
                    ToolId::RequestMovie,
                    json!({"tmdb_id":42,"profile":name})
                )
                .await
                .expect_err("unsafe or mismatched profile rejected"),
            ApiError::PROFILE_UNAVAILABLE
        );
    }

    let receipt = commands
        .execute(
            &principal,
            ToolId::RequestMovie,
            json!({"tmdb_id":42,"profile":"1080p"}),
        )
        .await
        .expect("verified linked profile reaches broker");

    assert_eq!(
        receipt["selection"],
        json!({"server_id":4,"profile_id":1,"name":"1080p"})
    );
    assert_eq!(receipt["actor_id"], 13, "bound requester remains unchanged");
}

#[tokio::test]
async fn library_normalization_permit_is_not_held_while_waiting_for_source_io() {
    let (commands, principal, probe) = fixture_with_probe(None);
    let commands = Arc::new(commands);

    probe.pause_fresh.store(true, Ordering::SeqCst);
    let first_commands = Arc::clone(&commands);
    let first_principal = principal.clone();
    let first = tokio::spawn(async move {
        first_commands
            .execute(
                &first_principal,
                ToolId::ListMovies,
                json!({"no_cache":true}),
            )
            .await
    });

    tokio::time::timeout(
        std::time::Duration::from_secs(1),
        probe.snapshot_started.notified(),
    )
    .await
    .expect("fresh snapshot entered source I/O");
    let second = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        commands.execute(&principal, ToolId::ListMovies, json!({})),
    )
    .await
    .expect("cached read completes while fresh source is blocked")
    .expect("single CPU permit remains available");

    assert_eq!(second["items"][0]["title"], "A funny film");
    assert!(
        !first.is_finished(),
        "source remains blocked until explicit release"
    );
    probe.snapshot_released.notify_one();
    assert_eq!(
        first.await.expect("fresh task").expect("released read")["total"],
        1
    );
}

#[tokio::test]
async fn malformed_generic_request_payload_is_a_typed_error_instead_of_a_panic() {
    let (commands, principal) = fixture(None);
    let result = commands
        .execute(&principal, ToolId::RequestStatus, json!({"request_id":19}))
        .await;

    assert_eq!(
        result.expect_err("generic broker returned null"),
        ApiError::INVALID_RESPONSE
    );
}

#[tokio::test]
async fn calendar_admission_exhaustion_is_retryable_instead_of_a_partial_success() {
    let (mut commands, mut principal) = fixture(None);

    commands
        .sources
        .insert(Provider {
            descriptor: descriptor("busy"),
            error: Some(ApiError::BUSY),
            probe: Arc::new(Probe::default()),
        })
        .expect("busy source");
    principal.permissions.insert("busy:calendar-view".into());
    let result = commands
        .calendar(
            &principal,
            Dates {
                start: "2026-10-15".into(),
                end: "2026-10-16".into(),
                no_cache: true,
            },
        )
        .await;

    assert!(
        matches!(result, Err(ApiError::BUSY)),
        "local saturation is not a successful partial calendar"
    );
}

#[tokio::test]
async fn combined_calendar_cap_is_explicit_and_independent_of_source_completion_order() {
    let (mut commands, mut principal) = fixture(None);
    let probe = Arc::new(Probe::default());

    probe.calendar_events.store(4096, Ordering::SeqCst);
    commands.sources = Sources::new();

    for name in ["first", "second"] {
        commands
            .sources
            .insert(Provider {
                descriptor: descriptor(name),
                error: None,
                probe: Arc::clone(&probe),
            })
            .expect("healthy calendar source");
        principal
            .permissions
            .insert(format!("{name}:calendar-view").into());
    }

    let dates = || Dates {
        start: "2026-10-15".into(),
        end: "2026-10-16".into(),
        no_cache: true,
    };
    let calendar = commands
        .calendar(&principal, dates())
        .await
        .expect("exact capacity");

    assert_eq!(calendar.events.len(), 8192);
    probe.calendar_events.store(5000, Ordering::SeqCst);
    let error = commands
        .calendar(&principal, dates())
        .await
        .expect_err("combined cap");

    assert_eq!(error, ApiError::CALENDAR_TOO_LARGE);
    assert_eq!(error.code(), "calendar_too_large");
    assert!(error.to_string().contains("shorter date range"));
    assert_eq!(ApiError::from_payload(&error.payload()), Some(error));
}

#[tokio::test]
async fn profile_discovery_requires_a_request_capability() {
    let (mut commands, principal) = fixture(None);
    let mut sources = Sources::new();

    sources
        .insert(Broker {
            descriptor: descriptor("account-only"),
            requests_enabled: false,
        })
        .expect("account without request support");
    commands.sources = sources;
    let tools = commands.list(&principal).await.expect("account discovery");

    assert!(!tools.iter().any(|tool| tool.name == "request_profiles"));
    assert_eq!(
        commands
            .execute(
                &principal,
                ToolId::RequestProfiles,
                json!({"media_type":"movie"})
            )
            .await,
        Err(ApiError::FORBIDDEN)
    );
}
