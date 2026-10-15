//! `SQLite` extraction preserves private state, revisions and feed capabilities.

use arrpi_adapters::storage::{
    Database, FeedAction, SqliteEvents, Store, SubscriptionStore, public, restore_secrets,
};
use arrpi_core::{
    config::Config,
    contracts::AuditTarget,
    error::ApiError,
    executor::{AuditEvent, EventSink},
    registry::ToolId,
};
use chrono::NaiveDate;
use rusqlite::Connection;
use secrecy::SecretString;
use serde_json::Value;

fn config() -> Config {
    Config::parse(include_str!("../../../tests/arrpi.json")).expect("fake configuration")
}

fn memory() -> Store {
    Store::new(
        Connection::open_in_memory()
            .expect("memory database")
            .into(),
    )
    .expect("settings schema")
}

fn today() -> Result<NaiveDate, ApiError> {
    NaiveDate::from_ymd_opt(2026, 10, 15).ok_or(ApiError::INTERNAL)
}

fn feed(connection: Connection, namespace: [u8; 32]) -> SubscriptionStore {
    let config = config();

    SubscriptionStore::new(
        Database::from(connection),
        config.subscriptions.master_key.clone(),
        config.subscriptions,
        today,
        namespace,
    )
    .expect("feed schema")
}

fn token(links: &arrpi_adapters::storage::FeedLinks) -> SecretString {
    links
        .rss_path
        .strip_prefix("/feeds/")
        .and_then(|path| path.strip_suffix("/calendar.rss"))
        .expect("capability path")
        .to_owned()
        .into()
}

#[test]
fn masked_roundtrip_preserves_every_credential_and_rejects_stale_writes() {
    let store = memory();
    let initial = config();

    assert_eq!(store.revision().expect("empty revision"), None);
    store.seed(&initial).expect("seed once");
    let (revision, loaded) = store.load().expect("load").expect("seeded document");

    assert_eq!(revision, 1);
    assert_eq!(
        loaded.sources["sonarr"].api_key.value(),
        initial.sources["sonarr"].api_key.value()
    );
    assert_eq!(
        loaded.clients[0].key.value(),
        initial.clients[0].key.value()
    );
    assert_eq!(
        loaded.seerr.as_ref().expect("Seerr").api_key.value(),
        initial.seerr.as_ref().expect("Seerr").api_key.value()
    );
    assert_eq!(
        loaded.subscriptions.master_key.value(),
        initial.subscriptions.master_key.value()
    );
    let masked = public(&loaded).expect("masked document");

    assert!(!masked.to_string().contains("fake-"));
    let mut edited: Config = serde_json::from_value(masked).expect("public update document");

    edited.cache.fresh_seconds = 17;
    restore_secrets(&mut edited, &loaded).expect("retain credentials");
    assert_eq!(store.save(revision, &edited).expect("next revision"), 2);
    assert_eq!(
        store.save(revision, &initial).expect_err("stale writer"),
        ApiError::CONFLICT
    );
    store
        .seed(&initial)
        .expect("repeat seed leaves state intact");
    let (_, saved) = store.load().expect("reload").expect("saved document");

    assert_eq!(saved.cache.fresh_seconds, 17);
    assert_eq!(
        saved.sources["radarr"].api_key.value(),
        initial.sources["radarr"].api_key.value()
    );
    assert_eq!(saved.clients[0].key.value(), initial.clients[0].key.value());
    assert_eq!(
        saved.subscriptions.master_key.value(),
        initial.subscriptions.master_key.value()
    );
}

#[test]
fn credential_retention_requires_matching_stable_identity() {
    let current = config();
    let mut edited: Config =
        serde_json::from_value(public(&current).expect("public document")).expect("masked config");

    edited.clients[0].name = "new-account".to_owned();
    assert_eq!(
        restore_secrets(&mut edited, &current).expect_err("new client needs key"),
        ApiError::BAD_QUERY
    );
    let mut edited: Config =
        serde_json::from_value(public(&current).expect("public document")).expect("masked config");
    let source = edited.sources.remove("sonarr").expect("existing source");

    edited.sources.insert("different".to_owned(), source);
    assert_eq!(
        restore_secrets(&mut edited, &current).expect_err("new source needs key"),
        ApiError::BAD_QUERY
    );
}

#[test]
fn unsupported_future_database_versions_are_not_modified() {
    let database = Connection::open_in_memory().expect("memory database");

    database
        .pragma_update(None, "user_version", 99)
        .expect("future marker");
    assert_eq!(
        Store::new(database.into()).expect_err("future settings version"),
        ApiError::INTERNAL
    );
    let database = Connection::open_in_memory().expect("memory database");

    database
        .pragma_update(None, "user_version", 99)
        .expect("future marker");
    let config = config();

    SubscriptionStore::new(
        database.into(),
        config.subscriptions.master_key.clone(),
        config.subscriptions,
        today,
        [7; 32],
    )
    .expect_err("future feed version");
}

#[tokio::test]
async fn awaited_observations_persist_account_scoped_stats_and_bounded_audits() {
    let store = memory();
    let sink = SqliteEvents::new(store.clone());

    sink.statistics("first", ToolId::SearchMedia, "ok", 14)
        .await
        .expect("first sample");
    sink.statistics("first", ToolId::SearchMedia, "ok", 21)
        .await
        .expect("second sample");
    sink.statistics("second", ToolId::SearchMedia, "error", 2)
        .await
        .expect("other account");
    let page = store.tool_stats("first").expect("stats page");

    assert_eq!(page.client, "first");
    assert_eq!(page.items.len(), 1);
    assert_eq!(
        (
            page.items[0].calls,
            page.items[0].duration_ms,
            page.items[0].last_duration_ms
        ),
        (2, 35, 21)
    );
    let events: Vec<_> = (1..=3)
        .map(|id| AuditEvent {
            tool: "request_movie".to_owned(),
            target: AuditTarget {
                tmdb_id: Some(id),
                season: Some(1),
                episode: Some(2),
                ..AuditTarget::default()
            },
            outcome: "ok".to_owned(),
        })
        .collect();

    sink.audit("first", Some(101), &events, 2)
        .await
        .expect("atomic bounded audit");
    sink.audit("second", None, &events[..1], 2)
        .await
        .expect("other account audit");
    let page = store.write_audit("first", 0, 1).expect("audit first page");

    assert_eq!((page.total, page.next_offset), (2, Some(1)));
    assert_eq!(page.items[0].actor_id, Some(101));
    assert_eq!(page.items[0].target.tmdb_id, Some(3));
    assert_eq!(
        (page.items[0].target.season, page.items[0].target.episode),
        (Some(1), Some(2))
    );
    let page = store.write_audit("first", 1, 1).expect("audit last page");

    assert_eq!(page.next_offset, None);
    assert_eq!(page.items[0].target.tmdb_id, Some(2));
    assert_eq!(
        store
            .write_audit("absent", 0, 5)
            .expect("isolated account")
            .total,
        0
    );
}

#[tokio::test]
async fn one_database_copy_preserves_settings_audit_stats_and_feed_namespace() {
    let directory = tempfile::tempdir().expect("temporary state");
    let path = directory.path().join("arrpi.db");
    let backup = directory.path().join("copy.db");
    let store =
        Store::new(Connection::open(&path).expect("settings connection").into()).expect("schema");
    let subscriptions = feed(Connection::open(&path).expect("feed connection"), [7; 32]);

    store.seed(&config()).expect("settings");
    store
        .record_tool("owner", "search", "ok", 1)
        .expect("stats");
    store
        .record_audit(
            "owner",
            Some(1),
            &[AuditEvent {
                tool: "request_movie".to_owned(),
                target: AuditTarget {
                    tmdb_id: Some(42),
                    ..AuditTarget::default()
                },
                outcome: "ok".to_owned(),
            }],
            5,
        )
        .expect("audit");
    let issued = subscriptions
        .update(
            "owner".to_owned(),
            vec!["sonarr".to_owned()],
            FeedAction::Issue,
        )
        .await
        .expect("issue")
        .expect("links");
    let original = issued.rss_path.clone();

    assert_eq!(
        subscriptions.query().expect("rolling dates"),
        "start=2026-10-08&end=2026-12-14"
    );
    drop(store);
    drop(subscriptions);
    std::fs::copy(path, &backup).expect("copy closed database");
    let restored = Store::new(
        Connection::open(&backup)
            .expect("restore connection")
            .into(),
    )
    .expect("supported schema");
    let subscriptions = feed(
        Connection::open(&backup).expect("restore feed connection"),
        [9; 32],
    );

    assert_eq!(restored.revision().expect("revision"), Some(1));
    assert_eq!(
        restored.tool_stats("owner").expect("stats").items[0].calls,
        1
    );
    assert_eq!(restored.write_audit("owner", 0, 5).expect("audit").total, 1);
    assert_eq!(
        subscriptions
            .sources(token(&issued))
            .await
            .expect("old capability"),
        vec!["sonarr"]
    );
    let reissued = subscriptions
        .update(
            "owner".to_owned(),
            vec!["sonarr".to_owned()],
            FeedAction::Issue,
        )
        .await
        .expect("reissue")
        .expect("links");

    assert_eq!(
        reissued.rss_path, original,
        "persisted namespace wins over fresh random seed"
    );
    verify_revocation(&subscriptions, &issued).await;
}

async fn verify_revocation(
    subscriptions: &SubscriptionStore,
    issued: &arrpi_adapters::storage::FeedLinks,
) {
    let rotated = subscriptions
        .update(
            "owner".to_owned(),
            vec!["radarr".to_owned()],
            FeedAction::Rotate,
        )
        .await
        .expect("rotate")
        .expect("new links");

    assert_eq!(
        subscriptions
            .sources(token(issued))
            .await
            .expect_err("old token revoked"),
        ApiError::UNAUTHORIZED
    );
    assert_eq!(
        subscriptions
            .sources(token(&rotated))
            .await
            .expect("new grants"),
        vec!["radarr"]
    );
    assert_eq!(
        subscriptions
            .update("owner".to_owned(), Vec::new(), FeedAction::Revoke)
            .await
            .expect("revoke")
            .map(|_| Value::Null),
        None
    );
    assert_eq!(
        subscriptions
            .sources(token(&rotated))
            .await
            .expect_err("revoked token"),
        ApiError::UNAUTHORIZED
    );
    assert_eq!(
        subscriptions
            .update(
                "owner".to_owned(),
                vec!["sonarr".to_owned()],
                FeedAction::Issue
            )
            .await
            .expect_err("issue cannot undo revoke"),
        ApiError::FORBIDDEN
    );
}

#[test]
fn missing_disabled_credentials_remain_missing_and_zero_page_limit_is_rejected() {
    let store = memory();
    let mut config = config();

    config.subscriptions.enabled = false;
    config.subscriptions.master_key = arrpi_core::secret::Secret::new();
    store.seed(&config).expect("seed disabled feeds");
    let (_, loaded) = store
        .load()
        .expect("valid stored config")
        .expect("seeded document");

    assert!(
        loaded.subscriptions.master_key.missing(),
        "null credentials retain omission semantics in private state"
    );
    assert_eq!(
        store
            .write_audit("owner", 0, 0)
            .expect_err("pagination must advance"),
        ApiError::BAD_QUERY
    );
}
