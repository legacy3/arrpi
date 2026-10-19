//! External community metadata joins by database identity, never display title.
use super::LibraryItem;
use crate::{arguments::MediaType, error::ApiError, ratings::Rating};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Minimal metadata retained by external rating adapters; no private playback state.
#[derive(Debug, Deserialize, Serialize)]
pub struct RatingRecord {
    pub media_type: MediaType,
    pub tmdb_id: Option<u64>,
    pub tvdb_id: Option<u64>,
    pub imdb_id: Option<String>,
    pub ratings: Vec<Rating>,
}

/// An index shared by every candidate and its final selected-page projection.
#[derive(Debug, Default)]
pub struct RatingIndex {
    tmdb: BTreeMap<(MediaType, u64), Vec<Rating>>,
    tvdb: BTreeMap<u64, Vec<Rating>>,
    imdb_movies: BTreeMap<String, Vec<Rating>>,
    imdb_series: BTreeMap<String, Vec<Rating>>,
}

impl RatingIndex {
    /// Construct an empty metadata index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode a bounded source snapshot before any record is indexed.
    ///
    /// # Errors
    /// Returns malformed external metadata rather than a false complete result.
    pub fn extend(&mut self, body: &[u8]) -> Result<(), ApiError> {
        let records: Vec<RatingRecord> = serde_json::from_slice(body)?;

        for record in records {
            if let Some(id) = record.tmdb_id.filter(|id| *id > 0) {
                merge(
                    self.tmdb.entry((record.media_type, id)).or_default(),
                    &record.ratings,
                );
            }

            if record.media_type == MediaType::Tv
                && let Some(id) = record.tvdb_id.filter(|id| *id > 0)
            {
                merge(self.tvdb.entry(id).or_default(), &record.ratings);
            }

            if let Some(id) = record.imdb_id {
                let imdb = if record.media_type == MediaType::Movie {
                    &mut self.imdb_movies
                } else {
                    &mut self.imdb_series
                };

                merge(imdb.entry(id).or_default(), &record.ratings);
            }
        }

        Ok(())
    }

    pub(super) fn for_item(&self, item: &LibraryItem<'_>) -> &[Rating] {
        item.tmdb_id
            .and_then(|id| self.tmdb.get(&(item.media_type, id)))
            .or_else(|| item.tvdb_id.and_then(|id| self.tvdb.get(&id)))
            .or_else(|| {
                item.imdb_id.as_deref().and_then(|id| {
                    if item.media_type == MediaType::Movie {
                        self.imdb_movies.get(id)
                    } else {
                        self.imdb_series.get(id)
                    }
                })
            })
            .map_or(&[], Vec::as_slice)
    }
}

fn merge(target: &mut Vec<Rating>, ratings: &[Rating]) {
    for rating in ratings {
        if !target.iter().any(|existing| {
            existing.provider == rating.provider && existing.origin == rating.origin
        }) {
            target.push(*rating);
        }
    }
}
