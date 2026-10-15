//! Shared normalization admission must not block or retain upstream I/O slots.
use arrpi_core::{config::Config, error::ApiError, source::ReadMode};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Debug)]
struct Server(tokio::task::JoinHandle<()>);

impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn calendar_fetches_before_normalization_admission_and_reuses_the_snapshot() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let router = axum::Router::new().route(
        "/api/v3/calendar",
        axum::routing::get(move || {
            let counted = Arc::clone(&counted);

            async move {
                counted.fetch_add(1, Ordering::Relaxed);
                axum::Json(serde_json::json!([]))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("native listener");
    let address = listener.local_addr().expect("native address");
    let _server = Server(tokio::spawn(async move {
        axum::serve(listener, router).await.expect("native fixture");
    }));
    let config:Config=serde_json::from_value(serde_json::json!({"listen":"127.0.0.1:0","cache":{"max_normalizations":1},"sources":{"shows":{"kind":"sonarr","url":format!("http://{address}/"),"api_key":"fake-native-key"}},"clients":[{"name":"test","key":"fake-reader-key","permissions":["shows:calendar-view"]}]})).expect("fixture settings");
    let built = crate::build(&config, tokio::time::Instant::now).expect("sources");
    let held = built.normalizations.try_acquire().expect("occupy CPU pool");
    let calendar = built
        .sources
        .source("shows")
        .expect("source")
        .calendar()
        .expect("calendar capability");
    let error = calendar
        .calendar("2026-10-14", "2026-10-15", ReadMode::Cached)
        .await
        .expect_err("normalization is occupied");

    assert_eq!(error, ApiError::BUSY);
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "I/O completes before acquiring CPU admission"
    );
    drop(held);
    let snapshot = calendar
        .calendar("2026-10-14", "2026-10-15", ReadMode::Cached)
        .await
        .expect("released CPU pool");

    assert!(snapshot.data.is_empty());
    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "the completed upstream snapshot remains cached"
    );
}
