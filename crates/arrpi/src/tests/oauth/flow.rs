//! A fake authorization server exercises a real PKCE/resource authorization-code flow.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use axum::{
    Form, Json, Router,
    extract::{Query, State},
    http::{StatusCode, header},
    response::{IntoResponse as _, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use oauth_resource_server::{
    Algorithm,
    testing::{KID_A, jwk_rsa_a, mint_with, now},
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{RESOURCE, rpc, setup};
use crate::tests::server;

#[derive(Debug, Default)]
struct Authority {
    issuer: OnceLock<String>,
    pending: Mutex<Option<(String, String)>>,
}

async fn metadata(State(state): State<Arc<Authority>>) -> Json<Value> {
    let issuer = state.issuer.get().expect("initialized authority");

    Json(
        json!({"issuer":issuer,"jwks_uri":format!("{issuer}jwks"),"authorization_endpoint":format!("{issuer}authorize"),"token_endpoint":format!("{issuer}token"),"code_challenge_methods_supported":["S256"],"response_types_supported":["code"],"grant_types_supported":["authorization_code"],"token_endpoint_auth_methods_supported":["none"]}),
    )
}

async fn authorize(
    State(state): State<Arc<Authority>>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if query.get("client_id").map(String::as_str) != Some("fixture-client")
        || query.get("redirect_uri").map(String::as_str)
            != Some("https://client.example.test/callback")
        || query.get("resource").map(String::as_str) != Some(RESOURCE)
        || query.get("code_challenge_method").map(String::as_str) != Some("S256")
        || query.get("response_type").map(String::as_str) != Some("code")
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let challenge = query.get("code_challenge").expect("PKCE challenge").clone();

    *state.pending.lock().expect("grant lock") = Some((challenge, RESOURCE.into()));
    let mut callback = url::Url::parse("https://client.example.test/callback").expect("callback");

    callback
        .query_pairs_mut()
        .append_pair("code", "throwaway-authorization-code")
        .append_pair("state", query.get("state").expect("state"));
    (StatusCode::FOUND, [(header::LOCATION, callback.as_str())]).into_response()
}

async fn token(
    State(state): State<Arc<Authority>>,
    Form(form): Form<HashMap<String, String>>,
) -> Response {
    let grant = state.pending.lock().expect("grant lock").take();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(
        form.get("code_verifier")
            .map_or("", String::as_str)
            .as_bytes(),
    ));

    if form.get("grant_type").map(String::as_str) != Some("authorization_code")
        || form.get("client_id").map(String::as_str) != Some("fixture-client")
        || form.get("code").map(String::as_str) != Some("throwaway-authorization-code")
        || grant != Some((challenge, form.get("resource").cloned().unwrap_or_default()))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let instant = now();
    let access = mint_with(
        Algorithm::RS256,
        Some(KID_A),
        Some("at+jwt"),
        &json!({"iss":state.issuer.get().expect("issuer"),"sub":"alice","aud":RESOURCE,"scope":"arrpi:mcp","iat":instant,"exp":instant+300,"client_id":"fixture-client","jti":"fixture-once"}),
    );

    Json(json!({"access_token":access,"token_type":"Bearer","expires_in":300,"scope":"arrpi:mcp"}))
        .into_response()
}

#[tokio::test]
async fn protected_resource_discovery_then_pkce_resource_code_flow_reaches_bound_mcp_tools() {
    let state = Arc::new(Authority::default());
    let authority = server(
        Router::new()
            .route("/.well-known/oauth-authorization-server", get(metadata))
            .route("/.well-known/openid-configuration", get(metadata))
            .route(
                "/jwks",
                get(|| async { Json(json!({"keys":[jwk_rsa_a()]})) }),
            )
            .route("/authorize", get(authorize))
            .route("/token", post(token))
            .with_state(Arc::clone(&state)),
    )
    .await;

    state.issuer.set(authority.url.clone()).expect("set issuer");
    let (api, _native, _) = setup(&authority.url).await;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("client");
    let protected: Value = client
        .get(format!(
            "{}.well-known/oauth-protected-resource/v1/mcp",
            api.url
        ))
        .send()
        .await
        .expect("resource metadata")
        .json()
        .await
        .expect("metadata JSON");

    assert_eq!(protected["resource"], RESOURCE);
    let issuer = protected["authorization_servers"][0]
        .as_str()
        .expect("issuer");
    let metadata: Value = client
        .get(format!("{issuer}.well-known/oauth-authorization-server"))
        .send()
        .await
        .expect("AS discovery")
        .json()
        .await
        .expect("AS metadata");

    assert_eq!(
        metadata["code_challenge_methods_supported"],
        json!(["S256"])
    );
    let verifier = "fixture-pkce-verifier-abcdefghijklmnopqrstuvwxyz-0123456789";
    let authorization = authorization_url(&metadata, verifier);
    let response = client.get(authorization).send().await.expect("authorize");

    assert_eq!(response.status(), StatusCode::FOUND);
    let callback = url::Url::parse(
        response.headers()[header::LOCATION]
            .to_str()
            .expect("redirect"),
    )
    .expect("callback");
    let values: HashMap<_, _> = callback.query_pairs().into_owned().collect();

    assert_eq!(values.get("state").expect("state"), "fixture-csrf-state");
    let body = grant_body(values.get("code").expect("authorization code"), verifier);
    let endpoint = metadata["token_endpoint"].as_str().expect("token endpoint");
    let response = client
        .post(endpoint)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body.clone())
        .send()
        .await
        .expect("token exchange");

    assert_eq!(response.status(), StatusCode::OK);
    let token_response: Value = response.json().await.expect("token JSON");
    let response = rpc(
        &api,
        token_response["access_token"]
            .as_str()
            .expect("access token"),
        "tools/call",
    )
    .await;
    let result: Value = response.json().await.expect("MCP JSON");

    assert_eq!(result["result"]["structuredContent"]["movie"]["limit"], 1);
    let replay = client
        .post(endpoint)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .expect("replay response");

    assert_eq!(
        replay.status(),
        StatusCode::BAD_REQUEST,
        "authorization code is single-use"
    );
}

fn authorization_url(metadata: &Value, verifier: &str) -> url::Url {
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut authorization = url::Url::parse(
        metadata["authorization_endpoint"]
            .as_str()
            .expect("authorize URL"),
    )
    .expect("URL");

    authorization.query_pairs_mut().extend_pairs([
        ("client_id", "fixture-client"),
        ("redirect_uri", "https://client.example.test/callback"),
        ("response_type", "code"),
        ("resource", RESOURCE),
        ("scope", "arrpi:mcp"),
        ("code_challenge_method", "S256"),
        ("code_challenge", challenge.as_str()),
        ("state", "fixture-csrf-state"),
    ]);
    authorization
}

fn grant_body(code: &str, verifier: &str) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("grant_type", "authorization_code"),
            ("client_id", "fixture-client"),
            ("code", code),
            ("code_verifier", verifier),
            ("resource", RESOURCE),
        ])
        .finish()
}
