//! Identity probes never send credentials or promote a listening port to verified identity.
use super::*;
use axum::{Json, Router, routing::get};

struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn probe_distinguishes_identity_authentication_and_ambiguous_json() {
    let router = Router::new()
        .route(
            "/identified",
            get(|headers: axum::http::HeaderMap| async move {
                assert!(!headers.contains_key("authorization"));
                assert!(!headers.contains_key("x-api-key"));
                Json(serde_json::json!({"appName":"Sonarr","version":"4.0.1"}))
            }),
        )
        .route(
            "/ambiguous",
            get(|| async { Json(serde_json::json!({"status":"ok"})) }),
        )
        .route(
            "/protected",
            get(|| async { axum::http::StatusCode::UNAUTHORIZED }),
        )
        .route("/huge", get(|| async { "x".repeat(70_000) }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let port = listener.local_addr().expect("address").port();
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, router).await.expect("fixture");
    }));
    let http = Http::new(HttpOptions {
        timeout: Duration::from_secs(2),
        connect_timeout: Duration::from_secs(1),
        pool_idle_per_host: 1,
    })
    .expect("client");
    let address = std::net::Ipv4Addr::LOCALHOST.into();
    let known = probe(&http, address, port, "sonarr", "identified", "http").await;

    assert_eq!(known.detected, Some("sonarr"));
    assert_eq!(known.status, "identified");

    for (path, status) in [
        ("ambiguous", "reachable_unidentified"),
        ("protected", "authentication_required"),
        ("huge", "reachable_unidentified"),
    ] {
        let result = probe(&http, address, port, "sonarr", path, "http").await;

        assert!(result.detected.is_none());
        assert_eq!(result.status, status);
    }

    let wrong = probe(&http, address, port, "radarr", "identified", "http").await;

    assert!(wrong.detected.is_none());
}

#[tokio::test]
async fn invalid_discovery_inputs_are_rejected_before_network_io() {
    discover(&[], "http", std::time::Instant::now)
        .await
        .expect_err("empty target list");
    let addresses = [std::net::Ipv4Addr::LOCALHOST.into(); 65];

    discover(&addresses, "http", std::time::Instant::now)
        .await
        .expect_err("too many targets");
    discover(&addresses[..1], "ftp", std::time::Instant::now)
        .await
        .expect_err("unsupported scheme");
}
