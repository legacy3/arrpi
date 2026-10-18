//! End-to-end source grants, asynchronous commands and missing-episode search contracts.

mod fixture;

use super::CLIENT_KEY;
use arrpi_core::policy::Limits;
use axum::http::StatusCode;
use fixture::{NativeState, call, fixture, fixture_with_limits};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering};

#[tokio::test]
async fn refresh_then_inspect_then_search_missing_uses_scoped_native_commands() {
    let state = Arc::new(NativeState::default());
    let (_native, api) = fixture(
        &[
            "shows:library-refresh",
            "shows:library-search",
            "shows:library-view",
        ],
        Arc::clone(&state),
    )
    .await;
    let (status, refresh) =
        call(&api, "refresh_media", json!({"source":"shows","item_id":7})).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(refresh["command_id"], 77);
    assert_eq!(refresh["status"], "queued");
    assert_eq!(refresh["finished"], false);
    let (_, done) = call(
        &api,
        "media_command_status",
        json!({"source":"shows","command_id":77}),
    )
    .await;

    assert_eq!(done["successful"], true);
    assert!(!done.to_string().contains("private"));
    let (_, episodes) = call(
        &api,
        "series_episodes",
        json!({"source":"shows","item_id":7,"missing_only":true,"aired_only":true,"limit":2}),
    )
    .await;

    assert_eq!(episodes["total"], 3);
    assert_eq!(episodes["next_offset"], 2);
    assert_eq!(episodes["items"][0]["id"], 1);
    assert!(!episodes.to_string().contains("private"));
    let (_,next)=call(&api,"series_episodes",json!({"source":"shows","item_id":7,"missing_only":true,"aired_only":true,"limit":2,"offset":2})).await;

    assert_eq!(next["items"][0]["id"], 6);
    assert_eq!(next["next_offset"], Value::Null);
    let (status, search) = call(
        &api,
        "search_releases",
        json!({"source":"shows","item_id":7}),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(search["accepted"], true);
    assert_eq!(
        *state.writes.lock().await,
        vec![
            json!({"name":"RefreshSeries","seriesIds":[7]}),
            json!({"name":"MissingEpisodeSearch","seriesId":7,"monitored":true})
        ]
    );
}

#[tokio::test]
async fn season_and_explicit_episode_searches_exclude_files_future_and_unmonitored() {
    let state = Arc::new(NativeState::default());
    let (_native, api) = fixture(&["shows:library-search"], Arc::clone(&state)).await;

    for (args, ids) in [
        (json!({"season":1}), vec![1]),
        (json!({"season":1,"missing_only":false}), vec![1, 2]),
        (json!({"episode_ids":[1,2,3,4,5,6]}), vec![1, 6]),
    ] {
        let mut input = args;

        input["source"] = json!("shows");
        input["item_id"] = json!(7);
        let (status, result) = call(&api, "search_releases", input).await;

        assert_eq!(status, StatusCode::OK, "{result}");
        assert_eq!(result["episode_ids"], json!(ids));
        assert_eq!(
            state.writes.lock().await.last().expect("command"),
            &json!({"name":"EpisodeSearch","episodeIds":ids})
        );
    }

    let before = state.writes.lock().await.len();
    let (_, noop) = call(
        &api,
        "search_releases",
        json!({"source":"shows","item_id":7,"episode_ids":[2]}),
    )
    .await;

    assert_eq!(noop["status"], "nothing_to_search");
    assert_eq!(state.writes.lock().await.len(), before);
}

#[tokio::test]
async fn movie_refresh_and_search_are_narrow_and_tv_arguments_are_rejected() {
    let state = Arc::new(NativeState::default());
    let (_native, api) = fixture(
        &["movies:library-refresh", "movies:library-search"],
        Arc::clone(&state),
    )
    .await;

    assert_eq!(
        call(
            &api,
            "refresh_media",
            json!({"source":"movies","item_id":7})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &api,
            "search_releases",
            json!({"source":"movies","item_id":7})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &api,
            "search_releases",
            json!({"source":"movies","item_id":7,"season":1})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        *state.writes.lock().await,
        vec![
            json!({"name":"RefreshMovie","movieIds":[7]}),
            json!({"name":"MoviesSearch","movieIds":[7]})
        ]
    );
}

#[tokio::test]
async fn read_grants_and_other_sources_cannot_authorize_mutations_or_native_paths() {
    let state = Arc::new(NativeState::default());
    let (_native, api) = fixture(
        &["shows:library-view", "movies:library-search"],
        Arc::clone(&state),
    )
    .await;

    for tool in ["refresh_media", "search_releases"] {
        assert_eq!(
            call(&api, tool, json!({"source":"shows","item_id":7}))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }

    assert_eq!(state.reads.load(Ordering::Relaxed), 0);
    assert_eq!(
        call(
            &api,
            "search_releases",
            json!({"source":"movies","item_id":7,"command":"DeleteMovie"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(state.reads.load(Ordering::Relaxed), 0);
    let (status, _) = call(
        &api,
        "media_command_status",
        json!({"source":"movies","command_id":78}),
    )
    .await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(state.writes.lock().await.is_empty());
}

#[tokio::test]
async fn malformed_coordinates_never_dispatch_and_uhd_profiles_are_refused() {
    let state = Arc::new(NativeState {
        uhd: true,
        ..NativeState::default()
    });
    let (_native, api) = fixture(&["shows:library-search"], Arc::clone(&state)).await;

    for input in [
        json!({"item_id":0}),
        json!({"item_id":7,"season":1,"episode_ids":[1]}),
        json!({"item_id":7,"episode_ids":[1,1]}),
    ] {
        let mut input = input;

        input["source"] = json!("shows");
        assert_eq!(
            call(&api, "search_releases", input).await.0,
            StatusCode::BAD_REQUEST
        );
    }

    assert_eq!(state.reads.load(Ordering::Relaxed), 0);
    let (status, body) = call(
        &api,
        "search_releases",
        json!({"source":"shows","item_id":7}),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "unsafe_search_profile");
    assert!(state.writes.lock().await.is_empty());
}

#[tokio::test]
async fn unmonitored_series_is_not_changed_or_searched() {
    let state = Arc::new(NativeState {
        unmonitored: true,
        ..NativeState::default()
    });
    let (_native, api) = fixture(&["shows:library-search"], Arc::clone(&state)).await;
    let (_, body) = call(
        &api,
        "search_releases",
        json!({"source":"shows","item_id":7}),
    )
    .await;

    assert_eq!(body["accepted"], false);
    assert_eq!(body["reason"], "unmonitored");
    assert!(state.writes.lock().await.is_empty());
}

#[tokio::test]
async fn upstream_mutation_failure_is_uncertain_and_never_retried() {
    let state = Arc::new(NativeState::default());

    state.post_status.store(503, Ordering::Relaxed);
    let (_native, api) = fixture(&["shows:library-refresh"], Arc::clone(&state)).await;
    let (status, body) = call(&api, "refresh_media", json!({"source":"shows","item_id":7})).await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["code"], "unknown_outcome");
    assert_eq!(state.writes.lock().await.len(), 1);
    assert!(!body.to_string().contains("private"));
}

#[tokio::test]
async fn command_catalog_exposes_only_permitted_sources_without_broker_identity() {
    let state = Arc::new(NativeState::default());
    let (_native, api) = fixture(
        &[
            "shows:library-refresh",
            "shows:library-view",
            "movies:library-search",
        ],
        Arc::clone(&state),
    )
    .await;
    let response = reqwest::Client::new()
        .get(format!("{}v1/tools", api.url))
        .bearer_auth(CLIENT_KEY)
        .send()
        .await
        .expect("catalog");
    let bytes = response.bytes().await.expect("catalog body");
    let body: Value = serde_json::from_slice(&bytes).expect("catalog JSON");
    let tools = body["tools"].as_array().expect("tools");

    for (tool, sources) in [
        ("refresh_media", json!(["shows"])),
        ("search_releases", json!(["movies"])),
        ("series_episodes", json!(["shows"])),
    ] {
        let definition = tools
            .iter()
            .find(|entry| entry["name"] == tool)
            .expect("tool");

        assert_eq!(definition["sources"], sources);
    }

    assert_eq!(state.reads.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn configured_search_batch_and_page_limits_are_enforced_before_dispatch() {
    let state = Arc::new(NativeState::default());
    let (_native, api) = fixture_with_limits(
        &["shows:library-view", "shows:library-search"],
        Arc::clone(&state),
        Limits {
            max_search_episodes: 1,
            max_batch_items: 2,
            max_page_items: 2,
            ..Limits::default()
        },
    )
    .await;

    for (tool, args) in [
        ("search_releases", json!({"episode_ids":[1,2,3]})),
        ("series_episodes", json!({"limit":3})),
    ] {
        let mut input = args;

        input["source"] = json!("shows");
        input["item_id"] = json!(7);
        assert_eq!(call(&api, tool, input).await.0, StatusCode::BAD_REQUEST);
    }

    assert_eq!(state.reads.load(Ordering::Relaxed), 0);
    let (status, body) = call(
        &api,
        "search_releases",
        json!({"source":"shows","item_id":7}),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "search_too_large");
    assert!(state.writes.lock().await.is_empty());
    assert_eq!(
        call(
            &api,
            "search_releases",
            json!({"source":"shows","item_id":7,"season":1})
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn episode_ids_from_another_series_are_rejected_without_a_command() {
    let state = Arc::new(NativeState::default());
    let (_native, api) = fixture(&["shows:library-search"], Arc::clone(&state)).await;
    let (status, body) = call(
        &api,
        "search_releases",
        json!({"source":"shows","item_id":7,"episode_ids":[999]}),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_arguments");
    assert!(state.writes.lock().await.is_empty());
}

#[tokio::test]
async fn unsupported_native_versions_never_write_and_missing_scope_is_uncertain() {
    for version in ["3.0.10.1567", "5.0.0.1", "4.0.0-beta"] {
        let state = Arc::new(NativeState {
            version: Some(version),
            ..NativeState::default()
        });
        let (_native, api) = fixture(&["shows:library-refresh"], Arc::clone(&state)).await;
        let (status, body) =
            call(&api, "refresh_media", json!({"source":"shows","item_id":7})).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["code"], "maintenance_disabled_unverified_version");
        assert!(state.writes.lock().await.is_empty());
    }

    let state = Arc::new(NativeState {
        ignore_scope: true,
        ..NativeState::default()
    });
    let (_native, api) = fixture(&["shows:library-refresh"], Arc::clone(&state)).await;
    let (status, body) = call(&api, "refresh_media", json!({"source":"shows","item_id":7})).await;

    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(body["code"], "unknown_outcome");
    assert_eq!(state.writes.lock().await.len(), 1);
}
