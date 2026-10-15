//! Named choices combine broker metadata with independently validated library profiles.

use super::Commands;
use crate::{
    arguments as args,
    error::ApiError,
    policy::Binding,
    source::{ActorRights, ApiSource, BrokerProfiles, ProfileSelection},
};
use serde_json::{Value, json};

pub(super) async fn list(
    commands: &Commands,
    broker: &dyn ApiSource,
    binding: &Binding,
    args: args::Profiles,
) -> Result<Value, ApiError> {
    let accounts = broker.accounts().ok_or(ApiError::FORBIDDEN)?;
    let actor = accounts.identity(binding).await?;
    let permission = if args.media_type == args::MediaType::Movie {
        ActorRights::REQUEST_MOVIES
    } else {
        ActorRights::REQUEST_SERIES
    };

    if !actor.rights.contains(permission) {
        return Err(ApiError::FORBIDDEN);
    }

    let profiles = accounts.service_profiles(binding, args.media_type).await?;
    let default = profiles
        .as_ref()
        .and_then(|profiles| configured_profile(profiles, profiles.default_profile_id));
    let anime = profiles
        .as_ref()
        .and_then(|profiles| configured_profile(profiles, profiles.anime_profile_id));
    let (choices, reason, freshness) = if !actor.rights.contains(ActorRights::SELECT_PROFILE) {
        let (choices, reason, _) = empty("Seerr advanced-request permission is required");
        let freshness = profiles.as_ref().map_or_else(
            || json!({"cache":"not_cached"}),
            |profiles| profiles.freshness.public(args.no_cache),
        );

        (choices, reason, freshness)
    } else if let Some(profiles) = &profiles {
        catalog_with_profiles(commands, profiles, args.media_type).await?
    } else {
        empty("No default non-4K Seerr service")
    };
    let mut items =
        vec![json!({"name":"Seerr configured default","profile":null,"uses_default":true})];

    items.extend(
        choices
            .iter()
            .map(|choice| json!({"name":choice.name,"profile":choice.name,"uses_default":false})),
    );
    let mut freshness = freshness;

    freshness["bypass_requested"] = json!(args.no_cache);
    Ok(
        json!({"items":items,"default_profile":default,"anime_default_profile":anime,"can_select_profile":reason.is_none(),"reason":reason,"freshness":freshness}),
    )
}

pub(super) async fn submit(
    commands: &Commands,
    broker: &dyn ApiSource,
    binding: &Binding,
    item: args::MediaRequest,
) -> Result<Value, ApiError> {
    validate_name(item.profile.as_deref())?;
    let selection = match &item.profile {
        None => None,

        Some(name) => {
            if !broker
                .accounts()
                .ok_or(ApiError::FORBIDDEN)?
                .identity(binding)
                .await?
                .rights
                .contains(ActorRights::SELECT_PROFILE)
            {
                return Err(ApiError::FORBIDDEN);
            }

            let (choices, _, _) = catalog(commands, broker, binding, item.media_type).await?;
            let mut matches = choices.into_iter().filter(|choice| &choice.name == name);
            let selected = matches.next().ok_or(ApiError::PROFILE_UNAVAILABLE)?;

            if matches.next().is_some() {
                return Err(ApiError::PROFILE_UNAVAILABLE);
            }

            Some(selected)
        }
    };

    broker
        .requests()
        .ok_or(ApiError::FORBIDDEN)?
        .submit(binding, item, selection)
        .await
}

type Catalog = (Vec<ProfileSelection>, Option<&'static str>, Value);

async fn catalog(
    commands: &Commands,
    broker: &dyn ApiSource,
    binding: &Binding,
    kind: args::MediaType,
) -> Result<Catalog, ApiError> {
    let Some(profiles) = broker
        .accounts()
        .ok_or(ApiError::FORBIDDEN)?
        .service_profiles(binding, kind)
        .await?
    else {
        return Ok(empty("No default non-4K Seerr service"));
    };

    catalog_with_profiles(commands, &profiles, kind).await
}

async fn catalog_with_profiles(
    commands: &Commands,
    profiles: &BrokerProfiles,
    kind: args::MediaType,
) -> Result<Catalog, ApiError> {
    let source = commands.sources.iter().find(|(_, source)| {
        source.descriptor().service_binding.map(u64::from) == Some(profiles.server_id)
            && source
                .library()
                .is_some_and(|library| library.media_type() == kind)
    });
    let Some((_, source)) = source else {
        return Ok(empty(
            "Link this Seerr server ID to its Arr source in Settings",
        ));
    };
    let definitions = source
        .library()
        .ok_or(ApiError::PROFILE_UNAVAILABLE)?
        .profiles()
        .await?;
    let choices = choices(profiles, &definitions);
    let reason = choices.is_empty().then_some("No verified non-4K profiles");

    Ok((choices, reason, profiles.freshness.public(false)))
}

fn configured_profile(profiles: &BrokerProfiles, id: Option<u64>) -> Option<Value> {
    let id = id?;
    let mut matches = profiles.profiles.iter().filter(|profile| profile.id == id);
    let profile = matches.next()?;

    if matches.next().is_some() || validate_name(Some(&profile.name)).is_err() {
        return None;
    }

    Some(json!({"id":profile.id,"name":profile.name}))
}

fn choices(
    profiles: &BrokerProfiles,
    definitions: &[crate::source::QualityProfile],
) -> Vec<ProfileSelection> {
    let mut choices = Vec::with_capacity(profiles.profiles.len());

    for profile in &profiles.profiles {
        if validate_name(Some(&profile.name)).is_err() {
            continue;
        }

        let mut matches = definitions
            .iter()
            .filter(|definition| definition.id == profile.id && definition.name == profile.name);
        let selected = matches.next();

        if matches.next().is_none() && selected.is_some_and(|definition| definition.non_4k) {
            choices.push(ProfileSelection {
                server_id: profiles.server_id,
                profile_id: profile.id,
                name: profile.name.clone(),
            });
        }
    }

    choices
}

fn empty(reason: &'static str) -> Catalog {
    (Vec::new(), Some(reason), json!({"cache":"not_cached"}))
}

pub(crate) fn validate_name(profile: Option<&str>) -> Result<(), ApiError> {
    if profile.is_some_and(|name| {
        name.is_empty()
            || name.trim() != name
            || name.len() > 256
            || name.chars().any(char::is_control)
    }) {
        return Err(ApiError::BAD_QUERY);
    }

    Ok(())
}
