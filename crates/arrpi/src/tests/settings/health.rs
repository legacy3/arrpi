//! Operator probes authenticate before reads and never return native identities.
use super::*;
use axum::{Json, extract::State, http::HeaderMap, response::IntoResponse as _, routing::get};
use serde_json::json;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use tokio::sync::Mutex;

#[derive(Default)]
struct Native {
    calls: AtomicUsize,
    status: AtomicU16,
    issues: AtomicUsize,
    seen: Mutex<Vec<(String, HeaderMap)>>,
}

async fn native(
    State(native): State<Arc<Native>>,
    headers: HeaderMap,
    uri: axum::http::Uri,
) -> axum::response::Response {
    native.calls.fetch_add(1, Ordering::Relaxed);
    native
        .seen
        .lock()
        .await
        .push((uri.path().to_owned(), headers));

    if let Ok(status) = StatusCode::from_u16(native.status.load(Ordering::Relaxed)) {
        return (status, "private native failure details").into_response();
    }

    let value = if uri.path() == "/api/v1/auth/me" {
        json!({"id":1,"email":"private-owner@example.test","plexId":999})
    } else {
        json!(
            (0..native.issues.load(Ordering::Relaxed))
                .map(|_| json!({"message":"private native warning"}))
                .collect::<Vec<_>>()
        )
    };

    Json(value).into_response()
}

#[tokio::test]
async fn connection_health_is_operator_only_fresh_and_independent_of_requester_setup() {
    let native_state = Arc::new(Native::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let origin = format!("http://{}/", listener.local_addr().expect("address"));
    let upstream = Router::new()
        .route("/api/v3/health", get(native))
        .route("/api/v1/auth/me", get(native))
        .with_state(Arc::clone(&native_state));
    let server = tokio::spawn(async move {
        axum::serve(listener, upstream)
            .await
            .expect("native fixture");
    });
    let mut settings = config();

    for source in settings.sources.values_mut() {
        source.url.clone_from(&origin);
    }

    settings.seerr = Some(
        serde_json::from_value(json!({"url":origin,"api_key":"fake-seerr-key"}))
            .expect("Seerr settings"),
    );
    assert!(
        !settings.mcp.enabled,
        "operator health does not require MCP"
    );
    assert!(settings.clients[0].seerr_user.is_none());
    let store =
        Arc::new(Store::new(Connection::open_in_memory().expect("SQLite").into()).expect("store"));

    store.seed(&settings).expect("seed");
    let app = Runtime::new(store, "fixture.sqlite".into(), build)
        .expect("runtime")
        .router();
    let path = "/v1/config/connections/sonarr/health";

    assert_eq!(
        request(&app, "GET", path, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&app, "GET", path, Some(OTHER_KEY)).await.status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        request(&app, "PUT", path, Some(CLIENT_KEY)).await.status(),
        StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(native_state.calls.load(Ordering::Relaxed), 0);

    let healthy = json(request(&app, "GET", path, Some(CLIENT_KEY)).await).await;

    assert_eq!(healthy["status"], "healthy");
    assert_eq!(healthy["health_issue_count"], 0);
    let seerr = json(
        request(
            &app,
            "GET",
            "/v1/config/connections/seerr/health",
            Some(CLIENT_KEY),
        )
        .await,
    )
    .await;

    assert_eq!(seerr["status"], "healthy");
    assert!(!seerr.to_string().contains("private-owner"));
    native_state.issues.store(1, Ordering::Relaxed);
    let warning = json(request(&app, "GET", path, Some(CLIENT_KEY)).await).await;

    assert_eq!(warning["status"], "attention");
    assert_eq!(warning["health_issue_count"], 1);
    assert!(!warning.to_string().contains("private native"));

    for (code, expected) in [
        (401, "authentication_failed"),
        (502, "unavailable"),
        (200, "unavailable"),
    ] {
        native_state.status.store(code, Ordering::Relaxed);

        for source in ["sonarr", "seerr"] {
            let result = json(
                request(
                    &app,
                    "GET",
                    &format!("/v1/config/connections/{source}/health"),
                    Some(CLIENT_KEY),
                )
                .await,
            )
            .await;

            assert_eq!(
                result["status"], expected,
                "fresh response must replace a previous success"
            );
            assert!(!result.to_string().contains("private"));
            assert!(!result.to_string().contains(&origin));
            assert!(!result.to_string().contains("fake-seerr-key"));
        }
    }

    assert_operator_headers(&native_state).await;

    server.abort();
}

async fn assert_operator_headers(native: &Native) {
    for (path, headers) in native.seen.lock().await.iter() {
        assert!(
            !headers.contains_key("X-API-User"),
            "operator probes must not impersonate requesters"
        );
        let expected_key = if path == "/api/v1/auth/me" {
            "fake-seerr-key"
        } else {
            "fake-upstream-api-key"
        };

        assert_eq!(headers["X-API-Key"], expected_key);
    }
}

#[tokio::test]
async fn missing_seerr_is_explicitly_unconfigured_and_unknown_source_is_not_found() {
    let (app, _) = application();
    let value = json(
        request(
            &app,
            "GET",
            "/v1/config/connections/seerr/health",
            Some(CLIENT_KEY),
        )
        .await,
    )
    .await;

    assert_eq!(value["status"], "unconfigured");
    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/config/connections/unknown/health",
            Some(CLIENT_KEY)
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}
