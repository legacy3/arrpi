//! Translate Seerr's native role encoding into provider-independent account actions.
use arrpi_core::source::{Actor, ActorRights};
use serde::Deserialize;

const ADMIN: u64 = 2;
const MANAGE_REQUESTS: u64 = 16;
const REQUEST: u64 = 32;
const ADVANCED: u64 = 8192;
const MOVIE: u64 = 262_144;
const TV: u64 = 524_288;
const MANAGE_ISSUES: u64 = 1_048_576;
const VIEW_ISSUES: u64 = 2_097_152;
const CREATE_ISSUES: u64 = 4_194_304;

/// Only this adapter decodes Plex bindings and Seerr native permissions.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct NativeActor {
    pub(super) id: u64,
    pub(super) plex_id: u64,
    permissions: u64,
}

impl NativeActor {
    pub(super) fn normalized(self) -> Actor {
        let mut rights = ActorRights::empty();

        for (action, mask) in [
            (ActorRights::REQUEST_MOVIES, REQUEST | MOVIE),
            (ActorRights::REQUEST_SERIES, REQUEST | TV),
            (ActorRights::SELECT_PROFILE, ADVANCED | MANAGE_REQUESTS),
            (
                ActorRights::VIEW_ISSUES,
                VIEW_ISSUES | CREATE_ISSUES | MANAGE_ISSUES,
            ),
            (ActorRights::REPORT_ISSUES, CREATE_ISSUES | MANAGE_ISSUES),
        ] {
            rights.set(action, self.allows(mask));
        }

        // Native privileged DELETE can remove approved media requests, so it is excluded.
        rights.set(
            ActorRights::CANCEL_OWN_PENDING,
            self.permissions & (ADMIN | MANAGE_REQUESTS) == 0,
        );
        Actor {
            id: self.id,
            rights,
        }
    }

    const fn allows(self, mask: u64) -> bool {
        self.permissions & (ADMIN | mask) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_native_roles_preserve_any_of_permissions_and_restrict_privileged_cancel() {
        let requests = ActorRights::REQUEST_MOVIES | ActorRights::REQUEST_SERIES;
        let issues = ActorRights::VIEW_ISSUES | ActorRights::REPORT_ISSUES;
        let cancel = ActorRights::CANCEL_OWN_PENDING;
        let select = ActorRights::SELECT_PROFILE;

        for (permissions, expected) in [
            (0, cancel),
            (ADMIN, requests | issues | select),
            (MANAGE_REQUESTS, select),
            (REQUEST, requests | cancel),
            (ADVANCED, select | cancel),
            (MOVIE, ActorRights::REQUEST_MOVIES | cancel),
            (TV, ActorRights::REQUEST_SERIES | cancel),
            (MANAGE_ISSUES, issues | cancel),
            (VIEW_ISSUES, ActorRights::VIEW_ISSUES | cancel),
            (CREATE_ISSUES, issues | cancel),
            (1_u64 << 48, cancel),
            (
                MOVIE | ADVANCED,
                ActorRights::REQUEST_MOVIES | select | cancel,
            ),
            (REQUEST | MANAGE_REQUESTS, requests | select),
            (ADMIN | VIEW_ISSUES, requests | issues | select),
        ] {
            let actor = NativeActor {
                id: 7,
                plex_id: 107,
                permissions,
            }
            .normalized();

            assert_eq!(actor.id, 7);
            assert_eq!(actor.rights, expected, "native mask {permissions}");
        }
    }
}
