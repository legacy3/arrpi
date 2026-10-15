//! Public contracts preserve masking, schema bounds and cheap shared descriptors.

use std::sync::Arc;

use arrpi_core::{
    config::{self, CachePolicy, Config},
    contracts::{Endpoint, EventKind, SourceAvailability},
    policy::{Limits, Policy},
    registry::{Registry, ToolId},
    schema,
    secret::Secret,
    subscriptions,
};
use serde_json::{Map, Value, json};

fn configured() -> Config {
    Config::parse(include_str!("../../../tests/arrpi.json")).expect("valid fake configuration")
}

#[test]
fn catalog_defaults_to_english_and_preserves_explicit_languages() {
    let mut document = json!({"url":"http://seerr:5055/","api_key":"fake-service-key"});
    let default: config::SeerrConnection =
        serde_json::from_value(document.clone()).expect("default language");

    assert_eq!(default.language, "en");
    document["language"] = json!("de");
    let explicit: config::SeerrConnection =
        serde_json::from_value(document).expect("chosen language");

    assert_eq!(explicit.language, "de");
}

#[test]
fn configuration_credentials_cannot_become_shared_requester_accounts() {
    for permission in ["admin", "config:manage"] {
        let mut config = configured();

        config.clients[0].permissions = vec![permission.into()];
        config.clients[0].seerr_user = Some(arrpi_core::policy::Binding {
            id: 1,
            plex_id: 101,
        });
        assert!(
            config
                .validate()
                .expect_err("shared admin key must stay unbound")
                .reason()
                .contains("configuration clients cannot bind")
        );
    }
}

#[test]
fn credentials_deserialize_into_zeroizing_storage_and_never_serialize() {
    let secret: Secret = serde_json::from_str("\"fake-native-secret\"").expect("string credential");
    let omitted: Secret = serde_json::from_str("null").expect("omitted credential");

    assert_eq!(
        secret.value(),
        "fake-native-secret",
        "authorized borrowed exposure"
    );
    assert!(!secret.missing(), "present credential has a value");
    assert!(
        omitted.missing(),
        "null retains an existing credential during update"
    );
    assert_eq!(
        serde_json::to_value(&secret).expect("masked serialization"),
        Value::Null
    );
    assert_eq!(
        serde_json::to_value(secret.clone()).expect("masked clone"),
        Value::Null
    );
    assert_eq!(
        format!("{secret:?}"),
        "[REDACTED]",
        "diagnostics never expose credentials"
    );
}

#[test]
fn canonical_configuration_rejects_unusable_or_duplicate_client_keys() {
    for key in [
        "short".to_owned(),
        "a".repeat(129),
        " ".repeat(32),
        "ä".repeat(32),
    ] {
        let mut config = configured();

        config.clients[0].key = key.into();

        assert!(config.validate().is_err());
    }

    let mut config = configured();

    config.clients[0].key = config.clients[1].key.clone();

    assert!(config.validate().is_err());
}

#[test]
fn native_sources_cannot_overlap_broker_and_global_grant_namespaces() {
    for name in [
        "calendar", "seerr", "catalog", "requests", "issues", "config", "feeds",
    ] {
        let mut config = configured();
        let source = config.sources["radarr"].clone();

        config.seerr = None;
        config.sources.insert(name.to_owned(), source);

        assert!(config.validate().is_err(), "reserved namespace {name}");
    }
}

#[test]
fn enabled_subscription_master_matches_storage_credential_bounds() {
    for length in [0, 31, 32, 128, 129] {
        let policy = subscriptions::Policy {
            enabled: true,
            master_key: "a".repeat(length).into(),
            ..subscriptions::Policy::default()
        };

        assert_eq!(policy.validate().is_ok(), (32..=128).contains(&length));
    }
}

#[test]
fn configuration_serialization_masks_every_credential_location() {
    let value = serde_json::to_value(configured()).expect("public settings document");

    assert_eq!(value["sources"]["sonarr"]["api_key"], Value::Null);
    assert_eq!(value["sources"]["radarr"]["api_key"], Value::Null);
    assert_eq!(value["seerr"]["api_key"], Value::Null);
    assert_eq!(value["subscriptions"]["master_key"], Value::Null);
    assert!(
        value["clients"]
            .as_array()
            .expect("client array")
            .iter()
            .all(|client| client["key"].is_null()),
        "all client credentials are masked"
    );
    assert!(
        !value.to_string().contains("fake-"),
        "no fake credential survives serialization"
    );
}

#[test]
fn exported_components_match_the_committed_public_contract() {
    let checked: Value = serde_json::from_str(include_str!("../../../docs/api/public.schema.json"))
        .expect("committed public schema");

    assert_eq!(
        schema::document(),
        checked,
        "regenerate public Rust and TypeScript contracts together"
    );
}

#[test]
fn emitted_cache_bounds_match_runtime_validation() {
    let document = schema::document();
    let properties = document["components"]["schemas"]["CachePolicy"]["properties"]
        .as_object()
        .expect("cache properties");

    for (name, field) in properties {
        let minimum = field["minimum"].as_u64().expect("explicit lower bound");
        let maximum = field["maximum"].as_u64().expect("explicit upper bound");
        let mut config = configured();

        for number in [minimum, maximum] {
            config.cache = cache_field(name, number);
            config.validate().expect("supported schema boundary");
        }

        for number in minimum.checked_sub(1).into_iter().chain([maximum + 1]) {
            config.cache = cache_field(name, number);
            config
                .validate()
                .expect_err("unsupported {name} boundary {number}");
        }
    }
}

fn cache_field(name: &str, number: u64) -> CachePolicy {
    let mut value = serde_json::to_value(CachePolicy::default()).expect("cache defaults");

    value[name] = json!(number);
    serde_json::from_value(value).expect("bounded integer cache field")
}

#[test]
fn emitted_mcp_bounds_match_runtime_validation() {
    let document = schema::document();
    let fields = document["components"]["schemas"]["Limits"]["properties"]
        .as_object()
        .expect("MCP limit fields");

    for (name, field) in fields {
        let minimum = field["minimum"].as_u64().expect("explicit lower bound");
        let maximum = field["maximum"].as_u64().expect("explicit upper bound");
        let mut policy = Policy::default();

        for number in [minimum, maximum] {
            policy.limits = limits_field(name, number);
            policy
                .validate()
                .expect("supported {name} boundary {number}");
        }

        for number in [minimum - 1, maximum + 1] {
            policy.limits = limits_field(name, number);
            policy
                .validate()
                .expect_err("unsupported {name} boundary {number}");
        }
    }
}

fn limits_field(name: &str, number: u64) -> Limits {
    let mut value = serde_json::to_value(Limits::default()).expect("MCP defaults");

    value[name] = json!(number);
    serde_json::from_value(value).expect("bounded integer MCP field")
}

#[test]
fn subscription_window_bounds_and_combined_budget_are_both_enforced() {
    let document = schema::document();
    let fields = document["components"]["schemas"]["SubscriptionPolicy"]["properties"]
        .as_object()
        .expect("subscription fields");

    for name in ["max_subjects", "past_days", "future_days"] {
        let minimum = fields[name]["minimum"]
            .as_u64()
            .expect("explicit lower bound");
        let maximum = fields[name]["maximum"]
            .as_u64()
            .expect("explicit upper bound");

        for number in [minimum, maximum] {
            let mut value = json!({"past_days":0,"future_days":0});

            value[name] = json!(number);
            let policy: subscriptions::Policy =
                serde_json::from_value(value).expect("subscription policy");

            policy
                .validate()
                .expect("supported {name} boundary {number}");
        }
    }

    let combined: subscriptions::Policy =
        serde_json::from_value(json!({"past_days":60,"future_days":60}))
            .expect("individual valid day bounds");

    combined
        .validate()
        .expect_err("two valid fields cannot exceed the combined 92-day window");
}

#[test]
fn descriptors_share_schema_backing_and_classify_operation_effects() {
    let registry = Registry::new(Limits {
        max_batch_items: 7,
        max_page_items: 31,
        ..Limits::default()
    });
    let read = registry.definition(ToolId::ListMovies);
    let first: Arc<Map<String, Value>> = read.input_schema().clone().into();
    let second: Arc<Map<String, Value>> = read.input_schema().clone().into();

    assert!(
        Arc::ptr_eq(&first, &second),
        "SDK conversion clones the owner, not the schema"
    );
    assert_eq!(first["properties"]["limit"]["maximum"], 31);
    let batch = registry.definition(ToolId::CloseIssuesBatch);

    assert_eq!(
        batch.input_schema().as_ref()["properties"]["issue_ids"]["maxItems"],
        7
    );
    assert!(read.effects.is_read_only());
    assert!(!read.effects.is_destructive());
    let cancel = registry.definition(ToolId::CancelRequest).effects;

    assert!(!cancel.is_read_only());
    assert!(cancel.is_destructive());
}

#[test]
fn typed_wire_enums_and_validated_origins_keep_stable_spellings() {
    assert_eq!(
        serde_json::to_value(EventKind::Physical).expect("release kind"),
        "physical"
    );
    assert_eq!(
        serde_json::to_value(SourceAvailability::Unavailable).expect("source failure"),
        "unavailable"
    );
    assert_eq!(Endpoint::Qualityprofile.name(), "qualityprofile");
    let origin: url::Url = config::base_url("https://example.invalid/sonarr")
        .expect("valid base")
        .into();

    assert_eq!(origin.as_str(), "https://example.invalid/sonarr/");
    config::base_url("https://user:password@example.invalid/")
        .expect_err("credentials cannot be embedded in an origin");
}

#[test]
fn registry_metadata_matches_strict_argument_schemas_and_small_page_defaults() {
    let registry = Registry::new(Limits {
        max_page_items: 5,
        ..Limits::default()
    });
    let mut previous = "";

    for id in ToolId::ALL {
        let schema = registry.definition(*id).input_schema().as_ref();

        assert_eq!(schema["type"], "object", "tool arguments are objects");
        assert_eq!(
            schema["additionalProperties"], false,
            "unknown fields cannot override native policy"
        );
        assert_eq!(
            schema["properties"].get("no_cache").is_some(),
            id.accepts_fresh(),
            "freshness metadata matches the actual argument contract"
        );
        assert!(
            id.name() > previous,
            "stable public names are unique and sorted"
        );
        previous = id.name();

        if id.is_paginated() {
            assert_eq!(
                schema["properties"]["limit"]["default"], 5,
                "omitted page limits agree with configured dispatch defaults"
            );
            assert_eq!(schema["properties"]["limit"]["maximum"], 5);
        }
    }
}

#[test]
fn public_error_payloads_keep_conflicts_and_schema_failures_distinct() {
    use arrpi_core::error::ApiError;

    for error in [
        ApiError::CONFLICT,
        ApiError::INTERRUPTED,
        ApiError::UNKNOWN_OUTCOME,
        ApiError::profile_mismatch(42),
    ] {
        assert_eq!(
            ApiError::from_payload(&error.payload()),
            Some(error),
            "known public outcomes survive a remote bridge"
        );
    }

    let semantic =
        serde_json::from_value::<u64>(json!("not a number")).expect_err("schema mismatch");
    let malformed = serde_json::from_str::<Value>("{no").expect_err("invalid JSON syntax");

    assert_eq!(
        ApiError::from(semantic),
        ApiError::INVALID_RESPONSE,
        "typed data failures are not invented line-zero syntax errors"
    );
    assert_eq!(ApiError::from(malformed).code(), "invalid_upstream_json");
}

#[test]
fn calendar_dates_reject_non_exact_or_impossible_values_before_native_conversion() {
    let first = arrpi_core::arguments::date("2026-10-15").expect("valid wire date");
    let second = arrpi_core::arguments::date("2026-10-16").expect("following day");

    assert_eq!(second.days_since(first), 1);
    assert_eq!(chrono::NaiveDate::from(first).to_string(), "2026-10-15");

    for value in [
        "2026-1-15",
        "2026-10-5",
        "2026-10-15T00:00:00Z",
        "2026-02-30",
        " 2026-10-15",
    ] {
        assert_eq!(
            arrpi_core::arguments::date(value).expect_err("strict calendar date"),
            arrpi_core::error::ApiError::BAD_QUERY
        );
    }
}
