//! Existing library maintenance uses explicit native commands, never a generic write proxy.

mod command;
mod episodes;

use super::Native;
use arrpi_core::{
    arguments::MediaType,
    error::ApiError,
    maintenance::{Episodes, MaintenanceSource, ReleaseSearch},
    source::LibrarySource as _,
};
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    id: u32,
    monitored: bool,
    quality_profile_id: u64,
    #[serde(default)]
    has_file: bool,
}

impl Native {
    async fn item(&self, id: u32) -> Result<Item, ApiError> {
        let endpoint = if self.media_type == MediaType::Movie {
            "movie"
        } else {
            "series"
        };
        let bytes = self.fetch(&format!("{endpoint}/{id}"), "").await?;
        let item: Item = serde_json::from_slice(&bytes)?;

        if item.id != id {
            return Err(ApiError::INVALID_RESPONSE);
        }

        Ok(item)
    }

    async fn episode_list(&self, series: u32) -> Result<Vec<episodes::Episode>, ApiError> {
        if self.media_type != MediaType::Tv {
            return Err(ApiError::BAD_QUERY);
        }

        let bytes = self.fetch("episode", &format!("seriesId={series}")).await?;
        let _lease = self.normalizations.try_acquire()?;

        episodes::decode(&bytes, series)
    }

    async fn search_body(&self, args: &ReleaseSearch) -> Result<Option<Value>, ApiError> {
        if self.media_type == MediaType::Movie {
            if args.season.is_some() || !args.episode_ids.is_empty() {
                return Err(ApiError::BAD_QUERY);
            }

            return Ok(Some(
                json!({"name":"MoviesSearch","movieIds":[args.item_id]}),
            ));
        }

        let episodes = self.episode_list(args.item_id).await?;

        if args
            .episode_ids
            .iter()
            .any(|id| !episodes.iter().any(|episode| episode.id == *id))
            || args.season.is_some_and(|season| {
                !episodes
                    .iter()
                    .any(|episode| episode.season_number == season)
            })
        {
            return Err(ApiError::BAD_QUERY);
        }

        let now = Utc::now();
        let selected: Vec<_> = episodes
            .iter()
            .filter(|episode| {
                episode.monitored
                    && episode.aired(now) == Some(true)
                    && (!args.missing_only || !episode.has_file)
                    && args
                        .season
                        .is_none_or(|season| episode.season_number == season)
                    && (args.episode_ids.is_empty() || args.episode_ids.contains(&episode.id))
            })
            .map(|episode| episode.id)
            .collect();

        if selected.is_empty() {
            return Ok(None);
        }

        if selected.len() > self.max_search_episodes {
            return Err(ApiError::SEARCH_TOO_LARGE);
        }

        // Sonarr's bulk missing search groups seasons and skips queued episodes.
        // EpisodeSearch handles explicit selections one episode at a time.
        if args.missing_only && args.season.is_none() && args.episode_ids.is_empty() {
            return Ok(Some(
                json!({"name":"MissingEpisodeSearch","seriesId":args.item_id,"monitored":true}),
            ));
        }

        if selected.len() > self.max_episode_search {
            return Err(ApiError::SEARCH_TOO_LARGE);
        }

        Ok(Some(json!({"name":"EpisodeSearch","episodeIds":selected})))
    }
}

#[async_trait]
impl MaintenanceSource for Native {
    async fn refresh(&self, item_id: u32) -> Result<Value, ApiError> {
        self.maintenance_version().await?;
        self.item(item_id).await?;
        let body = if self.media_type == MediaType::Movie {
            json!({"name":"RefreshMovie","movieIds":[item_id]})
        } else {
            json!({"name":"RefreshSeries","seriesIds":[item_id]})
        };
        let mut receipt = self.submit_command(body).await?;

        receipt["item_id"] = json!(item_id);
        Ok(receipt)
    }

    async fn search(&self, args: ReleaseSearch) -> Result<Value, ApiError> {
        args.validate(100)?;

        if self.media_type == MediaType::Movie
            && (args.season.is_some() || !args.episode_ids.is_empty())
        {
            return Err(ApiError::BAD_QUERY);
        }

        self.maintenance_version().await?;
        let item = self.item(args.item_id).await?;
        let mut noop = json!({"source":self.descriptor.name,"item_id":args.item_id,"accepted":false,"status":"nothing_to_search","reason":"no_eligible_episodes"});

        if !item.monitored
            || (self.media_type == MediaType::Movie && args.missing_only && item.has_file)
        {
            noop["reason"] = json!(if item.monitored {
                "already_available"
            } else {
                "unmonitored"
            });

            return Ok(noop);
        }

        if !self
            .profiles()
            .await?
            .iter()
            .any(|profile| profile.id == item.quality_profile_id && profile.non_4k)
        {
            return Err(ApiError::UNSAFE_SEARCH_PROFILE);
        }

        let Some(body) = self.search_body(&args).await? else {
            return Ok(noop);
        };
        let mut receipt = self.submit_command(body).await?;

        receipt["item_id"] = json!(args.item_id);
        receipt["missing_only"] = json!(args.missing_only);
        receipt["requested_episode_ids"] = args.episode_ids.into();
        receipt["season"] = args.season.into();
        Ok(receipt)
    }

    async fn episodes(&self, args: Episodes) -> Result<Value, ApiError> {
        let episodes = self.episode_list(args.item_id).await?;

        episodes::page(episodes, &args, Utc::now())
    }

    async fn command(&self, command_id: u32) -> Result<Value, ApiError> {
        let bytes = self.fetch(&format!("command/{command_id}"), "").await?;
        let value: Value = serde_json::from_slice(&bytes)?;

        if value["id"] != command_id {
            return Err(ApiError::INVALID_RESPONSE);
        }

        command::project(&self.descriptor.name, &value)
    }
}
