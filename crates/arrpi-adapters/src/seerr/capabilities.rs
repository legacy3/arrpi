//! Capability implementations keep native paths and decoding inside this adapter.
use super::{Backend, links, model};
use arrpi_core::{
    arguments as args,
    error::ApiError,
    policy::Binding,
    source::{
        AccountSource, Actor, ApiSource, BrokerProfile, BrokerProfiles, CatalogSource, Descriptor,
        Health, IssueSource, Principal, ProfileSelection, ReadMode, RequestSource,
    },
};
use async_trait::async_trait;
use reqwest::Method;
use serde_json::{Value, json};

impl Backend {
    pub(super) async fn verify_selection(
        &self,
        binding: &Binding,
        kind: args::MediaType,
        choice: &ProfileSelection,
    ) -> Result<(), ApiError> {
        let profiles = AccountSource::service_profiles(self, binding, kind)
            .await?
            .ok_or(ApiError::PROFILE_UNAVAILABLE)?;

        if profiles.server_id != choice.server_id
            || profiles
                .profiles
                .iter()
                .filter(|profile| profile.id == choice.profile_id && profile.name == choice.name)
                .count()
                != 1
        {
            return Err(ApiError::PROFILE_UNAVAILABLE);
        }

        Ok(())
    }
}

#[async_trait]
impl ApiSource for Backend {
    fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    async fn connection_health(
        &self,
        principal: &Principal,
        _mode: ReadMode,
    ) -> Result<Health, ApiError> {
        if !principal.allows("config", "manage") {
            return Err(ApiError::FORBIDDEN);
        }

        let _permit = self.slot(&self.admission).await?;
        let response = self
            .prepare(None, &Method::GET, "auth/me", &[], None)?
            .send()
            .await
            .map_err(|error| crate::http::transport_error(&error))?;
        let value = self
            .decode(
                response,
                &Method::GET,
                "auth/me",
                crate::http::Intent::Read,
                None,
            )
            .await?;

        if value
            .data
            .get("id")
            .and_then(Value::as_u64)
            .is_none_or(|id| id == 0)
        {
            return Err(ApiError::INVALID_RESPONSE);
        }

        Ok(Health {
            reachable: Some(true),
            version: None,
            health_issue_count: None,
            writes_supported: None,
            needs_account: false,
            freshness: Some(value.freshness),
        })
    }

    async fn health(&self, principal: &Principal, _mode: ReadMode) -> Result<Health, ApiError> {
        let Some(binding) = &principal.binding else {
            return Ok(Health {
                reachable: None,
                version: None,
                health_issue_count: None,
                writes_supported: None,
                needs_account: true,
                freshness: None,
            });
        };
        let value = self
            .call(
                binding,
                Method::GET,
                "status",
                &[("checkUpdateAvailable", "false".to_owned())],
                None,
            )
            .await?;

        Ok(Health {
            reachable: Some(true),
            version: value["version"].as_str().map(str::to_owned),
            health_issue_count: None,
            writes_supported: Some(super::version::supports_writes(value["version"].as_str())),
            needs_account: false,
            freshness: Some(value.freshness),
        })
    }

    fn catalog(&self) -> Option<&dyn CatalogSource> {
        Some(self)
    }

    fn accounts(&self) -> Option<&dyn AccountSource> {
        Some(self)
    }

    fn requests(&self) -> Option<&dyn RequestSource> {
        Some(self)
    }

    fn issues(&self) -> Option<&dyn IssueSource> {
        Some(self)
    }
}

#[async_trait]
impl CatalogSource for Backend {
    async fn search(&self, binding: &Binding, args: args::Search) -> Result<Value, ApiError> {
        if args.query.trim().is_empty()
            || args.query.len() > 256
            || args.query.chars().any(char::is_control)
            || !(1..=100).contains(&args.page)
        {
            return Err(ApiError::BAD_QUERY);
        }

        let value = self
            .call(
                binding,
                Method::GET,
                "search",
                &[("query", args.query), ("page", args.page.to_string())],
                None,
            )
            .await?;
        let rows = value
            .get("results")
            .and_then(Value::as_array)
            .filter(|rows| rows.len() <= 50)
            .ok_or(ApiError::INVALID_RESPONSE)?;
        let items = rows
            .iter()
            .filter(|row| matches!(row["mediaType"].as_str(), Some("movie" | "tv")))
            .map(model::media)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(
            json!({"items":items,"page":value.get("page"),"total_pages":value.get("totalPages"),"freshness":value.freshness.public(args.no_cache)}),
        )
    }

    async fn details(
        &self,
        binding: &Binding,
        args: args::Details,
        settings: &Value,
    ) -> Result<Value, ApiError> {
        let value = Backend::metadata(self, binding, args.media_type.name(), args.tmdb_id).await?;

        let mut output = links::media(&value, settings, args.no_cache)?;

        self.community_ratings(binding, &value, &mut output, args.no_cache)
            .await?;
        Ok(output)
    }

    async fn public_settings(&self, binding: &Binding) -> Value {
        Backend::settings(self, binding).await
    }
}

#[async_trait]
impl AccountSource for Backend {
    async fn linked_accounts(
        &self,
        principal: &Principal,
        offset: usize,
        limit: usize,
    ) -> Result<arrpi_core::contracts::LinkedAccountPage, ApiError> {
        self.operator_accounts(principal, offset, limit).await
    }

    async fn identity(&self, binding: &Binding) -> Result<Actor, ApiError> {
        Backend::verified_actor(self, binding).await
    }

    async fn compatible(&self, binding: &Binding) -> Result<bool, ApiError> {
        Backend::supports_writes(self, binding).await
    }

    async fn quota(&self, binding: &Binding, bypass: bool) -> Result<Value, ApiError> {
        let value = self
            .call(
                binding,
                Method::GET,
                &format!("user/{}/quota", binding.id),
                &[],
                None,
            )
            .await?;
        let mut output = json!({"freshness":value.freshness.public(bypass)});

        for kind in ["movie", "tv"] {
            let quota = &value[kind];
            let limit = quota["limit"].as_u64().ok_or(ApiError::INVALID_RESPONSE)?;

            output[kind] = json!({"limit":limit,"unlimited":limit==0,"used":quota["used"].as_u64(),"remaining":quota["remaining"].as_u64(),"days":quota["days"].as_u64(),"restricted":quota["restricted"].as_bool(),"unit":if kind=="movie" {"movies"} else {"seasons"}});
        }

        Ok(output)
    }

    async fn service_profiles(
        &self,
        binding: &Binding,
        kind: args::MediaType,
    ) -> Result<Option<BrokerProfiles>, ApiError> {
        let service = if kind == args::MediaType::Tv {
            "sonarr"
        } else {
            "radarr"
        };
        let servers = self
            .call(
                binding,
                Method::GET,
                &format!("service/{service}"),
                &[],
                None,
            )
            .await?;
        let Some(server_id) = default_server(&servers)? else {
            return Ok(None);
        };
        let value = self
            .call(
                binding,
                Method::GET,
                &format!("service/{service}/{server_id}"),
                &[],
                None,
            )
            .await?;
        let server = value.get("server").ok_or(ApiError::INVALID_RESPONSE)?;

        if server["id"] != server_id || server["is4k"] != false || server["isDefault"] != true {
            return Err(ApiError::INVALID_RESPONSE);
        }

        let rows = value
            .get("profiles")
            .and_then(Value::as_array)
            .filter(|rows| rows.len() <= 200)
            .ok_or(ApiError::INVALID_RESPONSE)?;
        let profiles = rows.iter().map(profile).collect::<Result<Vec<_>, _>>()?;

        Ok(Some(BrokerProfiles {
            server_id,
            default_profile_id: server["activeProfileId"].as_u64().filter(|id| *id > 0),
            anime_profile_id: if kind == args::MediaType::Tv {
                server["activeAnimeProfileId"].as_u64().filter(|id| *id > 0)
            } else {
                None
            },
            profiles,
            freshness: value.freshness,
        }))
    }
}

fn default_server(value: &Value) -> Result<Option<u64>, ApiError> {
    let rows = value
        .as_array()
        .filter(|rows| rows.len() <= 200)
        .ok_or(ApiError::INVALID_RESPONSE)?;
    let mut candidates = rows
        .iter()
        .filter(|row| row["is4k"] == false && row["isDefault"] == true);
    let result = candidates.next();

    if candidates.next().is_some() {
        return Err(ApiError::INVALID_RESPONSE);
    }

    result
        .map(|row| {
            row["id"]
                .as_u64()
                .filter(|id| u32::try_from(*id).is_ok())
                .ok_or(ApiError::INVALID_RESPONSE)
        })
        .transpose()
}

fn profile(row: &Value) -> Result<BrokerProfile, ApiError> {
    let id = model::id(row, "id")?;
    let name = row["name"]
        .as_str()
        .ok_or(ApiError::INVALID_RESPONSE)?
        .to_owned();

    Ok(BrokerProfile { id, name })
}

#[async_trait]
impl RequestSource for Backend {
    async fn submit(
        &self,
        binding: &Binding,
        item: args::MediaRequest,
        selection: Option<ProfileSelection>,
    ) -> Result<Value, ApiError> {
        Backend::submit_request(self, binding, item, selection).await
    }

    async fn cancel(&self, binding: &Binding, request_id: u64) -> Result<Value, ApiError> {
        Backend::cancel_request(self, binding, request_id).await
    }

    async fn requests(&self, binding: &Binding, args: args::Page) -> Result<Value, ApiError> {
        Backend::request_page(self, binding, args).await
    }

    async fn request(&self, binding: &Binding, request_id: u64) -> Result<Value, ApiError> {
        Backend::request_receipt(self, binding, request_id).await
    }
}

#[async_trait]
impl IssueSource for Backend {
    async fn issues(&self, binding: &Binding, args: args::Page) -> Result<Value, ApiError> {
        Backend::issue_page(self, binding, args).await
    }

    async fn report(&self, binding: &Binding, args: args::Issue) -> Result<Value, ApiError> {
        Backend::report_issue(self, binding, args).await
    }

    async fn close(&self, binding: &Binding, issue_id: u64) -> Result<Value, ApiError> {
        Backend::close_issue(self, binding, issue_id).await
    }
}
