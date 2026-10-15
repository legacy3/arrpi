//! Named choices preserve native permissions, defaults and strict non-4K profiles.
use super::{CLIENT_KEY, Ordering, StatusCode, call, json, response_json, setup};
#[tokio::test]
async fn explicit_choices_require_permission_and_validate_native_quality_definitions() {
    let (api, _native, state) = setup().await;
    let choices = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_profiles",
            json!({"media_type":"movie"}),
        )
        .await,
    )
    .await;

    assert_eq!(choices["can_select_profile"], false);
    assert_eq!(choices["default_profile"], json!({"id":3,"name":"Default"}));
    assert!(choices["anime_default_profile"].is_null());
    assert!(
        choices["items"][0]["profile"].is_null(),
        "configured-default metadata never forces an explicit selection"
    );
    assert_eq!(choices["items"].as_array().expect("choices").len(), 1);
    assert_eq!(
        call(
            &api,
            CLIENT_KEY,
            "request_movie",
            json!({"tmdb_id":42,"profile":"English required"})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    state.permissions.store(32 | 8192, Ordering::Relaxed);
    let choices = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_profiles",
            json!({"media_type":"movie"}),
        )
        .await,
    )
    .await;

    assert_eq!(choices["can_select_profile"], true);
    assert_eq!(choices["items"].as_array().expect("choices").len(), 3);
    assert!(!choices.to_string().contains("UHD") && !choices.to_string().contains("/private"));
    assert_eq!(
        call(
            &api,
            CLIENT_KEY,
            "request_movie",
            json!({"tmdb_id":42,"profile":"UHD"})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    let result = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_movie",
            json!({"tmdb_id":42,"profile":"English required"}),
        )
        .await,
    )
    .await;

    assert_eq!(result["profile_applied"], true);
    assert_eq!(result["profile"], "English required");
    let calls = state.calls.lock().await;
    let body = &calls
        .iter()
        .find(|(path, _, body)| path == "/api/v1/request" && !body.is_null())
        .expect("request")
        .2;

    assert_eq!(body["profileId"], 1);
    assert_eq!(body["serverId"], 0);
    assert!(body.get("tags").is_none() && body.get("rootFolder").is_none());
}

#[tokio::test]
async fn default_null_named_default_and_batch_use_the_same_request_contract() {
    let (api, _native, state) = setup().await;

    for args in [json!({"tmdb_id":42}), json!({"tmdb_id":42,"profile":null})] {
        assert_eq!(
            call(&api, CLIENT_KEY, "request_movie", args).await.status(),
            StatusCode::OK
        );
    }

    assert!(
        !state
            .calls
            .lock()
            .await
            .iter()
            .any(|(p, _, _)| p.contains("service/") || p.contains("qualityprofile"))
    );
    state.permissions.store(32 | 8192, Ordering::Relaxed);
    let result=response_json(call(&api,CLIENT_KEY,"request_media_batch",json!({"items":[{"media_type":"movie","tmdb_id":42,"profile":"Default"},{"media_type":"tv","tmdb_id":42,"seasons":[2],"profile":"English required"}]})).await).await;

    assert_eq!(result["succeeded"], 2);
    assert_eq!(result["items"][0]["result"]["profile"], "Default");
    let calls = state.calls.lock().await;
    let bodies = calls
        .iter()
        .filter(|(p, _, b)| p == "/api/v1/request" && !b.is_null())
        .map(|(_, _, b)| b)
        .collect::<Vec<_>>();

    assert!(bodies[0].get("profileId").is_none());
    assert_eq!(bodies[2]["profileId"], 3);
    assert_eq!(bodies[3]["profileId"], 1);
}

#[tokio::test]
async fn unsafe_groups_unknown_quality_and_receipt_mismatch_fail_closed() {
    let (api, _native, state) = setup().await;

    state.permissions.store(32 | 8192, Ordering::Relaxed);

    for items in [
        json!([{"allowed":true,"quality":{}}]),
        json!([{"allowed":true,"items":[{"allowed":false,"quality":{"resolution":2160}}]}]),
        json!([]),
    ] {
        *state.quality_profiles.lock().await =
            json!([{"id":1,"name":"English required","items":items}]);
        assert_eq!(
            call(
                &api,
                CLIENT_KEY,
                "request_movie",
                json!({"tmdb_id":42,"profile":"English required"})
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
    }

    *state.quality_profiles.lock().await = json!([{"id":1,"name":"English required","items":[{"allowed":true,"quality":{"resolution":1080}},{"allowed":false,"items":[{"allowed":true,"quality":{"resolution":2160}}]}]}]);
    state.bad_profile_receipt.store(true, Ordering::Relaxed);
    let result = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_movie",
            json!({"tmdb_id":42,"profile":"English required"}),
        )
        .await,
    )
    .await;

    assert_eq!(result["code"], "unknown_outcome");
    assert_eq!(result["request_id"], 7);
    assert_eq!(result["profile_applied"], false);
    assert_eq!(
        state
            .calls
            .lock()
            .await
            .iter()
            .filter(|(p, _, b)| p == "/api/v1/request" && !b.is_null())
            .count(),
        1
    );
}

#[tokio::test]
async fn batch_validates_all_profile_names_before_any_write_and_available_is_noop() {
    let (api, _native, state) = setup().await;

    assert_eq!(call(&api,CLIENT_KEY,"request_media_batch",json!({"items":[{"media_type":"movie","tmdb_id":42},{"media_type":"tv","tmdb_id":42,"seasons":[2],"profile":" bad "}]})).await.status(),StatusCode::BAD_REQUEST);
    assert!(
        !state
            .calls
            .lock()
            .await
            .iter()
            .any(|(p, _, b)| p == "/api/v1/request" && !b.is_null())
    );
    state.permissions.store(32 | 8192, Ordering::Relaxed);
    state.available.store(true, Ordering::Relaxed);
    let result = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_movie",
            json!({"tmdb_id":42,"profile":"English required"}),
        )
        .await,
    )
    .await;

    assert_eq!(result["status"], "already_available");
    assert_eq!(result["profile_applied"], false);
}

pub(super) fn lookup(path: &str) -> Option<super::Value> {
    Some(match path {
        "/api/v1/settings/public" => {
            json!({"applicationUrl":"https://requests.example.test/base","apiKey":"never-return"})
        }

        "/api/v1/service/radarr" | "/api/v1/service/sonarr" => {
            json!([{"id":0,"isDefault":true,"is4k":false},{"id":1,"isDefault":true,"is4k":true}])
        }

        "/api/v1/service/radarr/0" | "/api/v1/service/sonarr/0" => {
            json!({"server":{"id":0,"isDefault":true,"is4k":false,"activeProfileId":3,"activeAnimeProfileId":1},"profiles":[{"id":1,"name":"English required"},{"id":2,"name":"UHD"},{"id":3,"name":"Default"}],"rootFolders":[{"path":"/private"}]})
        }

        _ => return None,
    })
}

pub(super) fn request(state: &super::Native, id: u64, body: &super::Value) -> super::Response {
    assert_eq!(body["is4k"], false);
    assert!(body.get("userId").is_none() && body.get("ignoreQuota").is_none());

    if state.outcome.load(Ordering::Relaxed) {
        return super::Response::builder()
            .status(500)
            .body(super::Body::from("uncertain outcome"))
            .expect("fixture");
    }

    let mut result = super::receipt(id, body);

    if state.bad_profile_receipt.load(Ordering::Relaxed) {
        result["profileId"] = json!(99);
    }

    let status = StatusCode::from_u16(
        u16::try_from(state.write_status.load(Ordering::Relaxed)).expect("status"),
    )
    .expect("status");

    super::native_json(status, &result)
}

#[tokio::test]
async fn missing_source_mapping_and_stale_names_never_guess_or_submit() {
    let (_api, native, state) = setup().await;

    state.permissions.store(32 | 8192, Ordering::Relaxed);
    let config:crate::config::Config=serde_json::from_value(json!({"listen":"127.0.0.1:0","sources":{},"mcp":{"enabled":true},"seerr":{"url":native.url,"api_key":"fake-seerr-key","language":"de"},"clients":[{"name":"one","key":CLIENT_KEY,"seerr_user":{"id":1,"plex_id":101},"permissions":["requests:create"]}]})).expect("configuration");
    let api = super::server(crate::proxy::router(
        crate::proxy::Gateway::new(&config, tokio::time::Instant::now).expect("gateway"),
    ))
    .await;
    let choices = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_profiles",
            json!({"media_type":"movie"}),
        )
        .await,
    )
    .await;

    assert_eq!(choices["can_select_profile"], false);
    assert!(
        choices["reason"]
            .as_str()
            .expect("reason")
            .contains("Settings")
    );
    assert_eq!(
        call(
            &api,
            CLIENT_KEY,
            "request_movie",
            json!({"tmdb_id":42,"profile":"English required"})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
    assert!(
        !state
            .calls
            .lock()
            .await
            .iter()
            .any(|(p, _, b)| p == "/api/v1/request" && !b.is_null())
    );
    let (api, _native, state) = setup().await;

    state.permissions.store(32 | 8192, Ordering::Relaxed);
    state.quality_profiles.lock().await[0]["name"] = json!("Renamed profile");
    assert_eq!(
        call(
            &api,
            CLIENT_KEY,
            "request_movie",
            json!({"tmdb_id":42,"profile":"English required"})
        )
        .await
        .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn details_batch_gets_public_urls_once_and_projects_only_public_links() {
    let (api, _native, state) = setup().await;

    state.available.store(true, Ordering::Relaxed);
    let result = response_json(
        call(
            &api,
            CLIENT_KEY,
            "media_details_batch",
            json!({"items":[{"media_type":"movie","tmdb_id":42},{"media_type":"tv","tmdb_id":42}]}),
        )
        .await,
    )
    .await;

    assert_eq!(result["succeeded"], 2);
    assert_eq!(
        result["items"][0]["result"]["links"]["seerr"],
        "https://requests.example.test/base/movie/42"
    );
    assert!(
        result["items"][0]["result"]["links"]["watch"]
            .as_str()
            .expect("watch")
            .starts_with("https://watch.example.test/")
    );
    assert_eq!(
        result["items"][1]["result"]["links"]["seerr"],
        "https://requests.example.test/base/tv/42"
    );
    assert_eq!(
        state
            .calls
            .lock()
            .await
            .iter()
            .filter(|(p, _, _)| p == "/api/v1/settings/public")
            .count(),
        1
    );
    assert!(
        !result.to_string().contains("never-return") && !result.to_string().contains("radarr:7878")
    );
}

#[tokio::test]
async fn multiword_and_reserved_character_catalog_queries_use_percent_encoding() {
    let (api, _native, state) = setup().await;

    for query in [
        "Hail Mary",
        "Big Lebowski",
        "Night Manager",
        "asdkjhasd nonsense query zzz",
        "ÄÖÜ + & / ? # ! ' ( ) *",
    ] {
        assert_eq!(
            call(&api, CLIENT_KEY, "search_media", json!({"query":query}))
                .await
                .status(),
            StatusCode::OK
        );
    }

    assert_eq!(
        state
            .calls
            .lock()
            .await
            .iter()
            .filter(|(p, _, _)| p == "/api/v1/search")
            .count(),
        5
    );
}

pub(super) async fn raw(
    state: &super::Native,
    path: &str,
    headers: &axum::http::HeaderMap,
) -> Option<super::Response> {
    if !matches!(path, "/api/v3/health" | "/api/v3/qualityprofile") {
        return None;
    }

    assert_eq!(headers["X-API-Key"], "fake-upstream-api-key");
    let result = if path == "/api/v3/health" {
        json!([{"message":"/private path / secret","type":"warning"}])
    } else {
        state
            .calls
            .lock()
            .await
            .push((path.to_owned(), "native".to_owned(), super::Value::Null));
        state.quality_profiles.lock().await.clone()
    };

    Some(super::native_json(StatusCode::OK, &result))
}

pub(super) fn fault(state: &super::Native, path: &str) -> Option<super::Response> {
    use super::native_json;

    if path == "/api/v1/status" && state.fail_status.load(Ordering::Relaxed) {
        return Some(native_json(
            StatusCode::SERVICE_UNAVAILABLE,
            &json!({"message":"private detail"}),
        ));
    }

    if path == "/api/v1/auth/me" && state.identity_status.load(Ordering::Relaxed) != 200 {
        return Some(native_json(
            StatusCode::from_u16(
                u16::try_from(state.identity_status.load(Ordering::Relaxed)).expect("status"),
            )
            .expect("status"),
            &json!({"message":"private identity error"}),
        ));
    }

    if path == "/api/v1/search" && state.search_status.load(Ordering::Relaxed) != 200 {
        return Some(native_json(
            StatusCode::from_u16(
                u16::try_from(state.search_status.load(Ordering::Relaxed)).expect("status"),
            )
            .expect("status"),
            &json!({"message":"private upstream error"}),
        ));
    }

    None
}

#[tokio::test]
async fn native_radarr_sd_dvd_is_safe_but_other_zero_resolutions_are_rejected() {
    let (api, _native, state) = setup().await;

    state.permissions.store(32 | 8192, Ordering::Relaxed);

    for (quality, expected) in [
        (
            json!({"id":2,"name":"DVD","source":"dvd","resolution":0,"modifier":"none"}),
            true,
        ),
        (
            json!({"id":0,"name":"Unknown","source":"unknown","resolution":0,"modifier":"none"}),
            false,
        ),
        (
            json!({"id":2,"name":"DVD","source":"dvd","resolution":2160,"modifier":"none"}),
            false,
        ),
        (json!({"id":2,"name":"DVD","resolution":0}), false),
    ] {
        *state.quality_profiles.lock().await = json!([{"id":1,"name":"English required","items":[{"allowed":true,"name":"480p","items":[{"allowed":true,"quality":quality}]}]}]);
        let result = response_json(
            call(
                &api,
                CLIENT_KEY,
                "request_profiles",
                json!({"media_type":"movie","no_cache":true}),
            )
            .await,
        )
        .await;

        assert_eq!(result["can_select_profile"], expected);
        assert_eq!(
            result["items"].as_array().expect("choices").len(),
            if expected { 2 } else { 1 }
        );
    }
}

#[tokio::test]
async fn malformed_unrelated_native_profiles_do_not_hide_verified_choices() {
    let (api, _native, state) = setup().await;

    state.permissions.store(32 | 8192, Ordering::Relaxed);
    state
        .quality_profiles
        .lock()
        .await
        .as_array_mut()
        .expect("profiles")
        .extend([json!({"id":99}), json!({"name":"Unrelated"}), json!(null)]);
    let choices = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_profiles",
            json!({"media_type":"movie"}),
        )
        .await,
    )
    .await;

    assert_eq!(choices["can_select_profile"], true);
    assert_eq!(choices["items"].as_array().expect("choices").len(), 3);
}

#[tokio::test]
async fn configured_regular_and_anime_defaults_do_not_grant_profile_selection() {
    let (api, _native, state) = setup().await;
    let choices = response_json(
        call(
            &api,
            CLIENT_KEY,
            "request_profiles",
            json!({"media_type":"tv"}),
        )
        .await,
    )
    .await;

    assert_eq!(choices["default_profile"], json!({"id":3,"name":"Default"}));
    assert_eq!(
        choices["anime_default_profile"],
        json!({"id":1,"name":"English required"})
    );
    assert_eq!(choices["can_select_profile"], false);
    assert_eq!(choices["items"].as_array().expect("default only").len(), 1);
    let calls = state.calls.lock().await;

    assert_eq!(
        calls
            .iter()
            .filter(|(path, _, _)| path.starts_with("/api/v1/service/"))
            .count(),
        2,
        "default metadata uses one bounded service listing and one detail response"
    );
    assert!(
        !calls
            .iter()
            .any(|(path, _, _)| path.ends_with("qualityprofile")),
        "ordinary default discovery does not fetch native quality definitions"
    );
    assert!(!choices.to_string().contains("/private"));
}
