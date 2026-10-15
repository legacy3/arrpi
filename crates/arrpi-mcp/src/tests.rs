//! Fake executors verify credential identity, schema conversion and SDK transport boundaries.

use arrpi_core::{
    error::ApiError,
    executor::{CommandExecutor, Executor},
    policy::Limits,
    registry::{Registry, ToolDefinition, ToolId},
    secret::Secret,
};
use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use rmcp::{ServerHandler as _, ServiceExt as _};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};
use tower::ServiceExt as _;

use crate::{HttpEndpoint, handler::Handler};

const FIRST: &str = "fake-mcp-account-a-00000000000000000";
const SECOND: &str = "fake-mcp-account-b-00000000000000000";

#[derive(Debug)]
struct Fake {
    registry: Registry,
}

#[async_trait]
impl CommandExecutor for Fake {
    async fn list(&self, credential: &str) -> Result<Vec<ToolDefinition>, ApiError> {
        allowed(credential)?;
        Ok(vec![self.registry.definition(ToolId::ListMovies).clone()])
    }

    async fn execute(
        &self,
        credential: &str,
        _tool: ToolId,
        _arguments: Value,
    ) -> Result<Value, ApiError> {
        allowed(credential)?;
        Ok(json!({"account": if credential == FIRST {"first"} else {"second"}}))
    }

    fn definition(&self, credential: Option<&str>, tool: ToolId) -> Option<ToolDefinition> {
        if let Some(credential) = credential {
            allowed(credential).ok()?;
        }

        Some(self.registry.definition(tool).clone())
    }
}

fn allowed(credential: &str) -> Result<(), ApiError> {
    if matches!(credential, FIRST | SECOND) {
        Ok(())
    } else {
        Err(ApiError::UNAUTHORIZED)
    }
}

fn executor() -> Executor {
    Executor::new(Fake {
        registry: Registry::new(Limits {
            max_page_items: 7,
            ..Limits::default()
        }),
    })
}

#[test]
fn sdk_projection_preserves_discovery_schema_and_protocol_hints() {
    let registry = Registry::new(Limits::default());

    for (id, read, destructive) in [
        (ToolId::ListMovies, true, false),
        (ToolId::RequestMovie, false, false),
        (ToolId::CancelRequest, false, true),
    ] {
        let definition = registry.definition(id);
        let wire = serde_json::to_value(crate::describe(definition.clone()))
            .expect("SDK tool serialization");

        let http = serde_json::to_value(arrpi_core::registry::CommandDescriptor::from(
            definition.clone(),
        ))
        .expect("HTTP command serialization");

        assert_eq!(
            wire, http,
            "HTTP and MCP discovery must describe the same command"
        );
        assert_eq!(wire["name"], id.name());
        assert_eq!(
            wire["inputSchema"],
            Value::Object(definition.input_schema().as_ref().clone())
        );
        assert_eq!(
            wire["annotations"],
            json!({
                "readOnlyHint": read, "destructiveHint": destructive,
                "idempotentHint": read, "openWorldHint": true,
            })
        );
    }
}

#[test]
fn stdio_schema_lookup_retains_its_fixed_caller_and_configured_limits() {
    let handler = Handler::stdio(executor(), FIRST.to_owned().into());
    let tool = handler
        .get_tool("list_movies")
        .expect("known caller schema");

    assert_eq!(tool.input_schema["properties"]["limit"]["maximum"], 7);
    assert!(handler.get_tool("unknown_tool").is_none());
    let rejected = Handler::stdio(executor(), "unknown-caller".to_owned().into());

    assert!(rejected.get_tool("list_movies").is_none());
}

fn router() -> Router {
    HttpEndpoint::new(
        executor(),
        Some("https://arrpi.example.invalid/v1/mcp"),
        Some("arrpi:8780"),
    )
    .expect("valid endpoint")
    .into()
}

fn call() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_movies","arguments":{}}})
}

fn request(value: &Value, host: &str, credential: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/mcp")
        .header(header::HOST, host)
        .header(header::AUTHORIZATION, format!("Bearer {credential}"))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18")
        .body(Body::from(value.to_string()))
        .expect("fake RPC request")
}

#[tokio::test]
async fn incoming_bearer_is_used_per_request_without_fallback() {
    for (credential, account) in [(FIRST, "first"), (SECOND, "second")] {
        let response = router()
            .oneshot(request(&call(), "arrpi:8780", credential))
            .await
            .expect("SDK response");

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 128 * 1024)
            .await
            .expect("bounded response");
        let value: Value = serde_json::from_slice(&bytes).expect("SDK JSON");

        assert_eq!(
            value["result"]["structuredContent"]["account"], account,
            "actor changes with each incoming bearer"
        );
    }

    let mut duplicate = request(&call(), "arrpi:8780", FIRST);

    duplicate.headers_mut().append(
        header::AUTHORIZATION,
        format!("Bearer {SECOND}").parse().expect("second bearer"),
    );
    let response = router()
        .oneshot(duplicate)
        .await
        .expect("duplicate response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let mut missing = request(&call(), "arrpi:8780", FIRST);

    missing.headers_mut().remove(header::AUTHORIZATION);
    assert_eq!(
        router()
            .oneshot(missing)
            .await
            .expect("missing response")
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn exact_host_port_origin_and_body_limits_are_enforced() {
    let wrong_port = router()
        .oneshot(request(&call(), "arrpi:8781", FIRST))
        .await
        .expect("host check");

    assert_eq!(wrong_port.status(), StatusCode::FORBIDDEN);
    let mut wrong_origin = request(&call(), "arrpi:8780", FIRST);

    wrong_origin.headers_mut().insert(
        header::ORIGIN,
        "https://evil.example.invalid".parse().expect("origin"),
    );
    assert_eq!(
        router()
            .oneshot(wrong_origin)
            .await
            .expect("origin check")
            .status(),
        StatusCode::FORBIDDEN
    );
    let mut valid_origin = request(&call(), "arrpi.example.invalid", FIRST);

    valid_origin.headers_mut().insert(
        header::ORIGIN,
        "https://arrpi.example.invalid".parse().expect("origin"),
    );
    assert_eq!(
        router()
            .oneshot(valid_origin)
            .await
            .expect("allowed public origin")
            .status(),
        StatusCode::OK
    );
    let large = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_movies","arguments":{"padding":"x".repeat(129*1024)}}});

    assert_eq!(
        router()
            .oneshot(request(&large, "arrpi:8780", FIRST))
            .await
            .expect("body bound")
            .status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[test]
fn proxy_authorities_cannot_broaden_sdk_hosts() {
    for host in [
        "arrpi",
        "*",
        "arrpi:*",
        "http://arrpi:8780",
        "user@arrpi:8780",
        "arrpi:8780/",
        "arrpi:8780 ",
        "arrpi:65536",
    ] {
        HttpEndpoint::new(executor(), None, Some(host))
            .expect_err("proxy allowlist requires an exact host and numeric port");
    }
}

#[tokio::test]
async fn stdio_binding_emits_protocol_frames_with_configured_schemas() {
    let (server, client) = tokio::io::duplex(16 * 1024);
    let task = tokio::spawn(async move {
        let service = Handler::stdio(executor(), Secret::from(FIRST.to_owned()))
            .serve(server)
            .await
            .expect("stdio initialize");

        service.waiting().await.expect("stdio EOF");
    });
    let (reader, mut writer) = tokio::io::split(client);
    let mut reader = BufReader::new(reader);
    let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}});

    writer
        .write_all(format!("{initialize}\n").as_bytes())
        .await
        .expect("initialize frame");
    let mut line = String::new();

    reader.read_line(&mut line).await.expect("initialize reply");
    let initialized: Value = serde_json::from_str(&line).expect("stdout is only a JSON-RPC frame");

    assert_eq!(initialized["result"]["serverInfo"]["name"], "arrpi");
    writer
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .expect("initialized notification");
    writer
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n")
        .await
        .expect("list frame");
    line.clear();
    reader.read_line(&mut line).await.expect("list reply");
    let tools: Value = serde_json::from_str(&line).expect("tool frame");

    assert_eq!(
        tools["result"]["tools"][0]["inputSchema"]["properties"]["limit"]["maximum"], 7,
        "configured core schema reaches the SDK without regeneration"
    );
    assert!(
        !line.contains(FIRST),
        "credential never appears in protocol output"
    );
    let call = call();

    writer
        .write_all(format!("{call}\n").as_bytes())
        .await
        .expect("call frame");
    line.clear();
    reader.read_line(&mut line).await.expect("call reply");
    let result: Value = serde_json::from_str(&line).expect("tool call frame");

    assert_eq!(result["result"]["structuredContent"]["account"], "first");
    assert!(!line.contains(FIRST), "call results exclude credentials");
    drop(writer);
    drop(reader);
    task.await.expect("stdio worker");
}

#[tokio::test]
async fn unknown_credentials_and_tools_have_distinct_protocol_errors() {
    let mut unknown = call();

    unknown["params"]["name"] = json!("missing_tool");
    let response = router()
        .oneshot(request(&unknown, "arrpi:8780", FIRST))
        .await
        .expect("unknown tool response");
    let bytes = to_bytes(response.into_body(), 128 * 1024)
        .await
        .expect("bounded error");
    let value: Value = serde_json::from_slice(&bytes).expect("protocol error");

    assert_eq!(value["error"]["code"], -32602);

    for body in [
        call(),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    ] {
        let response = router()
            .oneshot(request(
                &body,
                "arrpi:8780",
                "fake-unknown-credential-000000000000",
            ))
            .await
            .expect("unknown credential response");
        let bytes = to_bytes(response.into_body(), 128 * 1024)
            .await
            .expect("bounded auth error");
        let value: Value = serde_json::from_slice(&bytes).expect("protocol auth error");

        assert_eq!(value["error"]["code"], -32001);
        assert_eq!(value["error"]["data"]["code"], "authentication_required");
    }
}

#[tokio::test]
async fn missing_host_and_stateless_methods_fail_closed() {
    let mut missing = request(&call(), "arrpi:8780", FIRST);

    missing.headers_mut().remove(header::HOST);
    assert_eq!(
        router()
            .oneshot(missing)
            .await
            .expect("missing host response")
            .status(),
        StatusCode::BAD_REQUEST
    );

    for method in [axum::http::Method::GET, axum::http::Method::DELETE] {
        let mut request = request(&call(), "arrpi:8780", FIRST);

        *request.method_mut() = method;
        assert_eq!(
            router()
                .oneshot(request)
                .await
                .expect("stateless method response")
                .status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
    }
}

#[tokio::test]
async fn bearer_scheme_is_case_insensitive_without_relaxing_token_whitespace() {
    for scheme in ["bearer", "bEaReR"] {
        let mut request = request(&call(), "arrpi:8780", FIRST);

        request.headers_mut().insert(
            header::AUTHORIZATION,
            format!("{scheme} {FIRST}").parse().expect("header"),
        );
        assert_eq!(
            router()
                .oneshot(request)
                .await
                .expect("case-insensitive bearer")
                .status(),
            StatusCode::OK
        );
    }

    for raw in [
        format!("Bearer  {FIRST}"),
        format!("Bearer {FIRST} "),
        format!("Basic {FIRST}"),
    ] {
        let mut request = request(&call(), "arrpi:8780", FIRST);

        request
            .headers_mut()
            .insert(header::AUTHORIZATION, raw.parse().expect("header"));
        assert_eq!(
            router()
                .oneshot(request)
                .await
                .expect("strict credential whitespace")
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
}
