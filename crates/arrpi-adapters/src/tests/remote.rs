//! Remote tool responses retain receipts and uncertain writes without retries.
mod discovery;

use crate::remote::RemoteApi;
use arrpi_core::{error::ApiError, executor::CommandExecutor, registry::ToolId};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
const CLIENT_KEY: &str = "fake-client-key-00000000000000000000";
#[derive(Debug)]
struct Server {
    url: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
#[derive(Debug)]
struct Remote(RemoteApi);
impl Remote {
    async fn call(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        self.0
            .execute(
                CLIENT_KEY,
                ToolId::parse(name).expect("known test tool"),
                args,
            )
            .await
    }
}
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn ipv6_loopback_is_allowed_but_plaintext_remote_hosts_are_not() {
    RemoteApi::new("http://[::1]:8780/", false).expect("loopback allowed");
    RemoteApi::new("http://[2001:db8::1]:8780/", false).expect_err("remote HTTP rejected");
}

#[tokio::test]
async fn incomplete_and_invalid_write_responses_remain_unknown() {
    let oversized = format!("{{\"padding\":\"{}\"}}", "x".repeat(8 * 1024 * 1024));

    for reply in [
        Vec::new(),
        response(200, "", None),
        response(206, "{}", None),
        response(200, "{\"request_id\":7", Some(100)),
        response(200, "not JSON", None),
        response(200, "null", None),
        response(200, "[]", None),
        response(200, &oversized, None),
    ] {
        let (server, calls) = fixture(reply).await;
        let error = remote(&server)
            .call("request_movie", json!({"tmdb_id":42}))
            .await
            .expect_err("ambiguous response cannot prove the write failed");

        assert_eq!(error.code(), "unknown_outcome");
        assert!(error.payload()["follow_up"].as_str().is_some());
        assert_eq!(calls.load(Ordering::Relaxed), 1, "writes never retry");
    }
}

#[tokio::test]
async fn read_response_failures_keep_their_read_categories() {
    let oversized = format!("{{\"padding\":\"{}\"}}", "x".repeat(8 * 1024 * 1024));

    for (reply, code) in [
        (response(200, "{", Some(100)), "upstream_unavailable"),
        (response(200, "not JSON", None), "invalid_upstream_response"),
        (response(200, "[]", None), "invalid_upstream_response"),
        (response(200, &oversized, None), "invalid_upstream_response"),
    ] {
        let (server, calls) = fixture(reply).await;
        let error = remote(&server)
            .call("my_quota", json!({}))
            .await
            .expect_err("invalid read response");

        assert_eq!(error.code(), code);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}

#[tokio::test]
async fn complete_structured_errors_and_known_write_receipts_are_preserved() {
    for (status, payload) in [
        (429, json!({"code":"queue_timeout_retry_safe"})),
        (429, json!({"code":"quota_exceeded"})),
        (403, json!({"code":"permission_denied"})),
        (502, json!({"code":"unknown_outcome","request_id":7})),
    ] {
        let (server, calls) = fixture(response(status, &payload.to_string(), None)).await;
        let error = remote(&server)
            .call("request_movie", json!({"tmdb_id":42}))
            .await
            .expect_err("structured rejection");

        assert_eq!(error.code(), payload["code"]);

        if payload.get("request_id").is_some() {
            assert_eq!(error.payload()["request_id"], 7);
        }

        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    let receipt = json!({"request_id":7,"status":"approved","profile_applied":true});
    let (server, calls) = fixture(response(200, &receipt.to_string(), None)).await;
    let result = remote(&server)
        .call("request_movie", json!({"tmdb_id":42}))
        .await
        .expect("known successful receipt");

    assert_eq!(result, receipt);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn malformed_server_errors_do_not_claim_a_write_is_retry_safe() {
    for reply in [
        response(502, "private upstream failure", None),
        response(303, "", None),
        response(400, "private proxy rejection", None),
        response(429, "{\"code\":\"queue_timeout_retry_safe\"}", Some(100)),
    ] {
        let (server, _calls) = fixture(reply).await;
        let error = remote(&server)
            .call("request_movie", json!({"tmdb_id":42}))
            .await
            .expect_err("unconfirmed mutation");

        assert_eq!(error.code(), "unknown_outcome");
        assert!(!error.payload().to_string().contains("private upstream"));
    }
}

fn remote(server: &Server) -> Remote {
    Remote(RemoteApi::new(&server.url, false).expect("local remote connection"))
}

fn response(status: u16, body: &str, length: Option<usize>) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        length.unwrap_or(body.len())
    )
    .into_bytes()
}

async fn fixture(reply: Vec<u8>) -> (Server, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind remote fixture");
    let address = listener.local_addr().expect("fixture address");
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&calls);
    let task = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.expect("accept fixture request");
            let mut request = Vec::with_capacity(1024);
            let mut chunk = [0; 512];

            while !complete_request(&request) {
                let size = socket.read(&mut chunk).await.expect("read fixture request");

                assert!(size > 0, "client sends a complete request");
                request.extend_from_slice(&chunk[..size]);
            }

            seen.fetch_add(1, Ordering::Relaxed);
            let _ = socket.write_all(&reply).await;
            let _ = socket.shutdown().await;
        }
    });

    (
        Server {
            url: format!("http://{address}/"),
            task,
        },
        calls,
    )
}

fn complete_request(request: &[u8]) -> bool {
    let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let headers = String::from_utf8_lossy(&request[..headers_end]);
    let body_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;

            name.eq_ignore_ascii_case("content-length").then(|| {
                value
                    .trim()
                    .parse::<usize>()
                    .expect("fixture content length")
            })
        })
        .unwrap_or(0);

    request.len() >= headers_end + 4 + body_length
}

#[derive(Debug, Default)]
struct Seen {
    requests: tokio::sync::Mutex<Vec<(String, String, serde_json::Value)>>,
}

async fn central(
    axum::extract::State(seen): axum::extract::State<Arc<Seen>>,
    request: axum::extract::Request,
) -> axum::response::Response {
    use axum::response::IntoResponse as _;
    let path = request.uri().path().to_owned();
    let key = request
        .headers()
        .get("authorization")
        .and_then(|header| header.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let body = axum::body::to_bytes(request.into_body(), 4096)
        .await
        .expect("bounded fake arguments");
    let args = if body.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&body).expect("fake arguments")
    };

    seen.requests.lock().await.push((path.clone(), key, args));
    let data = if path.ends_with("/tools") {
        json!({"tools":[{"name":"my_quota","description":"Account quota","inputSchema":{"type":"object","properties":{"no_cache":{"type":"boolean"}}}},{"name":"unsafe_unknown_tool","inputSchema":{}}]})
    } else {
        json!({"ok":true})
    };

    ([("x-arrpi-cache", "hit"), ("age", "7")], axum::Json(data)).into_response()
}

async fn central_fixture() -> (Server, Arc<Seen>) {
    let seen = Arc::new(Seen::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("central listener");
    let address = listener.local_addr().expect("central address");
    let router = axum::Router::new()
        .route("/{*path}", axum::routing::any(central))
        .with_state(Arc::clone(&seen));
    let task = tokio::spawn(async move {
        axum::serve(listener, router)
            .await
            .expect("central fixture");
    });

    (
        Server {
            url: format!("http://{address}/base/"),
            task,
        },
        seen,
    )
}

#[tokio::test]
async fn remote_uses_only_each_supplied_credential_and_discovers_known_schemas() {
    let (server, seen) = central_fixture().await;
    let remote = RemoteApi::new(&server.url, true).expect("remote");
    let tools = remote.list(CLIENT_KEY).await.expect("remote list");

    assert_eq!(
        tools.len(),
        1,
        "unknown upstream names never become executable tools"
    );
    assert_eq!(tools[0].name, "my_quota");
    remote
        .execute(CLIENT_KEY, ToolId::MyQuota, json!({}))
        .await
        .expect("first account");
    remote
        .execute("second-key", ToolId::MyQuota, json!({"no_cache":false}))
        .await
        .expect("second account");
    assert_eq!(
        remote
            .execute("", ToolId::MyQuota, json!({}))
            .await
            .expect_err("missing caller key")
            .code(),
        "authentication_required"
    );
    let requests = seen.requests.lock().await;

    assert_eq!(
        requests.len(),
        3,
        "missing credential cannot reach the server"
    );
    assert_eq!(requests[0].0, "/base/v1/tools");
    assert_eq!(requests[1].1, format!("Bearer {CLIENT_KEY}"));
    assert_eq!(requests[2].1, "Bearer second-key");
    assert_eq!(requests[1].2["no_cache"], true);
    assert_eq!(
        requests[2].2["no_cache"], true,
        "CLI --no-cache forces a fresh read"
    );
}

#[tokio::test]
async fn cli_api_keeps_cache_metadata_and_rejects_base_path_escapes_before_dispatch() {
    let (server, seen) = central_fixture().await;
    let api = crate::remote::ApiClient::new(&server.url, false).expect("API");
    let response = api
        .get(Some(CLIENT_KEY), "v1/status", "")
        .await
        .expect("status");

    assert_eq!(response.data["ok"], true);
    assert_eq!(response.cache_state.as_deref(), Some("hit"));
    assert_eq!(response.age_seconds, Some(7));

    for path in [
        "/outside",
        "../outside",
        "https://evil.invalid/",
        "v1/status?token=bad",
        "v1/status#bad",
    ] {
        api.get(Some(CLIENT_KEY), path, "")
            .await
            .expect_err("unsafe path");
    }

    assert_eq!(
        seen.requests.lock().await.len(),
        1,
        "escape attempts never dispatch"
    );
}
