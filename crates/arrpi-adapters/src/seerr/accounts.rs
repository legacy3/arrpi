//! Operator-only account lookup supplies the verified binding pair, never an impersonation parameter.
use super::Backend;
use arrpi_core::{
    contracts::{LinkedAccount, LinkedAccountPage},
    error::ApiError,
    source::Principal,
};
use reqwest::Method;
use serde_json::Value;

impl Backend {
    pub(super) async fn operator_accounts(
        &self,
        principal: &Principal,
        offset: usize,
        limit: usize,
    ) -> Result<LinkedAccountPage, ApiError> {
        if !principal.allows("config", "manage") {
            return Err(ApiError::FORBIDDEN);
        }

        if !(1..=100).contains(&limit) || offset > 1_000_000 {
            return Err(ApiError::BAD_QUERY);
        }

        let _permit = self.slot(&self.admission).await?;
        let response = self
            .prepare(
                None,
                &Method::GET,
                "user",
                &[("take", limit.to_string()), ("skip", offset.to_string())],
                None,
            )?
            .send()
            .await
            .map_err(|error| crate::http::transport_error(&error))?;
        let value = self
            .decode(
                response,
                &Method::GET,
                "user",
                crate::http::Intent::Read,
                None,
            )
            .await?;
        let rows = value
            .data
            .get("results")
            .and_then(Value::as_array)
            .ok_or(ApiError::INVALID_RESPONSE)?;

        if rows.len() > limit {
            return Err(ApiError::INVALID_RESPONSE);
        }

        let items = rows
            .iter()
            .filter_map(|row| {
                let id = row.get("id")?.as_u64().filter(|id| *id > 0)?;
                let plex_id = row.get("plexId")?.as_u64().filter(|id| *id > 0)?;
                let name = ["plexUsername", "username", "displayName"]
                    .into_iter()
                    .find_map(|key| {
                        row.get(key)
                            .and_then(Value::as_str)
                            .filter(|value| !value.trim().is_empty() && !value.contains('@'))
                    })
                    .map_or_else(|| format!("Plex account {plex_id}"), str::to_owned);

                if name.len() > 256 || name.chars().any(char::is_control) {
                    return None;
                }

                Some(LinkedAccount { id, plex_id, name })
            })
            .collect();
        let next = offset + rows.len();
        let total = value
            .data
            .pointer("/pageInfo/results")
            .and_then(Value::as_u64);
        let more = !rows.is_empty()
            && total.map_or(rows.len() == limit, |count| {
                u64::try_from(next).unwrap_or(u64::MAX) < count
            });

        Ok(LinkedAccountPage {
            items,
            offset,
            next_offset: more.then_some(next),
        })
    }
}
