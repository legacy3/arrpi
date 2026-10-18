//! Maintenance authorization is per source, independent of requester account versions.

use super::Commands;
use crate::{arguments, error::ApiError, maintenance as args, registry::ToolId, source::Principal};
use serde_json::Value;

impl Commands {
    pub(super) async fn maintenance(
        &self,
        principal: &Principal,
        tool: ToolId,
        input: Value,
    ) -> Result<Value, ApiError> {
        let name = input["source"].as_str().ok_or(ApiError::BAD_QUERY)?;
        let source = self.sources.source(name).ok_or(ApiError::FORBIDDEN)?;

        if !maintenance_permitted(principal, name, tool) {
            return Err(ApiError::FORBIDDEN);
        }

        let native = source.maintenance().ok_or(ApiError::FORBIDDEN)?;

        match tool {
            ToolId::RefreshMedia => {
                let args: args::MediaTarget = arguments::decode(input)?;

                args::validate_id(args.item_id)?;
                native.refresh(args.item_id).await
            }

            ToolId::SearchReleases => {
                let args: args::ReleaseSearch = arguments::decode(input)?;

                args.validate(self.policy.limits.max_batch_items)?;
                native.search(args).await
            }

            ToolId::SeriesEpisodes => {
                let args: args::Episodes = arguments::decode(input)?;

                args::validate_id(args.item_id)?;

                if !(1..=self.policy.limits.max_page_items).contains(&args.limit)
                    || args
                        .season
                        .is_some_and(|season| i32::try_from(season).is_err())
                {
                    return Err(ApiError::BAD_QUERY);
                }

                native.episodes(args).await
            }

            ToolId::MediaCommandStatus => {
                let args: args::CommandStatus = arguments::decode(input)?;

                args::validate_id(args.command_id)?;
                native.command(args.command_id).await
            }

            _ => Err(ApiError::NOT_FOUND),
        }
    }
}

pub(super) fn maintenance_permitted(principal: &Principal, source: &str, tool: ToolId) -> bool {
    match tool {
        ToolId::RefreshMedia => principal.allows(source, "library-refresh"),

        ToolId::SearchReleases => principal.allows(source, "library-search"),

        ToolId::SeriesEpisodes => principal.allows(source, "library-view"),

        ToolId::MediaCommandStatus => {
            principal.allows(source, "library-refresh")
                || principal.allows(source, "library-search")
        }

        _ => false,
    }
}
