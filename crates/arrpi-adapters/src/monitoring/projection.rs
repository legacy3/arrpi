//! Decode only consumed native fields and construct the owned, typed summary contract.
mod native;
#[cfg(test)]
mod tests;
use arrpi_core::{
    config::SourceKind,
    error::ApiError,
    summaries::{
        BazarrSummary, CleanupBytesScope, CleanupScope, CleanuparrSummary, CollectionCounts,
        MaintainerrSummary, SabnzbdSummary, SubtitleRequirements, SummaryData,
    },
};

pub(super) fn project(kind: SourceKind, body: &[u8]) -> Result<SummaryData, ApiError> {
    match kind {
        SourceKind::Sabnzbd => {
            let response: native::SabResponse = serde_json::from_slice(body)?;

            if response.status == Some(false) {
                return Err(ApiError::REJECTED);
            }

            let queue = response.queue.ok_or(ApiError::INVALID_RESPONSE)?;

            if !queue.version.starts_with("4.") && !queue.version.starts_with("5.") {
                return Err(ApiError::INVALID_RESPONSE);
            }

            Ok(SummaryData::Sabnzbd(SabnzbdSummary {
                complete: true,
                version: queue.version,
                queued_jobs: queue.noofslots,
                active_jobs: queue.noofslots_total,
                paused: queue.paused,
                speed_kib_per_second: queue.kbpersec.value()?,
                active_remaining_mebibytes: queue.mbleft.value()?,
            }))
        }

        SourceKind::Cleanuparr => {
            let native: native::CleanupResponse = serde_json::from_slice(body)?;

            if native.timeframe_hours != 168 {
                return Err(ApiError::INVALID_RESPONSE);
            }

            Ok(SummaryData::Cleanuparr(CleanuparrSummary {
                complete: true,
                window_hours: native.timeframe_hours,
                includes_dry_run: false,
                removed_downloads: native.removals.total,
                searches: native.searches,
                replacement_count: None,
                generated_at: timestamp(native.generated_at)?,
            }))
        }

        SourceKind::Bazarr => {
            let native: native::Badges = serde_json::from_slice(body)?;

            Ok(SummaryData::Bazarr(BazarrSummary {
                complete: true,
                missing_subtitle_requirements: SubtitleRequirements {
                    episodes: native.episodes,
                    movies: native.movies,
                },
                throttled_providers: native.providers,
                health_issues: native.status,
            }))
        }

        SourceKind::Maintainerr => {
            let native: native::StorageMetrics = serde_json::from_slice(body)?;
            let collections = native.collection_summary;
            let unknown = collections
                .reclaimable_count
                .checked_sub(collections.reclaimable_sized_count)
                .ok_or(ApiError::INVALID_RESPONSE)?;

            Ok(SummaryData::Maintainerr(MaintainerrSummary {
                complete: true,
                generated_at: timestamp(native.generated_at)?,
                cleanup_scope: CleanupScope::Cumulative,
                cleanup_bytes_scope: CleanupBytesScope::SuccessfulDeleteActionsWithKnownSizes,
                cleanup: native.cleanup_totals,
                collections: CollectionCounts {
                    reclaimable_count: collections.reclaimable_count,
                    reclaimable_sized_count: collections.reclaimable_sized_count,
                    active_size_bytes: collections.active_size_bytes,
                    unknown_size_collections: unknown,
                    reclaimable_size_complete: unknown == 0
                        && !collections.reclaimable_using_fallback,
                },
                reclaimable_uses_fallback: collections.reclaimable_using_fallback,
            }))
        }

        _ => Err(ApiError::NOT_FOUND),
    }
}

fn timestamp(value: String) -> Result<String, ApiError> {
    chrono::DateTime::parse_from_rfc3339(&value).map_err(|error| {
        tracing::debug!(
            event = "invalid_monitoring_timestamp",
            category = std::any::type_name_of_val(&error)
        );
        ApiError::INVALID_RESPONSE
    })?;

    Ok(value)
}
