//! Small, validated counter projections; upstream paths, credentials and identities stay private.
use arrpi_core::{config::SourceKind, error::ApiError};
use serde_json::{Value, json};

fn count(value: &Value) -> Result<u64, ApiError> {
    value.as_u64().ok_or(ApiError::INVALID_RESPONSE)
}

fn numeric(value: &Value) -> Result<f64, ApiError> {
    let number = value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
        .ok_or(ApiError::INVALID_RESPONSE)?;

    if number.is_finite() && number >= 0.0 {
        Ok(number)
    } else {
        Err(ApiError::INVALID_RESPONSE)
    }
}

fn numbers(value: &Value, keys: &[&str]) -> Result<Value, ApiError> {
    let mut output = serde_json::Map::new();

    for key in keys {
        output.insert((*key).to_owned(), json!(count(&value[key])?));
    }

    Ok(Value::Object(output))
}

pub(super) fn project(kind: SourceKind, body: &[u8]) -> Result<Value, ApiError> {
    let native: Value = serde_json::from_slice(body)?;
    let mut output = match kind {
        SourceKind::Sabnzbd => {
            if native["status"] == false {
                return Err(ApiError::REJECTED);
            }

            let queue = &native["queue"];
            let version = queue["version"]
                .as_str()
                .filter(|version| version.starts_with("4.") || version.starts_with("5."))
                .ok_or(ApiError::INVALID_RESPONSE)?;

            json!({
                "version": version,
                "queued_jobs": count(&queue["noofslots"])?,
                "active_jobs": count(&queue["noofslots_total"])?,
                "paused": queue["paused"].as_bool().ok_or(ApiError::INVALID_RESPONSE)?,
                "speed_kib_per_second": numeric(&queue["kbpersec"])?,
                "active_remaining_mebibytes": numeric(&queue["mbleft"])?,
            })
        }

        SourceKind::Cleanuparr => {
            let window = count(&native["timeframeHours"])?;

            if window != 168 {
                return Err(ApiError::INVALID_RESPONSE);
            }

            json!({
                "window_hours": window,
                "includes_dry_run": false,
                "removed_downloads": count(&native["removals"]["total"])?,
                "searches": numbers(&native["searches"], &["total", "completed", "failed", "grabbed"])?,
                "replacement_count": null,
                "generated_at": timestamp(&native["generatedAt"])?,
            })
        }

        SourceKind::Bazarr => {
            json!({
                "missing_subtitle_requirements": {
                    "episodes": count(&native["episodes"])?,
                    "movies": count(&native["movies"])?,
                },
                "throttled_providers": count(&native["providers"])?,
                "health_issues": count(&native["status"])?,
            })
        }

        SourceKind::Maintainerr => {
            json!({
                "generated_at": timestamp(&native["generatedAt"])?,
                "cleanup_scope": "cumulative",
                "cleanup_bytes_scope": "successful_delete_actions_with_known_sizes",
                "cleanup": numbers(&native["cleanupTotals"], &[
                    "itemsHandled", "bytesHandled", "moviesHandled", "showsHandled", "seasonsHandled", "episodesHandled",
                ])?,
                "collections": numbers(&native["collectionSummary"], &[
                    "reclaimableCount", "reclaimableSizedCount", "activeSizeBytes",
                ])?,
                "reclaimable_uses_fallback": native["collectionSummary"]["reclaimableUsingFallback"].as_bool().ok_or(ApiError::INVALID_RESPONSE)?,
            })
        }

        _ => return Err(ApiError::NOT_FOUND),
    };

    if kind == SourceKind::Maintainerr {
        let total = count(&native["collectionSummary"]["reclaimableCount"])?;
        let sized = count(&native["collectionSummary"]["reclaimableSizedCount"])?;
        let unknown = total.checked_sub(sized).ok_or(ApiError::INVALID_RESPONSE)?;

        output["collections"]["unknown_size_collections"] = json!(unknown);
        output["collections"]["reclaimable_size_complete"] =
            json!(unknown == 0 && output["reclaimable_uses_fallback"] == false);
    }

    output["complete"] = json!(true);
    Ok(output)
}

fn timestamp(value: &Value) -> Result<&str, ApiError> {
    value
        .as_str()
        .filter(|text| chrono::DateTime::parse_from_rfc3339(text).is_ok())
        .ok_or(ApiError::INVALID_RESPONSE)
}
