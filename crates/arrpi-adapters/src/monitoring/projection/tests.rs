//! Shared native fixtures also satisfy the pinned upstream TypeScript declarations.
use super::project;
use arrpi_core::{config::SourceKind, error::ApiError, summaries::SummaryData};
use serde_json::{Value, json};

const BAZARR: &[u8] =
    include_bytes!("../../../../../web/tests/fixtures/native/bazarr-1.6.2-badges.json");
const CLEANUPARR: &[u8] =
    include_bytes!("../../../../../web/tests/fixtures/native/cleanuparr-2.10.10-stats.json");
const MAINTAINERR: &[u8] =
    include_bytes!("../../../../../web/tests/fixtures/native/maintainerr-3.30.0-storage.json");
const SABNZBD: &[u8] =
    include_bytes!("../../../../../web/tests/fixtures/native/sabnzbd-5.1.3-queue.json");

#[test]
fn pinned_native_fixtures_produce_the_exact_owned_wire_contract() {
    let cases = [
        (
            SourceKind::Bazarr,
            BAZARR,
            json!({"complete":true,"missing_subtitle_requirements":{"episodes":12,"movies":3},"throttled_providers":0,"health_issues":0}),
        ),
        (
            SourceKind::Cleanuparr,
            CLEANUPARR,
            json!({"complete":true,"window_hours":168,"includes_dry_run":false,"removed_downloads":8,"searches":{"total":6,"completed":5,"failed":1,"grabbed":4},"replacement_count":null,"generated_at":"2026-10-15T12:00:00Z"}),
        ),
        (
            SourceKind::Sabnzbd,
            SABNZBD,
            json!({"complete":true,"version":"5.1.3","queued_jobs":3,"active_jobs":2,"paused":false,"speed_kib_per_second":2048.0,"active_remaining_mebibytes":1024.0}),
        ),
        (
            SourceKind::Maintainerr,
            MAINTAINERR,
            json!({"complete":true,"generated_at":"2026-10-15T12:00:00Z","cleanup_scope":"cumulative","cleanup_bytes_scope":"successful_delete_actions_with_known_sizes","cleanup":{"itemsHandled":4,"bytesHandled":12_884_901_888_u64,"moviesHandled":4,"showsHandled":0,"seasonsHandled":0,"episodesHandled":0},"collections":{"reclaimableCount":4,"reclaimableSizedCount":2,"activeSizeBytes":8_589_934_592_u64,"unknown_size_collections":2,"reclaimable_size_complete":false},"reclaimable_uses_fallback":true}),
        ),
    ];

    for (kind, body, expected) in cases {
        let data = project(kind, body).expect("pinned native fixture");
        let wire = serde_json::to_value(data).expect("typed wire document");

        assert_eq!(
            wire, expected,
            "adapter output must preserve public field names and semantics"
        );
        let decoded: SummaryData = serde_json::from_value(wire).expect("owned contract round trip");

        assert!(decoded.is_complete());
    }
}

#[test]
fn sab_numeric_representations_are_validated_before_constructing_public_counters() {
    let original: Value = serde_json::from_slice(SABNZBD).expect("native fixture");

    for value in [json!(2048), json!("2048")] {
        let mut native = original.clone();

        native["queue"]["kbpersec"] = value;
        let data = serde_json::to_vec(&native).expect("fixture");

        project(SourceKind::Sabnzbd, &data).expect("valid numeric counter");
    }

    for value in [
        Value::Null,
        json!(-1),
        json!("NaN"),
        json!("inf"),
        json!(true),
    ] {
        let mut native = original.clone();

        native["queue"]["kbpersec"] = value;
        let data = serde_json::to_vec(&native).expect("fixture");

        assert_eq!(
            project(SourceKind::Sabnzbd, &data).expect_err("invalid counter"),
            ApiError::INVALID_RESPONSE
        );
    }
}
