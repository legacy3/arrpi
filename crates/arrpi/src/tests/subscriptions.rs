//! Capability separation, transactional rotation, persistence and rolling dates.

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use chrono::NaiveDate;
use rusqlite::Connection;
use secrecy::{ExposeSecret as _, SecretString};
use tower::ServiceExt as _;

use crate::{
    config::Config,
    error::ApiError,
    proxy::{Gateway, router},
    subscriptions::{Action, Store},
};

use arrpi_adapters::storage::FeedLinks as Links;
use arrpi_core::subscriptions::Policy;

use super::{
    EXAMPLE, OTHER_KEY, mock,
    proxy::{json, request},
};

const MASTER: &str = "fake-feed-master-key-00000000000000000000000000";
const ISSUER: &str = "fake-feed-issuer-key-00000000000000000000000000";

fn today() -> Result<NaiveDate, ApiError> {
    NaiveDate::from_ymd_opt(2026, 10, 14).ok_or(ApiError::INTERNAL)
}

fn store(database: Connection, master: &str, namespace: u8, max_subjects: usize) -> Arc<Store> {
    Arc::new(
        Store::new(
            database.into(),
            master.to_owned().into(),
            Policy {
                enabled: true,
                max_subjects,
                ..Policy::default()
            },
            today,
            [namespace; 32],
        )
        .expect("fixture store"),
    )
}

fn token(links: &Links) -> SecretString {
    links
        .rss_path
        .split('/')
        .nth(2)
        .expect("capability path")
        .to_owned()
        .into()
}

async fn links(store: &Arc<Store>, subject: &str, action: Action) -> Links {
    store
        .update(subject.to_owned(), vec!["radarr".to_owned()], action)
        .await
        .expect("update fixture")
        .expect("active links")
}

#[tokio::test]
async fn personal_capabilities_are_stable_distinct_rotatable_and_revocable() {
    let store = store(Connection::open_in_memory().expect("SQLite"), MASTER, 1, 2);
    let alice = links(&store, "alice", Action::Issue).await;
    let bob = links(&store, "bob", Action::Issue).await;
    let repeated = links(&store, "alice", Action::Issue).await;

    assert_eq!(
        alice.rss_path, repeated.rss_path,
        "repeated issuance preserves subscriptions"
    );
    assert_ne!(
        alice.rss_path, bob.rss_path,
        "each subject has its own token"
    );
    assert_eq!(
        store.sources(token(&alice)).await.expect("authorized"),
        vec!["radarr"]
    );
    assert!(
        !format!("{alice:?}").contains(token(&alice).expose_secret()),
        "Debug redacts links"
    );
    assert!(
        matches!(
            store
                .update("third".into(), vec!["radarr".into()], Action::Issue)
                .await,
            Err(ApiError::CAPACITY)
        ),
        "registry bound"
    );
    let rotated = links(&store, "alice", Action::Rotate).await;

    assert_ne!(alice.rss_path, rotated.rss_path);
    assert!(
        matches!(
            store.sources(token(&alice)).await,
            Err(ApiError::UNAUTHORIZED)
        ),
        "old token invalid immediately"
    );
    store
        .update("alice".into(), vec!["radarr".into()], Action::Revoke)
        .await
        .expect("revoke");
    assert!(
        matches!(
            store.sources(token(&rotated)).await,
            Err(ApiError::UNAUTHORIZED)
        ),
        "revoked token rejected"
    );
    assert!(
        matches!(
            store
                .update("alice".into(), vec!["radarr".into()], Action::Issue)
                .await,
            Err(ApiError::FORBIDDEN)
        ),
        "visiting the page cannot undo revocation"
    );
    assert!(
        store.sources(token(&bob)).await.is_ok(),
        "other users unaffected"
    );
}

#[tokio::test]
async fn capability_state_survives_restart_and_database_loss_cannot_resurrect_tokens() {
    let file = tempfile::NamedTempFile::new().expect("temporary SQLite");
    let first = store(Connection::open(file.path()).expect("SQLite"), MASTER, 1, 4);
    let original = links(&first, "alice", Action::Issue).await;

    drop(first);
    let reopened = store(Connection::open(file.path()).expect("SQLite"), MASTER, 2, 4);
    let restored = links(&reopened, "alice", Action::Issue).await;

    assert_eq!(
        original.rss_path, restored.rss_path,
        "namespace and generations persist"
    );
    assert_eq!(
        reopened.query().expect("rolling dates"),
        "start=2026-10-07&end=2026-12-13"
    );
    drop(reopened);
    let changed = store(
        Connection::open(file.path()).expect("SQLite"),
        "another-fake-master-00000000000000000000000000",
        3,
        4,
    );

    assert!(
        matches!(
            changed.sources(token(&original)).await,
            Err(ApiError::UNAUTHORIZED)
        ),
        "master rotation invalidates old signatures"
    );
    let fresh = store(Connection::open_in_memory().expect("SQLite"), MASTER, 4, 4);
    let replacement = links(&fresh, "alice", Action::Issue).await;

    assert_ne!(
        original.rss_path, replacement.rss_path,
        "new registry gets a fresh namespace even with the same master"
    );
    assert!(matches!(
        fresh.sources(token(&original)).await,
        Err(ApiError::UNAUTHORIZED)
    ));
}

#[tokio::test]
async fn concurrent_issuance_keeps_one_generation_and_unknown_schema_fails_closed() {
    let database = Connection::open_in_memory().expect("SQLite");

    database
        .pragma_update(None, "user_version", 99)
        .expect("future version");

    assert!(
        Store::new(
            database.into(),
            MASTER.to_owned().into(),
            Policy::default(),
            today,
            [1; 32]
        )
        .is_err(),
        "do not overwrite unknown schema versions"
    );
    let store = store(Connection::open_in_memory().expect("SQLite"), MASTER, 1, 4);
    let expected = links(&store, "alice", Action::Issue).await;
    let mut jobs = tokio::task::JoinSet::new();

    for _ in 0..16 {
        let store = Arc::clone(&store);

        jobs.spawn(async move {
            store
                .update("alice".into(), vec!["radarr".into()], Action::Issue)
                .await
                .map(|links| links.expect("active links").rss_path)
        });
    }

    let mut accepted = 0;

    while let Some(result) = jobs.join_next().await {
        match result.expect("issuance task") {
            Ok(path) => {
                accepted += 1;
                assert_eq!(path, expected.rss_path, "no accidental key rotations");
            }

            Err(error) => assert_eq!(error, ApiError::BUSY, "only bounded overload is rejected"),
        }
    }

    assert!(accepted > 0, "admitted issuance completes");
    assert_eq!(
        links(&store, "alice", Action::Issue).await.rss_path,
        expected.rss_path,
        "overload does not change generations"
    );
}

fn application(url: &str) -> Router {
    let text = EXAMPLE
        .replace("http://sonarr:8989/", url)
        .replace("http://radarr:7878/", url);
    let mut config = Config::parse(&text).expect("fixture config");

    config.clients.push(crate::config::Client {
        tracking_user: None,
        seerr_user: None,
        name: "issuer".into(),
        key: ISSUER.to_owned().into(),
        permissions: vec!["feeds:issue".into(), "radarr:calendar-view".into()],
    });
    let mut gateway = Gateway::new(&config, tokio::time::Instant::now).expect("fixture gateway");

    gateway.subscriptions = Some(store(
        Connection::open_in_memory().expect("SQLite"),
        MASTER,
        1,
        4,
    ));

    router(gateway)
}

async fn publish(app: &Router, path: &str, key: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("authorization", format!("Bearer {key}"))
                .header("x-arrpi-subject", "alice")
                .body(Body::empty())
                .expect("publisher request"),
        )
        .await
        .expect("response")
}

async fn assert_header_capability(app: &Router, credential: &str) {
    let mirrored = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/calendar.rss")
                .header("x-arrpi-feed-token", credential)
                .body(Body::empty())
                .expect("header capability"),
        )
        .await
        .expect("response");

    assert_eq!(
        mirrored.status(),
        StatusCode::OK,
        "nested API middleware accepts only export capabilities"
    );
}

#[tokio::test]
async fn public_feeds_need_no_browser_auth_and_cannot_read_json_native_data_or_issue_keys() {
    let (server, upstream) = mock(r#"[{"id":7,"title":"Film","digitalRelease":"2026-10-15","airDateUtc":"2026-10-15T20:00:00Z"}]"#).await;
    let app = application(&server.url);

    assert_eq!(
        publish(&app, "/v1/subscription", OTHER_KEY).await.status(),
        StatusCode::FORBIDDEN,
        "reader cannot mint tokens"
    );
    let issued = json(publish(&app, "/v1/subscription", ISSUER).await).await;
    let rss = issued["rss_path"].as_str().expect("RSS link");
    let ical = issued["ical_path"].as_str().expect("iCal link");
    let credential = rss.split('/').nth(2).expect("token");

    for path in [rss, ical] {
        assert_eq!(
            request(&app, "GET", path, None).await.status(),
            StatusCode::OK,
            "no Authentik or API key needed"
        );
        assert_eq!(
            request(&app, "POST", path, None).await.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "read-only public feeds"
        );
    }

    assert_header_capability(&app, credential).await;

    assert_eq!(
        upstream.calls.load(std::sync::atomic::Ordering::Relaxed),
        1,
        "only authorized Radarr source, shared feed cache"
    );

    for path in [
        "/v1/calendar?start=2026-10-01&end=2026-10-31",
        "/v1/status",
        "/metrics",
        "/v1/radarr/api/v3/movie",
        "/v1/subscription",
    ] {
        assert_eq!(
            request(&app, "GET", path, Some(credential)).await.status(),
            StatusCode::UNAUTHORIZED,
            "feed token has no other API permission"
        );
    }

    assert_eq!(
        publish(&app, "/v1/subscription", credential).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        request(&app, "GET", &format!("{rss}?apikey=secret"), None)
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("/feeds/{credential}/calendar.json"),
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        request(
            &app,
            "GET",
            &format!("{rss}?start=1900-01-01&end=2100-01-01"),
            None
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST,
        "public capabilities retain the 93-day query bound"
    );

    assert_eq!(
        publish(&app, "/v1/subscription/rotate", ISSUER)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        request(&app, "GET", rss, None).await.status(),
        StatusCode::UNAUTHORIZED,
        "old URL stops working"
    );
}
