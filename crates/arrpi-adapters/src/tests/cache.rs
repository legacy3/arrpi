//! Cache freshness, failure backoff, capacity and cancellation contracts.

mod pressure;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use bytes::Bytes;
use tokio::{
    sync::Notify,
    time::{Duration, Instant},
};

use crate::{cache::Cache, metrics::Metrics};
use arrpi_core::{config::CachePolicy, error::ApiError, source::ReadMode};

#[tokio::test(start_paused = true)]
async fn fresh_stale_expired_and_retry_backoff() {
    let policy = CachePolicy {
        fresh_seconds: 10,
        stale_seconds: 20,
        retry_seconds: 5,
        ..CachePolicy::default()
    };
    let cache = Cache::new(policy, Instant::now);
    let metrics = Metrics::default();
    let first = cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Ok(Bytes::from_static(b"[]"))
        })
        .await
        .expect("cold success");
    let hit = cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Err(ApiError::REJECTED)
        })
        .await
        .expect("fresh cached success");

    assert_eq!(
        serde_json::to_value(first.state).expect("cache state"),
        "miss",
        "first fetch misses"
    );
    assert_eq!(
        serde_json::to_value(hit.state).expect("cache state"),
        "hit",
        "fresh data avoids upstream"
    );
    tokio::time::advance(Duration::from_secs(11)).await;

    let stale = cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Err(ApiError::UNAVAILABLE)
        })
        .await
        .expect("stale during outage");
    let retry = cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Ok(Bytes::from_static(b"new"))
        })
        .await
        .expect("failure backoff preserves old data");

    assert_eq!(
        serde_json::to_value(stale.state).expect("cache state"),
        "stale",
        "outage is explicitly stale"
    );
    assert_eq!(retry.body, b"[]"[..], "backoff suppresses repeated refresh");
    assert_eq!(stale.age, 11, "age uses original successful fetch");
    tokio::time::advance(Duration::from_secs(20)).await;

    let expired = cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Err(ApiError::UNAVAILABLE)
        })
        .await;

    assert!(
        matches!(expired, Err(ApiError::UNAVAILABLE)),
        "stale lifetime is bounded"
    );
    assert_eq!(
        metrics.misses.load(Ordering::Relaxed),
        3,
        "backoff does not call upstream"
    );
}

#[tokio::test(start_paused = true)]
async fn upstream_rejection_drops_last_known_data() {
    let cache = Cache::new(CachePolicy::default(), Instant::now);
    let metrics = Metrics::default();

    cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Ok(Bytes::from_static(b"private"))
        })
        .await
        .expect("first successful read");
    tokio::time::advance(Duration::from_secs(61)).await;

    let rejected = cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Err(ApiError::REJECTED)
        })
        .await;

    assert!(
        matches!(rejected, Err(ApiError::REJECTED)),
        "never mask permission rejection with stale data"
    );
    tokio::time::advance(Duration::from_secs(6)).await;

    let outage = cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Err(ApiError::UNAVAILABLE)
        })
        .await;

    assert!(
        matches!(outage, Err(ApiError::UNAVAILABLE)),
        "rejected private data was discarded"
    );
}

#[tokio::test]
async fn concurrent_identical_reads_share_one_fetch() {
    let cache = Arc::new(Cache::new(CachePolicy::default(), Instant::now));
    let metrics = Arc::new(Metrics::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let mut jobs = tokio::task::JoinSet::new();

    for _ in 0..32 {
        let cache = Arc::clone(&cache);
        let metrics = Arc::clone(&metrics);
        let calls = Arc::clone(&calls);

        jobs.spawn(async move {
            cache
                .get("same".into(), &metrics, ReadMode::Cached, async {
                    calls.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(Bytes::from_static(b"[]"))
                })
                .await
                .expect("shared response")
        });
    }

    while let Some(job) = jobs.join_next().await {
        assert_eq!(
            job.expect("fetch job completed").body,
            b"[]"[..],
            "every reader receives shared data"
        );
    }

    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "only one upstream refresh"
    );
}

#[tokio::test]
async fn capacity_rejects_unique_key_while_slot_busy_and_recovers_after_cancellation() {
    let policy = CachePolicy {
        max_entries: 1,
        ..CachePolicy::default()
    };
    let cache = Arc::new(Cache::new(policy, Instant::now));
    let metrics = Arc::new(Metrics::default());
    let started = Arc::new(Notify::new());
    let cache_task = Arc::clone(&cache);
    let task_metrics = Arc::clone(&metrics);
    let task_started = Arc::clone(&started);
    let job = tokio::spawn(async move {
        cache_task
            .get("busy".into(), &task_metrics, ReadMode::Cached, async {
                task_started.notify_one();
                std::future::pending::<Result<Bytes, ApiError>>().await
            })
            .await
    });

    started.notified().await;

    let full = cache
        .get("other".into(), &metrics, ReadMode::Cached, async {
            Ok(Bytes::from_static(b"[]"))
        })
        .await;

    assert!(
        matches!(full, Err(ApiError::CAPACITY)),
        "in-flight entries cannot be evicted"
    );
    job.abort();

    assert!(
        job.await.expect_err("cancel fixture").is_cancelled(),
        "fixture was cancelled"
    );

    let recovered = cache
        .get("other".into(), &metrics, ReadMode::Cached, async {
            Ok(Bytes::from_static(b"new"))
        })
        .await
        .expect("evict now-idle slot");

    assert_eq!(
        recovered.body,
        b"new"[..],
        "cancelled refresh does not leak capacity"
    );
}

#[tokio::test(start_paused = true)]
async fn forced_read_bypasses_fresh_data_and_backoff_without_serving_stale() {
    let cache = Cache::new(CachePolicy::default(), Instant::now);
    let metrics = Metrics::default();

    cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Ok(Bytes::from_static(b"old"))
        })
        .await
        .expect("seed success");

    let fresh = cache
        .get("key".into(), &metrics, ReadMode::Fresh, async {
            Ok(Bytes::from_static(b"new"))
        })
        .await
        .expect("force refresh");

    assert_eq!(fresh.body, b"new"[..], "fresh TTL was bypassed");
    assert_eq!(
        serde_json::to_value(fresh.state).expect("cache state"),
        "miss",
        "force actually fetched"
    );
    tokio::time::advance(Duration::from_secs(61)).await;

    let failure = cache
        .get("key".into(), &metrics, ReadMode::Fresh, async {
            Err(ApiError::UNAVAILABLE)
        })
        .await;

    assert!(
        matches!(failure, Err(ApiError::UNAVAILABLE)),
        "forced reads never serve stale"
    );

    let retry = cache
        .get("key".into(), &metrics, ReadMode::Fresh, async {
            Ok(Bytes::from_static(b"recovered"))
        })
        .await
        .expect("forced retry bypasses backoff");

    assert_eq!(
        retry.body,
        b"recovered"[..],
        "debugger can retry immediately"
    );
}

#[tokio::test]
async fn concurrent_forced_reads_share_one_new_result() {
    let cache = Arc::new(Cache::new(CachePolicy::default(), Instant::now));
    let metrics = Arc::new(Metrics::default());
    let calls = Arc::new(AtomicUsize::new(0));
    let mut jobs = tokio::task::JoinSet::new();

    cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Ok(Bytes::from_static(b"old"))
        })
        .await
        .expect("initial cache");

    for _ in 0..32 {
        let cache = Arc::clone(&cache);
        let metrics = Arc::clone(&metrics);
        let calls = Arc::clone(&calls);

        jobs.spawn(async move {
            cache
                .get("key".into(), &metrics, ReadMode::Fresh, async {
                    calls.fetch_add(1, Ordering::Relaxed);
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    Ok(Bytes::from_static(b"new"))
                })
                .await
                .expect("forced shared result")
        });
    }

    while let Some(job) = jobs.join_next().await {
        assert_eq!(
            job.expect("reader completed").body,
            b"new"[..],
            "every force reader gets new data"
        );
    }

    assert_eq!(
        calls.load(Ordering::Relaxed),
        1,
        "concurrent force reads do not stampede"
    );
}

#[tokio::test]
async fn valid_hits_do_not_wait_for_a_forced_refresh() {
    let cache = Arc::new(Cache::new(CachePolicy::default(), Instant::now));
    let metrics = Arc::new(Metrics::default());
    let started = Arc::new(Notify::new());

    cache
        .get("key".into(), &metrics, ReadMode::Cached, async {
            Ok(Bytes::from_static(b"valid"))
        })
        .await
        .expect("initial fresh data");

    let task_cache = Arc::clone(&cache);
    let task_metrics = Arc::clone(&metrics);
    let task_started = Arc::clone(&started);
    let refresh = tokio::spawn(async move {
        task_cache
            .get("key".into(), &task_metrics, ReadMode::Fresh, async {
                task_started.notify_one();
                std::future::pending::<Result<Bytes, ApiError>>().await
            })
            .await
    });

    started.notified().await;

    let hit = tokio::time::timeout(
        Duration::from_millis(100),
        cache.get("key".into(), &metrics, ReadMode::Cached, async {
            Err(ApiError::REJECTED)
        }),
    )
    .await
    .expect("fresh hit is not blocked")
    .expect("cached success");

    assert_eq!(
        hit.body,
        b"valid"[..],
        "existing fresh data remains available"
    );
    refresh.abort();
    assert!(
        refresh
            .await
            .expect_err("cancel pending refresh")
            .is_cancelled(),
        "test refresh cancelled"
    );
}
