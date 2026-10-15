//! HTTP OAuth discovery and account isolation through the real MCP SDK.

mod flow;
mod headers;
mod runtime;

use arrpi_core::{
    config::Config,
    oauth::{Binding, Policy},
};
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    routing::get,
};
use oauth_resource_server::testing::TestAuthority;
use serde_json::{Value, json};

use super::{Server, server};
use crate::proxy::Gateway;

const RESOURCE: &str = "https://arrpi.example.test/v1/mcp";

async fn native(headers: HeaderMap) -> Json<Value> {
    assert_eq!(headers["x-api-key"], "fake-seerr-key");
    assert!(
        !headers.contains_key("authorization"),
        "external access tokens never reach native APIs"
    );
    let actor: u64 = headers["x-api-user"]
        .to_str()
        .expect("actor header")
        .parse()
        .expect("actor ID");

    Json(json!({"id":actor,"plexId":actor+100,"permissions":32}))
}

async fn quota(headers: HeaderMap) -> Json<Value> {
    let actor: u64 = headers["x-api-user"]
        .to_str()
        .expect("actor header")
        .parse()
        .expect("actor ID");

    Json(
        json!({"movie":{"limit":actor,"used":0,"remaining":actor,"days":7,"restricted":false},"tv":{"limit":actor,"used":0,"remaining":actor,"days":7,"restricted":false}}),
    )
}

async fn setup(issuer: &str) -> (Server, Server, Config) {
    let native = server(
        Router::new()
            .route("/api/v1/auth/me", get(native))
            .route(
                "/api/v1/status",
                get(|| async { Json(json!({"version":"3.5.0"})) }),
            )
            .route("/api/v1/user/{id}/quota", get(quota)),
    )
    .await;
    let mut config =
        Config::parse(include_str!("../../../../tests/arrpi.json")).expect("fake config");

    config
        .seerr
        .as_mut()
        .expect("Seerr")
        .url
        .clone_from(&native.url);
    config.mcp.public_url = Some(RESOURCE.into());
    config.oauth = Some(Policy {
        enabled: true,
        issuer: issuer.into(),
        required_scope: "arrpi:mcp".into(),
        bindings: [("alice", "requester-1"), ("bob", "requester-2")]
            .into_iter()
            .map(|(subject, client)| Binding {
                issuer: issuer.into(),
                subject: subject.into(),
                client: client.into(),
            })
            .collect(),
    });
    config.validate().expect("valid OAuth config");
    let gateway = Gateway::new(&config, tokio::time::Instant::now).expect("gateway");
    let api = server(crate::proxy::router(gateway)).await;

    (api, native, config)
}

async fn rpc(api: &Server, credential: &str, method: &str) -> reqwest::Response {
    let params = if method == "tools/call" {
        json!({"name":"my_quota","arguments":{}})
    } else {
        json!({})
    };

    reqwest::Client::new()
        .post(format!("{}v1/mcp", api.url))
        .bearer_auth(credential)
        .header("host", "arrpi.example.test")
        .header("accept", "application/json, text/event-stream")
        .header("mcp-protocol-version", "2025-06-18")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
        .send()
        .await
        .expect("MCP response")
}

#[tokio::test]
async fn oauth_challenges_and_metadata_are_public_but_not_privileged_routes() {
    let authority = TestAuthority::start().await;
    let (api, _native, _) = setup(authority.issuer()).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}v1/mcp", api.url))
        .send()
        .await
        .expect("challenge");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let challenge = response.headers()["www-authenticate"]
        .to_str()
        .expect("challenge header");

    assert!(
        challenge
            .contains("https://arrpi.example.test/.well-known/oauth-protected-resource/v1/mcp")
    );

    for path in [
        ".well-known/oauth-protected-resource",
        ".well-known/oauth-protected-resource/v1/mcp",
    ] {
        let metadata: Value = client
            .get(format!("{}{path}", api.url))
            .send()
            .await
            .expect("metadata")
            .json()
            .await
            .expect("metadata JSON");

        assert_eq!(metadata["resource"], RESOURCE);
        assert_eq!(metadata["authorization_servers"][0], authority.issuer());
        assert_eq!(metadata["scopes_supported"], json!(["arrpi:mcp"]));
    }

    let token = authority
        .token()
        .subject("alice")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .sign();
    let response = client
        .get(format!("{}v1/status", api.url))
        .bearer_auth(&token)
        .send()
        .await
        .expect("protected native route");

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "OAuth never authenticates non-MCP APIs"
    );
}

#[tokio::test]
async fn jwt_requesters_are_isolated_per_sdk_request_and_native_identity_remains_authoritative() {
    let authority = TestAuthority::start().await;
    let (api, _native, mut config) = setup(authority.issuer()).await;
    let alice = authority
        .token()
        .subject("alice")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .claim("roles", ["admin"])
        .sign();
    let bob = authority
        .token()
        .subject("bob")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .sign();
    let (first, second) = tokio::join!(
        rpc(&api, &alice, "tools/call"),
        rpc(&api, &bob, "tools/call")
    );

    for (response, actor) in [(first, 1), (second, 2)] {
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.expect("SDK result");

        assert_eq!(body["result"]["structuredContent"]["movie"]["limit"], actor);
    }

    let legacy = rpc(&api, "fake-requester-key-0000000000000000000", "tools/call").await;

    assert_eq!(
        legacy.status(),
        StatusCode::OK,
        "existing account keys keep working"
    );
    config
        .clients
        .iter_mut()
        .find(|client| client.name == "requester-1")
        .expect("client")
        .seerr_user
        .as_mut()
        .expect("binding")
        .plex_id = 999;
    let gateway = Gateway::new(&config, tokio::time::Instant::now).expect("replacement gateway");
    let changed = server(crate::proxy::router(gateway)).await;
    let response = rpc(&changed, &alice, "tools/call").await;
    let body: Value = response.json().await.expect("denied SDK result");

    assert_eq!(
        body["result"]["isError"], true,
        "OIDC identity cannot bypass the Plex/Seerr identity check"
    );
}

#[tokio::test]
async fn oauth_refusals_preserve_http_semantics_and_do_not_fallback_to_owner_credentials() {
    let authority = TestAuthority::start().await;
    let (api, _native, _) = setup(authority.issuer()).await;

    for (token, status, has_challenge) in [
        (
            authority
                .token()
                .subject("alice")
                .audience(RESOURCE)
                .scopes(["wrong"])
                .sign(),
            StatusCode::FORBIDDEN,
            true,
        ),
        (
            authority
                .token()
                .subject("alice")
                .audience("wrong")
                .scopes(["arrpi:mcp"])
                .sign(),
            StatusCode::UNAUTHORIZED,
            true,
        ),
        (
            authority
                .token()
                .subject("unknown")
                .audience(RESOURCE)
                .scopes(["arrpi:mcp"])
                .sign(),
            StatusCode::FORBIDDEN,
            false,
        ),
    ] {
        let response = rpc(&api, &token, "tools/list").await;

        assert_eq!(response.status(), status);
        assert_eq!(
            response.headers().contains_key("www-authenticate"),
            has_challenge
        );
        assert!(!response.text().await.expect("body").contains(&token));
    }
}
