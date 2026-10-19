//! Borrowed native projections and paginated recommendation metadata.
mod filter;
mod model;
mod native;
mod ratings;
mod selection;

use crate::{arguments::MediaType, error::ApiError};
use serde::Serialize;

#[doc(inline)]
pub use crate::ratings::Rating;
pub use filter::{CompiledFilter, LibraryFilter, Order};
pub use model::{Availability, FileMetadata, FileQuality, LibraryItem};
pub use ratings::{RatingIndex, RatingRecord};

/// A validated source selection whose text borrows its native snapshot.
#[derive(Debug)]
pub struct Selection<'a> {
    rows: Vec<model::Candidate<'a>>,
    total: usize,
    capacity: usize,
    order: Order,
}

/// One page across all successfully selected source snapshots.
#[derive(Debug, Serialize)]
pub struct LibraryPage<'a> {
    /// Public metadata for the requested page only.
    pub items: Vec<LibraryItem<'a>>,
    /// Requested starting offset, including offsets beyond the last item.
    pub offset: usize,
    /// Matching items across all supplied selections.
    pub total: usize,
    /// Whether another page exists in this snapshot.
    pub has_more: bool,
    /// Starting offset for the next page, or null at the end.
    pub next_offset: Option<usize>,
    /// Native rating normalization used by the community score.
    pub rating_scale: &'static str,
}

/// Validate every record and select matching minimal rows from one source.
///
/// # Errors
/// Returns malformed JSON or invalid required native identifiers/titles.
pub fn select<'a>(
    source: &'a str,
    kind: MediaType,
    body: &'a [u8],
    filter: &CompiledFilter,
) -> Result<Selection<'a>, ApiError> {
    selection::decode(source, kind, body, filter, None)
}

/// Select after joining authorized community ratings, so filters and sorting see them.
///
/// # Errors
/// Returns the same validation failures as the native library selector.
pub fn select_with_ratings<'a>(
    source: &'a str,
    kind: MediaType,
    body: &'a [u8],
    filter: &CompiledFilter,
    ratings: &'a RatingIndex,
) -> Result<Selection<'a>, ApiError> {
    selection::decode(source, kind, body, filter, Some(ratings))
}

/// Sort a combined selection once and retain only the requested page.
///
/// Every selection must use the predicates compiled from this same filter.
/// A smaller page is permitted; changing predicates requires new selections.
///
/// # Errors
/// Returns invalid metadata if a selected native record cannot be projected.
pub fn paginate<'a>(
    selections: Vec<Selection<'a>>,
    filter: &LibraryFilter,
) -> Result<LibraryPage<'a>, ApiError> {
    filter.validate()?;

    if selections.iter().any(|selection| {
        selection.capacity < filter.offset.saturating_add(filter.limit)
            || selection.order != filter.sort
    }) {
        return Err(ApiError::BAD_QUERY);
    }

    let total = selections.iter().map(|selection| selection.total).sum();
    let capacity = selections
        .iter()
        .map(|selection| selection.rows.len())
        .sum();
    let mut rows = Vec::with_capacity(capacity);

    for (source_index, selection) in selections.into_iter().enumerate() {
        rows.extend(selection.rows.into_iter().map(|mut candidate| {
            candidate.source_index = source_index;
            candidate
        }));
    }

    rows.sort_unstable();
    let mut items = Vec::with_capacity(rows.len().saturating_sub(filter.offset).min(filter.limit));

    for candidate in rows.into_iter().skip(filter.offset).take(filter.limit) {
        items.push(candidate.project()?);
    }

    let next = filter.offset.saturating_add(items.len());

    Ok(LibraryPage {
        items,
        offset: filter.offset,
        total,
        has_more: next < total,
        next_offset: (next < total).then_some(next),
        rating_scale: "0-10; IMDb, TMDB, Trakt or Sonarr-reported community rating",
    })
}
