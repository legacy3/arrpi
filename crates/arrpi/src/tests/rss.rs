//! RSS serialization, scoped cache access and complete-feed failure contracts.

use std::sync::atomic::Ordering;

use axum::{Router, body::to_bytes, http::StatusCode, response::Response};
use rss::Channel;

use crate::{config::Config, proxy::Gateway};

use super::{CLIENT_KEY, EXAMPLE, OTHER_KEY, gateway, mock, proxy::request};

const PATH: &str = "/v1/calendar.rss?start=2026-10-01&end=2026-10-31";

async fn channel(response: Response) -> Channel {
    assert_eq!(response.status(), StatusCode::OK, "authorized RSS response");
    assert_eq!(
        response.headers()["content-type"],
        "application/rss+xml; charset=utf-8",
        "RSS MIME type"
    );
    assert_eq!(response.headers()["cache-control"], "private, no-store");

    let body = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .expect("bounded RSS body");

    Channel::read_from(body.as_ref()).expect("valid RSS XML")
}

#[tokio::test]
async fn rss_preserves_unicode_escapes_xml_and_uses_stable_release_identity() {
    let payload = r#"[{"id":7,"title":"ÄÖÜ & <item> \"quoted\" ]]>\u0000\u001b\ufffe\uffff","digitalRelease":"2026-10-15","imdbId":"tt1234567","airDateUtc":"2026-10-13T23:00:00+02:00","seasonNumber":2,"episodeNumber":3,"series":{"title":"Serie","path":"/private/media"},"hasFile":true}]"#;
    let (server, upstream) = mock(payload).await;
    let app = crate::proxy::router(gateway(&server.url));
    let feed = channel(request(&app, "GET", PATH, Some(OTHER_KEY)).await).await;

    assert_eq!(feed.items().len(), 2, "title cannot inject RSS items");

    let episode = feed
        .items()
        .iter()
        .find(|item| item.guid().expect("GUID").value() == "sonarr-7-episode@arrpi")
        .expect("episode item");

    assert_eq!(
        episode.title(),
        Some("Serie · S02E03 · ÄÖÜ & <item> \"quoted\" ]]>"),
        "Unicode and XML text round trip without illegal controls"
    );
    assert_eq!(episode.pub_date(), Some("Tue, 13 Oct 2026 21:00:00 +0000"));
    assert!(!episode.guid().expect("GUID").is_permalink());
    let movie = feed
        .items()
        .iter()
        .find(|item| item.guid().expect("GUID").value() == "radarr-7-digital@arrpi")
        .expect("movie item");

    assert_eq!(movie.link(), Some("https://www.imdb.com/title/tt1234567/"));
    assert_eq!(movie.pub_date(), Some("Thu, 15 Oct 2026 00:00:00 +0000"));
    assert!(
        movie
            .description()
            .expect("description")
            .contains("Available")
    );
    assert!(!feed.to_string().contains("/private/media"));
    assert!(!feed.to_string().contains(OTHER_KEY));
    assert!(!feed.to_string().contains(super::UPSTREAM_KEY));

    *upstream.body.lock().await = payload
        .replace("2026-10-15", "2026-10-16")
        .replace("ÄÖÜ", "Updated");
    let changed = channel(
        request(
            &app,
            "GET",
            &PATH.replace("10-31", "10-30"),
            Some(OTHER_KEY),
        )
        .await,
    )
    .await;

    assert_eq!(
        feed.items()
            .iter()
            .map(|item| item.guid())
            .collect::<Vec<_>>(),
        changed
            .items()
            .iter()
            .map(|item| item.guid())
            .collect::<Vec<_>>(),
        "title/date updates preserve identity"
    );
}

#[tokio::test]
async fn rss_rejects_auth_methods_and_invalid_queries_before_upstream() {
    let (server, upstream) = mock("[]").await;
    let app = crate::proxy::router(gateway(&server.url));

    for key in [None, Some("fake-invalid-key-000000000000000000000")] {
        assert_eq!(
            request(&app, "GET", PATH, key).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }

    assert_eq!(
        request(&app, "POST", PATH, Some(OTHER_KEY)).await.status(),
        StatusCode::METHOD_NOT_ALLOWED
    );

    for query in [
        "",
        "?start=2026-01-01&end=2026-12-31",
        "?start=2026-10-01&end=2026-10-31&apikey=secret",
        "?start=2026-10-01&end=2026-10-31&start=2026-10-01",
    ] {
        let path = format!("/v1/calendar.rss{query}");

        assert_eq!(
            request(&app, "GET", &path, Some(OTHER_KEY)).await.status(),
            StatusCode::BAD_REQUEST
        );
    }

    assert_eq!(upstream.calls.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn rss_scopes_warm_cache_and_rejects_partial_source_failure() {
    let (healthy, upstream) = mock(r#"[{"id":1,"title":"Film","digitalRelease":"2026-10-15","airDateUtc":"2026-10-15T22:00:00Z"}]"#).await;
    let (failed, failure) = mock("[]").await;

    failure.status.store(503, Ordering::Relaxed);
    let text = EXAMPLE
        .replace("http://radarr:7878/", &healthy.url)
        .replace("http://sonarr:8989/", &failed.url)
        .replace(
            "\"permissions\": [\"sonarr:calendar-view\", \"radarr:calendar-view\"]",
            "\"permissions\": [\"radarr:calendar-view\"]",
        );
    let config = Config::parse(&text).expect("scoped config");
    let app: Router =
        crate::proxy::router(Gateway::new(&config, tokio::time::Instant::now).expect("gateway"));

    assert_eq!(
        request(&app, "GET", PATH, Some(CLIENT_KEY)).await.status(),
        StatusCode::BAD_GATEWAY,
        "partial feed rejected"
    );
    let feed = channel(request(&app, "GET", PATH, Some(OTHER_KEY)).await).await;

    assert_eq!(feed.items().len(), 1);
    assert_eq!(
        feed.items()[0].guid().expect("GUID").value(),
        "radarr-1-digital@arrpi"
    );
    assert_eq!(
        upstream.calls.load(Ordering::Relaxed),
        1,
        "authorized warm cache reused"
    );
    assert_eq!(
        failure.calls.load(Ordering::Relaxed),
        1,
        "unauthorized source not queried"
    );
    assert_eq!(
        request(&app, "GET", PATH, None).await.status(),
        StatusCode::UNAUTHORIZED,
        "warm cache remains protected"
    );
}
