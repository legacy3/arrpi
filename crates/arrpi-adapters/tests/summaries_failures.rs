//! Pagination, error provenance and operational freshness are independent of happy-path fixtures.
use arrpi_core::{config::Config, error::ApiError, source::ReadMode, summaries::SummaryKind};
use axum::{Json, Router, extract::Query, response::IntoResponse as _, routing::get};
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

struct Server(tokio::task::JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn serve(router: Router) -> (Server, String) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let address = listener.local_addr().expect("address");
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.expect("fixture");
    });

    (Server(task), format!("http://{address}/"))
}

fn settings(url: &str, kind: &str) -> Config {
    serde_json::from_value(json!({"listen":"127.0.0.1:0","sources":{"test":{"kind":kind,"url":url,"api_key":"fake-native-key"}},"clients":[{"name":"reader","key":"fake-reader-0000000000000000000000","permissions":["test:queue-summary","test:status-view"]}]})).expect("settings")
}

#[tokio::test]
async fn first_page_auth_rejection_is_preserved() {
    let (_server, url) = serve(Router::new().route(
        "/api/v3/queue",
        get(|| async { axum::http::StatusCode::FORBIDDEN }),
    ))
    .await;
    let built = arrpi_adapters::build(&settings(&url, "sonarr"), tokio::time::Instant::now)
        .expect("sources");
    let error = built
        .sources
        .source("test")
        .expect("source")
        .summaries()
        .expect("summary")
        .summary(SummaryKind::Queue, ReadMode::Fresh)
        .await
        .expect_err("auth error");

    assert_eq!(error, ApiError::FORBIDDEN);
}

#[tokio::test]
async fn duplicate_queue_pages_remain_incomplete() {
    let router = Router::new().route(
        "/api/v3/queue",
        get(|Query(query): Query<BTreeMap<String, String>>| async move {
            let page: usize = query["page"].parse().expect("page");
            let records: Vec<_> = if page < 3 {
                (0..100)
                    .map(|id| json!({"id":id,"downloadClient":"SABnzbd","downloadId":format!("fake-{id}"),"size":100.5,"sizeleft":50.25,"status":"queued"}))
                    .collect()
            } else {
                vec![]
            };

            Json(json!({"page":page,"pageSize":100,"totalRecords":200,"records":records}))
                .into_response()
        }),
    );
    let (_server, url) = serve(router).await;
    let built = arrpi_adapters::build(&settings(&url, "radarr"), tokio::time::Instant::now)
        .expect("sources");
    let result = built
        .sources
        .source("test")
        .expect("source")
        .summaries()
        .expect("summary")
        .summary(SummaryKind::Queue, ReadMode::Fresh)
        .await
        .expect("partial");

    assert_eq!(result.data["complete"], false);
    assert_eq!(result.data["reason"], "queue_changed");
    assert_eq!(result.data["items_observed"], 100);
    assert_eq!(result.data["remaining_bytes_observed"], 5025.0);
}

static CLOCK_OFFSET: AtomicU64 = AtomicU64::new(0);
fn shifted_clock() -> tokio::time::Instant {
    tokio::time::Instant::now()
        + std::time::Duration::from_secs(CLOCK_OFFSET.load(Ordering::Relaxed))
}

#[tokio::test]
async fn season_pack_bytes_are_counted_once_and_unknown_downloads_stay_explicit() {
    let router = Router::new().route("/api/v3/queue", get(|| async {
        Json(json!({"page":1,"pageSize":100,"totalRecords":3,"records":[
            {"id":1,"downloadClient":"SABnzbd","downloadId":"pack-1","size":100.5,"sizeleft":50.25,"status":"downloading"},
            {"id":2,"downloadClient":"SABnzbd","downloadId":"pack-1","size":100.5,"sizeleft":50.25,"status":"downloading"},
            {"id":3,"size":200.0,"sizeleft":100.0,"status":"queued"}
        ]}))
    }));
    let (_server, url) = serve(router).await;
    let built = arrpi_adapters::build(&settings(&url, "sonarr"), tokio::time::Instant::now)
        .expect("sources");
    let result = built
        .sources
        .source("test")
        .expect("source")
        .summaries()
        .expect("summary")
        .summary(SummaryKind::Queue, ReadMode::Fresh)
        .await
        .expect("queue");

    assert_eq!(result.data["complete"], true);
    assert_eq!(result.data["items_observed"], 3);
    assert_eq!(result.data["downloads_observed"], 1);
    assert_eq!(result.data["size_bytes_observed"], 100.5);
    assert_eq!(result.data["remaining_bytes_observed"], 50.25);
    assert_eq!(result.data["rows_without_download_identity"], 1);
    assert_eq!(result.data["byte_counts_complete"], false);
}

#[tokio::test]
async fn operational_cache_expires_after_fifteen_seconds_and_never_serves_stale() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let router = Router::new().route(
        "/api/badges",
        get(move || {
            let counted = Arc::clone(&counted);

            async move {
                if counted.fetch_add(1, Ordering::Relaxed) == 0 {
                    Json(json!({"episodes":1,"movies":0,"providers":0,"status":0})).into_response()
                } else {
                    axum::http::StatusCode::SERVICE_UNAVAILABLE.into_response()
                }
            }
        }),
    );
    let (_server, url) = serve(router).await;
    let built = arrpi_adapters::build(&settings(&url, "bazarr"), shifted_clock).expect("sources");
    let summary = built
        .sources
        .source("test")
        .expect("source")
        .summaries()
        .expect("summary");

    summary
        .summary(SummaryKind::Status, ReadMode::Cached)
        .await
        .expect("initial");
    summary
        .summary(SummaryKind::Status, ReadMode::Cached)
        .await
        .expect("cached");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    CLOCK_OFFSET.store(16, Ordering::Relaxed);
    summary
        .summary(SummaryKind::Status, ReadMode::Cached)
        .await
        .expect_err("expired operational counters never serve stale");
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    CLOCK_OFFSET.store(0, Ordering::Relaxed);
}
