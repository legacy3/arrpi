//! Accepted connection quota and idle expiry regressions.

use axum::serve::Listener as _;
use tokio::{
    io::AsyncReadExt as _,
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};

use crate::{config::CachePolicy, transport::LimitedListener};

#[tokio::test]
async fn accepted_connection_retains_quota_until_dropped() {
    let socket = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture");
    let address = socket.local_addr().expect("fixture address");
    let policy = CachePolicy {
        max_connections: 1,
        ..CachePolicy::default()
    };
    let mut listener = LimitedListener::new(socket, policy, Instant::now);
    let first_client = TcpStream::connect(address).await.expect("first client");
    let (first, _) = listener.accept().await;
    let second_client = TcpStream::connect(address)
        .await
        .expect("second queued socket");

    assert!(
        tokio::time::timeout(Duration::from_millis(20), listener.accept())
            .await
            .is_err(),
        "connection slot is still leased"
    );
    drop(first);

    let (second, _) = tokio::time::timeout(Duration::from_millis(100), listener.accept())
        .await
        .expect("released quota admits second client");

    drop((first_client, second_client, second));
}

#[tokio::test(start_paused = true)]
async fn idle_connection_expires_without_progress() {
    let socket = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fixture");
    let address = socket.local_addr().expect("fixture address");
    let policy = CachePolicy {
        connection_idle_seconds: 5,
        ..CachePolicy::default()
    };
    let mut listener = LimitedListener::new(socket, policy, Instant::now);
    let client = TcpStream::connect(address).await.expect("fixture client");
    let (mut connection, _) = listener.accept().await;

    tokio::time::advance(Duration::from_secs(6)).await;

    let mut buffer = [0_u8; 1];
    let error = connection
        .read(&mut buffer)
        .await
        .expect_err("idle reader times out");

    assert_eq!(
        error.kind(),
        std::io::ErrorKind::TimedOut,
        "deadline is enforced at I/O boundary"
    );
    drop(client);
}
