//! Fixed native command endpoint, bounded pre-dispatch admission and safe receipts.

use super::super::Native;
use crate::http::{self, Intent};
use arrpi_core::{arguments::MediaType, error::ApiError};
use reqwest::header::{self, HeaderValue};
use secrecy::ExposeSecret as _;
use serde_json::{Value, json};

impl Native {
    pub(super) async fn maintenance_version(&self) -> Result<(), ApiError> {
        let bytes = self.fetch("system/status", "").await?;
        let status: Value = serde_json::from_slice(&bytes)?;
        let version = status["version"]
            .as_str()
            .ok_or(ApiError::UNSUPPORTED_MAINTENANCE_VERSION)?;
        let mut parts = version.split('.');
        let major = parts.next().and_then(|value| value.parse::<u32>().ok());
        let mut count = 0;
        let invalid = parts.any(|part| {
            count += 1;
            part.parse::<u32>().is_err()
        });

        if invalid
            || !(2..=3).contains(&count)
            || !matches!(
                (self.media_type, major),
                (MediaType::Tv, Some(4)) | (MediaType::Movie, Some(3..=6))
            )
        {
            return Err(ApiError::UNSUPPORTED_MAINTENANCE_VERSION);
        }

        Ok(())
    }

    pub(super) async fn submit_command(&self, body: Value) -> Result<Value, ApiError> {
        let expected = body["name"].as_str().ok_or(ApiError::INTERNAL)?;
        let _slot = tokio::time::timeout(self.queue_timeout, self.writes.acquire())
            .await
            .map_err(|error| http::redacted_error(&error, ApiError::QUEUE_TIMEOUT))?
            .map_err(|error| http::redacted_error(&error, ApiError::UNAVAILABLE))?;
        let _upstream = tokio::time::timeout(self.queue_timeout, self.upstream.acquire())
            .await
            .map_err(|error| http::redacted_error(&error, ApiError::QUEUE_TIMEOUT))?
            .map_err(|error| http::redacted_error(&error, ApiError::UNAVAILABLE))?;
        let limit = self.limit.min(128 * 1024);
        let reserved = self.budget.reserve(limit).await?;
        let url = self
            .base
            .join("api/v3/command")
            .map_err(|error| http::redacted_error(&error, ApiError::BAD_QUERY))?;
        let mut key = HeaderValue::from_str(self.key.expose_secret())
            .map_err(|error| http::redacted_error(&error, ApiError::REJECTED))?;

        key.set_sensitive(true);
        let response = self
            .http
            .client()
            .post(url)
            .header("X-Api-Key", key)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json")
            .body(serde_json::to_vec(&body)?)
            .send()
            .await
            .map_err(|error| Intent::Write.failure(http::transport_error(&error)))?;
        let status = response.status();

        if status.is_client_error() {
            return Err(ApiError::upstream_read(status.as_u16()));
        }

        if !status.is_success() {
            return Err(ApiError::UNKNOWN_OUTCOME);
        }

        let bytes = http::collect(response, limit, Intent::Write, Some(reserved)).await?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|error| http::redacted_error(&error, ApiError::UNKNOWN_OUTCOME))?;

        if !value["name"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(expected))
        {
            return Err(ApiError::UNKNOWN_OUTCOME);
        }

        // Verify the scope decoded by the native command type, not just its name.
        for key in [
            "seriesId",
            "seriesIds",
            "movieIds",
            "episodeIds",
            "seasonNumber",
            "monitored",
        ] {
            if let Some(scope) = body.get(key)
                && value["body"].get(key) != Some(scope)
            {
                return Err(ApiError::UNKNOWN_OUTCOME);
            }
        }

        let mut receipt = project(&self.descriptor.name, &value)
            .map_err(|error| http::redacted_error(&error, ApiError::UNKNOWN_OUTCOME))?;

        receipt["accepted"] = json!(true);

        if let Some(ids) = body.get("episodeIds") {
            receipt["episode_ids"] = ids.clone();
        }

        Ok(receipt)
    }
}

pub(super) fn project(source: &str, value: &Value) -> Result<Value, ApiError> {
    let id = value["id"]
        .as_u64()
        .filter(|id| *id > 0 && i32::try_from(*id).is_ok())
        .ok_or(ApiError::INVALID_RESPONSE)?;
    let name = value["name"].as_str().ok_or(ApiError::INVALID_RESPONSE)?;
    let status = value["status"].as_str().ok_or(ApiError::INVALID_RESPONSE)?;

    if ![
        "RefreshMovie",
        "RefreshSeries",
        "MoviesSearch",
        "SeriesSearch",
        "SeasonSearch",
        "EpisodeSearch",
        "MissingEpisodeSearch",
    ]
    .iter()
    .any(|known| name.eq_ignore_ascii_case(known))
    {
        return Err(ApiError::NOT_FOUND);
    }

    if ![
        "queued",
        "started",
        "completed",
        "failed",
        "aborted",
        "cancelled",
        "orphaned",
    ]
    .contains(&status)
    {
        return Err(ApiError::INVALID_RESPONSE);
    }

    Ok(
        json!({"source":source,"command_id":id,"command":name,"status":status,
        "finished":!matches!(status,"queued"|"started"),"successful":match status {"completed"=>Some(true),"queued"|"started"=>None,_=>Some(false)}}),
    )
}
