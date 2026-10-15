//! Actor-owned issue summaries and safe Seerr internal-media-ID resolution.
use super::{Backend, model};
use arrpi_core::{arguments as args, error::ApiError, policy::Binding};
use reqwest::Method;
use serde_json::{Value, json};
impl Backend {
    pub(crate) async fn issue_page(
        &self,
        binding: &Binding,
        page: args::Page,
    ) -> Result<Value, ApiError> {
        page.validate()?;

        let value = self
            .call(
                binding,
                Method::GET,
                "issue",
                &[
                    ("take", page.limit.to_string()),
                    ("skip", page.offset.to_string()),
                ],
                None,
            )
            .await?;
        let rows = value
            .get("results")
            .and_then(Value::as_array)
            .filter(|a| a.len() <= page.limit)
            .ok_or(ApiError::INVALID_RESPONSE)?;
        let items = rows
            .iter()
            .filter(|row| row.pointer("/createdBy/id").and_then(Value::as_u64) == Some(binding.id))
            .map(|r| issue(r, binding.id))
            .collect::<Result<Vec<_>, _>>()?;

        let next = page.offset.saturating_add(rows.len());
        let total = value
            .pointer("/pageInfo/results")
            .and_then(Value::as_u64)
            .ok_or(ApiError::INVALID_RESPONSE)?;
        let has_more = !rows.is_empty() && u64::try_from(next).unwrap_or(u64::MAX) < total;

        Ok(
            json!({"items":items,"offset":page.offset,"scanned":rows.len(),"has_more":has_more,"next_offset":has_more.then_some(next),"freshness":model::freshness(&value,page.no_cache)}),
        )
    }

    pub(crate) async fn close_issue(&self, binding: &Binding, id: u64) -> Result<Value, ApiError> {
        args::positive(id)?;
        let value = self
            .call(binding, Method::GET, &format!("issue/{id}"), &[], None)
            .await?;
        let result = issue(&value, binding.id)?;

        if value["status"] == 2 {
            return Ok(result);
        }

        if !self
            .verified_actor(binding)
            .await?
            .rights
            .contains(super::ActorRights::REPORT_ISSUES)
        {
            return Err(ApiError::FORBIDDEN);
        }

        let _permit = self.write_slot().await?;

        self.authorize_issue(binding).await?;
        let value = self
            .call(
                binding,
                Method::POST,
                &format!("issue/{id}/resolved"),
                &[],
                None,
            )
            .await?;
        let result = issue(&value, binding.id).map_err(|error| {
            tracing::debug!(event="issue_readback",category=%error);
            ApiError::UNKNOWN_OUTCOME
        })?;

        if result["issue_id"] != id || result["status"] != 2 {
            return Err(ApiError::UNKNOWN_OUTCOME);
        }

        Ok(result)
    }

    pub(crate) async fn report_issue(
        &self,
        binding: &Binding,
        args: args::Issue,
    ) -> Result<Value, ApiError> {
        if args.message.trim().is_empty()
            || args.message.len() > 2000
            || args.message.chars().any(|c| c.is_control() && c != '\n')
            || args.episode.is_some() && args.season.is_none()
            || args.episode == Some(0)
        {
            return Err(ApiError::BAD_QUERY);
        }

        let metadata = self
            .metadata(binding, args.media_type.name(), args.tmdb_id)
            .await?;
        let media = metadata
            .get("mediaInfo")
            .filter(|v| v.is_object())
            .ok_or(ApiError::MEDIA_NOT_TRACKED)?;
        let id = model::id(media, "id")?;

        if let Some(season) = args.season {
            if args.media_type.name() != "tv" || !(1..=1000).contains(&season) {
                return Err(ApiError::BAD_QUERY);
            }

            let detail = self
                .call(
                    binding,
                    Method::GET,
                    &format!("tv/{}/season/{season}", args.tmdb_id),
                    &[],
                    None,
                )
                .await?;

            if args.episode.is_some_and(|episode| {
                !detail
                    .get("episodes")
                    .and_then(Value::as_array)
                    .is_some_and(|episodes| {
                        episodes.iter().any(|e| {
                            e.get("episodeNumber").and_then(Value::as_u64)
                                == Some(u64::from(episode))
                        })
                    })
            }) {
                return Err(ApiError::BAD_QUERY);
            }
        }

        let mut body =
            json!({"mediaId":id,"issueType":args.category.number(),"message":args.message});

        if let Some(season) = args.season {
            body["problemSeason"] = json!(season);
        }

        if let Some(episode) = args.episode {
            body["problemEpisode"] = json!(episode);
        }

        if !self
            .verified_actor(binding)
            .await?
            .rights
            .contains(super::ActorRights::REPORT_ISSUES)
        {
            return Err(ApiError::FORBIDDEN);
        }

        let _gate = self.write_slot().await?;

        self.authorize_issue(binding).await?;
        let value = self
            .call(binding, Method::POST, "issue", &[], Some(body))
            .await?;

        issue(&value, binding.id).map_err(|error| {
            tracing::debug!(
                event = "command_failure",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::UNKNOWN_OUTCOME
        })
    }
}
fn issue(value: &Value, actor: u64) -> Result<Value, ApiError> {
    if value.pointer("/createdBy/id").and_then(Value::as_u64) != Some(actor) {
        return Err(ApiError::FORBIDDEN);
    }

    Ok(
        json!({"issue_id":model::id(value,"id")?,"status":value.get("status"),"status_name":if value["status"]==2 {"resolved"} else {"open"},"category":value.get("issueType"),"tmdb_id":value.pointer("/media/tmdbId"),"media_type":value.pointer("/media/mediaType"),"season":value.get("problemSeason"),"episode":value.get("problemEpisode")}),
    )
}
