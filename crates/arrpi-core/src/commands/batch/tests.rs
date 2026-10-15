//! Read deadlines are retry-safe; dispatched write deadlines remain indeterminate.

use super::{Mode, ordered};
use crate::error::ApiError;
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::time::Instant;

async fn slow_batch(mode: Mode) -> serde_json::Value {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let result = ordered(
        Instant::now,
        vec![1_u64, 2, 3],
        Instant::now() + Duration::from_millis(1500),
        mode,
        move |id| {
            observed.fetch_add(1, Ordering::SeqCst);
            async move {
                tokio::time::sleep(Duration::from_secs(1)).await;
                Ok(json!({"id": id}))
            }
        },
    )
    .await
    .expect("batch has ordered receipts");

    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "no dispatch after the deadline"
    );
    assert_eq!(result["succeeded"], 1);
    assert_eq!(result["failed"], 2);
    assert_eq!(result["items"][0]["result"]["id"], 1);
    assert_eq!(result["items"][2]["code"], "not_attempted");
    result
}

#[tokio::test(start_paused = true)]
async fn read_batch_timeout_is_safe_and_leaves_the_tail_unattempted() {
    let result = slow_batch(Mode::Read).await;

    assert_eq!(result["items"][1]["code"], "batch_timeout");
    assert_eq!(result["items"][1]["retry_safe"], true);
    assert_eq!(ApiError::BATCH_TIMEOUT.status(), 504);
    assert!(!result.to_string().contains("unknown_outcome"));
}

#[tokio::test(start_paused = true)]
async fn write_batch_timeout_preserves_indeterminate_dispatch() {
    let result = slow_batch(Mode::Write).await;

    assert_eq!(result["items"][1]["code"], "unknown_outcome");
}

#[tokio::test]
async fn one_failed_detail_does_not_hide_later_independent_reads() {
    let result = ordered(
        Instant::now,
        vec![1_u64, 2, 3],
        Instant::now() + Duration::from_secs(1),
        Mode::Read,
        |id| async move {
            if id == 1 {
                Err(ApiError::UNAVAILABLE)
            } else {
                Ok(json!({"id": id}))
            }
        },
    )
    .await
    .expect("ordered read receipts");

    assert_eq!(result["failed"], 1);
    assert_eq!(result["succeeded"], 2);
    assert_eq!(result["items"][1]["result"]["id"], 2);
    assert_eq!(result["items"][2]["result"]["id"], 3);
}

#[tokio::test]
async fn resource_pressure_stops_reads_and_infrastructure_failure_stops_writes() {
    for (mode, error) in [
        (Mode::Read, ApiError::BUSY),
        (Mode::Read, ApiError::CAPACITY),
        (Mode::Read, ApiError::UPSTREAM_RATE_LIMITED),
        (Mode::Write, ApiError::UNAVAILABLE),
    ] {
        let calls = AtomicUsize::new(0);
        let result = ordered(
            Instant::now,
            vec![1_u64, 2],
            Instant::now() + Duration::from_secs(1),
            mode,
            |_id| {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err(error) }
            },
        )
        .await
        .expect("batch resource receipt");

        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result["items"][1]["code"], "not_attempted");
    }
}
