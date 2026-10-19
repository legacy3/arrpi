//! Recommendation queries, unrestricted page offsets, fresh reads and source isolation.
use super::{CLIENT_KEY, EXAMPLE, Server, mock, server};
use crate::{config::Config, proxy::Gateway};
use arrpi_core::{executor::CommandExecutor as _, registry::ToolId};
use serde_json::{Value, json};
async fn gateway(native: &Server, sources: &[&str]) -> Server {
    let mut config = Config::parse(EXAMPLE).expect("configuration");

    for source in config.sources.values_mut() {
        source.url = native.url.clone();
    }

    config.mcp.enabled = true;
    config.seerr = Some(
        serde_json::from_value(json!({"url":native.url,"api_key":"fake-seerr-key"}))
            .expect("connection"),
    );
    config.clients[0].permissions = sources
        .iter()
        .map(|source| format!("{source}:library-view").into())
        .collect();
    server(crate::proxy::router(
        Gateway::new(&config, tokio::time::Instant::now).expect("gateway"),
    ))
    .await
}

async fn query(api: &Server, args: Value) -> (reqwest::StatusCode, Value) {
    let response = reqwest::Client::new()
        .post(format!("{}v1/tools/search_library", api.url))
        .bearer_auth(CLIENT_KEY)
        .header("content-type", "application/json")
        .body(args.to_string())
        .send()
        .await
        .expect("query");

    let status = response.status();
    let body = response.text().await.expect("body");

    (status, serde_json::from_str(&body).expect("JSON"))
}

#[tokio::test]
async fn plex_ratings_join_queries_with_cache_freshness_and_source_authorization() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let denied = Arc::new(AtomicBool::new(false));
    let counted = Arc::clone(&calls);
    let failure = Arc::clone(&denied);
    let router = axum::Router::new().fallback(axum::routing::get(move |uri: axum::http::Uri, headers: axum::http::HeaderMap| {
        let counted = Arc::clone(&counted);
        let failure = Arc::clone(&failure);

        async move {
            if uri.path() == "/api/v3/movie" {
                return (axum::http::StatusCode::OK, axum::Json(json!([
                    {"id":1,"tmdbId":42,"title":"Moonberry","hasFile":true},
                    {"id":2,"tmdbId":43,"title":"Cloudberry","hasFile":true}
                ])));
            }

            assert_eq!(headers.get("x-plex-token").and_then(|value| value.to_str().ok()), Some("fake-plex-key"));
            assert!(!uri.to_string().contains("fake-plex-key"));
            counted.fetch_add(1, Ordering::Relaxed);

            if failure.load(Ordering::Relaxed) { return (axum::http::StatusCode::UNAUTHORIZED, axum::Json(json!({}))); }

            let metadata = if uri.path().starts_with("/library/metadata/") {
                json!([
                    {"type":"movie","ratingKey":"42","Guid":[{"id":"tmdb://42"}],"Rating":[{"image":"imdb://image.rating","type":"audience","value":8.5}],"userRating":1,"path":"never-leak"},
                    {"type":"movie","ratingKey":"43","Guid":[{"id":"tmdb://43"}],"Rating":[{"image":"imdb://image.rating","type":"audience","value":6.5}]}
                ])
            } else if uri.query().unwrap_or_default().starts_with("type=1&") {
                json!([
                    {"type":"movie","ratingKey":"42","Guid":[{"id":"tmdb://42"}],"rating":6.0,"ratingImage":"rottentomatoes://image.rating.rotten"},
                    {"type":"movie","ratingKey":"43","Guid":[{"id":"tmdb://43"}]}
                ])
            } else { json!([]) };

            (axum::http::StatusCode::OK, axum::Json(json!({"MediaContainer":{"offset":0,"Metadata":metadata}})))
        }
    }));
    let native = server(router).await;
    let config: Config = serde_json::from_value(json!({
        "listen":"127.0.0.1:0","mcp":{"enabled":true},"seerr":{"url":native.url,"api_key":"fake-seerr-key"},
        "sources":{"radarr":{"kind":"radarr","url":native.url,"api_key":"fake-arr-key"},"plex":{"kind":"plex","url":native.url,"api_key":"fake-plex-key"}},
        "clients":[{"name":"reader","key":CLIENT_KEY,"permissions":["radarr:library-view","plex:ratings-view"]},
            {"name":"limited","key":super::OTHER_KEY,"permissions":["radarr:library-view"]}]
    })).expect("rating settings");
    let api = server(crate::proxy::router(
        Gateway::new(&config, tokio::time::Instant::now).expect("gateway"),
    ))
    .await;
    let (status, value) = query(
        &api,
        json!({"rating_provider":"imdb","min_rating":8,"sort":"rating"}),
    )
    .await;

    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(value["total"], 1);
    assert_eq!(value["items"][0]["ratings"][0]["origin"], "plex");
    assert_eq!(calls.load(Ordering::Relaxed), 3);
    assert_eq!(value["items"][0]["rating"], 8.5);
    assert_eq!(value["rating_sources"][0]["status"], "ok");
    assert_eq!(value["complete"], true);
    assert!(!value.to_string().contains("never-leak"));
    let fetched = calls.load(Ordering::Relaxed);
    let (_, cached) = query(&api, json!({})).await;

    assert_eq!(cached["rating_sources"][0]["freshness"]["cache"], "hit");
    assert_eq!(calls.load(Ordering::Relaxed), fetched);
    let response = reqwest::Client::new()
        .post(format!("{}v1/tools/search_library", api.url))
        .bearer_auth(super::OTHER_KEY)
        .json(&json!({"rating_provider":"imdb","min_rating":8}))
        .send()
        .await
        .expect("restricted query");
    let restricted: Value = response.json().await.expect("restricted result");

    assert_eq!(restricted["total"], 0);
    assert_eq!(restricted["rating_sources"], json!([]));
    assert_eq!(calls.load(Ordering::Relaxed), fetched);
    let (_, fresh) = query(&api, json!({"no_cache":true})).await;

    assert_eq!(fresh["rating_sources"][0]["freshness"]["cache"], "miss");
    assert!(calls.load(Ordering::Relaxed) > fetched);
    denied.store(true, Ordering::Relaxed);
    let (_, failed) = query(&api, json!({"no_cache":true})).await;

    assert_eq!(failed["items"].as_array().expect("native rows").len(), 2);
    assert_eq!(failed["complete"], false);
    assert_eq!(failed["ratings_complete"], false);
    assert_eq!(failed["rating_sources"][0]["code"], "permission_denied");
}

#[tokio::test]
async fn structured_filters_return_recommendation_metadata_without_private_fields() {
    let data = json!([
        {"id":1,"title":"Funny Movie","year":2023,"genres":["Comedy","Drama"],"overview":"A funny story","runtime":110,"inCinemas":"2023-01-01","releaseDate":"2024-01-01","digitalRelease":"2023-06-01","physicalRelease":"2024-01-01","imdbId":"tt12345","hasFile":true,"path":"/private","ratings":{"imdb":{"value":8.2,"votes":100},"rottenTomatoes":{"value":95,"votes":0}},"movieFile":{"relativePath":"secret"}},
        {"id":2,"title":"Funny Old Movie","year":1999,"genres":["Comedy"],"hasFile":true,"ratings":{"tmdb":{"value":6}}},
        {"id":3,"title":"Missing Comedy","year":2024,"genres":["Comedy"],"hasFile":false,"ratings":{"imdb":{"value":9}}},
        {"id":4,"title":"Drama","year":2023,"genres":["Drama"],"hasFile":true,"ratings":{"imdb":{"value":9}}}
    ]);
    let (native, _state) = mock(&data.to_string()).await;
    let api = gateway(&native, &["radarr"]).await;
    let (status,value)=query(&api,json!({"media_type":"movie","genres":["comedy"],"available":true,"min_rating":7,"year_from":2020,"year_to":2025,"sort":"rating","limit":1})).await;

    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(value["total"], 1);
    assert_eq!(value["items"][0]["overview"], "A funny story");
    assert_eq!(value["items"][0]["runtime_minutes"], 110);
    assert_eq!(value["items"][0]["release_date"], "2023-01-01");
    assert_eq!(value["items"][0]["physical_release_date"], "2024-01-01");
    assert_eq!(value["items"][0]["digital_release_date"], "2023-06-01");
    assert_eq!(value["items"][0]["rating"], 8.2);
    assert_eq!(value["items"][0]["ratings"][0]["score"], 8.2);
    assert_eq!(value["items"][0]["ratings"][1]["raw_value"], 95.0);
    assert_eq!(value["items"][0]["ratings"][1]["raw_scale"], 100.0);
    assert!(!value.to_string().contains("/private") && !value.to_string().contains("secret"));
    assert_eq!(value["items"][0]["media_type"], "movie");
    assert_eq!(value["next_offset"], Value::Null);
    let (_, filtered) = query(&api, json!({"query":"story"})).await;

    assert_eq!(filtered["total"], 1);
    let (_, denied) = query(&api, json!({"media_type":"tv"})).await;

    assert_eq!(denied["total"], 0);

    for args in [
        json!({"min_rating":11}),
        json!({"genres":[""]}),
        json!({"year_from":2025,"year_to":2000}),
        json!({"sort":"random"}),
        json!({"root_folder":"/private"}),
        json!({"limit":101}),
    ] {
        assert_eq!(query(&api, args).await.0, reqwest::StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn libraries_over_ten_thousand_are_fully_paginated_and_fresh_reads_bypass_cache() {
    let data = (1..=10003)
        .map(|id| json!({"id":id,"title":format!("Movie {id:05}"),"hasFile":true}))
        .collect::<Vec<_>>();
    let (native, state) = mock(&serde_json::to_string(&data).expect("JSON")).await;
    let api = gateway(&native, &["radarr"]).await;
    let (_, page) = query(&api, json!({"offset":10000,"limit":2})).await;

    assert_eq!(page["total"], 10003);
    assert_eq!(page["items"][0]["id"], 10001);
    assert_eq!(page["next_offset"], 10002);
    let (_, last) = query(&api, json!({"offset":page["next_offset"],"limit":2})).await;

    assert_eq!(last["items"].as_array().expect("page").len(), 1);
    assert_eq!(last["has_more"], false);
    assert_eq!(state.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    *state.body.lock().await = "[]".to_owned();
    let (_, cached) = query(&api, json!({})).await;

    assert_eq!(cached["total"], 10003);
    let (_, fresh) = query(&api, json!({"no_cache":true})).await;

    assert_eq!(fresh["total"], 0);
    assert_eq!(fresh["sources"][0]["age_seconds"], 0);
}

#[tokio::test]
async fn partial_episode_availability_and_failed_sources_are_explicit() {
    let data = json!([{"id":1,"title":"Comedy Show","genres":["Comedy"],"ratings":{"value":8,"votes":10},"statistics":{"episodeFileCount":2,"episodeCount":10,"totalEpisodeCount":20}}]);
    let (native, _state) = mock(&data.to_string()).await;
    let mut config = Config::parse(EXAMPLE).expect("configuration");

    config.sources.get_mut("sonarr").expect("source").url = native.url.clone();
    config.sources.get_mut("radarr").expect("source").url = "http://127.0.0.1:1/".to_owned();
    config.mcp.enabled = true;
    config.seerr = Some(
        serde_json::from_value(json!({"url":native.url,"api_key":"fake-seerr-key"}))
            .expect("connection"),
    );
    config.clients[0].permissions =
        vec!["sonarr:library-view".into(), "radarr:library-view".into()];
    let api = server(crate::proxy::router(
        Gateway::new(&config, tokio::time::Instant::now).expect("gateway"),
    ))
    .await;
    let (_, value) = query(&api, json!({"genres":["Comedy"],"available":true})).await;

    assert_eq!(value["complete"], false);
    assert_eq!(value["total"], 1);
    assert_eq!(value["items"][0]["availability"]["status"], "partial");
    assert_eq!(value["items"][0]["availability"]["episode_files"], 2);
    assert_eq!(
        value["items"][0]["availability"]["coverage_scope"],
        "monitored_aired_episodes"
    );
    assert_eq!(value["items"][0]["rating"], 8.0);
    assert_eq!(value["sources"][0]["status"], "unavailable");
}

#[tokio::test]
async fn an_oversized_native_library_is_explicitly_unavailable_not_a_truncated_page() {
    let data = json!([{"id":1,"title":"One","overview":"a".repeat(1_048_577)}]);
    let (native, _state) = mock(&data.to_string()).await;
    let api = gateway(&native, &["radarr"]).await;
    let (_, value) = query(&api, json!({"no_cache":true})).await;

    assert_eq!(value["complete"], false);
    assert_eq!(value["sources"][0]["code"], "upstream_response_too_large");
    assert_eq!(value["items"], json!([]));
}

#[tokio::test]
async fn rating_and_added_order_keep_nulls_last_and_parse_fractional_timestamps() {
    let data = json!([
        {"id":1,"title":"One","added":"2026-10-14T18:00:00Z"},
        {"id":2,"title":"Two","added":"2026-10-14T18:00:00.123Z","ratings":{"imdb":{"value":7.4,"votes":5}}},
        {"id":3,"title":"Three","added":"2026-10-14T20:00:00+02:00","ratings":{"imdb":{"value":7.4,"votes":5}}},
        {"id":4,"title":"Four","added":"bad timestamp","ratings":{"imdb":{"value":8.0,"votes":10}}}
    ]);
    let (native, _state) = mock(&data.to_string()).await;
    let api = gateway(&native, &["radarr"]).await;
    let (_, added) = query(&api, json!({"sort":"recently_added"})).await;
    let ids = |value: &Value| {
        value["items"]
            .as_array()
            .expect("items")
            .iter()
            .map(|item| item["id"].clone())
            .collect::<Vec<_>>()
    };

    assert_eq!(ids(&added), vec![json!(2), json!(1), json!(3), json!(4)]);
    let (_, rating) = query(&api, json!({"sort":"rating","limit":2})).await;

    assert_eq!(ids(&rating), vec![json!(4), json!(2)]);
    let (_, next) = query(
        &api,
        json!({"sort":"rating","limit":2,"offset":rating["next_offset"]}),
    )
    .await;

    assert_eq!(ids(&next), vec![json!(3), json!(1)]);
}

#[tokio::test]
async fn cli_no_cache_uses_the_structured_library_schema_to_refresh() {
    let (native, state) = mock(r#"[{"id":1,"title":"Before","hasFile":true}]"#).await;
    let api = gateway(&native, &["radarr"]).await;
    let (_, warm) = query(&api, json!({})).await;

    assert_eq!(warm["total"], 1);
    *state.body.lock().await = "[]".to_owned();
    let remote = arrpi_adapters::remote::RemoteApi::new(&api.url, true).expect("remote");
    let fresh = remote
        .execute(
            CLIENT_KEY,
            ToolId::parse("search_library").expect("tool"),
            json!({}),
        )
        .await
        .expect("fresh library");

    assert_eq!(fresh["total"], 0);
    assert_eq!(state.calls.load(std::sync::atomic::Ordering::Relaxed), 2);
}
