//! Read-only library aggregation uses provider capabilities and selected-page serialization.

use crate::{
    arguments::MediaType,
    error::ApiError,
    library::{self, LibraryFilter},
    registry::ToolId,
    source::{Principal, ReadMode, Sources},
};
use serde_json::{Value, json};

pub(super) async fn load(
    sources: &Sources,
    principal: &Principal,
    tool: ToolId,
    args: LibraryFilter,
    normalizations: &crate::admission::Normalizations,
) -> Result<Value, ApiError> {
    if args.media_type.is_some_and(|kind| !selected(tool, kind)) {
        return Err(ApiError::BAD_QUERY);
    }

    let filter = args.compile()?;
    let mut snapshots = Vec::with_capacity(sources.len());
    let mut statuses = Vec::with_capacity(sources.len());

    for (name, source) in sources {
        let Some(library) = source.library() else {
            continue;
        };
        let kind = library.media_type();

        if !principal.allows(name, "library-view")
            || !selected(tool, kind)
            || args.media_type.is_some_and(|requested| requested != kind)
        {
            continue;
        }

        match library
            .snapshot(if args.no_cache {
                ReadMode::Fresh
            } else {
                ReadMode::Cached
            })
            .await
        {
            Ok(read) => snapshots.push((name, kind, read)),

            Err(error) => {
                statuses.push(json!({"source":name,"status":"unavailable","code":error.code()}));
            }
        }
    }

    let (rating_reads, mut rating_statuses) = external_reads(
        sources,
        principal,
        if args.no_cache {
            ReadMode::Fresh
        } else {
            ReadMode::Cached
        },
    )
    .await;

    let _lease = normalizations.try_acquire()?;
    let ratings = build_index(&rating_reads, &mut rating_statuses, args.no_cache);

    let mut selections = Vec::with_capacity(snapshots.len());

    for (name, kind, read) in &snapshots {
        match library::select_with_ratings(name, *kind, read.data.as_ref(), &filter, &ratings) {
            Ok(selection) => {
                selections.push(selection);
                statuses.push(json!({"source":name,"status":"ok","cache":read.freshness.cache,"age_seconds":read.freshness.age_seconds.unwrap_or(0)}));
            }

            Err(error) => {
                statuses.push(json!({"source":name,"status":"unavailable","code":error.code()}));
            }
        }
    }

    statuses.sort_by(|left, right| left["source"].as_str().cmp(&right["source"].as_str()));
    let ratings_complete = rating_statuses
        .iter()
        .all(|status| status["status"] == "ok");
    let complete = !statuses.is_empty()
        && statuses.iter().all(|status| status["status"] == "ok")
        && ratings_complete;
    let mut output = serde_json::to_value(library::paginate(selections, &args)?)?;

    output["complete"] = json!(complete);
    output["sources"] = json!(statuses);
    output["rating_sources"] = json!(rating_statuses);
    output["ratings_complete"] = json!(ratings_complete);
    Ok(output)
}

fn selected(tool: ToolId, kind: MediaType) -> bool {
    match tool {
        ToolId::ListMovies => kind == MediaType::Movie,
        ToolId::ListSeries => kind == MediaType::Tv,
        _ => true,
    }
}

async fn external_reads<'a>(
    sources: &'a Sources,
    principal: &Principal,
    mode: ReadMode,
) -> (
    Vec<(&'a str, crate::source::SourceRead<crate::source::Body>)>,
    Vec<Value>,
) {
    let mut reads = Vec::new();
    let mut statuses = Vec::new();

    for (name, source) in sources {
        if !principal.allows(name, "ratings-view") {
            continue;
        }

        let Some(provider) = source.ratings() else {
            continue;
        };

        match provider.snapshot(mode).await {
            Ok(read) => reads.push((name, read)),

            Err(error) => {
                statuses.push(json!({"source":name,"status":"unavailable","code":error.code()}));
            }
        }
    }

    (reads, statuses)
}

fn build_index(
    rating_reads: &[(&str, crate::source::SourceRead<crate::source::Body>)],
    rating_statuses: &mut Vec<Value>,
    bypass: bool,
) -> library::RatingIndex {
    let mut ratings = library::RatingIndex::default();

    for (name, read) in rating_reads {
        match ratings.extend(read.data.as_ref()) {
            Ok(()) => rating_statuses.push(
                json!({"source":name,"status":"ok","freshness":read.freshness.public(bypass)}),
            ),

            Err(error) => rating_statuses
                .push(json!({"source":name,"status":"unavailable","code":error.code()})),
        }
    }

    ratings
}
