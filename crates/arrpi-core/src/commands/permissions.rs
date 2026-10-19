//! Fresh caller rights and source capabilities determine command visibility.

use super::{Caller, Commands};
use crate::{
    arguments::MediaType,
    registry::ToolId,
    source::{Actor, ActorRights, Principal},
};

impl Commands {
    /// Source choices for typed library maintenance, filtered by exact caller grants.
    pub fn command_sources<'a>(
        &'a self,
        principal: &'a Principal,
        tool: ToolId,
    ) -> impl Iterator<Item = &'a str> {
        self.sources
            .iter()
            .filter(move |(name, source)| {
                source.maintenance().is_some()
                    && super::maintenance::maintenance_permitted(principal, name, tool)
                    && (tool != ToolId::SeriesEpisodes
                        || source
                            .library()
                            .is_some_and(|library| library.media_type() == MediaType::Tv))
            })
            .map(|(name, _)| name)
    }

    pub(super) fn unavailable_reason(
        &self,
        caller: &Caller<'_>,
        tool: ToolId,
    ) -> Option<&'static str> {
        if !self.ready() {
            return Some("server_disabled");
        }

        if self
            .policy
            .disabled_tools
            .iter()
            .any(|name| name.as_ref() == tool.name())
        {
            return Some("tool_disabled");
        }

        if !tool.is_native() && caller.principal.binding.is_none() {
            return Some("account_not_linked");
        }

        if !self.locally_permitted(caller.principal, tool) {
            return Some("missing_grant");
        }

        if !actor_permitted(caller.actor, tool) {
            return Some(if caller.actor.is_none() {
                "account_unverified"
            } else {
                "account_permission"
            });
        }

        if tool.is_write() && !tool.is_native() && !caller.compatible {
            return Some("unverified_version");
        }

        None
    }

    pub(super) fn allowed(&self, caller: &Caller<'_>, tool: ToolId) -> bool {
        !self
            .policy
            .disabled_tools
            .iter()
            .any(|name| name.as_ref() == tool.name())
            && (!tool.is_write() || tool.is_native() || caller.compatible)
            && self.permitted(caller, tool)
    }

    pub(super) fn permitted(&self, caller: &Caller<'_>, tool: ToolId) -> bool {
        self.locally_permitted(caller.principal, tool) && actor_permitted(caller.actor, tool)
    }

    pub(super) fn locally_permitted(&self, principal: &Principal, tool: ToolId) -> bool {
        match tool {
            ToolId::RefreshMedia
            | ToolId::SearchReleases
            | ToolId::SeriesEpisodes
            | ToolId::MediaCommandStatus => self.command_sources(principal, tool).next().is_some(),

            ToolId::QueueSummary | ToolId::LibrarySummary | ToolId::MonitoringStatus => {
                super::summary_kind(tool).is_some_and(|kind| {
                    self.sources.iter().any(|(name, source)| {
                        source
                            .summaries()
                            .is_some_and(|summary| summary.supports(kind))
                            && principal.allows(name, kind.grant())
                    })
                })
            }

            ToolId::ListMovies | ToolId::ListSeries | ToolId::SearchLibrary => {
                self.sources.iter().any(|(name, source)| {
                    source.library().is_some_and(|library| {
                        principal.allows(name, "library-view")
                            && match tool {
                                ToolId::ListMovies => library.media_type() == MediaType::Movie,
                                ToolId::ListSeries => library.media_type() == MediaType::Tv,
                                _ => true,
                            }
                    })
                })
            }

            ToolId::UpcomingReleases => self.sources.iter().any(|(name, source)| {
                source.calendar().is_some() && principal.allows(name, "calendar-view")
            }),

            ToolId::ServiceStatus => {
                self.sources.iter().any(|(name, source)| {
                    ((source.summaries().is_some() || source.ratings().is_some())
                        && principal.allows(name, "status-view"))
                        || (source.library().is_some() && principal.allows(name, "library-view"))
                        || (source.calendar().is_some() && principal.allows(name, "calendar-view"))
                }) || (self.sources.account_source().is_some()
                    && principal.binding.is_some()
                    && (principal.allows("catalog", "view")
                        || principal.allows("requests", "view")))
            }

            _ => self.permitted_account(principal, tool),
        }
    }

    fn permitted_account(&self, principal: &Principal, tool: ToolId) -> bool {
        let Some(broker) = self.sources.account_source() else {
            return false;
        };

        if principal.binding.is_none() {
            return false;
        }

        match tool {
            ToolId::SearchMedia | ToolId::MediaDetails | ToolId::MediaDetailsBatch => {
                broker.catalog().is_some() && principal.allows("catalog", "view")
            }

            ToolId::MyQuota => principal.allows("requests", "view"),

            ToolId::MyRequests | ToolId::RequestStatus => {
                broker.requests().is_some() && principal.allows("requests", "view")
            }

            ToolId::RequestMovie
            | ToolId::RequestSeries
            | ToolId::RequestMediaBatch
            | ToolId::RequestProfiles => {
                broker.requests().is_some() && principal.allows("requests", "create")
            }

            ToolId::CancelRequest => {
                broker.requests().is_some() && principal.allows("requests", "cancel")
            }

            ToolId::ListIssues => broker.issues().is_some() && principal.allows("issues", "view"),

            ToolId::ReportIssue | ToolId::CloseIssue | ToolId::CloseIssuesBatch => {
                broker.issues().is_some() && principal.allows("issues", "create")
            }

            _ => false,
        }
    }
}

fn actor_permitted(actor: Option<Actor>, tool: ToolId) -> bool {
    if tool.is_native() {
        return true;
    }

    let Some(actor) = actor else {
        return false;
    };
    let rights = match tool {
        ToolId::RequestMovie => ActorRights::REQUEST_MOVIES,

        ToolId::RequestSeries => ActorRights::REQUEST_SERIES,

        ToolId::RequestMediaBatch | ToolId::RequestProfiles => {
            ActorRights::REQUEST_MOVIES | ActorRights::REQUEST_SERIES
        }

        ToolId::CancelRequest => ActorRights::CANCEL_OWN_PENDING,

        ToolId::ListIssues => ActorRights::VIEW_ISSUES,

        ToolId::ReportIssue | ToolId::CloseIssue | ToolId::CloseIssuesBatch => {
            ActorRights::REPORT_ISSUES
        }

        _ => return true,
    };

    actor.rights.intersects(rights)
}
