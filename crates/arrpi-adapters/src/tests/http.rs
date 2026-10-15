//! Bounded transport and retained-buffer ownership use local fake sockets only.
use crate::{
    budget::{self, Budget},
    cache::{Cache, Pruner},
    http::{Http, HttpOptions, Intent, bounded},
    metrics::Metrics,
};
use arrpi_core::{config::CachePolicy, error::ApiError, source::ReadMode};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    task::JoinHandle,
    time::Instant,
};

#[derive(Debug)]
struct Server {
    task: JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn response(wire: Vec<u8>, hold: bool) -> (Server, reqwest::Response) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("fake HTTP listener");
    let address = listener.local_addr().expect("listener address");
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("fake HTTP connection");
        let mut request = Vec::with_capacity(1024);
        let mut chunk = [0; 512];

        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let size = socket.read(&mut chunk).await.expect("read fake request");

            if size == 0 {
                return;
            }

            request.extend_from_slice(&chunk[..size]);
        }

        let _ = socket.write_all(&wire).await;

        if hold {
            std::future::pending::<()>().await;
        }

        let _ = socket.shutdown().await;
    });
    let http = Http::new(HttpOptions {
        timeout: Duration::from_secs(2),
        connect_timeout: Duration::from_secs(1),
        pool_idle_per_host: 1,
    })
    .expect("fake transport");
    let response = http
        .client()
        .get(format!("http://{address}/"))
        .send()
        .await
        .expect("HTTP fixture response");

    (Server { task }, response)
}

fn wire(body: &str, length: usize) -> Vec<u8> {
    format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}")
        .into_bytes()
}

#[tokio::test]
async fn actual_backing_capacity_stays_charged_until_last_cloned_slice_drops() {
    let budget = Budget::new(64).expect("budget");
    let (_server, response) = response(wire("1234567", 7), false).await;
    let bytes = bounded(response, 32, Intent::Read, Some(&budget))
        .await
        .expect("bounded response");

    assert_eq!(
        budget.used(),
        7,
        "unused reservation returned after collection"
    );
    let cloned = bytes.clone();
    let slice = bytes.slice(1..4);

    drop(bytes);
    drop(cloned);
    assert_eq!(budget.used(), 7, "slice owns complete backing capacity");
    assert_eq!(&*slice, b"234");
    drop(slice);
    assert_eq!(budget.used(), 0, "final owner releases reservation");
}

#[tokio::test]
async fn truncated_oversized_and_chunked_bodies_keep_operation_intent() {
    for (wire, limit, read_code) in [
        (wire("{}", 20), 32, "upstream_unavailable"),
        (wire("12345", 5), 4, "upstream_response_too_large"),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n12345\r\n0\r\n\r\n"
                .to_vec(),
            4,
            "upstream_response_too_large",
        ),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{}\r\n".to_vec(),
            32,
            "upstream_unavailable",
        ),
    ] {
        for (intent, code) in [
            (Intent::Read, read_code),
            (Intent::Write, "unknown_outcome"),
        ] {
            let budget = Budget::new(64).expect("budget");
            let (_server, response) = response(wire.clone(), false).await;
            let error = bounded(response, limit, intent, Some(&budget))
                .await
                .expect_err("invalid bounded body");

            assert_eq!(error.code(), code);
            assert_eq!(budget.used(), 0, "errors release collection reservation");
        }
    }
}

#[tokio::test]
async fn cancellation_releases_the_in_progress_collection_reservation() {
    let budget = Budget::new(64).expect("budget");
    let (_server, response) = response(wire("x", 16), true).await;
    let collecting = budget.clone();
    let task =
        tokio::spawn(async move { bounded(response, 32, Intent::Read, Some(&collecting)).await });

    tokio::time::timeout(Duration::from_secs(1), async {
        while budget.used() == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("collector acquired reservation");
    task.abort();
    assert!(task.await.expect_err("cancelled collection").is_cancelled());
    assert_eq!(budget.used(), 0, "cancelled future drops reservation");
}

#[tokio::test(start_paused = true)]
async fn pressure_prunes_expired_cross_source_caches_but_never_releases_held_bytes() {
    let policy = CachePolicy {
        fresh_seconds: 1,
        stale_seconds: 1,
        ..CachePolicy::default()
    };
    let first = Arc::new(Cache::new(policy, Instant::now));
    let second = Arc::new(Cache::new(policy, Instant::now));
    let pruner = Pruner::new(vec![Arc::downgrade(&first), Arc::downgrade(&second)]);
    let budget = Budget::new(32).expect("budget");

    budget.attach(pruner).expect("one cache group");
    let bytes = budget::leased(
        b"1234567".to_vec(),
        budget.reserve(32).await.expect("initial capacity"),
    )
    .expect("leased bytes");
    let outgoing = bytes.clone();

    drop(
        first
            .get(
                "source-a".into(),
                &Metrics::new(),
                ReadMode::Cached,
                async { Ok(bytes) },
            )
            .await
            .expect("cached response"),
    );
    tokio::time::advance(Duration::from_secs(2)).await;
    let blocked = second
        .get(
            "source-b".into(),
            &Metrics::new(),
            ReadMode::Cached,
            async {
                let reservation = budget.reserve(32).await?;

                budget::leased(b"[]".to_vec(), reservation)
            },
        )
        .await;

    assert_eq!(
        blocked
            .expect_err("held source-a body still charged")
            .code(),
        "busy"
    );
    assert_eq!(budget.used(), 7);
    drop(outgoing);
    let response = second
        .get("source-b".into(), &Metrics::new(), ReadMode::Fresh, async {
            let reservation = budget.reserve(32).await?;

            budget::leased(b"[]".to_vec(), reservation)
        })
        .await
        .expect("released source-a capacity allows source-b");

    assert_eq!(budget.used(), 2);
    drop(response);
    drop(second);
    assert_eq!(
        budget.used(),
        0,
        "cache eviction releases only its own backing reference"
    );
}

#[tokio::test(start_paused = true)]
async fn periodic_maintenance_is_weak_and_has_an_explicit_lifetime() {
    let cache = Arc::new(Cache::new(CachePolicy::default(), Instant::now));
    let weak = Arc::downgrade(&cache);
    let pruner = Pruner::new(vec![std::sync::Weak::clone(&weak)]);
    let guard = pruner.start().expect("async runtime");

    drop(cache);
    assert!(
        weak.upgrade().is_none(),
        "maintenance never keeps source caches alive"
    );
    drop(guard);
    Budget::new(0).expect_err("zero capacity must fail");
    assert_eq!(ApiError::CAPACITY.code(), "busy");
}

#[tokio::test]
async fn small_chunked_bodies_trim_unused_capacity_before_retention() {
    let budget = Budget::new(64).expect("budget");
    let wire =
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\n12345\r\n0\r\n\r\n".to_vec();
    let (_server, response) = response(wire, false).await;
    let bytes = bounded(response, 32, Intent::Read, Some(&budget))
        .await
        .expect("chunked body");

    assert_eq!(bytes.len(), 5);
    assert_eq!(
        budget.used(),
        5,
        "the initial chunk buffer does not become a retained capacity floor"
    );
    drop(bytes);
    assert_eq!(budget.used(), 0);
}
