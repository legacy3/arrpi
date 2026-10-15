//! Fresh request preflight and safe receipts; Seerr owns approval, quotas and tags.
use super::{Backend, model};
use arrpi_core::{arguments as args, error::ApiError, policy::Binding};
use reqwest::Method;
use serde_json::{Value, json};
impl Backend {
    pub(crate) async fn metadata(
        &self,
        binding: &Binding,
        kind: &str,
        id: u64,
    ) -> Result<arrpi_core::source::SourceRead<Value>, ApiError> {
        args::positive(id)?;
        self.call(binding, Method::GET, &format!("{kind}/{id}"), &[], None)
            .await
    }

    pub(crate) async fn request_page(
        &self,
        binding: &Binding,
        page: args::Page,
    ) -> Result<Value, ApiError> {
        page.validate()?;

        let value = self
            .call(
                binding,
                Method::GET,
                "request",
                &[
                    ("requestedBy", binding.id.to_string()),
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
        let receipts = rows
            .iter()
            .map(|r| model::receipt(r, binding.id))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(
            json!({"items":receipts,"page":value.get("pageInfo"),"freshness":model::freshness(&value,page.no_cache)}),
        )
    }

    pub(crate) async fn request_receipt(
        &self,
        binding: &Binding,
        id: u64,
    ) -> Result<Value, ApiError> {
        args::positive(id)?;
        let value = self
            .call(binding, Method::GET, &format!("request/{id}"), &[], None)
            .await?;

        let mut result = model::receipt(&value, binding.id)?;

        result["freshness"] = value.freshness.public(false);
        Ok(result)
    }

    pub(crate) async fn submit_request(
        &self,
        binding: &Binding,
        item: args::MediaRequest,
        selection: Option<arrpi_core::source::ProfileSelection>,
    ) -> Result<Value, ApiError> {
        let kind = item.media_type.name();
        let id = item.tmdb_id;
        let seasons = item.seasons;

        args::positive(id)?;
        args::validate_seasons(item.media_type, &seasons)?;

        let metadata = self.metadata(binding, kind, id).await?;
        let pending = remaining(&metadata, kind, &seasons)?;

        if pending.is_empty() {
            let mut result = json!({"status":noop_status(&metadata,kind,&seasons),"tmdb_id":id,"seasons":seasons,"accepted_seasons":[],"skipped_seasons":seasons,"profile_applied":false});

            model::with_media(&mut result, &metadata);

            return Ok(result);
        }

        let quota = self
            .call(
                binding,
                Method::GET,
                &format!("user/{}/quota", binding.id),
                &[],
                None,
            )
            .await?;
        let q = &quota[if kind == "movie" { "movie" } else { "tv" }];

        if exceeds_quota(q, pending.len()) {
            return Err(ApiError::QUOTA_EXCEEDED);
        }

        let _gate = self.write_slot().await?;
        let actor = self.write_identity(binding).await?;

        authorize_request(&actor, kind, selection.is_some())?;

        if let Some(choice) = &selection {
            self.verify_selection(binding, item.media_type, choice)
                .await?;
        }

        let mut body = request_body(kind, id, &pending);

        if let Some(choice) = &selection {
            body["serverId"] = json!(choice.server_id);
            body["profileId"] = json!(choice.profile_id);
        }

        let receipt = self
            .call(binding, Method::POST, "request", &[], Some(body))
            .await?;

        if let Some(status) = receipt.get("_arrpi_noop").and_then(Value::as_str) {
            let mut result = json!({"status":status,"tmdb_id":id,"accepted_seasons":[],"skipped_seasons":seasons,"profile_applied":false});

            model::with_media(&mut result, &metadata);

            return Ok(result);
        }

        let mut result = model::receipt(&receipt, binding.id).map_err(|error| {
            tracing::debug!(
                event = "command_failure",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::UNKNOWN_OUTCOME
        })?;

        if receipt.pointer("/media/tmdbId").and_then(Value::as_u64) != Some(id)
            || receipt.get("is4k").and_then(Value::as_bool) != Some(false)
        {
            return Err(ApiError::UNKNOWN_OUTCOME);
        }

        validate_selection(&receipt, selection.as_ref())?;
        let accepted = accepted_seasons(&result, kind)?;

        if accepted.iter().any(|n| !pending.contains(n)) {
            return Err(ApiError::UNKNOWN_OUTCOME);
        }

        if let Some(choice) = selection {
            result["profile"] = json!(choice.name);
            result["profile_applied"] = json!(true);
        }

        model::with_media(&mut result, &metadata);
        result["accepted_seasons"] = json!(accepted);
        result["skipped_seasons"] = json!(
            seasons
                .into_iter()
                .filter(|n| !accepted.contains(n))
                .collect::<Vec<_>>()
        );
        Ok(result)
    }

    pub(crate) async fn cancel_request(
        &self,
        binding: &Binding,
        id: u64,
    ) -> Result<Value, ApiError> {
        args::positive(id)?;

        let actor = self.verified_actor(binding).await?;

        if !actor
            .rights
            .contains(super::ActorRights::CANCEL_OWN_PENDING)
        {
            return Err(ApiError::FORBIDDEN);
        }

        let value = self.request_receipt(binding, id).await?;

        if value["status"] != "pending" || value["is_4k"] != false {
            return Err(ApiError::REQUEST_NOT_PENDING);
        }

        let _gate = self.write_slot().await?;
        let actor = self.write_identity(binding).await?;

        if !actor
            .rights
            .contains(super::ActorRights::CANCEL_OWN_PENDING)
        {
            return Err(ApiError::FORBIDDEN);
        }

        let value = self.request_receipt(binding, id).await?;

        if value["status"] != "pending" || value["is_4k"] != false {
            return Err(ApiError::REQUEST_NOT_PENDING);
        }

        self.call(binding, Method::DELETE, &format!("request/{id}"), &[], None)
            .await?;
        Ok(
            json!({"request_id":id,"status":"cancelled","tmdb_id":value["tmdb_id"],"media_type":value["media_type"]}),
        )
    }
}

fn remaining(metadata: &Value, kind: &str, seasons: &[u32]) -> Result<Vec<u32>, ApiError> {
    let state = metadata
        .pointer("/mediaInfo/status")
        .and_then(Value::as_u64)
        .unwrap_or(1);

    if state == 6 {
        return Err(ApiError::FORBIDDEN);
    }

    if kind == "movie" {
        return Ok(if matches!(state, 2 | 3 | 5) {
            Vec::new()
        } else {
            vec![1]
        });
    }

    let valid = metadata
        .get("seasons")
        .and_then(Value::as_array)
        .ok_or(ApiError::INVALID_RESPONSE)?;

    if seasons.iter().any(|n| {
        !valid
            .iter()
            .any(|s| s.get("seasonNumber").and_then(Value::as_u64) == Some(u64::from(*n)))
    }) {
        return Err(ApiError::BAD_QUERY);
    }

    let states = metadata
        .pointer("/mediaInfo/seasons")
        .and_then(Value::as_array);

    Ok(seasons
        .iter()
        .copied()
        .filter(|n| {
            !states.is_some_and(|all| {
                all.iter().any(|s| {
                    s.get("seasonNumber").and_then(Value::as_u64) == Some(u64::from(*n))
                        && s.get("status")
                            .and_then(Value::as_u64)
                            .is_some_and(|n| matches!(n, 2..=6))
                })
            })
        })
        .collect())
}

fn exceeds_quota(value: &Value, amount: usize) -> bool {
    value.get("restricted").and_then(Value::as_bool) == Some(true)
        || value
            .get("remaining")
            .and_then(Value::as_u64)
            .is_some_and(|n| n < u64::try_from(amount).unwrap_or(u64::MAX))
}

fn request_body(kind: &str, id: u64, seasons: &[u32]) -> Value {
    let mut body = json!({"mediaType":kind,"mediaId":id,"is4k":false});

    if kind == "tv" {
        body["seasons"] = json!(seasons);
    }

    body
}

fn authorize_request(actor: &super::Actor, kind: &str, explicit: bool) -> Result<(), ApiError> {
    let permission = if kind == "movie" {
        super::ActorRights::REQUEST_MOVIES
    } else {
        super::ActorRights::REQUEST_SERIES
    };

    if !actor.rights.contains(permission)
        || (explicit && !actor.rights.contains(super::ActorRights::SELECT_PROFILE))
    {
        return Err(ApiError::FORBIDDEN);
    }

    Ok(())
}

fn validate_selection(
    receipt: &Value,
    selection: Option<&arrpi_core::source::ProfileSelection>,
) -> Result<(), ApiError> {
    if let Some(choice) = selection
        && (receipt.get("serverId").and_then(Value::as_u64) != Some(choice.server_id)
            || receipt.get("profileId").and_then(Value::as_u64) != Some(choice.profile_id))
    {
        return Err(ApiError::profile_mismatch(model::id(receipt, "id")?));
    }

    Ok(())
}

fn accepted_seasons(result: &Value, kind: &str) -> Result<Vec<u32>, ApiError> {
    if kind != "tv" {
        return Ok(Vec::new());
    }

    result["seasons"]
        .as_array()
        .ok_or(ApiError::UNKNOWN_OUTCOME)?
        .iter()
        .map(|s| {
            s.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or(ApiError::UNKNOWN_OUTCOME)
        })
        .collect()
}

fn noop_status(metadata: &Value, kind: &str, seasons: &[u32]) -> &'static str {
    let available = if kind == "movie" {
        metadata.pointer("/mediaInfo/status") == Some(&json!(5))
    } else {
        metadata
            .pointer("/mediaInfo/seasons")
            .and_then(Value::as_array)
            .is_some_and(|all| {
                seasons.iter().all(|season| {
                    all.iter()
                        .any(|s| s["seasonNumber"] == *season && s["status"] == 5)
                })
            })
    };

    if available {
        "already_available"
    } else {
        "already_requested"
    }
}
