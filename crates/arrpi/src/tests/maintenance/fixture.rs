//! End-to-end source grants, asynchronous commands and missing-episode search contracts.

use super::super::{CLIENT_KEY, Server, server};
use crate::{config::Config, proxy::Gateway};
use arrpi_core::policy::Limits;
use axum::{
    Json, Router,
    extract::{Request, State},
    http::StatusCode,
    routing::any,
};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicU16, AtomicUsize, Ordering},
};
use tokio::sync::Mutex;

#[derive(Debug, Default)]
pub(super) struct NativeState {
    pub(super) reads: AtomicUsize,
    pub(super) writes: Mutex<Vec<Value>>,
    pub(super) post_status: AtomicU16,
    pub(super) uhd: bool,
    pub(super) unmonitored: bool,
    pub(super) version: Option<&'static str>,
    pub(super) ignore_scope: bool,
}

fn episode(id: u32, season: u32, has_file: bool, monitored: bool, date: Option<&str>) -> Value {
    json!({"id":id,"seriesId":7,"seasonNumber":season,"episodeNumber":id,"title":format!("Fictional episode {id}"),
        "hasFile":has_file,"monitored":monitored,"airDateUtc":date,"absoluteEpisodeNumber":id,
        "episodeFile":{"path":"/private/media"},"private":"never expose"})
}

async fn native(
    State(state): State<Arc<NativeState>>,
    request: Request,
) -> (StatusCode, Json<Value>) {
    assert_eq!(request.headers()["x-api-key"], "fake-native-key");

    if request.method() == "POST" {
        assert_eq!(request.uri().path(), "/api/v3/command");
        let bytes = axum::body::to_bytes(request.into_body(), 32 * 1024)
            .await
            .expect("command bytes");
        let command: Value = serde_json::from_slice(&bytes).expect("JSON command");

        state.writes.lock().await.push(command.clone());
        let code = state.post_status.load(Ordering::Relaxed);

        return (
            StatusCode::from_u16(if code == 0 { 201 } else { code }).expect("status"),
            Json(
                json!({"id":77,"name":command["name"],"status":"queued","body":if state.ignore_scope {json!({})}else{command.clone()},"message":"secret path /private"}),
            ),
        );
    }

    state.reads.fetch_add(1, Ordering::Relaxed);
    let value = match request.uri().path() {
        "/api/v3/system/status" => json!({"version":state.version.unwrap_or("4.0.20.3014")}),

        "/api/v3/series/7" | "/api/v3/movie/7" => {
            json!({"id":7,"title":"Fictional title","qualityProfileId":3,"monitored":!state.unmonitored,"hasFile":false,"path":"/private/media"})
        }

        "/api/v3/episode" => {
            assert_eq!(request.uri().query(), Some("seriesId=7"));
            json!([
                episode(1, 1, false, true, Some("2020-01-01T00:00:00Z")),
                episode(2, 1, true, true, Some("2020-01-02T00:00:00Z")),
                episode(3, 1, false, false, Some("2020-01-03T00:00:00Z")),
                episode(4, 1, false, true, Some("2099-01-01T00:00:00Z")),
                episode(5, 1, false, true, None),
                episode(6, 2, false, true, Some("2020-01-06T00:00:00Z"))
            ])
        }

        "/api/v3/qualityprofile" => {
            json!([{"id":3,"name":"Fictional profile","items":[{"allowed":true,"quality":{"id":3,"resolution":if state.uhd {2160} else {1080}}}]}])
        }

        "/api/v3/command/77" => {
            json!({"id":77,"name":"RefreshSeries","status":"completed","body":{"path":"/private/media"},"message":"private detail"})
        }

        "/api/v3/command/78" => json!({"id":78,"name":"DeleteMovie","status":"completed"}),

        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"private":"never expose"})),
            );
        }
    };

    (StatusCode::OK, Json(value))
}

pub(super) async fn fixture(grants: &[&str], state: Arc<NativeState>) -> (Server, Server) {
    fixture_with_limits(grants, state, Limits::default()).await
}

pub(super) async fn fixture_with_limits(
    grants: &[&str],
    state: Arc<NativeState>,
    limits: Limits,
) -> (Server, Server) {
    let upstream = server(Router::new().fallback(any(native)).with_state(state)).await;
    let config: Config = serde_json::from_value(json!({
        "listen":"127.0.0.1:0","mcp":{"enabled":true,"limits":limits},
        "seerr":{"url":upstream.url,"api_key":"fake-broker-key"},
        "sources":{
            "shows":{"kind":"sonarr","url":upstream.url,"api_key":"fake-native-key"},
            "movies":{"kind":"radarr","url":upstream.url,"api_key":"fake-native-key"}},
        "clients":[{"name":"test","key":CLIENT_KEY,"permissions":grants}]
    }))
    .expect("settings");

    config.validate().expect("valid source grants");
    let gateway = Gateway::new(&config, tokio::time::Instant::now).expect("gateway");
    let api = server(crate::proxy::router(gateway)).await;

    (upstream, api)
}

pub(super) async fn call(api: &Server, tool: &str, input: Value) -> (StatusCode, Value) {
    let response = reqwest::Client::new()
        .post(format!("{}v1/tools/{tool}", api.url))
        .bearer_auth(CLIENT_KEY)
        .header("content-type", "application/json")
        .body(input.to_string())
        .send()
        .await
        .expect("tool call");
    let status = response.status();
    let bytes = response.bytes().await.expect("response");

    (
        status,
        serde_json::from_slice(&bytes).expect("JSON response"),
    )
}
