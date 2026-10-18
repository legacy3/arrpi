//! Configuration validation and canonical query restrictions.

use crate::{
    auth::{Access, authenticate},
    config::Config,
    proxy::Gateway,
    query,
};

use super::{CLIENT_KEY, EXAMPLE};

#[test]
fn example_validates_without_environment_access() {
    let config = Config::parse(EXAMPLE).expect("checked-in example parses");

    assert_eq!(config.sources.len(), 2, "two native sources");
    assert_eq!(config.cache.max_entries, 32, "bounded per-source cache");
    assert!(
        Gateway::new(&config, tokio::time::Instant::now).is_ok(),
        "database settings contain their credentials"
    );
}

#[test]
fn invalid_origins_permissions_and_bounds_are_rejected() {
    for (old, new) in [
        ("http://sonarr:8989/", "ftp://sonarr/"),
        ("http://sonarr:8989/", "http://user:password@sonarr/"),
        ("http://sonarr:8989/", "http://sonarr/?apikey=secret"),
        ("http://sonarr:8989/", "http://sonarr/#fragment"),
        ("sonarr:series", "sonarr:command"),
        ("sonarr:series", "unknown:series"),
        ("\"max_entries\": 32", "\"max_entries\": 0"),
        (
            "\"max_body_bytes\": 1048576",
            "\"max_body_bytes\": 999999999",
        ),
        ("\"fresh_seconds\": 60", "\"fresh_seconds\": 0"),
        ("\"max_requests\": 64", "\"max_requests\": 1000"),
        ("\"sonarr\": {", "\"calendar\": {"),
    ] {
        assert!(
            Config::parse(&EXAMPLE.replace(old, new)).is_err(),
            "reject invalid policy: {new}"
        );
    }

    assert!(
        Config::parse(&format!("unknown = true\n{EXAMPLE}")).is_err(),
        "unknown fields are not ignored"
    );
}

#[test]
fn feed_metadata_is_public_bounded_and_cannot_embed_credentials() {
    let valid = EXAMPLE.replace("ArrPi", "Friends & Films").replace(
        "https://github.com/legacy3/arrpi",
        "https://example.test/calendar",
    );
    let config = Config::parse(&valid).expect("public feed metadata");

    assert_eq!(config.instance.title, "Friends & Films");
    assert_eq!(config.instance.link, "https://example.test/calendar");

    for link in [
        "javascript:alert(1)",
        "https://user:secret@example.test/calendar",
        "https://example.test/calendar?apikey=secret",
        "https://example.test/calendar#secret",
        "https://example.test/ca\tlendar",
        " https://example.test/calendar",
        "https://example.test/\u{fffe}",
    ] {
        assert!(
            Config::parse(&valid.replace("https://example.test/calendar", link)).is_err(),
            "reject private or non-web link"
        );
    }

    assert!(
        Config::parse(&valid.replace("Friends & Films", "")).is_err(),
        "empty feed title rejected"
    );
}

#[test]
fn secrets_are_redacted_and_short_duplicate_keys_rejected() {
    let config = Config::parse(EXAMPLE).expect("example config");
    let access = Access::new(&config.clients[0]).expect("fake reader");
    let debug = format!("{access:?}");

    assert!(
        !debug.contains(CLIENT_KEY),
        "Debug redacts client credential"
    );
    let mut invalid = config.clone();

    invalid.clients[0].key = "short".to_owned().into();
    Gateway::new(&invalid, tokio::time::Instant::now).expect_err("invalid credential");
    invalid.clients[0].key = invalid.clients[1].key.clone();
    Gateway::new(&invalid, tokio::time::Instant::now).expect_err("invalid credential");
}

#[test]
fn duplicate_authorization_headers_fail_closed() {
    let config = Config::parse(EXAMPLE).expect("example config");
    let access = Access::new(&config.clients[0]).expect("fake reader");
    let mut headers = axum::http::HeaderMap::new();
    let key = format!("Bearer {CLIENT_KEY}")
        .parse::<axum::http::HeaderValue>()
        .expect("fake bearer");

    headers.append("authorization", key.clone());
    headers.append("authorization", key);

    assert!(
        authenticate(&headers, &[access]).is_err(),
        "ambiguous auth never succeeds"
    );
}

#[test]
fn calendar_query_is_bounded_canonical_and_cannot_override_policy() {
    let first =
        query::canonical("calendar", Some("end=2026-11-01&start=2026-10-01")).expect("valid query");
    let second = query::canonical("calendar", Some("start=2026-10-01&end=2026-11-01"))
        .expect("reordered query");

    assert_eq!(first, second, "query ordering shares one cache key");
    assert!(
        first.contains("unmonitored=false"),
        "monitored calendar policy enforced"
    );

    for raw in [
        "start=2026-10-01",
        "start=2026-10-01&end=2027-10-01",
        "start=2026-10-01&end=2026-09-01",
        "start=2026-02-30&end=2026-03-01",
        "start=2026-10-01&end=2026-11-01&start=2026-09-01",
        "start=2026-10-01&end=2026-11-01&apikey=secret",
        "start=2026-10-01&end=2026-11-01&unmonitored=true",
        "start=2026-10-01&end=2026-11-01&url=http://evil.example",
    ] {
        assert!(
            query::canonical("calendar", Some(raw)).is_err(),
            "reject unsafe query: {raw}"
        );
    }

    assert!(
        query::canonical("queue", Some("pageSize=101")).is_err(),
        "queue page size bounded"
    );
    assert!(
        query::canonical("series", Some("apikey=secret")).is_err(),
        "native credential parameters blocked"
    );
}

#[test]
fn service_bindings_are_kind_scoped_unique_and_large_library_byte_caps_are_bounded() {
    let mut config = Config::parse(EXAMPLE).expect("configuration");

    for source in config.sources.values_mut() {
        source.seerr_server_id = Some(0);
    }

    config.cache.max_body_bytes = 32 * 1024 * 1024;
    config.validate().expect("same ID across different apps");
    let duplicate = config.sources["sonarr"].clone();

    config.sources.insert("sonarr-two".to_owned(), duplicate);
    assert!(config.validate().is_err());
    config.sources.remove("sonarr-two");
    config.cache.max_body_bytes = 64 * 1024 * 1024 + 1;
    assert!(config.validate().is_err());
}
