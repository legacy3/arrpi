//! Native forwarding, privacy, aggregation and outage contracts over local HTTP.

use std::sync::atomic::Ordering;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::Response,
};
use serde_json::Value;
use tower::ServiceExt as _;

use crate::proxy::router;

use super::{CLIENT_KEY, OTHER_KEY, UPSTREAM_KEY, gateway, mock};

#[derive(Debug)]
struct Chunked {
    frames: std::collections::VecDeque<bytes::Bytes>,
}

impl http_body::Body for Chunked {
    type Data = bytes::Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        std::task::Poll::Ready(
            self.frames
                .pop_front()
                .map(|body| Ok(http_body::Frame::data(body))),
        )
    }
}

pub(super) async fn request(
    router: &Router,
    method: &str,
    path: &str,
    key: Option<&str>,
) -> Response {
    let mut request = Request::builder().method(method).uri(path);

    if let Some(key) = key {
        request = request.header("authorization", format!("Bearer {key}"));
    }

    router
        .clone()
        .oneshot(request.body(Body::empty()).expect("fixture request"))
        .await
        .expect("router is infallible")
}

pub(super) async fn json(response: Response) -> Value {
    let body = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .expect("bounded fixture body");

    serde_json::from_slice(&body).expect("JSON response")
}

#[tokio::test]
async fn auth_and_methods_are_checked_before_upstream() {
    let (server, upstream) = mock("[]").await;
    let router = router(gateway(&server.url));

    for (method, path, key, expected) in [
        ("GET", "/health", None, StatusCode::OK),
        (
            "GET",
            "/v1/sonarr/api/v3/series",
            None,
            StatusCode::UNAUTHORIZED,
        ),
        ("GET", "/metrics", None, StatusCode::UNAUTHORIZED),
        (
            "GET",
            "/v1/sonarr/api/v3/series",
            Some("fake-invalid-client-0000000000000000"),
            StatusCode::UNAUTHORIZED,
        ),
        (
            "GET",
            "/v1/sonarr/api/v3/series",
            Some(OTHER_KEY),
            StatusCode::FORBIDDEN,
        ),
        (
            "POST",
            "/v1/sonarr/api/v3/series",
            Some(CLIENT_KEY),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "DELETE",
            "/v1/sonarr/api/v3/series",
            Some(CLIENT_KEY),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "GET",
            "/v1/sonarr/api/v3/command",
            Some(CLIENT_KEY),
            StatusCode::FORBIDDEN,
        ),
        (
            "GET",
            "/v1/sonarr/api/v3/config/host",
            Some(CLIENT_KEY),
            StatusCode::NOT_FOUND,
        ),
    ] {
        let response = request(&router, method, path, key).await;

        assert_eq!(response.status(), expected, "{method} {path}");
    }

    assert_eq!(
        upstream.calls.load(Ordering::Relaxed),
        0,
        "denied requests and liveness never reach upstream"
    );
}

#[tokio::test]
async fn native_json_preserved_headers_stripped_and_source_cache_isolated() {
    let payload = "[ {\"id\":1,\"title\":\"Example\",\"unknownField\":true} ]";
    let (server, upstream) = mock(payload).await;
    let router = router(gateway(&format!("{}base/", server.url)));
    let first = request(&router, "GET", "/v1/sonarr/api/v3/series", Some(CLIENT_KEY)).await;

    assert_eq!(first.status(), StatusCode::OK, "native request succeeds");
    assert_eq!(first.headers()["x-arrpi-cache"], "miss", "cold request");
    assert_eq!(
        first.headers()["cache-control"],
        "private, no-store",
        "browser caching disabled"
    );
    assert!(
        first.headers().get("set-cookie").is_none(),
        "native sessions not forwarded"
    );
    assert!(
        first.headers().get("location").is_none(),
        "native origin not leaked"
    );

    let body = to_bytes(first.into_body(), 1024)
        .await
        .expect("bounded response");

    assert_eq!(body, payload, "native JSON bytes retained");

    let hit = request(&router, "GET", "/v1/sonarr/api/v3/series", Some(CLIENT_KEY)).await;
    let other = request(&router, "GET", "/v1/radarr/api/v3/movie", Some(CLIENT_KEY)).await;

    assert_eq!(
        hit.headers()["x-arrpi-cache"],
        "hit",
        "second read shared cache"
    );
    assert_eq!(
        other.headers()["x-arrpi-cache"],
        "miss",
        "source identity retained"
    );
    assert_eq!(
        upstream.calls.load(Ordering::Relaxed),
        2,
        "two source/endpoint keys"
    );

    let seen = upstream.seen.lock().await;

    assert!(
        seen[0].0.starts_with("/base/api/v3/series"),
        "configured reverse-proxy base path respected"
    );
    assert_eq!(
        seen[0].1["x-api-key"], UPSTREAM_KEY,
        "native key injected only upstream"
    );
    assert!(
        seen[0].1.get("authorization").is_none(),
        "client bearer never reaches upstream"
    );
}

#[tokio::test]
async fn redirects_oversize_and_invalid_json_are_not_cached() {
    for (status, payload) in [
        (302, "[]".to_owned()),
        (200, "not JSON".to_owned()),
        (200, "x".repeat(1024 * 1024 + 1)),
    ] {
        let (server, upstream) = mock(&payload).await;

        upstream.status.store(status, Ordering::Relaxed);

        let router = router(gateway(&server.url));
        let response = request(&router, "GET", "/v1/sonarr/api/v3/series", Some(CLIENT_KEY)).await;

        assert_eq!(
            response.status(),
            StatusCode::BAD_GATEWAY,
            "bad upstream response rejected"
        );

        let data = json(response).await;

        assert!(
            !data.to_string().contains(&server.url),
            "error does not reveal upstream URL"
        );
        assert!(
            !data.to_string().contains(UPSTREAM_KEY),
            "error does not reveal native key"
        );
        assert_eq!(
            upstream.calls.load(Ordering::Relaxed),
            1,
            "redirect target never requested"
        );
    }
}

#[tokio::test]
async fn chunked_native_response_without_content_length_still_obeys_byte_limit() {
    let native = super::server(Router::new().route(
        "/api/v3/series",
        axum::routing::get(|| async {
            Body::new(Chunked {
                frames: [
                    bytes::Bytes::from(format!("\"{}", "x".repeat(600))),
                    bytes::Bytes::from(format!("{}\"", "x".repeat(600))),
                ]
                .into(),
            })
        }),
    ))
    .await;
    let text = super::EXAMPLE
        .replace("http://sonarr:8989/", &native.url)
        .replace("http://radarr:7878/", &native.url)
        .replace("\"max_body_bytes\": 1048576", "\"max_body_bytes\": 1024");
    let config = crate::config::Config::parse(&text).expect("small stream limit");
    let app =
        router(crate::proxy::Gateway::new(&config, tokio::time::Instant::now).expect("gateway"));
    let response = request(&app, "GET", "/v1/sonarr/api/v3/series", Some(CLIENT_KEY)).await;

    assert_eq!(
        response.status(),
        StatusCode::BAD_GATEWAY,
        "chunked body cannot evade the byte cap"
    );
    assert_eq!(
        json(response).await["error"],
        "upstream response exceeds cache.max_body_bytes; raise the configured limit for a larger library",
        "reject oversize before JSON parsing"
    );
}

#[tokio::test]
async fn calendar_normalizes_dates_and_omits_native_paths() {
    let payload = r#"[{"id":7,"title":"Folge","airDateUtc":"2026-10-13T21:00:00Z","seasonNumber":2,"episodeNumber":3,"series":{"title":"Beispiel","path":"/private/media"},"inCinemas":"2026-10-12T00:00:00Z","digitalRelease":"2026-10-15","physicalRelease":"2026-10-20T00:00:00Z","tmdbId":99,"monitored":true},{"id":8,"title":"Unmonitored","airDateUtc":"2026-10-15T21:00:00Z","inCinemas":"2026-10-15","monitored":false}]"#;
    let (server, upstream) = mock(payload).await;
    let router = router(gateway(&server.url));
    let response = request(
        &router,
        "GET",
        "/v1/calendar?start=2026-10-01&end=2026-11-01",
        Some(OTHER_KEY),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK, "both calendars succeed");

    let data = json(response).await;

    assert_eq!(
        data["events"].as_array().expect("events array").len(),
        4,
        "three movie dates and one episode"
    );
    assert_eq!(
        data["sources"].as_array().expect("sources array").len(),
        2,
        "independent source freshness"
    );
    assert!(
        !data.to_string().contains("/private"),
        "minimal calendar omits native filesystem paths"
    );
    assert!(
        !data.to_string().contains("Unmonitored"),
        "unmonitored records excluded"
    );
    assert_eq!(
        upstream.calls.load(Ordering::Relaxed),
        2,
        "one request per source"
    );

    let seen = upstream.seen.lock().await;

    assert!(
        seen.iter()
            .all(|(path, _)| path.contains("unmonitored=false")
                && path.contains("includeSeries=true")),
        "mandatory native query flags"
    );
}

#[tokio::test]
async fn partial_calendar_and_permissions_preserve_only_authorized_healthy_sources() {
    let (healthy, _) =
        mock(r#"[{"id":1,"title":"Film","digitalRelease":"2026-10-15","monitored":true}]"#).await;
    let (broken, failure) = mock("upstream failed").await;

    failure.status.store(503, Ordering::Relaxed);

    let config = super::EXAMPLE
        .replace("http://sonarr:8989/", &broken.url)
        .replace("http://radarr:7878/", &healthy.url);

    let gateway = crate::proxy::Gateway::new(
        &crate::config::Config::parse(&config).expect("mixed fixture config"),
        tokio::time::Instant::now,
    )
    .expect("mixed gateway");

    let router = router(gateway);
    let data = json(
        request(
            &router,
            "GET",
            "/v1/calendar?start=2026-10-01&end=2026-11-01",
            Some(OTHER_KEY),
        )
        .await,
    )
    .await;

    assert_eq!(
        data["events"].as_array().expect("events").len(),
        1,
        "healthy movie survives TV outage"
    );
    assert!(
        data["sources"]
            .as_array()
            .expect("sources")
            .iter()
            .any(|source| source["status"] == "unavailable"),
        "outage is explicit"
    );

    let restricted = config.replace(
        "\"permissions\": [\"sonarr:calendar-view\", \"radarr:calendar-view\"]",
        "\"permissions\": [\"radarr:calendar-view\"]",
    );
    let restricted = crate::proxy::Gateway::new(
        &crate::config::Config::parse(&restricted).expect("restricted config"),
        tokio::time::Instant::now,
    )
    .expect("restricted gateway");
    let router = crate::proxy::router(restricted);
    let data = json(
        request(
            &router,
            "GET",
            "/v1/calendar?start=2026-10-01&end=2026-11-01",
            Some(OTHER_KEY),
        )
        .await,
    )
    .await;

    assert_eq!(
        data["sources"].as_array().expect("sources").len(),
        1,
        "no unauthorized source metadata"
    );
    assert_eq!(
        failure.calls.load(Ordering::Relaxed),
        1,
        "restricted reader never calls unauthorized source"
    );
}

#[tokio::test]
async fn sanitized_calendar_permission_cannot_read_raw_metadata_or_global_metrics() {
    let (server, upstream) = mock("[]").await;
    let router = router(gateway(&server.url));

    for path in [
        "/v1/sonarr/api/v3/calendar?start=2026-10-01&end=2026-10-31",
        "/metrics",
    ] {
        let response = request(&router, "GET", path, Some(OTHER_KEY)).await;

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "calendar-view cannot read privileged data"
        );
    }

    assert_eq!(
        upstream.calls.load(Ordering::Relaxed),
        0,
        "unauthorized requests never touch cache or upstream"
    );
}

#[tokio::test]
async fn response_body_retains_admission_until_consumed_or_dropped() {
    let (server, _) = mock("[]").await;
    let text = super::EXAMPLE
        .replace("http://sonarr:8989/", &server.url)
        .replace("http://radarr:7878/", &server.url)
        .replace("\"max_requests\": 64", "\"max_requests\": 1");
    let config = crate::config::Config::parse(&text).expect("one request admission");
    let router =
        router(crate::proxy::Gateway::new(&config, tokio::time::Instant::now).expect("gateway"));
    let first = request(&router, "GET", "/v1/sonarr/api/v3/series", Some(CLIENT_KEY)).await;
    let busy = request(&router, "GET", "/v1/radarr/api/v3/movie", Some(CLIENT_KEY)).await;

    assert_eq!(
        busy.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "completed handler still holds body admission"
    );
    drop(first);

    let recovered = request(&router, "GET", "/v1/radarr/api/v3/movie", Some(CLIENT_KEY)).await;

    assert_eq!(
        recovered.status(),
        StatusCode::OK,
        "dropping body recovers admission"
    );
}

#[tokio::test]
async fn http_no_cache_forces_fetch_and_counters_remain_scoped() {
    let (server, upstream) = mock("[]").await;
    let config = super::EXAMPLE
        .replace("http://sonarr:8989/", &server.url)
        .replace("http://radarr:7878/", &server.url)
        .replace(
            "\"permissions\": [\"sonarr:calendar-view\", \"radarr:calendar-view\"]",
            "\"permissions\": [\"radarr:calendar-view\"]",
        );
    let router = router(
        crate::proxy::Gateway::new(
            &crate::config::Config::parse(&config).expect("scoped fixture"),
            tokio::time::Instant::now,
        )
        .expect("gateway"),
    );

    json(request(&router, "GET", "/v1/sonarr/api/v3/series", Some(CLIENT_KEY)).await).await;

    let fresh = Request::builder()
        .uri("/v1/sonarr/api/v3/series")
        .header("authorization", format!("Bearer {CLIENT_KEY}"))
        .header("cache-control", "no-cache")
        .body(Body::empty())
        .expect("force request");
    let response = router
        .clone()
        .oneshot(fresh)
        .await
        .expect("router response");

    assert_eq!(
        response.headers()["x-arrpi-cache"],
        "miss",
        "HTTP cache directive forces fetch"
    );
    drop(response);

    let data = json(request(&router, "GET", "/v1/status", Some(OTHER_KEY)).await).await;

    assert_eq!(
        data["sources"].as_array().expect("sources").len(),
        1,
        "only authorized source inventory"
    );
    assert_eq!(
        data["sources"][0]["counters"]["misses"], 0,
        "other source requests do not leak into counters"
    );
    assert_eq!(
        upstream.calls.load(Ordering::Relaxed),
        2,
        "force directive reached gateway cache policy"
    );
}

#[tokio::test]
async fn hung_source_does_not_starve_another_and_timeout_recovers() {
    let (slow_server, slow) = mock("[]").await;
    let (healthy_server, _) = mock("[]").await;

    slow.delay_millis.store(10_000, Ordering::Relaxed);

    let text = super::EXAMPLE
        .replace("http://sonarr:8989/", &slow_server.url)
        .replace("http://radarr:7878/", &healthy_server.url)
        .replace("\"timeout_seconds\": 10", "\"timeout_seconds\": 1")
        .replace(
            "\"max_upstream_requests\": 4",
            "\"max_upstream_requests\": 1",
        );
    let router = router(
        crate::proxy::Gateway::new(
            &crate::config::Config::parse(&text).expect("isolation config"),
            tokio::time::Instant::now,
        )
        .expect("gateway"),
    );
    let slow_router = router.clone();
    let job = tokio::spawn(async move {
        request(
            &slow_router,
            "GET",
            "/v1/sonarr/api/v3/series",
            Some(CLIENT_KEY),
        )
        .await
    });

    slow.entered.notified().await;

    let healthy = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        request(&router, "GET", "/v1/radarr/api/v3/movie", Some(CLIENT_KEY)),
    )
    .await
    .expect("healthy source stays responsive");

    assert_eq!(
        healthy.status(),
        StatusCode::OK,
        "per-source permits isolate failure"
    );
    drop(healthy);

    let overloaded = request(&router, "GET", "/v1/sonarr/api/v3/queue", Some(CLIENT_KEY)).await;

    assert_eq!(
        overloaded.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "bounded wait rejects exhausted source"
    );
    assert_eq!(
        job.await.expect("slow job finishes").status(),
        StatusCode::BAD_GATEWAY,
        "total upstream timeout enforced"
    );
    slow.delay_millis.store(0, Ordering::Relaxed);

    let recovered = request(&router, "GET", "/v1/sonarr/api/v3/health", Some(CLIENT_KEY)).await;

    assert_eq!(
        recovered.status(),
        StatusCode::OK,
        "timeout releases upstream permit"
    );
}
