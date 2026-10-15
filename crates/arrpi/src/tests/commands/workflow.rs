//! Native quota/status, owned batches and private bounded write history.
use super::{CLIENT_KEY, OTHER_KEY, Ordering, call, json, response_json, setup, setup_disabled};
use arrpi_core::executor::CommandExecutor as _;
use axum::http::StatusCode;

#[tokio::test]
async fn source_inventory_includes_the_bound_account_broker_without_native_reads() {
    let (api, _native, state) = setup().await;
    let response = reqwest::Client::new()
        .get(format!("{}v1/status", api.url))
        .bearer_auth(CLIENT_KEY)
        .send()
        .await
        .expect("status response");
    let inventory = response_json(response).await;
    let broker = inventory["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .find(|source| source["kind"] == "seerr")
        .expect("account broker is visible with catalog grants");

    assert_eq!(broker["endpoints"], json!([]));
    assert_eq!(broker["summaries"], json!([]));
    assert!(
        state.calls.lock().await.is_empty(),
        "metadata discovery never impersonates an account"
    );
    assert!(!inventory.to_string().contains("fake-seerr-key"));
}

#[tokio::test]
async fn quota_and_independent_health_are_read_only_scoped_and_fresh() {
    let (api, _native, state) = setup().await;
    let quota =
        response_json(call(&api, CLIENT_KEY, "my_quota", json!({"no_cache":true})).await).await;

    assert_eq!(quota["movie"]["limit"], 5);
    assert_eq!(quota["movie"]["days"], 7);
    assert_eq!(quota["tv"]["unit"], "seasons");
    assert_eq!(quota["freshness"]["cache"], "not_cached");
    assert_eq!(
        call(&api, CLIENT_KEY, "my_quota", json!({"user_id":2}))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    let status =
        response_json(call(&api, CLIENT_KEY, "service_status", json!({"no_cache":true})).await)
            .await;

    assert_eq!(status["complete"], true);
    assert_eq!(status["sources"].as_array().expect("sources").len(), 3);
    assert!(!status.to_string().contains("/private") && !status.to_string().contains("secret"));
    state.fail_status.store(true, Ordering::Relaxed);
    let status =
        response_json(call(&api, CLIENT_KEY, "service_status", json!({"no_cache":true})).await)
            .await;

    assert_eq!(status["complete"], false);
    assert_eq!(status["sources"][0]["reachable"], true);
    let seerr = status["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .find(|source| source["source"] == "seerr")
        .expect("Seerr health");

    assert_eq!(seerr["code"], "upstream_unavailable");
}

#[tokio::test]
async fn search_errors_distinguish_invalid_query_rate_limit_and_outage() {
    let (api, _native, state) = setup().await;

    for (status, code) in [
        (400, "upstream_invalid_arguments"),
        (429, "upstream_rate_limited"),
        (500, "upstream_unavailable"),
    ] {
        state.search_status.store(status, Ordering::Relaxed);
        let result = response_json(
            call(
                &api,
                CLIENT_KEY,
                "search_media",
                json!({"query":"Hail Mary"}),
            )
            .await,
        )
        .await;

        assert_eq!(result["code"], code);
        assert!(!result.to_string().contains("private upstream error"));
    }
}

#[tokio::test]
async fn owned_issue_batches_validate_before_dispatch_and_honor_leaf_switches() {
    let (api, _native, state) = setup().await;
    let value = response_json(
        call(
            &api,
            CLIENT_KEY,
            "close_issues_batch",
            json!({"issue_ids":[8,99]}),
        )
        .await,
    )
    .await;

    assert_eq!(value["succeeded"], 1);
    assert_eq!(value["items"][0]["result"]["status_name"], "resolved");
    assert_eq!(value["items"][1]["code"], "permission_denied");
    assert!(
        !state
            .calls
            .lock()
            .await
            .iter()
            .any(|(p, _, _)| p.ends_with("/99/resolved"))
    );
    assert_eq!(
        call(
            &api,
            CLIENT_KEY,
            "close_issues_batch",
            json!({"issue_ids":[8,8]})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let (api, _native, state) = setup_disabled(vec!["close_issue".into()]).await;
    let value = response_json(
        call(
            &api,
            CLIENT_KEY,
            "close_issues_batch",
            json!({"issue_ids":[8]}),
        )
        .await,
    )
    .await;

    assert_eq!(value["items"][0]["code"], "permission_denied");
    assert!(
        !state
            .calls
            .lock()
            .await
            .iter()
            .any(|(p, _, _)| p.ends_with("/resolved"))
    );
}

#[tokio::test]
async fn audit_attributes_each_write_and_batch_item_without_messages_or_keys() {
    let (api, _native, state) = setup().await;

    assert_eq!(
        call(&api, CLIENT_KEY, "request_movie", json!({"tmdb_id":42}))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(call(&api,OTHER_KEY,"report_issue",json!({"media_type":"movie","tmdb_id":42,"category":"other","message":"private test message fake-seerr-key"})).await.status(),StatusCode::OK);
    let _ = call(
        &api,
        CLIENT_KEY,
        "close_issues_batch",
        json!({"issue_ids":[8,99]}),
    )
    .await;
    let first = serde_json::to_value(state.statistics.write_audit("one", 0, 10).expect("audit"))
        .expect("audit JSON");
    let second = serde_json::to_value(state.statistics.write_audit("two", 0, 10).expect("audit"))
        .expect("audit JSON");

    assert_eq!(first["total"], 3);
    assert_eq!(second["total"], 1);
    assert_eq!(first["items"][2]["target"]["request_id"], 7);
    assert_eq!(second["items"][0]["actor_id"], 2);
    assert_eq!(second["items"][0]["target"]["issue_id"], 8);
    assert!(
        !second.to_string().contains("private test message")
            && !second.to_string().contains("fake-seerr-key")
    );
    assert_eq!(first["items"][0]["outcome"], "permission_denied");
}

#[tokio::test]
async fn already_available_tv_season_is_a_noop_even_if_the_series_is_partial() {
    let (api, _native, state) = setup().await;
    let value = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_series",
            json!({"tmdb_id":42,"seasons":[1]}),
        )
        .await,
    )
    .await;

    assert_eq!(value["status"], "already_available");
    assert!(
        !state
            .calls
            .lock()
            .await
            .iter()
            .any(|(p, _, body)| p == "/api/v1/request" && !body.is_null())
    );
}

#[tokio::test]
async fn source_health_remains_discoverable_during_a_seerr_outage() {
    let (api, _native, state) = setup().await;

    state.fail_status.store(true, Ordering::Relaxed);
    state.fail_identity.store(true, Ordering::Relaxed);
    let response = reqwest::Client::new()
        .get(format!("{}v1/tools", api.url))
        .bearer_auth(CLIENT_KEY)
        .send()
        .await
        .expect("tool list");

    assert_eq!(response.status(), StatusCode::OK);
    let data = response_json(response).await;
    let names = data["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|t| t["name"].as_str().expect("name"))
        .collect::<Vec<_>>();

    assert!(names.contains(&"service_status") && names.contains(&"list_movies"));
    assert!(!names.contains(&"request_movie") && !names.contains(&"search_media"));
    let value =
        response_json(call(&api, CLIENT_KEY, "service_status", json!({"no_cache":true})).await)
            .await;

    assert_eq!(value["complete"], false);
    assert_eq!(value["sources"][0]["reachable"], true);
}

#[tokio::test]
async fn cli_and_stdio_bridge_keep_error_codes_ids_and_schema_driven_cache_bypass() {
    let (api, _native, state) = setup().await;

    let remote = arrpi_adapters::remote::RemoteApi::new(&api.url, true).expect("remote");
    let quota = remote
        .execute(
            CLIENT_KEY,
            arrpi_core::registry::ToolId::parse("my_quota").expect("tool"),
            json!({}),
        )
        .await
        .expect("quota");

    assert_eq!(quota["freshness"]["bypass_requested"], true);
    let profiles = remote
        .execute(
            CLIENT_KEY,
            arrpi_core::registry::ToolId::parse("request_profiles").expect("tool"),
            json!({"media_type":"movie"}),
        )
        .await
        .expect("profiles");

    assert_eq!(profiles["freshness"]["bypass_requested"], true);
    state.search_status.store(429, Ordering::Relaxed);
    assert_eq!(
        remote
            .execute(
                CLIENT_KEY,
                arrpi_core::registry::ToolId::parse("search_media").expect("tool"),
                json!({"query":"Hail Mary"})
            )
            .await
            .expect_err("rate limit")
            .code(),
        "upstream_rate_limited"
    );
    state.permissions.store(32 | 8192, Ordering::Relaxed);
    state.bad_profile_receipt.store(true, Ordering::Relaxed);
    let error = remote
        .execute(
            CLIENT_KEY,
            arrpi_core::registry::ToolId::RequestMovie,
            json!({"tmdb_id":42,"profile":"English required"}),
        )
        .await
        .expect_err("mismatch");

    assert_eq!(error.code(), "unknown_outcome");
    assert_eq!(error.payload()["request_id"], 7);
}

#[tokio::test]
async fn account_lists_do_not_advertise_unsupported_queries() {
    let (api, _native, _state) = setup().await;
    let tools = response_json(super::rpc(&api, CLIENT_KEY, "tools/list", json!({})).await).await;

    for name in ["my_requests", "list_issues"] {
        let tool = tools["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|t| t["name"] == name)
            .expect("account list");

        assert!(tool["inputSchema"]["properties"].get("query").is_none());
        assert_eq!(
            call(&api, CLIENT_KEY, name, json!({"query":"Oppenheimer"}))
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&api, CLIENT_KEY, name, json!({"limit":2}))
                .await
                .status(),
            StatusCode::OK
        );
    }

    let missing = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_status",
            json!({"request_id":999_999}),
        )
        .await,
    )
    .await;

    assert_eq!(missing["code"], "not_found");
    assert!(
        missing["error"]
            .as_str()
            .expect("error")
            .contains("not found")
    );
    assert!(!missing.to_string().contains("unsupported endpoint"));
}

#[test]
fn write_schemas_describe_the_enforced_identifier_and_season_bounds() {
    let registry = arrpi_core::registry::Registry::new(arrpi_core::policy::Limits::default());
    let request = registry.definition(arrpi_core::registry::ToolId::RequestMediaBatch);
    let item = &request.input_schema().as_ref()["$defs"]["MediaRequest"];

    assert_eq!(item["properties"]["tmdb_id"]["minimum"], 1);
    assert_eq!(item["properties"]["tmdb_id"]["maximum"], u32::MAX);
    assert_eq!(item["properties"]["seasons"]["maxItems"], 50);
    assert_eq!(item["properties"]["seasons"]["items"]["minimum"], 1);
    let issues = registry.definition(arrpi_core::registry::ToolId::CloseIssuesBatch);

    assert_eq!(
        issues.input_schema().as_ref()["properties"]["issue_ids"]["items"]["minimum"],
        1
    );
    assert_eq!(
        issues.input_schema().as_ref()["properties"]["issue_ids"]["items"]["maximum"],
        u32::MAX
    );
}

#[tokio::test]
async fn request_summaries_keep_native_dates_and_reuse_preflight_metadata() {
    let (api, _native, state) = setup().await;

    for (tool, args) in [
        ("request_status", json!({"request_id":7})),
        ("my_requests", json!({"limit":2})),
    ] {
        let result = response_json(call(&api, CLIENT_KEY, tool, args).await).await;
        let row = if tool == "my_requests" {
            &result["items"][0]
        } else {
            &result
        };

        assert_eq!(row["created_at"], "2026-10-15T01:00:00.000Z");
        assert_eq!(row["updated_at"], "2026-10-15T02:00:00.000Z");
        assert!(
            row["title"].is_null(),
            "native request records do not contain catalog titles"
        );
    }

    assert!(
        !state
            .calls
            .lock()
            .await
            .iter()
            .any(|(path, _, _)| path.starts_with("/api/v1/movie/")
                || path.starts_with("/api/v1/tv/")),
        "request lists must not fan out to catalog details"
    );
    state.calls.lock().await.clear();
    let result =
        response_json(call(&api, CLIENT_KEY, "request_movie", json!({"tmdb_id":42})).await).await;

    assert_eq!(result["title"], "Movie");
    assert_eq!(result["release_date"], "2023-07-19");
    assert_eq!(result["created_at"], "2026-10-15T01:00:00.000Z");
    assert_eq!(
        state
            .calls
            .lock()
            .await
            .iter()
            .filter(|(path, _, _)| path == "/api/v1/movie/42")
            .count(),
        1,
        "title enrichment reuses the required preflight response"
    );
}
