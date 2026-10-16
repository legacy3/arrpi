//! Resolve immutable account scope before reading or changing native resources.

use arrpi_core::{error::ApiError, tracking::Binding};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use super::{
    Backend,
    providers::{DESTINATIONS, destination},
};

#[derive(Deserialize)]
struct Profiles {
    items: Vec<Profile>,
}

#[derive(Deserialize)]
struct Profile {
    id: String,
    instances: BTreeMap<String, Vec<String>>,
}

#[derive(Clone)]
pub(super) struct Scope {
    pub(super) plex: String,
    pub(super) sink_instance: String,
    pub(super) config: Arc<Value>,
    pub(super) overview: Arc<Value>,
    pub(super) destination: &'static str,
    instances: Arc<BTreeMap<String, Vec<String>>>,
    shared: Arc<BTreeSet<String>>,
}

impl Backend {
    pub(super) async fn scope(&self, binding: &Binding) -> Result<Scope, ApiError> {
        let value = self.client.get("api/user-profiles", &[]).await?;
        let profiles: Profiles = serde_json::from_value(value)
            .map_err(|error| crate::http::redacted_error(&error, ApiError::INVALID_RESPONSE))?;
        let profile = profiles
            .items
            .iter()
            .find(|profile| profile.id == binding.profile_id)
            .ok_or(ApiError::FORBIDDEN)?;
        let plex = one(profile, "PLEX")?;
        let trakt = one(profile, "TRAKT").unwrap_or("");

        if profiles
            .items
            .iter()
            .any(|other| other.id != binding.profile_id && owns(other, "PLEX", plex))
        {
            return Err(ApiError::FORBIDDEN);
        }

        let shared = profile
            .instances
            .iter()
            .filter(|(provider, instances)| {
                profiles.items.iter().any(|other| {
                    other.id != binding.profile_id
                        && instances
                            .iter()
                            .any(|instance| owns(other, provider, instance))
                })
            })
            .map(|(provider, _)| provider.to_ascii_lowercase())
            .collect();

        let config = self.client.get("api/config", &[]).await?;
        let overview = self.client.get("api/scrobbler/overview", &[]).await?;

        Ok(Scope {
            plex: plex.to_owned(),
            sink_instance: trakt.to_owned(),
            config: Arc::new(config),
            overview: Arc::new(overview),
            destination: "trakt",
            instances: Arc::new(profile.instances.clone()),
            shared: Arc::new(shared),
        })
    }
}

impl Scope {
    pub(super) fn select(&self, id: &str) -> Result<Self, ApiError> {
        let provider = destination(id).ok_or(ApiError::NOT_FOUND)?;

        if self.shared.contains(id) {
            return Err(ApiError::CONFLICT);
        }

        let instances = self
            .instances
            .get(&id.to_ascii_uppercase())
            .ok_or(ApiError::FORBIDDEN)?;
        let [instance] = instances.as_slice() else {
            return Err(ApiError::CONFLICT);
        };

        if !arrpi_core::tracking::safe_id(instance) {
            return Err(ApiError::FORBIDDEN);
        }

        let mut scope = self.clone();

        scope.destination = provider.id;
        scope.sink_instance.clone_from(instance);
        Ok(scope)
    }

    pub(super) fn route_id(&self, binding: &Binding) -> String {
        let base = route_id(binding);

        if self.destination == "trakt" {
            base
        } else {
            format!("{base}-{}", self.destination)
        }
    }

    pub(super) fn assigned(&self, provider: &str) -> bool {
        self.instances.contains_key(&provider.to_ascii_uppercase())
    }

    pub(super) fn provider(&self, provider: &str) -> &Value {
        let instance = if provider == "plex" {
            &self.plex
        } else {
            &self.sink_instance
        };

        if instance == "default" {
            &self.config[provider]
        } else {
            &self.config[provider]["instances"][instance]
        }
    }

    pub(super) fn route<'a>(&'a self, binding: &Binding) -> Result<Option<&'a Value>, ApiError> {
        if self.sink_instance.is_empty() || self.shared.contains(self.destination) {
            return Err(ApiError::CONFLICT);
        }

        let rows = self
            .overview
            .get("routes")
            .and_then(Value::as_array)
            .ok_or(ApiError::INVALID_RESPONSE)?;

        if self.config["pairs"].as_array().is_some_and(|pairs| {
            pairs.iter().any(|pair| {
                self.touches(pair, "source", "source_instance")
                    || self.touches(pair, "target", "target_instance")
            })
        }) {
            return Err(ApiError::CONFLICT);
        }

        let own_id = self.route_id(binding);
        let mut selected = None;

        for route in rows {
            if route.get("id").and_then(Value::as_str) == Some(own_id.as_str()) {
                if !self.matches(route, binding) || selected.is_some() {
                    return Err(ApiError::CONFLICT);
                }

                selected = Some(route);
            } else if self.touches(route, "provider", "provider_instance")
                || self.touches(route, "sink", "sink_instance")
            {
                let owned_sibling = DESTINATIONS.iter().any(|provider| {
                    self.select(provider.id).is_ok_and(|scope| {
                        route["id"] == scope.route_id(binding) && scope.matches(route, binding)
                    })
                });

                if !owned_sibling {
                    return Err(ApiError::CONFLICT);
                }
            }
        }

        Ok(selected)
    }

    pub(super) fn matches(&self, route: &Value, binding: &Binding) -> bool {
        route["profile_id"] == binding.profile_id
            && route["provider"] == "plex"
            && route["sink"] == self.destination
            && route["provider_instance"] == self.plex
            && route["sink_instance"] == self.sink_instance
            && route.pointer("/filters/username_whitelist")
                == Some(&serde_json::json!([format!(
                    "id:{}",
                    self.filter_id(binding)
                )]))
            && route.pointer("/filters/server_uuid") == Some(&self.config["plex"]["machine_id"])
            && self.server_id().is_some()
            && route.pointer("/options/ratings/targets") == Some(&serde_json::json!([]))
            && route
                .pointer("/options/auto_remove_watchlist")
                .and_then(Value::as_str)
                == Some("off")
            && route
                .pointer("/options/ratings/mode")
                .and_then(Value::as_str)
                == Some("off")
            && route
                .pointer("/options/watch/unresolved_user_fallback")
                .and_then(Value::as_bool)
                == Some(false)
    }

    pub(super) fn server_id(&self) -> Option<&str> {
        self.config["plex"]["machine_id"]
            .as_str()
            .filter(|id| !id.is_empty())
    }

    pub(super) fn filter_id(&self, binding: &Binding) -> u64 {
        if binding.plex_server_owner {
            number(&self.provider("plex")["account_id"]).unwrap_or(0)
        } else {
            binding.plex_id
        }
    }

    fn touches(&self, row: &Value, provider_key: &str, instance_key: &str) -> bool {
        let provider = row[provider_key].as_str().unwrap_or("");
        let instance = row[instance_key].as_str().unwrap_or("default");

        (provider.eq_ignore_ascii_case("plex") && instance == self.plex)
            || (provider.eq_ignore_ascii_case(self.destination) && instance == self.sink_instance)
    }
}

pub(super) fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn one<'a>(profile: &'a Profile, provider: &str) -> Result<&'a str, ApiError> {
    let items = profile.instances.get(provider).ok_or(ApiError::FORBIDDEN)?;

    match items.as_slice() {
        [instance] if arrpi_core::tracking::safe_id(instance) => Ok(instance),
        _ => Err(ApiError::FORBIDDEN),
    }
}

fn owns(profile: &Profile, provider: &str, instance: &str) -> bool {
    profile
        .instances
        .get(provider)
        .is_some_and(|items| items.iter().any(|item| item == instance))
}

pub(super) fn route_id(binding: &Binding) -> String {
    use base64::Engine as _;
    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(format!("{}:{}", binding.source, binding.profile_id));

    format!(
        "arrpi-{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&digest[..18])
    )
}
