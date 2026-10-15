//! Live database settings retain healthy keys while replacing account authorization.

use std::{path::Path, sync::Arc};

use arrpi_core::{config::Config, error::ApiError};
use axum::http::StatusCode;
use oauth_resource_server::testing::TestAuthority;
use rusqlite::Connection;
use serde_json::{Value, json};

use super::{RESOURCE, rpc, setup};
use crate::{
    proxy::Gateway,
    settings::{Runtime, Store},
    tests::server,
};

fn build(config: &Config, _database: &Path, _store: Store) -> Result<Gateway, ApiError> {
    Gateway::new(config, tokio::time::Instant::now).map_err(|error| {
        tracing::debug!(event = "oauth_fixture", category = error.reason());
        ApiError::BAD_QUERY
    })
}

async fn read(api: &crate::tests::Server, key: &str) -> Value {
    reqwest::Client::new()
        .get(format!("{}v1/config", api.url))
        .bearer_auth(key)
        .send()
        .await
        .expect("settings response")
        .json()
        .await
        .expect("settings JSON")
}

async fn save(api: &crate::tests::Server, key: &str, document: Value) {
    let response = reqwest::Client::new()
        .put(format!("{}v1/config", api.url))
        .bearer_auth(key)
        .json(&document)
        .send()
        .await
        .expect("settings update");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn live_saves_retain_jwks_during_outages_but_revoke_bindings_and_disable_oauth() {
    let authority = TestAuthority::start().await;
    let (_unused, _native, mut config) = setup(authority.issuer()).await;

    config.clients[0].permissions.push("config:manage".into());
    let admin_key = config.clients[0].key.value().to_owned();
    let store =
        Arc::new(Store::new(Connection::open_in_memory().expect("SQLite").into()).expect("store"));

    store.seed(&config).expect("seed");
    let api = server(
        Runtime::new(store, "oauth-fixture.sqlite".into(), build)
            .expect("runtime")
            .router(),
    )
    .await;
    let token = authority
        .token()
        .subject("alice")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .sign();

    assert_eq!(
        rpc(&api, &token, "tools/call").await.status(),
        StatusCode::OK
    );
    drop(authority);
    let mut document = read(&api, &admin_key).await;

    document["settings"]["feed"]["title"] = json!("Unrelated title update");
    save(&api, &admin_key, document).await;
    assert_eq!(
        rpc(&api, &token, "tools/call").await.status(),
        StatusCode::OK,
        "an unrelated settings save preserves verified keys during an AS outage"
    );
    let mut document = read(&api, &admin_key).await;

    document["settings"]["oauth"]["bindings"] = json!([]);
    save(&api, &admin_key, document).await;
    let denied = rpc(&api, &token, "tools/call").await;

    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert!(!denied.headers().contains_key("www-authenticate"));
    let mut document = read(&api, &admin_key).await;

    document["settings"]["oauth"]["enabled"] = json!(false);
    save(&api, &admin_key, document).await;
    let disabled = rpc(&api, &token, "tools/call").await;

    assert_eq!(disabled.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(disabled.headers()["www-authenticate"], "Bearer");
    assert_eq!(
        reqwest::get(format!("{}.well-known/oauth-protected-resource", api.url))
            .await
            .expect("disabled metadata")
            .status(),
        StatusCode::NOT_FOUND
    );
}
