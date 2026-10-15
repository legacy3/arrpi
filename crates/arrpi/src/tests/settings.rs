//! Database recovery, masked edits, authorization and live credential rotation.
mod health;

use std::{path::Path, sync::Arc};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use rusqlite::Connection;
use serde_json::Value;
use tower::ServiceExt as _;

use super::{
    CLIENT_KEY, EXAMPLE, OTHER_KEY,
    proxy::{json, request},
};
use crate::{
    config::Config,
    error::ApiError,
    proxy::Gateway,
    settings::{Runtime, Store},
};

fn build(config: &Config, _database: &Path, _store: Store) -> Result<Gateway, ApiError> {
    Gateway::new(config, tokio::time::Instant::now).map_err(|error| {
        tracing::debug!(event="fixture_gateway", category=%error);
        ApiError::BAD_QUERY
    })
}

fn config() -> Config {
    let mut config = Config::parse(EXAMPLE).expect("fixture config");

    config.clients[0].permissions.push("config:manage".into());
    config
}

#[test]
fn mcp_public_endpoint_is_exact_and_versioned() {
    let mut config = config();

    for (path, valid) in [
        ("/v1/mcp", true),
        ("/mcp", false),
        ("/v1/mcp/extra", false),
        ("/v1/mcp?token=fixture", false),
    ] {
        config.mcp.public_url = Some(format!("https://example.test{path}"));

        assert_eq!(config.mcp.validate().is_ok(), valid);
    }
}

fn application() -> (Router, Arc<Store>) {
    let store = Arc::new(
        Store::new(Connection::open_in_memory().expect("SQLite fixture").into()).expect("store"),
    );

    store.seed(&config()).expect("seed");
    let app = Runtime::new(Arc::clone(&store), "fixture.sqlite".into(), build)
        .expect("runtime")
        .router();

    (app, store)
}

async fn update(app: &Router, document: Value, key: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/v1/config")
                .header("authorization", format!("Bearer {key}"))
                .header("content-type", "application/json")
                .body(Body::from(document.to_string()))
                .expect("request"),
        )
        .await
        .expect("response")
}

#[tokio::test]
async fn settings_authorization_redaction_validation_and_rotation_apply_to_live_reads() {
    let (app, store) = application();

    assert_eq!(
        request(&app, "GET", "/v1/config", None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&app, "GET", "/v1/config", Some(OTHER_KEY))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    let mut document = json(request(&app, "GET", "/v1/config", Some(CLIENT_KEY)).await).await;
    let original = document.clone();

    assert!(document["settings"]["sources"]["sonarr"]["api_key"].is_null());
    assert!(document["settings"]["clients"][0]["key"].is_null());
    assert!(!document.to_string().contains(CLIENT_KEY));
    document["settings"]["feed"]["title"] = Value::String("Changed live".into());
    assert_eq!(
        update(&app, document, CLIENT_KEY).await.status(),
        StatusCode::OK
    );
    let (_, saved) = store.load().expect("stored config").expect("row");

    assert_eq!(saved.feed.title, "Changed live");
    assert_eq!(saved.clients[0].key.value(), CLIENT_KEY);
    assert_eq!(
        update(&app, original, CLIENT_KEY).await.status(),
        StatusCode::CONFLICT
    );
    let mut rotated = json(request(&app, "GET", "/v1/config", Some(CLIENT_KEY)).await).await;

    rotated["settings"]["clients"][0]["key"] =
        Value::String("new-admin-key-0000000000000000000000".into());
    assert_eq!(
        update(&app, rotated, CLIENT_KEY).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "GET", "/v1/status", Some(CLIENT_KEY))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/config",
            Some("new-admin-key-0000000000000000000000")
        )
        .await
        .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn invalid_settings_and_admin_lockout_leave_database_and_runtime_unchanged() {
    let (app, store) = application();

    for edit in [
        "invalid-origin",
        "short-key",
        "remove-admin",
        "unknown-field",
        "scalar-settings",
        "scalar-source",
        "scalar-client",
        "scalar-subscriptions",
    ] {
        let mut document = json(request(&app, "GET", "/v1/config", Some(CLIENT_KEY)).await).await;

        match edit {
            "invalid-origin" => {
                document["settings"]["sources"]["sonarr"]["url"] =
                    Value::String("file:///etc/passwd".into());
            }

            "short-key" => {
                document["settings"]["clients"][0]["key"] = Value::String("short".into());
            }

            "remove-admin" => {
                document["settings"]["clients"][0]["permissions"] = serde_json::json!(["metrics"]);
            }

            "scalar-settings" => document["settings"] = Value::Number(5.into()),

            "scalar-source" => document["settings"]["sources"]["sonarr"] = Value::Bool(true),

            "scalar-client" => document["settings"]["clients"][0] = Value::String("bad".into()),

            "scalar-subscriptions" => {
                document["settings"]["subscriptions"] = Value::String("bad".into());
            }

            _ => document["settings"]["unknown"] = Value::Bool(true),
        }

        assert_eq!(
            update(&app, document, CLIENT_KEY).await.status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(store.load().expect("store").expect("row").0, 1);
    }

    assert_eq!(
        request(&app, "GET", "/v1/status", Some(CLIENT_KEY))
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn settings_validation_returns_static_rules_without_echoing_owner_values() {
    let (app, store) = application();
    let mut document = json(request(&app, "GET", "/v1/config", Some(CLIENT_KEY)).await).await;

    document["settings"]["feed"]["title"] = Value::String("private-owner-value\n".into());
    let response = update(&app, document, CLIENT_KEY).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let error = json(response).await;

    assert_eq!(error["code"], "invalid_configuration");
    assert_eq!(error["reason"], "invalid feed metadata");
    assert!(!error.to_string().contains("private-owner-value"));
    assert!(!error.to_string().contains(CLIENT_KEY));
    assert_eq!(store.load().expect("store").expect("row").0, 1);
}

#[test]
fn database_restart_keeps_credentials_and_source_configuration_without_environment() {
    let path = tempfile::NamedTempFile::new().expect("temporary database");

    let store = Store::new(Connection::open(path.path()).expect("open").into()).expect("store");

    store.seed(&config()).expect("seed");
    drop(store);
    let restored =
        Store::new(Connection::open(path.path()).expect("reopen").into()).expect("store");
    let (_, config) = restored.load().expect("load").expect("stored settings");

    assert_eq!(config.clients[0].key.value(), CLIENT_KEY);
    assert_eq!(
        config.sources["sonarr"].api_key.value(),
        super::UPSTREAM_KEY
    );
    Gateway::new(&config, tokio::time::Instant::now).expect("no environment resolver");
}

#[tokio::test]
async fn mcp_console_is_admin_only_and_tool_switches_apply_live() {
    let mut config = config();

    config.mcp.enabled = true;
    config.seerr = Some(
        serde_json::from_value(
            serde_json::json!({"url":"http://127.0.0.1:1/","api_key":"fake-seerr-key"}),
        )
        .expect("connection"),
    );
    config.clients[1].seerr_user = Some(arrpi_core::policy::Binding {
        id: 2,
        plex_id: 102,
    });
    let name = config.clients[1].name.clone();
    let store = Arc::new(
        Store::new(Connection::open_in_memory().expect("database").into()).expect("store"),
    );

    store.seed(&config).expect("seed");
    let app = Runtime::new(store, "fixture.sqlite".into(), build)
        .expect("runtime")
        .router();

    for endpoint in ["status", "token"] {
        let path = format!("/v1/mcp/{endpoint}?client={name}");

        assert_eq!(
            request(&app, "GET", &path, Some(OTHER_KEY)).await.status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(&app, "GET", &path, None).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }

    let status = json(
        request(
            &app,
            "GET",
            &format!("/v1/mcp/status?client={name}"),
            Some(CLIENT_KEY),
        )
        .await,
    )
    .await;

    assert_eq!(
        status["tools"].as_array().expect("tools").len(),
        arrpi_core::registry::ToolId::ALL.len()
    );
    assert!(!status.to_string().contains(OTHER_KEY));
    let token = json(
        request(
            &app,
            "GET",
            &format!("/v1/mcp/token?client={name}"),
            Some(CLIENT_KEY),
        )
        .await,
    )
    .await;

    assert_eq!(token["token"], OTHER_KEY);
    let mut document = json(request(&app, "GET", "/v1/config", Some(CLIENT_KEY)).await).await;

    document["settings"]["mcp"]["disabled_tools"] = serde_json::json!(["upcoming_releases"]);
    assert_eq!(
        update(&app, document, CLIENT_KEY).await.status(),
        StatusCode::OK
    );
    let tools = json(request(&app, "GET", "/v1/tools", Some(CLIENT_KEY)).await).await;

    assert!(!tools.to_string().contains("upcoming_releases"));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/tools/upcoming_releases")
                .header("Authorization", format!("Bearer {CLIENT_KEY}"))
                .header("Content-Type", "application/json")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let mut document = json(request(&app, "GET", "/v1/config", Some(CLIENT_KEY)).await).await;

    document["settings"]["mcp"]["enabled"] = false.into();
    assert_eq!(
        update(&app, document, CLIENT_KEY).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "GET", "/v1/tools", Some(CLIENT_KEY))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn aggregate_statistics_survive_restart_and_keep_settings() {
    let file = tempfile::NamedTempFile::new().expect("database");
    let store = Store::new(Connection::open(file.path()).expect("open").into()).expect("store");

    store.seed(&config()).expect("seed");
    store
        .record_tool("one", "media_details", "ok", 10)
        .expect("record");
    store
        .record_tool("one", "media_details", "ok", 20)
        .expect("record");
    store
        .record_tool("two", "request_movie", "unknown_outcome", 30)
        .expect("record");
    drop(store);
    let store = Store::new(Connection::open(file.path()).expect("reopen").into()).expect("store");
    let value = serde_json::to_value(store.tool_stats("one").expect("statistics"))
        .expect("serialized statistics");

    assert_eq!(value["items"][0]["calls"], 2);
    assert_eq!(value["items"][0]["duration_ms"], 30);
    assert_eq!(value["items"].as_array().expect("rows").len(), 1);
    assert!(store.load().expect("load").is_some());
    let text = value.to_string();

    assert!(!text.contains(CLIENT_KEY) && !text.contains("tmdb") && !text.contains("url"));
}

#[tokio::test]
async fn statistics_are_manager_only_and_missing_seerr_disables_mcp_transport() {
    let (app, store) = application();
    let path = format!("/v1/config/mcp/stats?client={}", config().clients[0].name);

    for (key, status) in [
        (CLIENT_KEY, StatusCode::OK),
        (OTHER_KEY, StatusCode::FORBIDDEN),
    ] {
        assert_eq!(
            request(&app, "GET", &path, Some(key)).await.status(),
            status
        );
    }

    let mut config = config();

    config.mcp.enabled = true;
    store.save(1, &config).expect("enable without connection");
    let app = Runtime::new(store, "fixture.sqlite".into(), build)
        .expect("runtime")
        .router();

    assert_eq!(
        request(&app, "GET", "/v1/tools", Some(CLIENT_KEY))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(&app, "GET", "/v1/mcp", Some(CLIENT_KEY))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn write_history_is_bounded_paged_account_scoped_and_survives_restart() {
    use arrpi_core::executor::AuditEvent as Event;
    let file = tempfile::NamedTempFile::new().expect("database");
    let store = Store::new(Connection::open(file.path()).expect("open").into()).expect("store");

    store.seed(&config()).expect("seed");

    for id in 1..=3 {
        store
            .record_audit(
                "one",
                Some(1),
                &[Event {
                    tool: "request_movie".to_owned(),
                    target: arrpi_core::contracts::AuditTarget {
                        tmdb_id: Some(id),
                        ..Default::default()
                    },
                    outcome: "approved".to_owned(),
                }],
                2,
            )
            .expect("audit");
    }

    let page = store.write_audit("one", 0, 1).expect("page");

    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].target.tmdb_id.expect("movie id"), 3);
    assert_eq!(page.next_offset, Some(1));
    assert!(
        store
            .write_audit("two", 0, 1)
            .expect("other")
            .items
            .is_empty()
    );
    drop(store);
    let restored =
        Store::new(Connection::open(file.path()).expect("reopen").into()).expect("store");

    assert_eq!(
        restored.write_audit("one", 1, 1).expect("last").items[0]
            .target
            .tmdb_id
            .expect("movie id"),
        2
    );
    assert!(restored.load().expect("settings").is_some());
}

#[tokio::test]
async fn write_history_requires_admin_and_validates_page_and_account() {
    let (app, _store) = application();
    let path = format!(
        "/v1/config/mcp/audit?client={}&limit=1",
        config().clients[0].name
    );

    assert_eq!(
        request(&app, "GET", &path, Some(CLIENT_KEY)).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "GET", &path, Some(OTHER_KEY)).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{path}&offset=18446744073709551615"),
            Some(CLIENT_KEY)
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/config/mcp/audit?client=not-existing",
            Some(CLIENT_KEY)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}
