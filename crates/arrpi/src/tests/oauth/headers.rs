//! Bearer header grammar and admission remain bounded before key fetching.

use axum::http::{StatusCode, header};
use oauth_resource_server::testing::TestAuthority;
use serde_json::{Value, json};

use super::{RESOURCE, rpc, setup};
use crate::{proxy::Gateway, tests::server};

#[tokio::test]
async fn multiple_spaces_are_valid_but_duplicate_credentials_are_rejected() {
    let authority = TestAuthority::start().await;
    let (api, _native, _) = setup(authority.issuer()).await;
    let token = authority
        .token()
        .subject("alice")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .sign();

    for (duplicate, expected) in [(false, StatusCode::OK), (true, StatusCode::UNAUTHORIZED)] {
        let mut request = reqwest::Client::new().post(format!("{}v1/mcp", api.url))
            .header(header::AUTHORIZATION, format!("Bearer   {token}"))
            .header("host", "arrpi.example.test")
            .header("accept", "application/json, text/event-stream")
            .header("mcp-protocol-version", "2025-06-18")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"my_quota","arguments":{}}}));

        if duplicate {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }

        let response = request.send().await.expect("header response");

        assert_eq!(response.status(), expected);

        if expected == StatusCode::OK {
            let body: Value = response.json().await.expect("MCP body");

            assert_eq!(body["result"]["structuredContent"]["movie"]["limit"], 1);
        }
    }
}

#[tokio::test]
async fn saturated_requests_do_not_start_oauth_discovery() {
    let authority = TestAuthority::start().await;
    let (_unused, _native, mut config) = setup(authority.issuer()).await;

    config.cache.max_requests = 1;
    let gateway = Gateway::new(&config, tokio::time::Instant::now).expect("gateway");
    let permit = std::sync::Arc::clone(&gateway.requests)
        .try_acquire_owned()
        .expect("reserve capacity");
    let api = server(crate::proxy::router(gateway)).await;
    let token = authority
        .token()
        .subject("alice")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .sign();

    assert_eq!(
        rpc(&api, &token, "tools/call").await.status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(authority.discovery_fetches(), 0);
    assert_eq!(authority.jwks_fetches(), 0);
    drop(permit);
    assert_eq!(
        rpc(&api, &token, "tools/call").await.status(),
        StatusCode::OK
    );
}
