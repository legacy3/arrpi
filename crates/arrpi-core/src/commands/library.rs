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

    let _lease = normalizations.try_acquire()?;

    let mut selections = Vec::with_capacity(snapshots.len());

    for (name, kind, read) in &snapshots {
        match library::select(name, *kind, read.data.as_ref(), &filter) {
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
    let complete = !statuses.is_empty() && statuses.iter().all(|status| status["status"] == "ok");
    let mut output = serde_json::to_value(library::paginate(selections, &args)?)?;

    output["complete"] = json!(complete);
    output["sources"] = json!(statuses);
    Ok(output)
}

fn selected(tool: ToolId, kind: MediaType) -> bool {
    match tool {
        ToolId::ListMovies => kind == MediaType::Movie,
        ToolId::ListSeries => kind == MediaType::Tv,
        _ => true,
    }
}
