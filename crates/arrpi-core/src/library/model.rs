//! Typed public library metadata and precomputed stable sort keys.
use super::Order;
use crate::arguments::MediaType;
use crate::error::ApiError;
use serde::Serialize;
use serde_json::value::RawValue;
use std::{
    borrow::Cow,
    cmp::{Ordering, Reverse},
};

/// Recommendation metadata without native paths or administrative objects.
#[derive(Debug, Serialize)]
pub struct LibraryItem<'a> {
    pub source: &'a str,
    pub id: u64,
    pub media_type: MediaType,
    pub title: Cow<'a, str>,
    pub original_title: Option<Cow<'a, str>>,
    pub year: Option<u64>,
    pub tmdb_id: Option<u64>,
    pub tvdb_id: Option<u64>,
    pub imdb_id: Option<Cow<'a, str>>,
    pub overview: Option<Cow<'a, str>>,
    pub genres: Vec<Cow<'a, str>>,
    pub rating: Option<f64>,
    pub rating_provider: Option<&'static str>,
    pub ratings: Vec<Rating>,
    pub runtime_minutes: Option<u64>,
    pub certification: Option<Cow<'a, str>>,
    pub original_language: Option<Cow<'a, str>>,
    pub status: Option<Cow<'a, str>>,
    pub studio: Option<Cow<'a, str>>,
    pub network: Option<Cow<'a, str>>,
    pub added_at: Option<Cow<'a, str>>,
    pub release_date: Option<Cow<'a, str>>,
    pub digital_release_date: Option<Cow<'a, str>>,
    pub physical_release_date: Option<Cow<'a, str>>,
    pub monitored: Option<bool>,
    pub available: bool,
    pub availability: Availability,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<FileMetadata<'a>>,
}

/// File properties reported by the library snapshot, without paths or release names.
#[derive(Debug, Serialize)]
pub struct FileMetadata<'a> {
    pub quality: Option<FileQuality<'a>>,
    pub audio_languages: Option<Cow<'a, str>>,
    pub subtitle_languages: Option<Cow<'a, str>>,
    pub video_codec: Option<Cow<'a, str>>,
}

/// Native quality classification, distinct from the configured upgrade profile.
#[derive(Debug, Serialize)]
pub struct FileQuality<'a> {
    pub name: Option<Cow<'a, str>>,
    pub resolution: Option<u64>,
    pub source: Option<Cow<'a, str>>,
}

/// A native rating with explicit score normalization and vote provenance.
#[derive(Debug, Serialize)]
pub struct Rating {
    pub provider: &'static str,
    pub score: f64,
    pub raw_value: f64,
    pub raw_scale: f64,
    pub votes: Option<u64>,
}

/// Movie presence or Sonarr's monitored-aired episode coverage.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Availability {
    Movie {
        status: &'static str,
    },
    Series {
        status: &'static str,
        coverage_scope: &'static str,
        episode_files: Option<u64>,
        monitored_episodes: Option<u64>,
        total_episodes: Option<u64>,
        seasons: Option<u64>,
    },
}

#[derive(Debug)]
pub(super) struct Candidate<'a> {
    source: &'a str,
    kind: MediaType,
    id: u64,
    raw: &'a RawValue,
    pub(super) source_index: usize,
    index: usize,
    key: SortKey,
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SortKey {
    Title(String),
    Number(Reverse<Option<u64>>),
    Added(Reverse<Option<chrono::DateTime<chrono::FixedOffset>>>),
}

impl<'a> Candidate<'a> {
    pub(super) fn new(
        item: &LibraryItem<'a>,
        order: Order,
        raw: &'a RawValue,
        index: usize,
    ) -> Self {
        let key = match order {
            Order::Title => SortKey::Title(item.title.to_lowercase()),

            Order::Rating => SortKey::Number(Reverse(item.rating.map(f64::to_bits))),

            Order::Newest => SortKey::Number(Reverse(item.year)),

            Order::RecentlyAdded => SortKey::Added(Reverse(
                item.added_at
                    .as_ref()
                    .and_then(|date| chrono::DateTime::parse_from_rfc3339(date).ok()),
            )),
        };

        Self {
            source: item.source,
            kind: item.media_type,
            id: item.id,
            raw,
            source_index: 0,
            index,
            key,
        }
    }

    pub(super) fn project(self) -> Result<LibraryItem<'a>, ApiError> {
        super::native::Native::decode(self.raw)?.project(self.source, self.kind)
    }
}

impl Ord for Candidate<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key
            .cmp(&other.key)
            .then_with(|| self.source.cmp(other.source))
            .then_with(|| self.id.cmp(&other.id))
            .then_with(|| self.source_index.cmp(&other.source_index))
            .then_with(|| self.index.cmp(&other.index))
    }
}

impl PartialOrd for Candidate<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Candidate<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for Candidate<'_> {}
