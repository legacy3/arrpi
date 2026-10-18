//! Episode projection and deterministic selection never expose native file paths.

use arrpi_core::{error::ApiError, maintenance::Episodes};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all(deserialize = "camelCase", serialize = "snake_case"))]
pub(super) struct Episode {
    pub(super) id: u32,
    pub(super) series_id: u32,
    pub(super) season_number: u32,
    #[serde(rename(deserialize = "episodeNumber", serialize = "episode_number"))]
    pub(super) number: u32,
    pub(super) title: Option<String>,
    pub(super) monitored: bool,
    pub(super) has_file: bool,
    pub(super) air_date_utc: Option<DateTime<Utc>>,
    pub(super) absolute_episode_number: Option<u32>,
}

impl Episode {
    pub(super) fn aired(&self, now: DateTime<Utc>) -> Option<bool> {
        self.air_date_utc.map(|date| date <= now)
    }
}

pub(super) fn decode(bytes: &[u8], series: u32) -> Result<Vec<Episode>, ApiError> {
    let mut episodes: Vec<Episode> = serde_json::from_slice(bytes)?;
    let mut seen = std::collections::BTreeSet::new();

    if episodes.iter().any(|episode| {
        episode.series_id != series
            || episode.id == 0
            || i32::try_from(episode.id).is_err()
            || !seen.insert(episode.id)
    }) {
        return Err(ApiError::INVALID_RESPONSE);
    }

    episodes.sort_by_key(|episode| (episode.season_number, episode.number, episode.id));
    Ok(episodes)
}

pub(super) fn page(
    episodes: Vec<Episode>,
    args: &Episodes,
    now: DateTime<Utc>,
) -> Result<Value, ApiError> {
    let end = args
        .offset
        .checked_add(args.limit)
        .ok_or(ApiError::BAD_QUERY)?;
    let filtered = episodes.into_iter().filter(|episode| {
        args.season
            .is_none_or(|season| episode.season_number == season)
            && (!args.missing_only || !episode.has_file)
            && (!args.aired_only || episode.aired(now) == Some(true))
    });
    let mut total = 0usize;
    let mut items = Vec::with_capacity(args.limit);

    for episode in filtered {
        if (args.offset..end).contains(&total) {
            let aired = episode.aired(now);
            let mut item = serde_json::to_value(episode)?;

            item["aired"] = json!(aired);
            items.push(item);
        }

        total += 1;
    }

    Ok(
        json!({"source":args.source,"item_id":args.item_id,"total":total,"offset":args.offset,"limit":args.limit,
            "next_offset":(end < total).then_some(end),"items":items,"freshness":{"cache":"not_cached","bypass_requested":true}}),
    )
}
