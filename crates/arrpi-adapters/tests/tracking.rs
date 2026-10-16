//! Native tracking isolation, consent and route ownership over a fake upstream.

use std::sync::{Arc, Mutex};

use arrpi_core::{
    config::Config,
    error::ApiError,
    tracking::{Binding, DestinationAction, Provider, TrackingSource},
};
use axum::{
    Json, Router,
    extract::{OriginalUri, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse as _, Response},
    routing::any,
};
use serde_json::{Value, json};

#[derive(Default)]
struct Native {
    routes: Vec<Value>,
    pairs: Vec<Value>,
    writes: Vec<(String, Value)>,
    shared: bool,
    wrong_account: bool,
    wrong_server: bool,
    local_id: u64,
    destinations: Vec<&'static str>,
    poll: Option<&'static str>,
    shared_destination: Option<&'static str>,
    poll_delay_ms: u64,
    polls_started: u64,
}

struct Fixture {
    state: Arc<Mutex<Native>>,
    built: arrpi_adapters::BuiltSources,
    server: tokio::task::JoinHandle<()>,
    binding: Binding,
}

impl Fixture {
    async fn new() -> Self {
        let state = Arc::new(Mutex::new(Native::default()));
        let app = Router::new()
            .fallback(any(upstream))
            .with_state(Arc::clone(&state));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let address = listener.local_addr().expect("address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("upstream");
        });
        let config: Config = serde_json::from_value(json!({
            "listen":"127.0.0.1:0",
            "sources":{"crosswatch":{"kind":"crosswatch","url":format!("http://{address}/"),"api_key":"fake-native-token"}},
            "clients":[{"name":"user","key":"fake-account-key-0000000000000000","permissions":["crosswatch:tracking-view","crosswatch:tracking-manage"],
                "tracking_user":{"source":"crosswatch","profile_id":"alice","plex_id":42}}]
        })).expect("settings");

        config.validate().expect("valid tracking settings");
        let built = arrpi_adapters::build(&config, tokio::time::Instant::now).expect("adapters");

        Self {
            state,
            built,
            server,
            binding: Binding {
                source: "crosswatch".to_owned(),
                profile_id: "alice".to_owned(),
                plex_id: 42,
                plex_server_owner: false,
            },
        }
    }

    fn change(&self, operation: impl FnOnce(&mut Native)) {
        operation(&mut self.state.lock().expect("state"));
    }

    fn source(&self) -> &dyn TrackingSource {
        self.built
            .sources
            .source("crosswatch")
            .expect("source")
            .tracking()
            .expect("crosswatch")
    }
}

async fn upstream(
    State(state): State<Arc<Mutex<Native>>>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, StatusCode> {
    assert_eq!(headers["x-cw-token"], "fake-native-token");
    let path = uri.path();

    if method == Method::POST && path.ends_with("/poll") {
        let delay = {
            let mut native = state.lock().expect("state");

            native.polls_started += 1;
            native.poll_delay_ms
        };

        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
    }

    let mut native = state.lock().expect("native state");

    if method != Method::GET {
        let value: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);

        native.writes.push((path.to_owned(), value.clone()));

        if path == "/api/scrobbler/routes" {
            native.routes.push(value);
        } else if path.starts_with("/api/scrobbler/routes/") {
            let id = path.rsplit('/').next().expect("route id");

            if method == Method::DELETE {
                native.routes.retain(|route| route["id"] != id);
            } else if let Some(route) = native.routes.iter_mut().find(|route| route["id"] == id) {
                route["enabled"] = value["enabled"].clone();
            }
        }

        let response = if path.ends_with("/poll") {
            let status = native.poll.unwrap_or("authorization_pending");

            json!({"ok": status == "authorized", "status":status})
        } else if path == "/api/mdblist/device/start" {
            json!({"ok":true,"user_code":"ABCD-EFGH","expires_at":1_767_226_200,"device_code":"never-expose"})
        } else if path.contains("/pin/new") || path.ends_with("/start") {
            json!({"ok":true,"code":"ABCD","user_code":"EFGH","expiresIn":600,"access_token":"never-expose"})
        } else {
            json!({"ok":true})
        };

        return Ok(([("date", "Thu, 01 Jan 2026 00:00:00 GMT")], Json(response)).into_response());
    }

    let value = match path {
        "/api/user-profiles" => {
            if native.destinations.is_empty() {
                json!({"items":[{"id":"alice","instances":{"PLEX":["P1"],"TRAKT":["T1"]}},{"id":"bob","instances":{"PLEX":[if native.shared {"P1"} else {"P2"}],"TRAKT":["T2"]}}]})
            } else {
                let mut instances = json!({"PLEX":["P1"],"TRAKT":["T1"]});

                for id in &native.destinations {
                    instances[id.to_ascii_uppercase()] = json!([format!("{id}-alice")]);
                }

                instances["TRAKT"] = json!(["T1"]);
                let mut bob = json!({"PLEX":["P2"],"TRAKT":["T2"]});

                if let Some(provider) = native.shared_destination {
                    bob[provider.to_ascii_uppercase()] = json!([format!("{provider}-alice")]);
                }

                json!({"items":[{"id":"alice","instances":instances},{"id":"bob","instances":bob}]})
            }
        }

        "/api/config" => {
            let mut config = json!({"pairs": native.pairs, "plex":{"machine_id":"server-1", "instances":{"P1":{"account_token":"redacted","account_id":7,"machine_id":if native.wrong_server {"another-server"} else {"server-1"},"_cloud":{"account_id":42},"username":"alice"}}},"trakt":{"instances":{"T1":{"access_token":"redacted","username":"alice-trakt"}}}});

            if !native.destinations.is_empty() {
                for id in &native.destinations {
                    if *id != "trakt" {
                        config[*id] = json!({"instances":{format!("{id}-alice"): {"access_token":"redacted","api_token":"redacted","api_key":"redacted","webhook_url":"redacted","server":"fixture-server","server_url":"fixture-server","user_id":"fixture-user","connected":true,"connection_verified":true}}});
                    }
                }
            }

            if native.poll.is_some() {
                config["simkl"]["instances"]["simkl-alice"]["_pending_pin"] =
                    json!({"expires_at":1_767_226_200});

                for id in ["mdblist", "punchplay", "bingebase", "flicklist"] {
                    config[id]["instances"][format!("{id}-alice")]["_pending_device"] =
                        json!({"expires_at":1_767_226_200});
                }
            }

            config
        }

        "/api/scrobbler/overview" => {
            json!({"source_state":{"watch_autostart":true},"routes":native.routes})
        }

        "/api/plex/pickusers" => {
            json!({"users":[{"type":"self","cloud_account_id":if native.wrong_account {99} else {42},"pms_account_id":if native.local_id == 0 {3} else {native.local_id}}]})
        }

        "/api/activity/recent" => {
            json!({"items":[event("P2","T2","Private"),event("P1","T1","Watched")]})
        }

        _ => return Err(StatusCode::NOT_FOUND),
    };

    Ok(([("date", "Thu, 01 Jan 2026 00:00:00 GMT")], Json(value)).into_response())
}

fn event(plex: &str, trakt: &str, title: &str) -> Value {
    json!({"kind":"scrobble","status":"ok","source":"plex","target":"trakt","source_instance":plex,"target_instance":trakt,"title":title,"media_type":"movie","watched_at":1_792_000_000})
}

#[tokio::test]
async fn status_is_private_and_never_enables_or_authorizes() {
    let fixture = Fixture::new().await;
    let status = fixture
        .source()
        .status(&fixture.binding)
        .await
        .expect("status");

    assert_eq!(status.recent.len(), 1);
    assert_eq!(status.recent[0].title, "Watched");
    assert!(!status.watcher.enabled);
    assert!(fixture.state.lock().expect("state").writes.is_empty());
    let serialized = serde_json::to_string(&status).expect("public status");

    assert!(!serialized.contains("redacted"));
    assert!(!serialized.contains("Private"));
}

#[tokio::test]
async fn shared_instances_are_rejected_before_any_write() {
    let fixture = Fixture::new().await;

    fixture.state.lock().expect("state").shared = true;
    let error = fixture
        .source()
        .authorize(&fixture.binding, Provider::Trakt)
        .await
        .expect_err("shared profile rejected");

    assert_eq!(error, ApiError::FORBIDDEN);
    assert!(fixture.state.lock().expect("state").writes.is_empty());
}

#[tokio::test]
async fn wrong_plex_identity_cannot_enable_tracking() {
    let fixture = Fixture::new().await;

    fixture.state.lock().expect("state").wrong_account = true;
    let error = fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect_err("wrong Plex user");

    assert_eq!(error, ApiError::FORBIDDEN);
    assert!(fixture.state.lock().expect("state").writes.is_empty());
}

#[tokio::test]
async fn shared_user_route_uses_cloud_id_with_strict_server_filter() {
    let fixture = Fixture::new().await;

    fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect("enable");
    let native = fixture.state.lock().expect("state");
    let route = &native.routes[0];

    assert_eq!(route["filters"]["username_whitelist"], json!(["id:42"]));
    assert_eq!(route["filters"]["server_uuid"], "server-1");
    assert_eq!(route["options"]["watch"]["unresolved_user_fallback"], false);
    assert_eq!(route["options"]["auto_remove_watchlist"], "off");
    assert!(
        !native
            .writes
            .iter()
            .any(|(path, _)| path.contains("sync") || path.contains("pairs"))
    );
}

#[tokio::test]
async fn owner_route_uses_verified_pms_local_id() {
    let mut fixture = Fixture::new().await;

    fixture.binding.plex_server_owner = true;
    fixture.state.lock().expect("state").local_id = 7;
    fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect("enable owner");
    assert_eq!(
        fixture.state.lock().expect("state").routes[0]["filters"]["username_whitelist"],
        json!(["id:7"])
    );
}

#[tokio::test]
async fn reconnect_pauses_owned_route_and_returns_only_pin_fields() {
    let fixture = Fixture::new().await;

    fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect("enable");
    let authorization = fixture
        .source()
        .authorize(&fixture.binding, Provider::Trakt)
        .await
        .expect("PIN");

    assert_eq!(authorization.code, "EFGH");
    assert_eq!(
        authorization.verification_url,
        "https://auth.trakt.tv/activate"
    );
    assert_eq!(
        fixture.state.lock().expect("state").routes[0]["enabled"],
        false
    );
    assert!(
        !serde_json::to_string(&authorization)
            .expect("safe pin")
            .contains("access_token")
    );
}

#[tokio::test]
async fn foreign_route_is_not_adopted_or_mutated() {
    let fixture = Fixture::new().await;

    fixture.state.lock().expect("state").routes.push(json!({"id":"native-owner-route","provider":"plex","provider_instance":"P1","sink":"trakt","sink_instance":"T1"}));
    let status = fixture
        .source()
        .status(&fixture.binding)
        .await
        .expect("status");

    assert!(status.watcher.managed_elsewhere);
    let error = fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect_err("preserve foreign route");

    assert_eq!(error, ApiError::CONFLICT);
    assert!(fixture.state.lock().expect("state").writes.is_empty());
}

#[tokio::test]
async fn disconnect_removes_only_owned_route_before_native_token_delete() {
    let fixture = Fixture::new().await;

    fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect("enable");
    fixture.state.lock().expect("state").routes.push(json!({"id":"bob-route","provider":"plex","provider_instance":"P2","sink":"trakt","sink_instance":"T2"}));
    fixture
        .source()
        .disconnect(&fixture.binding)
        .await
        .expect("disconnect");
    let native = fixture.state.lock().expect("state");

    assert_eq!(native.routes.len(), 1);
    assert_eq!(native.routes[0]["id"], "bob-route");
    assert_eq!(
        native.writes.last().expect("token delete").0,
        "/api/trakt/token/delete"
    );
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}

#[tokio::test]
async fn foreign_sync_or_single_endpoint_route_prevents_authorization() {
    for pair in [true, false] {
        let fixture = Fixture::new().await;

        {
            let mut native = fixture.state.lock().expect("state");

            if pair {
                native.pairs.push(json!({"source":"PLEX","source_instance":"P1","target":"TRAKT","target_instance":"T2"}));
            } else {
                native.routes.push(json!({"id":"other-route","provider":"plex","provider_instance":"P2","sink":"trakt","sink_instance":"T1"}));
            }
        }
        let error = fixture
            .source()
            .authorize(&fixture.binding, Provider::Trakt)
            .await
            .expect_err("preserve foreign resources");

        assert_eq!(error, ApiError::CONFLICT);
        assert!(fixture.state.lock().expect("state").writes.is_empty());
    }
}

#[tokio::test]
async fn altered_managed_filter_is_not_silently_reenabled() {
    let fixture = Fixture::new().await;

    fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect("enable");
    let before = {
        let mut native = fixture.state.lock().expect("state");

        native.routes[0]["options"]["watch"]["unresolved_user_fallback"] = json!(true);
        native.writes.len()
    };
    let error = fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect_err("unexpected filter");

    assert_eq!(error, ApiError::CONFLICT);
    assert_eq!(fixture.state.lock().expect("state").writes.len(), before);
}

#[tokio::test]
async fn owner_plex_reauthorization_is_never_available_from_the_portal() {
    let mut fixture = Fixture::new().await;

    fixture.binding.plex_server_owner = true;
    let error = fixture
        .source()
        .authorize(&fixture.binding, Provider::Plex)
        .await
        .expect_err("owner connection is operator managed");

    assert_eq!(error, ApiError::FORBIDDEN);
    assert!(fixture.state.lock().expect("state").writes.is_empty());
}

#[tokio::test]
async fn participant_cannot_track_a_different_plex_server() {
    let fixture = Fixture::new().await;

    fixture.state.lock().expect("state").wrong_server = true;
    let state = fixture
        .source()
        .status(&fixture.binding)
        .await
        .expect("status");

    assert!(!state.plex.linked);
    assert!(state.plex.needs_reconnect);
    let error = fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect_err("wrong server");

    assert_eq!(error, ApiError::FORBIDDEN);
    assert!(fixture.state.lock().expect("state").writes.is_empty());
}

const DESTINATIONS: &[&str] = &[
    "trakt",
    "simkl",
    "anilist",
    "myanimelist",
    "kitsu",
    "mdblist",
    "bingebase",
    "flicklist",
    "punchplay",
    "wetrakr",
    "crosswatch",
    "floppy",
    "scrob",
    "jellyfin",
    "emby",
    "kodi",
];

#[tokio::test]
async fn multiple_destinations_have_independent_routes_and_controls() {
    let fixture = Fixture::new().await;

    fixture.state.lock().expect("state").destinations = DESTINATIONS.to_vec();
    let status = fixture
        .source()
        .status(&fixture.binding)
        .await
        .expect("status");

    assert_eq!(status.destinations.len(), DESTINATIONS.len());

    for id in DESTINATIONS {
        fixture
            .source()
            .destination(&fixture.binding, id, DestinationAction::Enable)
            .await
            .expect("enable destination");
    }

    assert_eq!(
        fixture.state.lock().expect("state").routes.len(),
        DESTINATIONS.len()
    );
    fixture
        .source()
        .destination(&fixture.binding, "simkl", DestinationAction::Pause)
        .await
        .expect("pause SIMKL");
    {
        let native = fixture.state.lock().expect("state");

        for route in &native.routes {
            assert_eq!(route["enabled"], route["sink"] != "simkl");
        }
    }
    fixture
        .source()
        .destination(&fixture.binding, "simkl", DestinationAction::Disconnect)
        .await
        .expect("disconnect SIMKL");
    let native = fixture.state.lock().expect("state");

    assert_eq!(native.routes.len(), DESTINATIONS.len() - 1);
    assert_eq!(
        native.writes.last().expect("write").0,
        "/api/simkl/token/delete"
    );
}

#[tokio::test]
async fn unsafe_global_local_tracker_disconnect_is_rejected_without_writes() {
    let fixture = Fixture::new().await;

    fixture.state.lock().expect("state").destinations = DESTINATIONS.to_vec();
    let error = fixture
        .source()
        .destination(
            &fixture.binding,
            "crosswatch",
            DestinationAction::Disconnect,
        )
        .await
        .expect_err("native endpoint is global");

    assert_eq!(error, ApiError::CONFLICT);
    assert!(fixture.state.lock().expect("state").writes.is_empty());
}

#[tokio::test]
async fn unassigned_and_unknown_destinations_are_rejected() {
    let fixture = Fixture::new().await;

    for id in ["simkl", "unknown", "../config", "tmdb"] {
        fixture
            .source()
            .destination(&fixture.binding, id, DestinationAction::Enable)
            .await
            .expect_err("unassigned or invalid destination");
    }

    assert!(fixture.state.lock().expect("state").writes.is_empty());
}

#[tokio::test]
async fn every_portal_pin_flow_uses_only_its_bound_native_instance() {
    for (provider, path) in [
        (Provider::Plex, "/api/plex/pin/new"),
        (Provider::Trakt, "/api/trakt/pin/new"),
        (Provider::Simkl, "/api/simkl/pin/start"),
        (Provider::Mdblist, "/api/mdblist/device/start"),
        (Provider::Punchplay, "/api/punchplay/device/start"),
        (Provider::Bingebase, "/api/bingebase/device/start"),
        (Provider::Flicklist, "/api/flicklist/device/start"),
    ] {
        let fixture = Fixture::new().await;

        fixture.state.lock().expect("state").destinations = DESTINATIONS.to_vec();
        let pin = fixture
            .source()
            .authorize(&fixture.binding, provider)
            .await
            .expect("native authorization");

        assert_eq!(pin.expires_in, 600);
        assert_eq!(
            fixture
                .state
                .lock()
                .expect("state")
                .writes
                .last()
                .expect("write")
                .0,
            path
        );
    }
}

#[tokio::test]
async fn plex_reauthorization_pauses_all_owned_destinations() {
    let fixture = Fixture::new().await;

    fixture.state.lock().expect("state").destinations = DESTINATIONS.to_vec();

    for id in ["trakt", "simkl"] {
        fixture
            .source()
            .destination(&fixture.binding, id, DestinationAction::Enable)
            .await
            .expect("enable");
    }

    fixture
        .source()
        .authorize(&fixture.binding, Provider::Plex)
        .await
        .expect("reauthorize source");
    assert!(
        fixture
            .state
            .lock()
            .expect("state")
            .routes
            .iter()
            .all(|row| row["enabled"] == false)
    );
}

#[tokio::test]
async fn native_pending_denied_and_expired_are_explicit_poll_results() {
    for (native_status, expected) in [
        ("authorization_pending", "pending"),
        ("authorized", "authorized"),
        ("expired_token", "expired"),
        ("access_denied", "denied"),
    ] {
        let fixture = Fixture::new().await;

        fixture.change(|state| {
            state.destinations = DESTINATIONS.to_vec();
            state.poll = Some(native_status);
        });

        for id in ["simkl", "mdblist", "punchplay", "bingebase", "flicklist"] {
            let result = fixture
                .source()
                .poll(&fixture.binding, id)
                .await
                .expect("poll outcome");

            assert_eq!(
                serde_json::to_value(result).expect("poll")["state"],
                expected
            );
        }
    }
}

#[tokio::test]
async fn shared_extra_destination_does_not_block_or_expose_other_connections() {
    let fixture = Fixture::new().await;

    fixture.change(|state| {
        state.destinations = DESTINATIONS.to_vec();
        state.shared_destination = Some("simkl");
    });
    let status = fixture
        .source()
        .status(&fixture.binding)
        .await
        .expect("status");
    let simkl = status
        .destinations
        .iter()
        .find(|row| row.id == "simkl")
        .expect("SIMKL");

    assert!(simkl.watcher.managed_elsewhere);
    assert!(!simkl.connection.linked);
    assert!(simkl.connection.username.is_none());
    fixture
        .source()
        .set_enabled(&fixture.binding, true)
        .await
        .expect("Trakt remains independent");
    assert_eq!(
        fixture
            .source()
            .destination(&fixture.binding, "simkl", DestinationAction::Enable)
            .await
            .expect_err("shared destination protected"),
        ApiError::CONFLICT
    );
}

#[tokio::test]
async fn an_interactive_write_waits_for_an_inflight_poll_without_capacity_failure() {
    let fixture = Fixture::new().await;

    fixture.change(|state| {
        state.destinations = DESTINATIONS.to_vec();
        state.poll = Some("authorization_pending");
        state.poll_delay_ms = 600;
    });
    let interactive = async {
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while fixture.state.lock().expect("state").polls_started == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("native poll started");
        fixture
            .source()
            .set_enabled(&fixture.binding, true)
            .await
            .expect("queued interactive write");
    };
    let (polled, ()) = tokio::join!(
        fixture.source().poll(&fixture.binding, "simkl"),
        interactive
    );

    polled.expect("pending poll");
    assert!(
        fixture
            .source()
            .status(&fixture.binding)
            .await
            .expect("status")
            .watcher
            .enabled
    );
}
