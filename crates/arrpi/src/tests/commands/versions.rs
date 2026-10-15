//! Discovery, health and execution share the same compatible native write contract.
use super::{CLIENT_KEY, StatusCode, call, json, response_json, setup};

#[tokio::test]
async fn stable_updates_keep_discovery_health_and_attributed_writes_consistent() {
    let (api, _native, state) = setup().await;

    for (version, supported) in [
        ("3.5.1", true),
        ("3.6.0", true),
        ("3.4.9", false),
        ("4.0.0", false),
        ("3.6.0-rc.1", false),
        ("untested", false),
    ] {
        *state.version.lock().await = version.to_owned();
        let health =
            response_json(call(&api, CLIENT_KEY, "service_status", json!({"no_cache":true})).await)
                .await;
        let source = health["sources"]
            .as_array()
            .expect("health sources")
            .iter()
            .find(|source| source["source"] == "seerr")
            .expect("Seerr health");

        assert_eq!(source["writes_supported"], supported, "{version}");
        let response = reqwest::Client::new()
            .get(format!("{}v1/tools", api.url))
            .bearer_auth(CLIENT_KEY)
            .send()
            .await
            .expect("discovery");
        let tools = response_json(response).await;
        let advertised = tools["tools"]
            .as_array()
            .expect("commands")
            .iter()
            .any(|tool| tool["name"] == "request_movie");

        assert_eq!(advertised, supported, "{version}");
        let result = call(&api, CLIENT_KEY, "request_movie", json!({"tmdb_id":42})).await;

        assert_eq!(
            result.status(),
            if supported {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            },
            "{version}"
        );
    }

    let writes = state
        .calls
        .lock()
        .await
        .iter()
        .filter(|(path, _, body)| path == "/api/v1/request" && !body.is_null())
        .map(|(_, actor, _)| actor.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        writes,
        ["1", "1"],
        "only compatible versions dispatch as the bound actor"
    );
}
