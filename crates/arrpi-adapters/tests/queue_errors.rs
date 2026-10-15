//! Queue failures retain provenance, successful observations and admission boundaries.

use arrpi_adapters::BuiltSources;
use arrpi_core::{
    Commands,
    admission::{Lease, Normalizations},
    arguments::Fresh,
    config::Config,
    error::ApiError,
    executor::Recording,
    source::{Principal, ReadMode, SourceRead},
    summaries::SummaryKind,
};
use axum::{
    Json, Router,
    extract::Query,
    http::StatusCode,
    response::IntoResponse as _,
    routing::{get, post},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

struct Server(tokio::task::JoinHandle<()>);

impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

struct Fixture {
    _server: Server,
    config: Config,
    built: BuiltSources,
}

impl Fixture {
    async fn new(router: Router) -> Self {
        Self::for_kind(router, "sonarr").await
    }

    async fn for_kind(router: Router, kind: &str) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("fixture listener");
        let address = listener.local_addr().expect("fixture address");
        let server = Server(tokio::spawn(async move {
            axum::serve(listener, router)
                .await
                .expect("fixture service");
        }));
        let mut config: Config = serde_json::from_value(json!({
            "listen":"127.0.0.1:0",
            "sources":{"test":{"kind":kind,"url":format!("http://{address}/"),"api_key":"fake-upstream-key"}},
            "clients":[{"name":"reader","key":"fake-reader-0000000000000000000000","permissions":["test:queue-summary"]}]
        }))
        .expect("fake settings");

        config.cache.max_normalizations = 1;
        let built = arrpi_adapters::build(&config, tokio::time::Instant::now).expect("sources");

        Self {
            _server: server,
            config,
            built,
        }
    }

    async fn queue(&self) -> Result<SourceRead<Value>, ApiError> {
        let read = self
            .built
            .sources
            .source("test")
            .expect("source")
            .summaries()
            .expect("queue capability")
            .summary(SummaryKind::Queue, ReadMode::Cached)
            .await?;

        Ok(SourceRead {
            data: serde_json::to_value(read.data)?,
            freshness: read.freshness,
        })
    }
}

#[tokio::test]
async fn first_page_rate_limit_remains_distinct_from_unavailable() {
    let fixture = Fixture::new(Router::new().route(
        "/api/v3/queue",
        get(|| async { StatusCode::TOO_MANY_REQUESTS }),
    ))
    .await;

    assert_eq!(
        fixture.queue().await.expect_err("rate limited"),
        ApiError::UPSTREAM_RATE_LIMITED
    );
}

#[tokio::test]
async fn later_page_failure_keeps_observed_counts_and_page_freshness() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let fixture = Fixture::new(Router::new().route(
        "/api/v3/queue",
        get(move |Query(query): Query<BTreeMap<String, String>>| {
            let counted = Arc::clone(&counted);

            async move {
                counted.fetch_add(1, Ordering::SeqCst);
                assert_eq!(query["sortKey"], "added");

                if query["page"] == "2" {
                    return StatusCode::SERVICE_UNAVAILABLE.into_response();
                }

                let rows: Vec<_> = (0..100).map(|id| json!({"id":id,"size":100.5,"sizeleft":40.25,"downloadClient":"SABnzbd","downloadId":format!("fake-{id}"),"status":"queued"})).collect();

                Json(json!({"page":1,"pageSize":100,"totalRecords":200,"records":rows}))
                    .into_response()
            }
        }),
    ))
    .await;
    let read = fixture.queue().await.expect("partial observations");

    assert_eq!(read.data["complete"], false);
    assert_eq!(read.data["reason"], "upstream_unavailable");
    assert_eq!(read.data["items_observed"], 100);
    assert_eq!(read.data["total_reported"], 200);
    assert_eq!(read.data["size_bytes_observed"], 10050.0);
    assert_eq!(read.data["remaining_bytes_observed"], 4025.0);
    assert_eq!(read.data["pages"].as_array().expect("provenance").len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

fn empty_queue(calls: Arc<AtomicUsize>) -> Router {
    Router::new().route(
        "/api/v3/queue",
        get(move || {
            let calls = Arc::clone(&calls);

            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                Json(json!({"page":1,"pageSize":100,"totalRecords":0,"records":[]}))
            }
        }),
    )
}

#[tokio::test]
async fn missing_summary_grants_reject_before_network_and_cache_lookup() {
    let calls = Arc::new(AtomicUsize::new(0));
    let fixture = Fixture::new(empty_queue(Arc::clone(&calls))).await;
    let commands = Commands::new(
        &fixture.config,
        fixture.built.sources.clone(),
        Recording::new(None, tokio::time::Instant::now),
        fixture.built.normalizations.clone(),
    );
    let mut principal = Principal {
        name: "reader".to_owned(),
        binding: None,
        permissions: ["test:library-view".into()].into(),
    };

    commands
        .summaries(&principal, SummaryKind::Queue, Fresh { no_cache: false })
        .await
        .expect_err("library metadata does not grant operational access");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    principal.permissions.insert("test:queue-summary".into());
    commands
        .summaries(&principal, SummaryKind::Queue, Fresh { no_cache: false })
        .await
        .expect("authorized cache fill");
    principal.permissions.clear();
    let error = commands
        .summaries(&principal, SummaryKind::Queue, Fresh { no_cache: false })
        .await
        .expect_err("a warm cache never grants access");

    assert_eq!(error, ApiError::FORBIDDEN);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        commands
            .sources
            .source("test")
            .expect("source")
            .counters()
            .hits,
        0,
        "denied access never reaches a warm cache"
    );
}

#[tokio::test]
async fn normalization_busy_is_retry_safe_and_does_not_hold_a_network_slot() {
    let calls = Arc::new(AtomicUsize::new(0));
    let fixture = Fixture::new(empty_queue(Arc::clone(&calls))).await;
    let lease = fixture
        .built
        .normalizations
        .try_acquire()
        .expect("occupy the shared CPU slot");
    let error = fixture.queue().await.expect_err("normalization capacity");

    assert_eq!(error, ApiError::BUSY);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "I/O precedes CPU admission"
    );
    drop(lease);
    let read = fixture
        .queue()
        .await
        .expect("recover with retained native bytes");

    assert_eq!(read.data["complete"], true);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "retry reuses the cached body"
    );
}

fn rows(start: u64, end: u64) -> Vec<Value> {
    (start..end).map(|id| json!({"id":id,"size":100.5,"sizeleft":40.25,"downloadClient":"SABnzbd","downloadId":format!("fake-{id}"),"status":"queued"})).collect()
}

#[tokio::test]
async fn shrinking_queue_does_not_recount_the_controllers_reset_page() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let fixture = Fixture::new(Router::new().route(
        "/api/v3/queue",
        get(move |Query(query): Query<BTreeMap<String, String>>| {
            counted.fetch_add(1, Ordering::SeqCst);
            async move {
                let total = if query["page"] == "1" { 101 } else { 100 };

                Json(json!({"page":1,"pageSize":100,"totalRecords":total,"records":rows(0,100)}))
            }
        }),
    ))
    .await;
    let read = fixture.queue().await.expect("observed prefix");

    assert_eq!(read.data["complete"], false);
    assert_eq!(read.data["reason"], "queue_changed");
    assert_eq!(read.data["items_observed"], 100);
    assert_eq!(read.data["size_bytes_observed"], 10050.0);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn later_normalization_pressure_preserves_the_successful_prefix() {
    let pool = Arc::new(Mutex::new(None::<Normalizations>));
    let held = Arc::new(Mutex::new(None::<Lease>));
    let calls = Arc::new(AtomicUsize::new(0));
    let occupy = Arc::clone(&held);
    let shared = Arc::clone(&pool);
    let counted = Arc::clone(&calls);
    let fixture = Fixture::new(Router::new().route(
        "/api/v3/queue",
        get(move |Query(query): Query<BTreeMap<String, String>>| {
            counted.fetch_add(1, Ordering::SeqCst);
            let page: u64 = query["page"].parse().expect("page");

            if page == 2 {
                let lease = shared.lock().expect("pool lock").as_ref().expect("configured pool").try_acquire().expect("first page released its CPU lease");

                *occupy.lock().expect("held lock") = Some(lease);
            }

            async move {
                Json(json!({"page":page,"pageSize":100,"totalRecords":200,"records":rows((page-1)*100,page*100)}))
            }
        }),
    )).await;

    *pool.lock().expect("pool lock") = Some(fixture.built.normalizations.clone());
    let read = fixture
        .queue()
        .await
        .expect("successful page remains available");

    assert_eq!(read.data["complete"], false);
    assert_eq!(read.data["reason"], "busy");
    assert_eq!(read.data["items_observed"], 100);
    assert_eq!(read.data["remaining_bytes_observed"], 4025.0);
    drop(held.lock().expect("held lock").take());
    let retry = fixture
        .queue()
        .await
        .expect("cached retry after CPU pressure");

    assert_eq!(retry.data["complete"], true);
    assert_eq!(retry.data["items_observed"], 200);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "both raw pages survive local pressure"
    );
}

#[tokio::test]
async fn native_pages_cannot_allocate_more_than_one_hundred_records() {
    let fixture = Fixture::new(Router::new().route(
        "/api/v3/queue",
        get(|| async {
            Json(json!({"page":1,"pageSize":100,"totalRecords":101,"records":rows(0,101)}))
        }),
    ))
    .await;

    assert_eq!(
        fixture.queue().await.expect_err("bounded native records"),
        ApiError::INVALID_RESPONSE
    );
}

#[tokio::test]
async fn sab_error_shaped_success_is_rejected_without_leaking_native_details() {
    let fixture = Fixture::for_kind(
        Router::new().route(
            "/api",
            post(|| async { Json(json!({"status":false,"error":"private-upstream-detail"})) }),
        ),
        "sabnzbd",
    )
    .await;
    let error = fixture
        .built
        .sources
        .source("test")
        .expect("source")
        .summaries()
        .expect("summary capability")
        .summary(SummaryKind::Status, ReadMode::Fresh)
        .await
        .expect_err("native failure");

    assert_eq!(error, ApiError::REJECTED);
    assert!(!format!("{error:?}").contains("private-upstream-detail"));
}

#[tokio::test]
async fn sab_unknown_or_missing_version_is_an_invalid_read_contract() {
    for version in [Value::Null, json!("3.7.2")] {
        let fixture = Fixture::for_kind(Router::new().route(
            "/api", post(move || {
                let version = version.clone();

                async move {
                    Json(json!({"queue":{"version":version,"noofslots":0,"noofslots_total":0,"paused":false,"kbpersec":"0","mbleft":"0"}}))
                }
            }),
        ), "sabnzbd").await;
        let error = fixture
            .built
            .sources
            .source("test")
            .expect("source")
            .summaries()
            .expect("summary capability")
            .summary(SummaryKind::Status, ReadMode::Fresh)
            .await
            .expect_err("unsupported native read shape");

        assert_eq!(error, ApiError::INVALID_RESPONSE);
    }
}
