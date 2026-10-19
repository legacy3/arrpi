//! Community scores retain their provider, reporting service and original scale.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Rating providers are distinct from the service reporting their values.
#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
pub enum Provider {
    #[serde(rename = "imdb")]
    Imdb,
    #[serde(rename = "tmdb")]
    Tmdb,
    #[serde(rename = "trakt")]
    Trakt,
    #[serde(rename = "sonarr")]
    Sonarr,
    #[serde(rename = "metacritic")]
    Metacritic,
    #[serde(rename = "rottenTomatoes")]
    RottenTomatoes,
    #[serde(rename = "rottenTomatoesAudience")]
    RottenTomatoesAudience,
    #[serde(rename = "plexCritic")]
    PlexCritic,
    #[serde(rename = "plexAudience")]
    PlexAudience,
}

/// An upstream service's community metadata, never its owner's personal score.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Radarr,
    Sonarr,
    Seerr,
    Plex,
}

/// One published score; all normalized scores use 0-10.
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Rating {
    pub provider: Provider,
    pub origin: Origin,
    pub score: f64,
    pub raw_value: f64,
    pub raw_scale: f64,
    pub votes: Option<u64>,
}

impl Rating {
    /// Reject invalid numbers rather than turning missing scores into zero.
    #[must_use]
    pub fn new(
        provider: Provider,
        origin: Origin,
        value: f64,
        scale: f64,
        votes: Option<u64>,
    ) -> Option<Self> {
        if !value.is_finite()
            || !scale.is_finite()
            || scale <= 0.0
            || !(0.0..=scale).contains(&value)
        {
            return None;
        }

        Some(Self {
            provider,
            origin,
            score: value * 10.0 / scale,
            raw_value: value,
            raw_scale: scale,
            votes,
        })
    }
}

/// Prefer community ratings for the convenience score; critic scores stay separate.
#[must_use]
pub fn community(ratings: &[Rating]) -> Option<&Rating> {
    [
        Provider::Imdb,
        Provider::Tmdb,
        Provider::Trakt,
        Provider::Sonarr,
    ]
    .into_iter()
    .find_map(|provider| ratings.iter().find(|rating| rating.provider == provider))
}
