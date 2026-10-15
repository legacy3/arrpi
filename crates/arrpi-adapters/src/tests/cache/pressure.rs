//! Byte pressure evicts idle cache ownership, never active work or backing leases.

use std::sync::Arc;

use arrpi_core::{config::CachePolicy, error::ApiError, source::ReadMode};
use tokio::{sync::Notify, time::Instant};

use crate::{
    budget::{self, Budget},
    cache::{Cache, Pruner},
    metrics::Metrics,
};

fn group(limit: usize) -> (Budget, [Arc<Cache>; 2]) {
    let caches =
        std::array::from_fn(|_| Arc::new(Cache::new(CachePolicy::default(), Instant::now)));
    let budget = Budget::new(limit).expect("budget");

    budget
        .attach(Pruner::new(caches.iter().map(Arc::downgrade).collect()))
        .expect("cache group");
    (budget, caches)
}

async fn put(cache: &Cache, budget: &Budget, key: &str, body: &[u8]) {
    drop(
        cache
            .get(key.into(), &Metrics::new(), ReadMode::Cached, async {
                budget::leased(body.to_vec(), budget.reserve(body.len()).await?)
            })
            .await
            .expect("cache seed"),
    );
}

#[tokio::test(start_paused = true)]
async fn fresh_pressure_reclaims_global_lru_and_stops_after_enough_capacity() {
    let (budget, [newer, older]) = group(12);

    put(&older, &budget, "old", b"old!").await;
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    put(&newer, &budget, "new", b"new!").await;
    let reservation = budget.reserve(8).await.expect("reclaim fresh idle body");

    assert_eq!(budget.used(), 12);
    let remaining = newer
        .get("new".into(), &Metrics::new(), ReadMode::Cached, async {
            Err(ApiError::REJECTED)
        })
        .await
        .expect("newer body was not evicted");

    assert_eq!(remaining.body.as_ref(), b"new!");
    assert!(matches!(
        older
            .get("old".into(), &Metrics::new(), ReadMode::Cached, async {
                Err(ApiError::UNAVAILABLE)
            })
            .await,
        Err(ApiError::UNAVAILABLE)
    ));
    drop(reservation);
    assert_eq!(budget.used(), 4);
    drop(remaining);
    assert!(budget.reserve(13).await.is_err(), "impossible capacity");
    assert_eq!(budget.used(), 4, "impossible request does not evict");
}

#[tokio::test(start_paused = true)]
async fn pressure_eviction_cannot_release_an_outgoing_backing_reference() {
    let (budget, [first, second]) = group(8);

    put(&first, &budget, "held", b"held").await;
    let outgoing = first
        .get("held".into(), &Metrics::new(), ReadMode::Cached, async {
            Err(ApiError::REJECTED)
        })
        .await
        .expect("outgoing body")
        .body;

    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    put(&second, &budget, "idle", b"idle").await;
    assert!(matches!(budget.reserve(8).await, Err(ApiError::CAPACITY)));
    assert_eq!(outgoing.as_ref(), b"held");
    assert_eq!(budget.used(), 4, "only outgoing bytes remain charged");
    drop(outgoing);
    let reservation = budget.reserve(8).await.expect("last backing released");

    assert_eq!(budget.used(), 8);
    drop(reservation);
    assert_eq!(budget.used(), 0);
}

#[tokio::test]
async fn pressure_keeps_active_refreshes_and_valid_hits_intact() {
    let (budget, [cache, _other]) = group(8);

    put(&cache, &budget, "active", b"12345678").await;
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let work = Arc::clone(&cache);
    let started = Arc::clone(&entered);
    let waiting = Arc::clone(&release);
    let refresh = tokio::spawn(async move {
        work.get("active".into(), &Metrics::new(), ReadMode::Fresh, async {
            started.notify_one();
            waiting.notified().await;
            Err(ApiError::UNAVAILABLE)
        })
        .await
    });

    entered.notified().await;
    assert!(matches!(budget.reserve(1).await, Err(ApiError::CAPACITY)));
    let hit = cache
        .get("active".into(), &Metrics::new(), ReadMode::Cached, async {
            Err(ApiError::REJECTED)
        })
        .await
        .expect("valid cache hit during refresh");

    assert_eq!(hit.body.as_ref(), b"12345678");
    drop(hit);
    release.notify_one();
    assert!(matches!(
        refresh.await.expect("refresh task"),
        Err(ApiError::UNAVAILABLE)
    ));
    let reservation = budget.reserve(1).await.expect("now idle and evictable");

    assert_eq!(budget.used(), 1);
    drop(reservation);
}

#[tokio::test]
async fn concurrent_pressure_reservations_still_obey_the_shared_limit() {
    let (budget, [cache, _other]) = group(8);

    put(&cache, &budget, "body", b"12345678").await;
    let (first, second) = tokio::join!(budget.reserve(8), budget.reserve(8));

    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert_eq!(budget.used(), 8);
    drop(first);
    drop(second);
    assert_eq!(budget.used(), 0);
}
