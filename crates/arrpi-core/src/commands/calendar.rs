//! Concurrent calendar aggregation depends only on normalized calendar capabilities.

use crate::{
    arguments::{Dates, date},
    contracts::{CacheState, Calendar, SourceAvailability, SourceStatus},
    error::ApiError,
    source::{Observation, ReadMode, Sources},
};

pub(super) async fn load(
    sources: &Sources,
    allowed: &[String],
    args: Dates,
) -> Result<Calendar, ApiError> {
    let start = date(&args.start)?;
    let end = date(&args.end)?;

    if end < start || end.days_since(start) > 93 || allowed.is_empty() {
        return Err(if allowed.is_empty() {
            ApiError::FORBIDDEN
        } else {
            ApiError::BAD_QUERY
        });
    }

    let mut jobs = tokio::task::JoinSet::new();
    let mode = if args.no_cache {
        ReadMode::Fresh
    } else {
        ReadMode::Cached
    };

    for name in allowed {
        let source = sources.shared(name).ok_or(ApiError::NOT_FOUND)?;
        let name = name.clone();
        let start = args.start.clone();
        let end = args.end.clone();

        jobs.spawn(async move {
            let result = match source.calendar() {
                Some(calendar) => calendar.calendar(&start, &end, mode).await,
                None => Err(ApiError::NOT_FOUND),
            };

            (name, result)
        });
    }

    let mut calendar = Calendar {
        events: Vec::with_capacity(64),
        sources: Vec::with_capacity(jobs.len()),
    };

    loop {
        let Some(result) = jobs.join_next().await else {
            break;
        };
        let (source, result) = result?;

        append(&mut calendar, source, result)?;
    }

    calendar.events.sort_by(|left, right| {
        (&left.date, &left.source, left.id, left.kind.name()).cmp(&(
            &right.date,
            &right.source,
            right.id,
            right.kind.name(),
        ))
    });
    calendar
        .sources
        .sort_by(|left, right| left.source.cmp(&right.source));
    Ok(calendar)
}

fn append(
    calendar: &mut Calendar,
    source: String,
    result: Result<crate::source::SourceRead<Vec<crate::contracts::CalendarEvent>>, ApiError>,
) -> Result<(), ApiError> {
    match result {
        Ok(read) => {
            if calendar.events.len().saturating_add(read.data.len()) > 8192 {
                return Err(ApiError::CALENDAR_TOO_LARGE);
            }

            calendar.events.extend(read.data);
            calendar.sources.push(SourceStatus {
                source,
                status: SourceAvailability::Ok,
                cache: match read.freshness.cache {
                    Observation::Hit => Some(CacheState::Hit),
                    Observation::Miss => Some(CacheState::Miss),
                    Observation::Stale => Some(CacheState::Stale),
                    Observation::NotCached => None,
                },
                age_seconds: read.freshness.age_seconds,
            });
        }

        Err(ApiError::BUSY) => return Err(ApiError::BUSY),

        _ => calendar.sources.push(SourceStatus {
            source,
            status: SourceAvailability::Unavailable,
            cache: None,
            age_seconds: None,
        }),
    }

    Ok(())
}
