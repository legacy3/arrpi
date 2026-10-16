# Personal playback tracking

The CrossWatch adapter exposes a small account-bound API for connecting Plex and
tracking destinations and controlling future-playback scrobbling. It is independent of Seerr,
MCP and bulk history synchronization. Native CrossWatch owns provider tokens,
PIN exchanges, refresh and playback processing. Device flows that require an
explicit poll are advanced through a scoped POST, never a status GET. The adapter is tested against
CrossWatch 0.13.4.

Add a `crosswatch` source with the native URL and a dedicated administrator API
token in `api_key`. User and monitor tokens cannot authorize providers or manage
watcher routes. Keep this integration credential on the server. It has native
administrator authority; ArrPi restricts which operations account clients can use.

Give each account a separate client with these fields:

```json
{
  "name": "tracking-alice",
  "key": "replace-with-a-generated-private-client-key",
  "permissions": ["crosswatch:tracking-view", "crosswatch:tracking-manage"],
  "tracking_user": {
    "source": "crosswatch",
    "profile_id": "native-profile-id",
    "plex_id": 42,
    "plex_server_owner": false
  }
}
```

The profile must own exactly one exclusive Plex instance. Each destination needs
one assigned instance; shared destinations are shown as operator-managed and
cannot be mutated through the portal. Additional destinations need one exclusive
native instance assigned to that same profile. Resolve Plex cloud IDs from verified
account authorization, never display names. A profile and Plex identity can each
be bound to only one client per source. Configuration/admin clients cannot carry
tracking bindings. Configure bindings through the revisioned settings API or
`config set`; editing client access in the UI preserves them.

| Route                          | Operation                                          |
| ------------------------------ | -------------------------------------------------- |
| `GET /v1/tracking`             | Own connection, watcher and recent delivery status |
| `POST /v1/tracking/plex`       | Start native Plex PIN authorization                |
| `POST /v1/tracking/trakt`      | Start native Trakt PIN authorization               |
| `POST /v1/tracking/enable`     | Create or enable the account's owned watcher route |
| `POST /v1/tracking/pause`      | Pause that route                                   |
| `POST /v1/tracking/disconnect` | Remove that route and disconnect its Trakt token   |

POST bodies must be `{}`. The authenticated client's binding supplies all
account and instance selectors. The source-specific `tracking-view` and
`tracking-manage` grants are separate. Responses are private and uncached. There
is no generic native write proxy or automatic retry.

Before linking, the adapter pauses its owned route. Starting tracking is a
separate explicit action after both accounts are linked. The operator must enable
CrossWatch watcher autostart first. End-user requests never change global watcher
settings. Server-owner/default Plex authorization is operator-managed; the portal
cannot replace those credentials. Shared users authorize their own Plex account.

The default Plex instance identifies the operator’s intended server. A participant’s
inspected server must match it before tracking can start. Routes use that exact
server UUID and an `id:` user filter. Shared Plex users use
cloud IDs; the server owner uses the verified PMS-local ID. Unresolved-user
fallback, ratings changes and watchlist removal are disabled. Foreign routes or
history-sync pairs referencing either instance block portal changes. Other
unchanged portal-managed destinations may share the same bound Plex source. A modified
managed route is also left alone.

`enabled` and `running` are distinct watcher states. Recent activity contains at
most five successful deliveries for the bound Plex and destination instances, from the
native recent-activity window; an empty list is not an all-time history claim.
Native pending authorization can survive expiry in CrossWatch configuration, so
clients must enforce the PIN response's `expires_in` deadline and permit retry.
Closing a PIN dialog does not revoke a previously issued native code. Pending
device approvals resume polling from native status when the user returns.

Back up the ArrPi SQLite database and CrossWatch configuration together, including
CrossWatch's encryption key. Browser gateways must resolve an authenticated,
stable subject to a dedicated account key server-side, reject cross-origin writes
and strip client-supplied authorization and identity headers. Never fall back to
an administrator or shared requester key.

## Additional destinations

Status includes `destinations`, each with an ID, label, connection and independent
watcher state. Supported IDs are `trakt`, `simkl`, `anilist`, `myanimelist`, `kitsu`,
`mdblist`, `bingebase`, `flicklist`, `punchplay`, `wetrakr`, `crosswatch`, `floppy`,
`scrob`, `jellyfin`, `emby` and `kodi`. Plex is the fixed source, not a second
destination instance. Other native integrations are not Watcher destinations.

`POST /v1/tracking/{id}-enable`, `{id}-pause` and `{id}-disconnect` resolve the
instance from the authenticated binding. POST bodies remain `{}`. Existing
Trakt-only actions remain compatible. Reauthorizing Plex validates then pauses
all owned destinations; relinking a destination pauses only its own route.

Plex and Trakt PIN endpoints remain available. `POST /v1/tracking/{id}` starts
native device login for `simkl`, `mdblist`, `punchplay`, `bingebase` and `flicklist`.
The sanitized response contains only a display code, fixed provider approval URL
and expiry. Call `{id}-poll` while approval is pending; it returns a `state` of `pending`,
`authorized`, `expired` or `denied`. Native pending
device credentials never leave CrossWatch. Native poll intervals and expiry
checks still apply. Clients should resume pending polling from status after navigation. Native HTTP
Date supplies the clock for providers that return absolute expiry timestamps.

Other providers need native operator authorization first. Managed CrossWatch
accounts cannot use its authentication screens. ArrPi does not expose a generic
credential/configuration write proxy or let users choose arbitrary server URLs.
Operator-linked providers support enable/pause; disconnect returns conflict
before any write so portal users cannot clear operator-managed server credentials
or trigger the local tracker's global disconnect. Device-code providers support
disconnect of their own account credentials.

Watcher route and native instance identity must match exactly, even for disabled
routes. The legacy top-level Trakt fields are retained alongside destinations for
existing clients. Provider tokens remain exclusively in CrossWatch state.
