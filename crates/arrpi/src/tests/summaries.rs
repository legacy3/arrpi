//! Minimal summaries remain usable without MCP/Seerr and preserve source-level failures.
use super::{CLIENT_KEY, OTHER_KEY, mock};
use crate::{
    config::Config,
    proxy::{Gateway, router},
};
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use tower::ServiceExt as _;

#[tokio::test]
async fn summary_http_grants_are_separate_and_partial_results_survive_source_failure() {
    let (healthy, state) =
        mock(r#"[{"title":"private-name","monitored":true,"hasFile":true,"sizeOnDisk":123}]"#)
            .await;
    let (failed, down) = mock("{}").await;

    down.status.store(503, Ordering::Relaxed);
    let config:Config = serde_json::from_value(json!({
        "listen":"127.0.0.1:0",
        "sources":{
            "movies":{"kind":"radarr","url":healthy.url,"api_key":"fake-native-key"},
            "other":{"kind":"radarr","url":failed.url,"api_key":"fake-native-key"}
        },
        "clients":[
            {"name":"summary","key":CLIENT_KEY,"permissions":["movies:library-summary","other:library-summary"]},
            {"name":"metadata","key":OTHER_KEY,"permissions":["movies:library-view"]}
        ]
    })).expect("settings");
    let app = router(Gateway::new(&config, tokio::time::Instant::now).expect("gateway"));

    for (credential, expected) in [(CLIENT_KEY, json!(["library"])), (OTHER_KEY, json!([]))] {
        let response = request(&app, "/v1/status", Some(credential)).await;
        let body = to_bytes(response.into_body(), 65_536).await.expect("body");
        let status: Value = serde_json::from_slice(&body).expect("status");

        assert_eq!(status["sources"][0]["summaries"], expected);
    }

    assert_eq!(
        state.calls.load(Ordering::Relaxed),
        0,
        "discovery never fetches operational data"
    );

    for credential in [None, Some(OTHER_KEY)] {
        let response = request(&app, "/v1/summaries/library", credential).await;

        assert_eq!(
            response.status(),
            if credential.is_none() {
                StatusCode::UNAUTHORIZED
            } else {
                StatusCode::FORBIDDEN
            }
        );
    }

    assert_eq!(state.calls.load(Ordering::Relaxed), 0);
    let response = request(
        &app,
        "/v1/summaries/library?no_cache=true",
        Some(CLIENT_KEY),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 65_536).await.expect("body");
    let result: Value = serde_json::from_slice(&body).expect("JSON");

    assert_eq!(result["complete"], false);
    assert_eq!(result["sources"][0]["status"], "ok");
    assert_eq!(result["sources"][0]["data"]["size_bytes"], 123);
    assert_eq!(result["sources"][0]["freshness"]["bypass_requested"], true);
    assert_eq!(result["sources"][1]["status"], "unavailable");
    assert!(!result.to_string().contains("private-name"));
    assert_eq!(
        request(&app, "/v1/movies/api/v3/movie", Some(CLIENT_KEY))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "/v1/summaries/library", Some(OTHER_KEY))
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(state.calls.load(Ordering::Relaxed), 1);
    let fresh = Request::builder()
        .uri("/v1/summaries/library")
        .header("authorization", format!("Bearer {CLIENT_KEY}"))
        .header("cache-control", "no-cache")
        .body(Body::empty())
        .expect("fresh request");
    let response = app.oneshot(fresh).await.expect("fresh response");
    let body = to_bytes(response.into_body(), 65_536).await.expect("body");
    let result: Value = serde_json::from_slice(&body).expect("JSON");

    assert_eq!(result["sources"][0]["freshness"]["bypass_requested"], true);
    assert_eq!(
        state.calls.load(Ordering::Relaxed),
        2,
        "header bypass reaches the native adapter"
    );
}

#[tokio::test]
async fn selecting_one_source_never_fetches_unrelated_or_unauthorized_connections() {
    let (upstream, state) = mock(r#"[{"monitored":true,"hasFile":true,"sizeOnDisk":123}]"#).await;
    let (unrelated, other) = mock("{}").await;
    let config: Config = serde_json::from_value(json!({
        "listen":"127.0.0.1:0",
        "sources": {
            "selected":{"kind":"radarr","url":upstream.url,"api_key":"fake-native-key"},
            "other":{"kind":"radarr","url":unrelated.url,"api_key":"fake-native-key"}
        },
        "clients":[{"name":"reader","key":CLIENT_KEY,"permissions":["selected:library-summary","other:library-summary"]},
            {"name":"restricted","key":OTHER_KEY,"permissions":["other:library-summary"]}]
    })).expect("settings");
    let app = router(Gateway::new(&config, tokio::time::Instant::now).expect("gateway"));
    let path = "/v1/sources/selected/summaries/library?no_cache=true";

    for (credential, status) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some(OTHER_KEY), StatusCode::FORBIDDEN),
    ] {
        assert_eq!(request(&app, path, credential).await.status(), status);
    }

    assert_eq!(state.calls.load(Ordering::Relaxed), 0);
    let response = request(&app, path, Some(CLIENT_KEY)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let value: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await.expect("body"))
            .expect("JSON");

    assert_eq!(value["complete"], true);
    assert_eq!(value["sources"].as_array().expect("sources").len(), 1);
    assert_eq!(value["sources"][0]["source"], "selected");
    assert_eq!(value["sources"][0]["freshness"]["bypass_requested"], true);
    assert_eq!(state.calls.load(Ordering::Relaxed), 1);
    assert_eq!(other.calls.load(Ordering::Relaxed), 0);
}

async fn request(
    app: &axum::Router,
    path: &str,
    credential: Option<&str>,
) -> axum::response::Response {
    let mut builder = Request::builder().uri(path);

    if let Some(credential) = credential {
        builder = builder.header("authorization", format!("Bearer {credential}"));
    }

    app.clone()
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response")
}
