//! Calendar metadata and iCalendar privacy, date, identity and outage contracts.

use std::sync::atomic::Ordering;

use axum::{body::to_bytes, http::StatusCode};

use crate::proxy::router;

use super::{
    OTHER_KEY, gateway, mock,
    proxy::{json, request},
};

#[tokio::test]
async fn calendar_exposes_valid_public_ids_and_availability_without_native_details() {
    let (server, upstream) = mock(r#"[{"id":1,"title":"Film","imdbId":"tt1234567","tmdbId":42,"hasFile":true,"digitalRelease":"2026-10-15","airDateUtc":"2026-10-15T22:00:00Z","series":{"title":"Serie","imdbId":"tt2345678","tvdbId":99,"path":"/secret"}},{"id":2,"title":"Invalid","imdbId":"tt1234567/evil","digitalRelease":"2026-10-16"}]"#).await;
    let app = router(gateway(&server.url));
    let data = json(
        request(
            &app,
            "GET",
            "/v1/calendar?start=2026-10-01&end=2026-10-31",
            Some(OTHER_KEY),
        )
        .await,
    )
    .await;
    let events = data["events"].as_array().expect("events");
    let seen = upstream.seen.lock().await;

    assert!(
        seen.iter()
            .all(|(url, _)| url.contains("end=2026-10-31T23%3A59%3A59.9999999Z")),
        "inclusive last-day native timestamp"
    );

    assert!(
        events
            .iter()
            .any(|event| event["imdb_id"] == "tt2345678" && event["tvdb_id"] == 99),
        "series public IDs retained"
    );
    assert!(
        events
            .iter()
            .any(|event| event["imdb_id"] == "tt1234567" && event["has_file"] == true),
        "movie availability retained"
    );
    assert!(!data.to_string().contains("evil"), "malformed ID excluded");
    assert!(
        !data.to_string().contains("/secret"),
        "native paths excluded"
    );
}

#[tokio::test]
async fn icalendar_uses_scoped_auth_stable_uids_utc_and_all_day_dates() {
    let (server, _) = mock(r#"[{"id":7,"title":"ÄÖÜ \\ & quoted\u0000\u001b\r\nBEGIN:VEVENT","imdbId":"tt1234567","digitalRelease":"2026-10-15","airDateUtc":"2026-10-13T23:00:00+02:00","series":{"title":"Serie","imdbId":"tt2345678"},"seasonNumber":2,"episodeNumber":3}]"#).await;
    let app = router(gateway(&server.url));
    let path = "/v1/calendar.ics?start=2026-10-01&end=2026-10-31";

    assert_eq!(
        request(&app, "GET", path, None).await.status(),
        StatusCode::UNAUTHORIZED,
        "feed requires credentials"
    );
    assert_eq!(
        request(
            &app,
            "GET",
            path,
            Some("fake-invalid-reader-key-0000000000000")
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED,
        "invalid key rejected"
    );
    assert_eq!(
        request(&app, "POST", path, Some(OTHER_KEY)).await.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "read only feed"
    );

    let response = request(&app, "GET", path, Some(OTHER_KEY)).await;

    assert_eq!(response.status(), StatusCode::OK, "authorized feed");
    assert_eq!(
        response.headers()["content-type"],
        "text/calendar; charset=utf-8",
        "calendar MIME type"
    );
    assert_eq!(
        response.headers()["cache-control"],
        "private, no-store",
        "private export"
    );

    let body = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .expect("bounded feed");
    let text = std::str::from_utf8(&body).expect("UTF-8 feed");

    assert_eq!(
        text.matches("\r\nPRODID:").count(),
        1,
        "one RFC 5545 product identifier"
    );

    assert!(
        text.contains("DTSTART:20261013T210000Z"),
        "episode normalized to UTC"
    );
    assert!(
        text.contains("DTSTART;VALUE=DATE:20261015"),
        "movie remains all day"
    );
    assert!(
        text.contains("UID:sonarr-7-episode@arrpi"),
        "UID independent of title or date"
    );
    assert!(
        text.contains("URL:https://www.imdb.com/title/tt1234567/"),
        "validated public title link"
    );
    assert_eq!(
        text.matches("\r\nBEGIN:VEVENT\r\n").count(),
        2,
        "title cannot inject components"
    );
    assert!(!text.contains("/api/"), "native URLs absent");
}

#[tokio::test]
async fn icalendar_rejects_invalid_ranges_and_partial_source_outages() {
    let (server, upstream) = mock("[]").await;
    let app = router(gateway(&server.url));

    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/calendar.ics?start=2026-01-01&end=2026-12-31",
            Some(OTHER_KEY)
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST,
        "bounded range"
    );
    assert_eq!(
        upstream.calls.load(Ordering::Relaxed),
        0,
        "reject before native request"
    );

    upstream.status.store(503, Ordering::Relaxed);

    assert_eq!(
        request(
            &app,
            "GET",
            "/v1/calendar.ics?start=2026-10-01&end=2026-10-31",
            Some(OTHER_KEY)
        )
        .await
        .status(),
        StatusCode::BAD_GATEWAY,
        "outage cannot publish an empty successful feed"
    );
}
