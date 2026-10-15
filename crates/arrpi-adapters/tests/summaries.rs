//! Operational projections use explicit grants and bounded native contracts.
use arrpi_core::{
    config::Config,
    source::{Principal, ReadMode},
    summaries::SummaryKind,
};
use axum::{Json, Router, extract::Query, routing::get};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
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
        axum::serve(listener, router).await.expect("server");
    });

    (Server(task), format!("http://{address}/"))
}

fn settings(url: &str, kind: &str) -> Config {
    serde_json::from_value(json!({"listen":"127.0.0.1:0","sources":{"test":{"kind":kind,"url":url,"api_key":if kind == "maintainerr" {""} else {"fake-native-key"}}},"clients":[{"name":"reader","key":"fake-reader-0000000000000000000000","permissions":["test:queue-summary","test:library-summary","test:status-view"]}]})).expect("settings")
}

#[tokio::test]
async fn queue_traverses_pages_and_does_not_retain_titles_or_paths() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let router = Router::new().route("/api/v3/queue",get(move |Query(query): Query<BTreeMap<String,String>>| {
        let counted = Arc::clone(&counted);

        async move {
            counted.fetch_add(1, Ordering::Relaxed);
            let page: usize = query["page"].parse().expect("page");
            let start = (page-1)*100;
            let records: Vec<_> = (start..(start+100).min(125)).map(|id| json!({"id":id,"downloadClient":"SABnzbd","downloadId":format!("fake-{id}"),"size":100.5,"sizeleft":40.0,"status":"downloading","title":"private-title","outputPath":"/private/path"})).collect();

            Json(json!({"page":page,"pageSize":100,"totalRecords":125,"records":records}))
        }
    }));
    let (_server, url) = serve(router).await;
    let built = arrpi_adapters::build(&settings(&url, "sonarr"), tokio::time::Instant::now)
        .expect("sources");
    let summary = built
        .sources
        .source("test")
        .expect("source")
        .summaries()
        .expect("summary");
    let result = summary
        .summary(SummaryKind::Queue, ReadMode::Cached)
        .await
        .expect("queue");

    assert_eq!(result.data["items_observed"], 125);
    assert_eq!(result.data["complete"], true);
    assert_eq!(result.data["remaining_bytes_observed"], 5000.0);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    assert!(!result.data.to_string().contains("private"));
    summary
        .summary(SummaryKind::Queue, ReadMode::Cached)
        .await
        .expect("cached");
    assert_eq!(calls.load(Ordering::Relaxed), 2);
    summary
        .summary(SummaryKind::Queue, ReadMode::Fresh)
        .await
        .expect("fresh");
    assert_eq!(calls.load(Ordering::Relaxed), 4);
}

#[tokio::test]
async fn excessive_queue_is_explicitly_incomplete_and_bounded() {
    let router = Router::new().route(
        "/api/v3/queue",
        get(|Query(query): Query<BTreeMap<String, String>>| async move {
            let page: usize = query["page"].parse().expect("page");
            let records: Vec<_> = (0..100)
                .map(|id| json!({"id":(page-1)*100+id,"downloadClient":"SABnzbd","downloadId":format!("fake-{page}-{id}"),"size":1,"sizeleft":1,"status":"queued"}))
                .collect();

            Json(json!({"page":page,"pageSize":100,"totalRecords":3000,"records":records}))
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
        .expect("bounded");

    assert_eq!(result.data["items_observed"], 2000);
    assert_eq!(result.data["complete"], false);
    assert_eq!(result.data["reason"], "page_limit");
}

#[tokio::test]
async fn library_unknown_sizes_are_not_invented_and_grants_do_not_imply_each_other() {
    let router = Router::new().route("/api/v3/series",get(|| async { Json(json!([
        {"title":"private","monitored":true,"statistics":{"sizeOnDisk":123,"episodeFileCount":2}},
        {"title":"private","statistics":{"episodeFileCount":0}}
    ])) }));
    let (_server, url) = serve(router).await;
    let config = settings(&url, "sonarr");

    config.validate().expect("separate grants valid");
    let built = arrpi_adapters::build(&config, tokio::time::Instant::now).expect("sources");
    let commands = arrpi_core::Commands::new(
        &config,
        built.sources,
        arrpi_core::executor::Recording::new(None, tokio::time::Instant::now),
        built.normalizations,
    );
    let principal = Principal {
        name: "test".to_owned(),
        binding: None,
        permissions: [Box::from("test:library-summary")].into(),
    };
    let result = commands
        .summaries(
            &principal,
            SummaryKind::Library,
            arrpi_core::arguments::Fresh { no_cache: false },
        )
        .await
        .expect("summary");

    assert_eq!(result["sources"][0]["data"]["items"], 2);
    assert_eq!(result["sources"][0]["data"]["size_bytes"], Value::Null);
    assert_eq!(result["sources"][0]["data"]["known_size_bytes"], 123);
    assert_eq!(result["sources"][0]["data"]["items_without_size"], 1);
    assert_eq!(
        commands
            .summaries(
                &principal,
                SummaryKind::Queue,
                arrpi_core::arguments::Fresh { no_cache: false }
            )
            .await
            .expect_err("no queue permission"),
        arrpi_core::error::ApiError::FORBIDDEN
    );
    assert!(!result.to_string().contains("private"));
}

#[tokio::test]
async fn monitoring_contracts_are_explicit_and_no_native_routes_are_exposed() {
    let fixtures = [
        (
            "sabnzbd",
            "/api",
            json!({"queue":{"version":"5.1.3","noofslots":7,"noofslots_total":4,"paused":false,"kbpersec":"1024.5","mbleft":"123.25","slots":[{"password":"private-password"}]}}),
        ),
        (
            "cleanuparr",
            "/api/v2/stats",
            json!({"generatedAt":"2026-10-15T00:00:00Z","timeframeHours":168,"removals":{"total":3},"searches":{"total":5,"completed":4,"failed":1,"grabbed":2},"strikes":{"recovered":999}}),
        ),
        (
            "bazarr",
            "/api/badges",
            json!({"episodes":5,"movies":1,"providers":2,"status":0,"announcements":900}),
        ),
        (
            "maintainerr",
            "/api/storage-metrics",
            json!({"generatedAt":"2026-10-15T00:00:00Z","cleanupTotals":{"itemsHandled":8,"bytesHandled":123,"moviesHandled":1,"showsHandled":2,"seasonsHandled":3,"episodesHandled":2},"collectionSummary":{"reclaimableCount":2,"reclaimableSizedCount":1,"activeSizeBytes":456,"reclaimableUsingFallback":true},"mounts":[{"path":"/private/path"}]}),
        ),
    ];

    for (kind, path, body) in fixtures {
        let response = body.clone();
        let router =
            Router::new().route(
                path,
                axum::routing::get(
                    move |headers: axum::http::HeaderMap,
                          Query(query): Query<BTreeMap<String, String>>| {
                        assert_ne!(
                            kind, "sabnzbd",
                            "SAB must use fixed read POST without secret URL"
                        );
                        assert!(!query.contains_key("apikey"));

                        if kind != "maintainerr" {
                            assert_eq!(headers["x-api-key"], "fake-native-key");
                        }

                        let response = response.clone();

                        async move { Json(response) }
                    },
                )
                .post(
                    move |Query(query): Query<BTreeMap<String, String>>,
                          axum::extract::Form(form): axum::extract::Form<
                        BTreeMap<String, String>,
                    >| {
                        let response = body.clone();

                        async move {
                            assert_eq!(kind, "sabnzbd");
                            assert!(query.is_empty());
                            assert_eq!(form["mode"], "queue");
                            assert_eq!(form["limit"], "1");
                            assert_eq!(form["apikey"], "fake-native-key");
                            Json(response)
                        }
                    },
                ),
            );
        let (_server, url) = serve(router).await;
        let built = arrpi_adapters::build(&settings(&url, kind), tokio::time::Instant::now)
            .expect("sources");
        let source = built.sources.source("test").expect("source");

        assert!(source.native().is_none());
        assert!(source.library().is_none());
        let result = source
            .summaries()
            .expect("summary")
            .summary(SummaryKind::Status, ReadMode::Fresh)
            .await
            .expect("monitor");

        assert_eq!(result.data["complete"], true);
        assert!(!result.data.to_string().contains("private"));

        if kind == "sabnzbd" {
            assert_eq!(result.data["queued_jobs"], 7);
            assert_eq!(result.data["active_jobs"], 4);
        }

        if kind == "cleanuparr" {
            assert_eq!(result.data["replacement_count"], Value::Null);
            assert_eq!(result.data["searches"]["grabbed"], 2);
        }

        if kind == "maintainerr" {
            assert_eq!(result.data["reclaimable_uses_fallback"], true);
        }
    }
}
