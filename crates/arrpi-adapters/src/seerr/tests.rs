//! Private native queue invariants require no production state.
use super::*;
use crate::http::HttpOptions;
#[tokio::test(start_paused = true)]
async fn write_queue_is_bounded_and_recovers_without_dispatch() {
    let connection: arrpi_core::config::SeerrConnection =
        serde_json::from_value(json!({"url":"http://127.0.0.1:1","api_key":"fake-seerr-key"}))
            .expect("connection");
    let limits = arrpi_core::policy::Limits {
        max_concurrent_writes: 1,
        queue_timeout_seconds: 1,
        ..Default::default()
    };
    let backend = Arc::new(
        Backend::new(
            &connection,
            CachePolicy::default(),
            limits,
            Http::new(HttpOptions {
                timeout: Duration::from_secs(1),
                connect_timeout: Duration::from_secs(1),
                pool_idle_per_host: 1,
            })
            .expect("transport"),
            Budget::new(2 * 1024 * 1024).expect("budget"),
        )
        .expect("backend"),
    );
    let first = backend.write_slot().await.expect("permit");
    let queued = Arc::clone(&backend);
    let task = tokio::spawn(async move { queued.write_slot().await });

    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_secs(2)).await;
    assert_eq!(
        task.await.expect("task").expect_err("full queue").code(),
        "queue_timeout_retry_safe"
    );
    drop(first);
    let _recovered = backend.write_slot().await.expect("recovered permit");
}

#[derive(Debug)]
struct Native {
    calls: std::sync::atomic::AtomicUsize,
    pending: std::sync::atomic::AtomicBool,
    safe_server: std::sync::atomic::AtomicBool,
    permissions: std::sync::atomic::AtomicU64,
    writes: std::sync::atomic::AtomicUsize,
    preflight: tokio::sync::Notify,
    receipt_reads: std::sync::atomic::AtomicUsize,
}

async fn reply(
    axum::extract::State(state): axum::extract::State<Arc<Native>>,
    request: axum::extract::Request,
) -> axum::response::Response {
    use axum::response::IntoResponse as _;
    use std::sync::atomic::Ordering;

    state.calls.fetch_add(1, Ordering::Relaxed);
    assert_eq!(request.headers()["X-API-Key"], "fake-seerr-key");
    assert_eq!(request.headers()["X-API-User"], "1");
    let value = match request.uri().path() {
        "/api/v1/auth/me" => {
            json!({"id":1,"plexId":101,"permissions":state.permissions.load(Ordering::Relaxed)})
        }

        "/api/v1/status" => json!({"version":"3.5.0"}),

        "/api/v1/movie/42" => json!({"id":42,"title":"Movie","mediaInfo":{"status":1}}),

        "/api/v1/user/1/quota" => {
            state.preflight.notify_one();
            json!({"movie":{"limit":0}})
        }

        "/api/v1/request/7" => {
            if request.method() == Method::DELETE {
                state.writes.fetch_add(1, Ordering::Relaxed);

                return axum::http::StatusCode::NO_CONTENT.into_response();
            }

            let value = json!({"id":7,"status":if state.pending.load(Ordering::Relaxed){1}else{2},"requestedBy":{"id":1},"media":{"tmdbId":42},"type":"movie","is4k":false,"seasons":[]});

            state.receipt_reads.fetch_add(1, Ordering::SeqCst);
            state.preflight.notify_one();
            value
        }

        "/api/v1/service/radarr" => {
            json!([{"id":0,"isDefault":true,"is4k":!state.safe_server.load(Ordering::Relaxed)}])
        }

        "/api/v1/service/radarr/0" => {
            json!({"server":{"id":0,"isDefault":true,"is4k":false},"profiles":[{"id":1,"name":"Default"}]})
        }

        "/api/v1/search" => {
            assert_eq!(
                request.uri().query(),
                Some("query=Project%20Hail%20Mary%20%26%20%C3%84&page=1&language=de")
            );
            json!({"page":1,"totalPages":1,"results":[{"id":42,"title":"Project Hail Mary","mediaType":"movie"}]})
        }

        "/api/v1/request" => {
            state.writes.fetch_add(1, Ordering::Relaxed);

            return (axum::http::StatusCode::CREATED, "{incomplete").into_response();
        }

        _ => return axum::http::StatusCode::NOT_FOUND.into_response(),
    };

    axum::Json(value).into_response()
}

#[derive(Debug)]
struct Fixture {
    task: tokio::task::JoinHandle<()>,
    backend: Arc<Backend>,
    state: Arc<Native>,
}

impl Fixture {
    async fn occupy_write_slot(&self) -> tokio::sync::OwnedSemaphorePermit {
        assert_eq!(
            self.backend.writes.available_permits(),
            1,
            "fixture has exactly one write slot"
        );
        let permit = self
            .backend
            .write_slot()
            .await
            .expect("occupy sole write slot");

        assert_eq!(
            self.backend.writes.available_permits(),
            0,
            "mutation must wait for admission"
        );
        permit
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn fixture() -> Fixture {
    let state = Arc::new(Native {
        calls: std::sync::atomic::AtomicUsize::new(0),
        pending: std::sync::atomic::AtomicBool::new(true),
        safe_server: std::sync::atomic::AtomicBool::new(true),
        permissions: std::sync::atomic::AtomicU64::new(32),
        writes: std::sync::atomic::AtomicUsize::new(0),
        preflight: tokio::sync::Notify::new(),
        receipt_reads: std::sync::atomic::AtomicUsize::new(0),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("fixture listener");
    let url = format!(
        "http://{}/",
        listener.local_addr().expect("fixture address")
    );
    let router = axum::Router::new()
        .route("/{*path}", axum::routing::any(reply))
        .with_state(Arc::clone(&state));
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("fixture service");
    });
    let connection = SeerrConnection {
        url,
        api_key: "fake-seerr-key".to_owned().into(),
        language: "de".to_owned(),
    };
    let backend = Arc::new(
        Backend::new(
            &connection,
            CachePolicy::default(),
            Limits {
                max_concurrent_writes: 1,
                ..Limits::default()
            },
            Http::new(HttpOptions {
                timeout: Duration::from_secs(2),
                connect_timeout: Duration::from_secs(1),
                pool_idle_per_host: 1,
            })
            .expect("transport"),
            Budget::new(2 * 1024 * 1024).expect("budget"),
        )
        .expect("backend"),
    );

    Fixture {
        task,
        backend,
        state,
    }
}

fn binding() -> Binding {
    Binding {
        id: 1,
        plex_id: 101,
    }
}

#[tokio::test]
async fn queued_mutation_rechecks_native_rights_after_admission() {
    use std::sync::atomic::Ordering;
    let fixture = fixture().await;
    let permit = fixture.occupy_write_slot().await;
    let backend = Arc::clone(&fixture.backend);
    let request = tokio::spawn(async move {
        backend
            .submit_request(
                &binding(),
                serde_json::from_value(json!({"media_type":"movie","tmdb_id":42}))
                    .expect("request"),
                None,
            )
            .await
    });

    fixture.state.preflight.notified().await;
    fixture.state.permissions.store(0, Ordering::Relaxed);
    drop(permit);
    assert_eq!(
        request
            .await
            .expect("request task")
            .expect_err("revoked native rights")
            .code(),
        "permission_denied"
    );
    assert_eq!(fixture.state.writes.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn dispatched_invalid_write_remains_unknown_and_search_metadata_stays_adjacent() {
    use arrpi_core::source::CatalogSource as _;
    use std::sync::atomic::Ordering;
    let fixture = fixture().await;
    let search = fixture
        .backend
        .search(
            &binding(),
            serde_json::from_value(json!({"query":"Project Hail Mary & Ä","no_cache":true}))
                .expect("search arguments"),
        )
        .await
        .expect("search");

    assert_eq!(search["items"][0]["tmdb_id"], 42);
    assert_eq!(search["freshness"]["cache"], "not_cached");
    assert_eq!(search["freshness"]["bypass_requested"], true);
    assert!(!search.to_string().contains("_arrpi_freshness"));
    let error = fixture
        .backend
        .submit_request(
            &binding(),
            serde_json::from_value(json!({"media_type":"movie","tmdb_id":42})).expect("request"),
            None,
        )
        .await
        .expect_err("invalid write receipt");

    assert_eq!(error.code(), "unknown_outcome");
    assert_eq!(fixture.state.writes.load(Ordering::Relaxed), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancellation_rechecks_pending_state_after_the_queue() {
    for _ in 0..32 {
        approved_while_queued_is_not_cancelled().await;
    }
}

async fn approved_while_queued_is_not_cancelled() {
    use std::sync::atomic::Ordering;
    let fixture = fixture().await;
    let permit = fixture.occupy_write_slot().await;
    let backend = Arc::clone(&fixture.backend);
    let request = tokio::spawn(async move { backend.cancel_request(&binding(), 7).await });

    fixture.state.preflight.notified().await;
    assert_eq!(
        fixture.state.receipt_reads.load(Ordering::SeqCst),
        1,
        "first pending snapshot precedes the queued state change"
    );
    assert!(
        !request.is_finished(),
        "the sole permit still prevents dispatch"
    );
    fixture.state.pending.store(false, Ordering::SeqCst);
    drop(permit);
    assert_eq!(
        request
            .await
            .expect("request task")
            .expect_err("request was approved while queued"),
        ApiError::REQUEST_NOT_PENDING
    );
    assert_eq!(
        fixture.state.receipt_reads.load(Ordering::SeqCst),
        2,
        "post-admission state was fetched again"
    );
    assert_eq!(fixture.state.writes.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn selected_service_must_still_be_the_default_non_4k_server_after_the_queue() {
    use std::sync::atomic::Ordering;
    let fixture = fixture().await;

    fixture
        .state
        .permissions
        .store(32 | 8192, Ordering::Relaxed);
    let permit = fixture.occupy_write_slot().await;
    let backend = Arc::clone(&fixture.backend);
    let request = tokio::spawn(async move {
        backend
            .submit_request(
                &binding(),
                serde_json::from_value(json!({"media_type":"movie","tmdb_id":42}))
                    .expect("request"),
                Some(arrpi_core::source::ProfileSelection {
                    server_id: 0,
                    profile_id: 1,
                    name: "Default".to_owned(),
                }),
            )
            .await
    });

    fixture.state.preflight.notified().await;
    fixture.state.safe_server.store(false, Ordering::Relaxed);
    drop(permit);
    assert_eq!(
        request
            .await
            .expect("request task")
            .expect_err("server became4k while queued"),
        ApiError::PROFILE_UNAVAILABLE
    );
    assert_eq!(fixture.state.writes.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn insufficient_response_budget_rejects_a_write_before_dispatch() {
    use std::sync::atomic::Ordering;
    let fixture = fixture().await;
    let held = fixture
        .backend
        .budget
        .reserve(2 * 1024 * 1024)
        .await
        .expect("occupy retained body budget");
    let error = fixture
        .backend
        .call(
            &binding(),
            Method::POST,
            "request",
            &[],
            Some(json!({"is4k":false})),
        )
        .await
        .expect_err("full budget");

    assert_eq!(
        error,
        ApiError::CAPACITY,
        "local capacity failure is retry-safe before dispatch"
    );
    assert_eq!(fixture.state.writes.load(Ordering::Relaxed), 0);
    drop(held);
}

#[tokio::test]
async fn invalid_cancel_identifiers_fail_before_native_identity_io() {
    use std::sync::atomic::Ordering;
    let fixture = fixture().await;
    let error = fixture
        .backend
        .cancel_request(&binding(), 0)
        .await
        .expect_err("invalid request identifier");

    assert_eq!(error, ApiError::BAD_QUERY);
    assert_eq!(fixture.state.calls.load(Ordering::Relaxed), 0);
}
