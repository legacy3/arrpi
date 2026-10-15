//! Precompiled Unicode-aware query and genre predicates.
use super::LibraryItem;
use crate::{arguments::MediaType, error::ApiError};
use schemars::JsonSchema;
use serde::Deserialize;

/// Stable sorting options for normalized installed media.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, JsonSchema, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    /// Unicode lowercase title, followed by source and native ID.
    #[default]
    Title,
    /// Community rating descending, with unrated items last.
    Rating,
    /// Release year descending, with unknown years last.
    Newest,
    /// Date added descending, with invalid dates last.
    RecentlyAdded,
}

/// Structured filters for installed media, independent of the Seerr catalog.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LibraryFilter {
    /// Optionally restrict the source media kind.
    #[serde(default)]
    pub media_type: Option<MediaType>,
    /// Case-insensitive title, original title or plot substring.
    #[serde(default)]
    pub query: Option<String>,
    /// Any-of native genre names, normalized once per query.
    #[serde(default)]
    #[schemars(length(max = 20))]
    pub genres: Vec<Box<str>>,
    /// Select installed files or missing files.
    #[serde(default)]
    pub available: Option<bool>,
    /// Inclusive release-year lower bound.
    #[serde(default)]
    pub year_from: Option<u32>,
    /// Inclusive release-year upper bound.
    #[serde(default)]
    pub year_to: Option<u32>,
    /// Minimum community rating, normalized to 0-10.
    #[serde(default)]
    #[schemars(range(min = 0, max = 10))]
    pub min_rating: Option<f64>,
    /// Stable sorting for the combined source snapshot.
    #[serde(default)]
    pub sort: Order,
    /// Starting offset without an arbitrary library-size cutoff.
    #[serde(default)]
    pub offset: usize,
    /// Maximum items to return in one page.
    #[serde(default = "limit")]
    #[schemars(range(min = 1, max = 500))]
    pub limit: usize,
    /// Require a fresh upstream snapshot in the adapter.
    #[serde(default)]
    pub no_cache: bool,
}

/// Prepared predicates reused for every record and source in a query.
#[derive(Debug)]
pub struct CompiledFilter {
    query: Option<String>,
    genres: Vec<Box<str>>,
    available: Option<bool>,
    year_from: Option<u32>,
    year_to: Option<u32>,
    min_rating: Option<f64>,
    sort: Order,
    capacity: usize,
    media_type: Option<MediaType>,
}

fn limit() -> usize {
    20
}

impl LibraryFilter {
    /// Validate untrusted query bounds before reading source snapshots.
    ///
    /// # Errors
    /// Returns invalid arguments for unsupported bounds or control characters.
    pub fn validate(&self) -> Result<(), ApiError> {
        let valid_year = |year: u32| (1800..=3000).contains(&year);

        if !(1..=500).contains(&self.limit)
            || self
                .query
                .as_ref()
                .is_some_and(|q| q.len() > 256 || q.chars().any(char::is_control))
            || self.genres.len() > 20
            || self.genres.iter().any(|g| !valid_text(g))
            || self.year_from.is_some_and(|y| !valid_year(y))
            || self.year_to.is_some_and(|y| !valid_year(y))
            || self.year_from.zip(self.year_to).is_some_and(|(a, b)| a > b)
            || self
                .min_rating
                .is_some_and(|r| !r.is_finite() || !(0.0..=10.0).contains(&r))
        {
            return Err(ApiError::BAD_QUERY);
        }

        Ok(())
    }

    /// Validate and normalize query terms once for every source.
    ///
    /// # Errors
    /// Returns invalid arguments using the same bounds as validation.
    pub fn compile(&self) -> Result<CompiledFilter, ApiError> {
        self.validate()?;

        Ok(CompiledFilter {
            query: self
                .query
                .as_ref()
                .map(|q| q.trim())
                .filter(|q| !q.is_empty())
                .map(str::to_lowercase),
            genres: self
                .genres
                .iter()
                .map(|genre| genre.trim().to_lowercase().into_boxed_str())
                .collect(),
            available: self.available,
            year_from: self.year_from,
            year_to: self.year_to,
            min_rating: self.min_rating,
            sort: self.sort,
            capacity: self.offset.saturating_add(self.limit),
            media_type: self.media_type,
        })
    }
}

impl CompiledFilter {
    pub(super) const fn order(&self) -> Order {
        self.sort
    }

    pub(super) const fn capacity(&self) -> usize {
        self.capacity
    }

    pub(super) fn matches(&self, item: &LibraryItem<'_>) -> bool {
        let text = self.query.as_ref().is_none_or(|query| {
            [
                Some(item.title.as_ref()),
                item.original_title.as_deref(),
                item.overview.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|text| text.to_lowercase().contains(query))
        });
        let genres = self.genres.is_empty()
            || item.genres.iter().any(|genre| {
                let genre = genre.to_lowercase();

                self.genres.iter().any(|term| term.as_ref() == genre)
            });

        text && genres
            && self.media_type.is_none_or(|kind| kind == item.media_type)
            && self
                .year_from
                .is_none_or(|year| item.year.is_some_and(|value| value >= u64::from(year)))
            && self
                .year_to
                .is_none_or(|year| item.year.is_some_and(|value| value <= u64::from(year)))
            && self
                .min_rating
                .is_none_or(|rating| item.rating.is_some_and(|value| value >= rating))
            && self
                .available
                .is_none_or(|available| item.available == available)
    }
}

fn valid_text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
