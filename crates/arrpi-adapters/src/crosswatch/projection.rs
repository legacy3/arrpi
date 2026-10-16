//! Minimal account-owned responses; native documents never pass through.

use arrpi_core::{
    error::ApiError,
    tracking::{Activity, Binding, Connection, DestinationStatus, Status, Watcher},
};
use serde_json::Value;

use super::{
    Backend,
    providers::DESTINATIONS,
    scope::{Scope, number},
};

impl Backend {
    pub(super) async fn project(
        &self,
        binding: &Binding,
        scope: &Scope,
    ) -> Result<Status, ApiError> {
        let plex = scope.provider("plex");
        let identity_matches = number(&plex["_cloud"]["account_id"]) == Some(binding.plex_id);
        let mut plex_state = connection(plex, "account_token", self.client.now());

        let server_matches =
            scope.server_id().is_some() && plex["machine_id"].as_str() == scope.server_id();

        if plex_state.linked && (!identity_matches || !server_matches) {
            plex_state.linked = false;
            plex_state.needs_reconnect = true;
            plex_state.username = None;
        }

        let trakt = scope
            .select("trakt")
            .map(|selected| {
                connection(
                    selected.provider("trakt"),
                    "access_token",
                    self.client.now(),
                )
            })
            .unwrap_or_default();
        let route = scope.route(binding);
        let managed_elsewhere = route.is_err();
        let route = route.ok().flatten();
        let enabled = route
            .and_then(|value| value["enabled"].as_bool())
            .unwrap_or(false);
        let running = route
            .and_then(|value| value.pointer("/runtime/running"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let activity = self
            .client
            .get(
                "api/activity/recent",
                &[("user_profile", &binding.profile_id), ("limit", "20")],
            )
            .await;
        let recent = activity
            .as_ref()
            .ok()
            .map(|value| recent(value, scope))
            .unwrap_or_default();

        Ok(Status {
            plex: plex_state,
            trakt,
            watcher: Watcher {
                enabled,
                running,
                managed_elsewhere,
            },
            activity_available: activity.is_ok(),
            plex_authorization_allowed: !binding.plex_server_owner && scope.plex != "default",
            watcher_ready: scope
                .overview
                .pointer("/source_state/watch_autostart")
                .and_then(Value::as_bool)
                == Some(true),
            recent,
            destinations: destinations(binding, scope, self.client.now()),
        })
    }
}

fn destinations(binding: &Binding, scope: &Scope, now: Option<u64>) -> Vec<DestinationStatus> {
    DESTINATIONS
        .iter()
        .filter_map(|provider| {
            if !scope.assigned(provider.id) {
                return None;
            }

            let Ok(selected) = scope.select(provider.id) else {
                return Some(DestinationStatus {
                    id: provider.id.to_owned(),
                    label: provider.label.to_owned(),
                    connection: Connection::default(),
                    watcher: Watcher {
                        enabled: false,
                        running: false,
                        managed_elsewhere: true,
                    },
                });
            };
            let block = selected.provider(provider.id);
            let mut state = connection(block, provider.credential, now);

            state.linked |= block[provider.credential] == true;

            if matches!(provider.id, "mdblist" | "flicklist") {
                state.linked |= text(block.get("api_key")).is_some();
            }

            if matches!(provider.id, "emby" | "jellyfin") {
                state.linked &=
                    text(block.get("server")).is_some() && text(block.get("user_id")).is_some();
            }

            if provider.id == "kodi" {
                state.linked &= text(block.get("server")).is_some();
            }

            if matches!(provider.id, "floppy" | "scrob") {
                state.linked &= text(block.get("server_url")).is_some();
            }

            state.needs_reconnect |= block["reauth_required"] == true;
            state.linked &= !state.needs_reconnect;
            let route = selected.route(binding);
            let managed_elsewhere = route.is_err();
            let route = route.ok().flatten();

            Some(DestinationStatus {
                id: provider.id.to_owned(),
                label: provider.label.to_owned(),
                connection: state,
                watcher: Watcher {
                    enabled: route.is_some_and(|row| row["enabled"] == true),
                    running: route.is_some_and(|row| {
                        row.pointer("/runtime/running") == Some(&Value::Bool(true))
                    }),
                    managed_elsewhere,
                },
            })
        })
        .collect()
}

fn connection(value: &Value, credential: &str, now: Option<u64>) -> Connection {
    let token_present = value
        .get(credential)
        .and_then(Value::as_str)
        .is_some_and(|token| !token.is_empty());
    let needs_reconnect =
        value.get("auth_error").and_then(Value::as_str) == Some("reconnect_required");

    Connection {
        authorizing: ["_pending_pin", "_pending_device"].iter().any(|field| {
            value.get(*field).is_some_and(|pending| {
                pending.is_object()
                    && number(&pending["expires_at"])
                        .zip(now)
                        .is_none_or(|(expiry, now)| expiry > now)
            })
        }),
        linked: token_present && !needs_reconnect,
        needs_reconnect,
        username: text(value.get("username")).or_else(|| text(value.pointer("/_cloud/username"))),
    }
}

fn recent(value: &Value, scope: &Scope) -> Vec<Activity> {
    value
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            if item["kind"] != "scrobble"
                || item["status"] != "ok"
                || item["source"] != "plex"
                || item["source_instance"] != scope.plex
                || !item["target"]
                    .as_str()
                    .and_then(|id| scope.select(id).ok())
                    .is_some_and(|selected| item["target_instance"] == selected.sink_instance)
            {
                return None;
            }

            let watched_at = item["watched_at"].as_i64().filter(|at| *at > 0)?;

            Some(Activity {
                title: text(item.get("title"))?,
                destination: text(item.get("target"))?,
                watched_at,
                media_type: match item["media_type"].as_str()? {
                    "movie" => "movie",
                    "episode" => "episode",
                    _ => return None,
                }
                .to_owned(),
                season: number(&item["season"]).and_then(|n| u32::try_from(n).ok()),
                episode: number(&item["episode"]).and_then(|n| u32::try_from(n).ok()),
            })
        })
        .take(5)
        .collect()
}

pub(super) fn text(value: Option<&Value>) -> Option<String> {
    let value = value?.as_str()?.trim();

    (!value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control))
        .then(|| value.to_owned())
}
