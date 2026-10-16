//! Explicit PIN consent and owned-route mutations, never bulk history import.

use arrpi_core::{
    error::ApiError,
    tracking::{Authorization, Binding, Poll, PollState, Provider},
};
use reqwest::Method;
use serde_json::{Value, json};

use super::{
    Backend,
    scope::{Scope, number},
};

impl Backend {
    pub(super) async fn start_authorization(
        &self,
        binding: &Binding,
        provider: Provider,
    ) -> Result<Authorization, ApiError> {
        let base = self.scope(binding).await?;
        let name = provider_name(provider);
        let scope = if name == "plex" {
            base
        } else {
            base.select(name)?
        };

        if matches!(provider, Provider::Plex)
            && (binding.plex_server_owner || scope.plex == "default")
        {
            return Err(ApiError::FORBIDDEN);
        }

        if name == "plex" {
            self.pause_source(binding, &scope).await?;
        } else {
            self.pause_route(binding, &scope).await?;
        }

        let instance = if name == "plex" {
            scope.plex.as_str()
        } else {
            scope.sink_instance.as_str()
        };
        let method = if scope.provider("trakt")["auth_method"] == "app" {
            "app"
        } else {
            "pin"
        };
        let body = if name == "trakt" {
            json!({"auth_method":method})
        } else {
            json!({})
        };
        let result = self
            .client
            .call(
                Method::POST,
                &super::providers::pin_flow(name).map_or_else(
                    || format!("api/{name}/pin/new"),
                    |(flow, _)| format!("api/{name}/{flow}/start"),
                ),
                &[("instance", instance)],
                Some(&body),
            )
            .await?;

        authorization(&result, provider, self.client.now())
    }

    pub(super) async fn change_enabled(
        &self,
        binding: &Binding,
        enabled: bool,
    ) -> Result<(), ApiError> {
        let scope = self.scope(binding).await?;

        self.change_destination(binding, &scope, enabled).await
    }

    pub(super) async fn change_destination(
        &self,
        binding: &Binding,
        scope: &Scope,
        enabled: bool,
    ) -> Result<(), ApiError> {
        if !enabled {
            return self.pause_route(binding, scope).await;
        }

        let state = self.project(binding, scope).await?;

        if !state.plex.linked
            || !state.destinations.iter().any(|destination| {
                destination.id == scope.destination && destination.connection.linked
            })
        {
            return Err(ApiError::FORBIDDEN);
        }

        self.verify_plex(binding, scope).await?;
        let route = scope.route(binding)?;
        let body = route.map_or_else(|| route_body(binding, scope), |_| json!({"enabled":true}));

        if !state.watcher_ready {
            return Err(ApiError::CONFLICT);
        }

        let path = if route.is_some() {
            format!("api/scrobbler/routes/{}", scope.route_id(binding))
        } else {
            "api/scrobbler/routes".to_owned()
        };

        self.client
            .call(
                if route.is_some() {
                    Method::PUT
                } else {
                    Method::POST
                },
                &path,
                &[],
                Some(&body),
            )
            .await?;
        let saved = self.scope(binding).await?.select(scope.destination)?;

        if saved
            .route(binding)?
            .is_none_or(|route| route["enabled"] != true)
        {
            return Err(ApiError::UNKNOWN_OUTCOME);
        }

        Ok(())
    }

    pub(super) async fn disconnect_trakt(&self, binding: &Binding) -> Result<(), ApiError> {
        let scope = self.scope(binding).await?;

        self.disconnect_destination(binding, &scope).await
    }

    pub(super) async fn disconnect_destination(
        &self,
        binding: &Binding,
        scope: &Scope,
    ) -> Result<(), ApiError> {
        if scope.destination != "trakt" && super::providers::pin_flow(scope.destination).is_none() {
            return Err(ApiError::CONFLICT);
        }

        self.pause_route(binding, scope).await?;

        if scope.route(binding)?.is_some() {
            self.client
                .call(
                    Method::DELETE,
                    &format!("api/scrobbler/routes/{}", scope.route_id(binding)),
                    &[],
                    None,
                )
                .await?;
        }

        self.client
            .call(
                Method::POST,
                &format!(
                    "api/{}/{}",
                    scope.destination,
                    super::providers::destination(scope.destination)
                        .ok_or(ApiError::NOT_FOUND)?
                        .disconnect
                ),
                &[("instance", &scope.sink_instance)],
                Some(&json!({})),
            )
            .await?;
        Ok(())
    }

    pub(super) async fn poll_destination(&self, scope: &Scope) -> Result<Poll, ApiError> {
        let (flow, _) = super::providers::pin_flow(scope.destination).ok_or(ApiError::NOT_FOUND)?;
        let pending = scope.provider(scope.destination).get(if flow == "pin" {
            "_pending_pin"
        } else {
            "_pending_device"
        });

        if !pending.is_some_and(Value::is_object) {
            return Ok(Poll {
                state: PollState::Expired,
            });
        }

        let result = self
            .client
            .call(
                Method::POST,
                &format!("api/{}/{flow}/poll", scope.destination),
                &[("instance", &scope.sink_instance)],
                Some(&json!({})),
            )
            .await?;
        let status = result["status"]
            .as_str()
            .or_else(|| result["error"].as_str())
            .unwrap_or("");
        let state = match status {
            "authorized" | "connected" | "ok" => PollState::Authorized,

            "pending" | "authorization_pending" | "slow_down" => PollState::Pending,

            "expired" | "expired_token" | "expired_flow" | "not_started" | "no_pending" => {
                PollState::Expired
            }

            "access_denied" | "denied" | "authorization_declined" => PollState::Denied,

            _ => return Err(ApiError::REJECTED),
        };

        Ok(Poll { state })
    }

    async fn pause_source(&self, binding: &Binding, scope: &Scope) -> Result<(), ApiError> {
        let scopes: Vec<_> = super::providers::DESTINATIONS
            .iter()
            .filter_map(|provider| scope.select(provider.id).ok())
            .collect();

        // Validate the entire source before pausing any sibling route.
        for selected in &scopes {
            selected.route(binding)?;
        }

        for selected in &scopes {
            self.pause_route(binding, selected).await?;
        }

        Ok(())
    }

    async fn verify_plex(&self, binding: &Binding, scope: &Scope) -> Result<(), ApiError> {
        if scope.server_id().is_none()
            || scope.provider("plex")["machine_id"].as_str() != scope.server_id()
        {
            return Err(ApiError::FORBIDDEN);
        }

        let users = self
            .client
            .get("api/plex/pickusers", &[("instance", &scope.plex)])
            .await?;
        let self_rows: Vec<_> = users["users"]
            .as_array()
            .ok_or(ApiError::INVALID_RESPONSE)?
            .iter()
            .filter(|row| row["type"] == "self")
            .collect();
        let [own] = self_rows.as_slice() else {
            return Err(ApiError::FORBIDDEN);
        };

        if number(&own["cloud_account_id"]) != Some(binding.plex_id)
            || scope.filter_id(binding) == 0
        {
            return Err(ApiError::FORBIDDEN);
        }

        if binding.plex_server_owner
            && number(&own["pms_account_id"]) != Some(scope.filter_id(binding))
        {
            return Err(ApiError::FORBIDDEN);
        }

        Ok(())
    }

    async fn pause_route(&self, binding: &Binding, scope: &Scope) -> Result<(), ApiError> {
        if let Some(route) = scope.route(binding)?
            && route["enabled"] == true
        {
            self.client
                .call(
                    Method::PUT,
                    &format!("api/scrobbler/routes/{}", scope.route_id(binding)),
                    &[],
                    Some(&json!({"enabled":false})),
                )
                .await?;
        }

        Ok(())
    }
}

fn route_body(binding: &Binding, scope: &Scope) -> Value {
    json!({"id":scope.route_id(binding),"profile_id":binding.profile_id,"enabled":true,"provider":"plex","provider_instance":scope.plex,"sink":scope.destination,"sink_instance":scope.sink_instance,
        "filters":{"username_whitelist":[format!("id:{}", scope.filter_id(binding))],"server_uuid":scope.server_id()},
        "options":{"auto_remove_watchlist":"off","ratings":{"mode":"off","targets":[]},"watch":{"unresolved_user_fallback":false}}})
}

fn authorization(
    result: &Value,
    provider: Provider,
    now: Option<u64>,
) -> Result<Authorization, ApiError> {
    let (code, url) = match provider {
        Provider::Plex => (result.get("code"), concat!("https://", "plex.tv", "/link")),

        Provider::Trakt => (
            result.get("user_code"),
            concat!("https://", "auth.trakt.tv", "/activate"),
        ),

        _ => (
            result.get("user_code"),
            super::providers::pin_flow(provider_name(provider))
                .ok_or(ApiError::NOT_FOUND)?
                .1,
        ),
    };
    let code = code
        .and_then(Value::as_str)
        .filter(|code| {
            !code.is_empty()
                && code.len() <= 32
                && code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        .ok_or(ApiError::INVALID_RESPONSE)?;
    let expires_in = number(&result["expiresIn"])
        .or_else(|| number(&result["expires_in"]))
        .or_else(|| number(&result["expires_at"])?.checked_sub(now?))
        .filter(|seconds| (1..=3600).contains(seconds))
        .ok_or(ApiError::INVALID_RESPONSE)?;

    Ok(Authorization {
        code: code.to_owned(),
        verification_url: url.to_owned(),
        expires_in,
    })
}

fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Plex => "plex",
        Provider::Trakt => "trakt",
        Provider::Simkl => "simkl",
        Provider::Mdblist => "mdblist",
        Provider::Punchplay => "punchplay",
        Provider::Bingebase => "bingebase",
        Provider::Flicklist => "flicklist",
    }
}
