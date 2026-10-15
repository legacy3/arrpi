//! Native projection compatibility and filtered multi-source pagination.
use arrpi_core::{
    arguments::MediaType,
    library::{LibraryFilter, paginate, select},
};
use serde_json::{Value, json};
use std::borrow::Cow;

fn query(value: Value) -> LibraryFilter {
    serde_json::from_value(value).expect("valid library query")
}

fn page(bytes: &[u8], kind: MediaType, args: &LibraryFilter) -> Value {
    let selected = select(
        "source",
        kind,
        bytes,
        &args.compile().expect("compiled filter"),
    )
    .expect("valid source snapshot");

    serde_json::to_value(paginate(vec![selected], args).expect("selected page"))
        .expect("public page")
}

#[test]
fn all_public_movie_fields_preserve_the_native_projection() {
    let native = json!([{
        "id":1,"title":"ÄÖÜ & original","originalTitle":"Original","year":2024,
        "tmdbId":42,"tvdbId":0,"imdbId":"tt1234567","overview":"Eine Geschichte",
        "genres":["Comedy",4,"Drama"],"ratings":{"imdb":{"value":7.5,"votes":100},"rottenTomatoes":{"value":82,"votes":0}},
        "runtime":123,"certification":"FSK 12","originalLanguage":{"name":"German"},
        "status":"released","studio":"Studio","network":"Network","added":"2026-10-01T00:00:00Z",
        "inCinemas":"2024-01-01","digitalRelease":"2024-02-01","physicalRelease":"2024-03-01",
        "monitored":true,"hasFile":true,"path":"never-leak","movieFile":{"path":"never-leak","apiKey":"never-leak"}
    }]);
    let bytes = serde_json::to_vec(&native).expect("fixture");
    let result = page(&bytes, MediaType::Movie, &query(json!({})));

    assert_eq!(
        result["items"][0],
        json!({
            "source":"source","id":1,"media_type":"movie","title":"ÄÖÜ & original","original_title":"Original",
            "year":2024,"tmdb_id":42,"tvdb_id":null,"imdb_id":"tt1234567","overview":"Eine Geschichte",
            "genres":["Comedy","Drama"],"rating":7.5,"rating_provider":"imdb",
            "ratings":[{"provider":"imdb","score":7.5,"raw_value":7.5,"raw_scale":10.0,"votes":100},{"provider":"rottenTomatoes","score":8.2,"raw_value":82.0,"raw_scale":100.0,"votes":0}],
            "runtime_minutes":123,"certification":"FSK 12","original_language":"German","status":"released",
            "studio":"Studio","network":"Network","added_at":"2026-10-01T00:00:00Z","release_date":"2024-01-01",
            "digital_release_date":"2024-02-01","physical_release_date":"2024-03-01","monitored":true,
            "available":true,"availability":{"status":"available"}
        })
    );
    assert!(!result.to_string().contains("never-leak"));
    let selected = select(
        "source",
        MediaType::Movie,
        &bytes,
        &query(json!({})).compile().expect("filter"),
    )
    .expect("selection");
    let result = paginate(vec![selected], &query(json!({}))).expect("selected page");

    assert!(
        matches!(result.items[0].title, Cow::Borrowed(_)),
        "plain UTF-8 titles borrow their snapshot"
    );
}

#[test]
fn optional_wrong_types_stay_null_and_series_coverage_is_explicit() {
    let body = br#"[{"id":1,"title":"Series","hasFile":"yes","year":"2024","originalTitle":2,"genres":"Drama","ratings":{"value":8,"votes":0},"monitored":2,"originalLanguage":{"name":4},"statistics":{"episodeFileCount":3,"episodeCount":2,"totalEpisodeCount":9,"seasonCount":2}}]"#;
    let result = page(body, MediaType::Tv, &query(json!({})));
    let row = &result["items"][0];

    assert!(row["year"].is_null() && row["original_title"].is_null() && row["rating"].is_null());
    assert_eq!(row["genres"], json!([]));
    assert_eq!(row["available"], true);
    assert_eq!(
        row["availability"],
        json!({"status":"monitored_available","coverage_scope":"monitored_aired_episodes","episode_files":3,"monitored_episodes":2,"total_episodes":9,"seasons":2})
    );
}

#[test]
fn required_fields_are_validated_even_when_the_row_is_excluded() {
    let args = query(json!({"query":"not present","available":false}));

    for invalid in [
        json!({"id":0,"title":"bad"}),
        json!({"id":1}),
        json!({"id":1,"title":4}),
        json!({"id":1,"title":"a".repeat(1025)}),
    ] {
        let body = serde_json::to_vec(&json!([invalid])).expect("invalid record fixture");
        let error = select(
            "source",
            MediaType::Movie,
            &body,
            &args.compile().expect("filter"),
        )
        .expect_err("excluded malformed record still fails its source");

        assert_eq!(error.code(), "invalid_upstream_response");
    }
}

#[test]
fn escaped_titles_and_contextual_unicode_lowercase_match() {
    let bytes = br#"[{"id":1,"title":"\u039f\u03a3","overview":"quote \"inside\""}]"#;
    let result = page(bytes, MediaType::Movie, &query(json!({"query":"ος"})));

    assert_eq!(
        result["total"], 1,
        "Greek final sigma uses str::to_lowercase context"
    );
    assert_eq!(result["items"][0]["title"], "ΟΣ");
    assert_eq!(result["items"][0]["overview"], "quote \"inside\"");
}

#[test]
fn sorts_keep_nulls_last_and_stable_source_id_ties() {
    let first = br#"[{"id":3,"title":"same","year":2020,"ratings":{"value":5},"added":"2020-01-01T00:00:00Z"},{"id":1,"title":"same","year":2024,"ratings":{"value":8},"added":"2024-01-01T00:00:00Z"}]"#;
    let second = br#"[{"id":2,"title":"same"}]"#;

    for (sort, expected) in [
        ("title", json!([2, 1, 3])),
        ("rating", json!([1, 3, 2])),
        ("newest", json!([1, 3, 2])),
        ("recently_added", json!([1, 3, 2])),
    ] {
        let args = query(json!({"sort":sort}));
        let compiled = args.compile().expect("filter");
        let selections = vec![
            select("z", MediaType::Movie, first, &compiled).expect("first source"),
            select("a", MediaType::Tv, second, &compiled).expect("second source"),
        ];
        let result = paginate(selections, &args).expect("selected page");

        assert_eq!(
            json!(result.items.iter().map(|item| item.id).collect::<Vec<_>>()),
            expected
        );
    }
}

#[test]
fn late_pages_and_offsets_beyond_the_end_keep_correct_totals() {
    let rows = (0..10_025)
        .map(|id| json!({"id":id+1,"title":format!("Title {id:05}")}))
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&rows).expect("large snapshot");
    let result = page(
        &bytes,
        MediaType::Movie,
        &query(json!({"offset":10_000,"limit":20})),
    );

    assert_eq!(result["total"], 10_025);
    assert_eq!(result["items"].as_array().expect("items").len(), 20);
    assert_eq!(result["items"][0]["id"], 10_001);
    assert_eq!(result["next_offset"], 10_020);
    let result = page(&bytes, MediaType::Movie, &query(json!({"offset":20_000})));

    assert_eq!(result["total"], 10_025);
    assert_eq!(result["items"], json!([]));
    assert_eq!(result["has_more"], false);
    assert!(result["next_offset"].is_null());
}

#[test]
fn per_source_top_k_merges_into_the_correct_global_page() {
    let bodies = [0, 1].map(|parity| {
        serde_json::to_vec(
            &(0..20)
                .rev()
                .map(|index| json!({"id":index+1,"title":format!("Item {:03}",index*2+parity)}))
                .collect::<Vec<_>>(),
        )
        .expect("interleaved source")
    });
    let args = query(json!({"offset":13,"limit":5}));
    let filter = args.compile().expect("filter");
    let selections = vec![
        select("movies", MediaType::Movie, &bodies[0], &filter).expect("movies"),
        select("shows", MediaType::Tv, &bodies[1], &filter).expect("shows"),
    ];
    let result = paginate(selections, &args).expect("combined page");

    assert_eq!(
        result.total, 40,
        "source totals include candidates outside top K"
    );
    assert_eq!(
        result
            .items
            .iter()
            .map(|item| item.title.as_ref())
            .collect::<Vec<_>>(),
        ["Item 013", "Item 014", "Item 015", "Item 016", "Item 017"]
    );
    assert_eq!(result.next_offset, Some(18));
}

#[test]
fn exact_duplicate_sort_keys_retain_input_order_and_json_errors_take_precedence() {
    let body = br#"[{"id":1,"title":"same","overview":"first"},{"id":1,"title":"same","overview":"second"}]"#;
    let result = page(body, MediaType::Movie, &query(json!({"limit":1})));

    assert_eq!(result["items"][0]["overview"], "first");
    assert_eq!(result["total"], 2);
    let filter = query(json!({})).compile().expect("filter");

    for body in [
        br#"[{"id":0,"title":"bad"},{"unfinished":"# as &[u8],
        br#"[{"id":1,"title":"valid"}] trailing"#,
    ] {
        let error = select("source", MediaType::Movie, body, &filter)
            .expect_err("validate all JSON before semantic errors");

        assert_eq!(error.code(), "invalid_upstream_json");
    }
}

#[test]
fn genre_limit_counts_native_positions_before_discarding_wrong_types() {
    let mut genres = vec![Value::Null];

    genres.extend((0..1000).map(|index| json!(format!("Genre {index}"))));
    let bytes = serde_json::to_vec(&json!([{"id":1,"title":"Example","genres":genres}]))
        .expect("large native genre array");
    let result = page(&bytes, MediaType::Movie, &query(json!({})));
    let genres = result["items"][0]["genres"]
        .as_array()
        .expect("public genre array");

    assert_eq!(
        genres.len(),
        63,
        "only the first64 native positions are considered"
    );
    assert_eq!(genres.last(), Some(&json!("Genre 62")));
}

#[test]
fn native_records_and_nested_metadata_require_objects_and_duplicate_keys_use_last_values() {
    let args = query(json!({}));
    let compiled = args.compile().expect("filter");

    for body in [
        br#"[[1,"positional title"]]"# as &[u8],
        br"[null]",
        br#"["text"]"#,
    ] {
        assert_eq!(
            select("source", MediaType::Movie, body, &compiled)
                .expect_err("native records are objects")
                .code(),
            "invalid_upstream_response"
        );
    }

    let bytes = br#"[{"id":0,"id":1,"title":"old","title":"new","statistics":{"episodeFileCount":1,"episodeFileCount":5,"episodeCount":5},"ratings":{"value":7,"value":8},"originalLanguage":{"name":"English","name":"German"}}]"#;
    let result = page(bytes, MediaType::Tv, &args);

    assert_eq!(result["items"][0]["title"], "new");
    assert_eq!(result["items"][0]["rating"], 8.0);
    assert_eq!(result["items"][0]["original_language"], "German");
    assert_eq!(result["items"][0]["availability"]["episode_files"], 5);
    let result = page(br#"[{"id":1,"title":"title","statistics":[3,2],"ratings":{"imdb":[8,100]},"originalLanguage":["German"]}]"#,MediaType::Tv,&args);

    assert_eq!(result["items"][0]["available"], false);
    assert!(
        result["items"][0]["rating"].is_null() && result["items"][0]["original_language"].is_null()
    );
}

#[test]
fn empty_inputs_media_kind_filters_and_changed_paging_bounds_are_explicit() {
    let args = query(json!({"media_type":"tv","limit":1}));
    let compiled = args.compile().expect("filter");
    let body = br#"[{"id":1,"title":"One"},{"id":2,"title":"Two"}]"#;
    let excluded = select("movies", MediaType::Movie, body, &compiled)
        .expect("source validates despite kind exclusion");
    let empty = select("shows", MediaType::Tv, b"[]", &compiled).expect("empty source");
    let result = paginate(vec![excluded, empty], &args).expect("empty page");

    assert_eq!(result.total, 0);
    assert!(result.items.is_empty());
    assert_eq!(
        paginate(vec![], &args).expect("no healthy sources").total,
        0
    );
    let selected = select("shows", MediaType::Tv, body, &compiled).expect("selection");

    assert_eq!(
        paginate(vec![selected], &query(json!({"limit":2})))
            .expect_err("insufficient per-source candidates for changed page")
            .code(),
        "invalid_arguments"
    );
    assert_eq!(
        select(
            "source",
            MediaType::Tv,
            br#"{"error":"native failed"}"#,
            &compiled
        )
        .expect_err("library root is an array")
        .code(),
        "invalid_upstream_response"
    );
}

#[test]
fn file_metadata_is_borrowed_native_data_without_paths_or_language_inference() {
    let bytes = br#"[{"id":1,"title":"Film","hasFile":true,"originalLanguage":{"name":"French"},"movieFile":{"path":"never-leak","languages":[{"name":"Not audio metadata"}],"quality":{"quality":{"name":"WEBDL-1080p","resolution":1080,"source":"webdl"}},"mediaInfo":{"audioLanguages":"German / English","subtitles":"German / English / Japanese","videoCodec":"x265","private":"never-leak"}}}]"#;
    let filter = query(json!({}));
    let selected = select(
        "source",
        MediaType::Movie,
        bytes,
        &filter.compile().expect("filter"),
    )
    .expect("native movie");
    let result = paginate(vec![selected], &filter).expect("page");
    let file = result.items[0]
        .file
        .as_ref()
        .expect("reported file metadata");

    assert!(matches!(
        file.audio_languages,
        Some(Cow::Borrowed("German / English"))
    ));
    assert!(matches!(file.subtitle_languages, Some(Cow::Borrowed(_))));
    let output = serde_json::to_value(&result).expect("public metadata");

    assert_eq!(
        output["items"][0]["file"],
        json!({"audio_languages":"German / English","subtitle_languages":"German / English / Japanese","video_codec":"x265","quality":{"name":"WEBDL-1080p","resolution":1080,"source":"webdl"}})
    );
    assert!(!output.to_string().contains("never-leak"));
    assert!(!output.to_string().contains("Not audio metadata"));
    assert!(
        page(bytes, MediaType::Tv, &filter)["items"][0]
            .get("file")
            .is_none(),
        "series-level snapshots do not establish any episode-file metadata"
    );
}

#[test]
fn malformed_missing_and_oversized_file_fields_remain_unknown() {
    for file in [
        Value::Null,
        json!({"path":"private"}),
        json!({"mediaInfo":{"audioLanguages":["German"],"subtitles":42},"quality":{"quality":{"name":{},"resolution":-1}}}),
        json!({"mediaInfo":{"audioLanguages":"x".repeat(1025),"subtitles":"x".repeat(1025),"videoCodec":"x".repeat(65)},"quality":{"quality":{"name":"x".repeat(129),"resolution":99999,"source":"x".repeat(65)}}}),
    ] {
        let body = serde_json::to_vec(&json!([{"id":1,"title":"Film","hasFile":true,"originalLanguage":{"name":"German"},"movieFile":file}])).expect("fixture");
        let result = page(&body, MediaType::Movie, &query(json!({})));

        assert!(result["items"][0].get("file").is_none());
    }
}
