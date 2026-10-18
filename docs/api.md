# HTTP API and feeds

Use `Authorization: Bearer <ArrPi client key>` for data endpoints and metrics.
Personal feed URLs need neither browser cookies nor a normal ArrPi client key.

| Route                                                  | Response                                                                             |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| `GET /health`                                          | Public process liveness; no upstream status or identities                            |
| `GET /v1/{source}/api/v3/{endpoint}`                   | Original upstream JSON with cache/age headers                                        |
| `GET /v1/calendar?start=YYYY-MM-DD&end=YYYY-MM-DD`     | Combined monitored calendar for authorized sources                                   |
| `GET /v1/calendar.rss?start=YYYY-MM-DD&end=YYYY-MM-DD` | Authenticated RSS 2.0 release feed using `calendar-view` grants                      |
| `GET /v1/calendar.ics?start=YYYY-MM-DD&end=YYYY-MM-DD` | Authenticated iCalendar export using `calendar-view` grants                          |
| `GET /feeds/{token}/calendar.rss` or `calendar.ics`    | Personal, revocable subscriptions; no browser SSO                                    |
| `POST /v1/subscription`                                | Issue/retrieve a subject’s stable feed URLs (private `feeds:issue` grant)            |
| `POST /v1/subscription/rotate`                         | Replace that subject’s URLs, invalidating the previous token                         |
| `DELETE /v1/subscription`                              | Revoke that subject’s URLs; ordinary issuance stays disabled until explicit rotation |
| `GET /v1/config`, `PUT /v1/config`                     | Admin settings; revisioned edits and masked credentials                              |
| `GET /v1/config/connections/{source}/health`           | Fresh operator-only connection check; requires `config:manage`                       |
| `GET /v1/summaries/queue`, `/library`, `/status`       | Minimal operational counters with separate grants and per-source completeness        |
| `GET /v1/sources/{source}/summaries/{kind}`            | The same projection for one authorized source, without querying unrelated apps       |
| `GET /v1/status`                                       | Policy and counters for authorized sources; no native URLs or keys                   |
| `GET /v1/tools`, `POST /v1/tools/{tool}`               | Account-scoped command discovery and execution; same checks as MCP                   |
| `POST /v1/mcp`                                         | Stateless Streamable HTTP MCP; account Bearer credential required                    |
| `GET /v1/tracking`, `POST /v1/tracking/{action}`       | [Account-bound CrossWatch playback tracking](tracking.md); separate source grants    |
| `GET /metrics`                                         | Private aggregate Prometheus counters                                                |

Allowlisted native endpoints are `calendar`, `queue`, `health`, Sonarr `series` and
Radarr `movie`, plus read-only `qualityprofile` for each app. A client must have the corresponding `source:endpoint` permission.
Native responses can contain filesystem paths and other administrative metadata;
the normalized calendar omits those fields. Do not hand broad reader
keys to an untrusted frontend.

The combined calendar needs `source:calendar-view`; this grants no raw calendar or
other native endpoint access. The `metrics` permission is separate and grants
aggregate counters across the server. `/v1/status` lists only sources granted to
the presented key. Each source includes its permitted native `endpoints`, normalized
`summaries` (`library`, `queue`, `status`), `calendar` access, `account` and `tracking` capabilities. The configured
account broker is also visible to configuration managers and bound accounts with
catalog, request or issue read access. Discovery does not probe upstreams or grant
additional operations.

The web overview reads only the summaries advertised for its client. Explorer
uses one source selector and one operation selector. Individual sources offer
their permitted native reads, summaries and account commands; **All sources**
groups combined projections and commands. Source changes clear the previous
response and pending input. Shared commands use `GET /v1/tools` for the current
account's names, input schemas and
read/write annotations, then `POST /v1/tools/{tool}` with JSON arguments. This uses
the same executor and policy as MCP. A command's `source` identifies its account
broker; omitted means a combined operation. The catalog's `account` is the caller's
local requester name or null. Configuration credentials cannot carry requester
bindings. Seerr actions need an account-bound
client; configuration access alone does not impersonate the owner. Writes require
explicit confirmation and are never automatically retried. Native reads and
operational summaries remain usable when MCP or Seerr is not configured.

Connection checks report `healthy`, `attention`, `authentication_failed`,
`unavailable` or `unconfigured`, with an optional health-issue count. They use the
configured service credential without impersonating a Seerr requester and return
no upstream identity, URL or response body. These checks are separate from the
account-scoped MCP `service_status` tool.

RSS and iCalendar use the same scoped, bounded calendar/cache; a failed authorized
source returns 502 rather than a successful partial feed. RSS uses stable non-permalink GUIDs and validated IMDb links. `pubDate` represents the release timestamp (UTC midnight for all-day
movie dates), not the time ArrPi discovered the item. Item descriptions report current
library availability. Feed metadata comes from `instance.title` and `instance.link`;
the public website link must contain no credentials, query or fragment.
Normal API-key exports require an Authorization header and explicit dates. Personal
capabilities are accepted only for RSS/iCalendar: through `/feeds/{token}/...`, the
`X-ArrPi-Feed-Token` header, or a feed-only Bearer token at the export endpoints.
They grant no JSON, native, status, metrics or issuance access. Arbitrary query-string
credentials remain rejected. Without dates, personal subscriptions roll forward daily;
an explicit bounded date window remains fixed.

Calendar queries require both dates and allow at most 93 days. Combined results
are limited to 8192 events; `calendar_too_large` means to request a shorter range.
`start` and `end` are
inclusive days for the normalized calendar. Upstream calendar calls enforce
`unmonitored=false` and include series metadata. Queue pagination accepts `page` and
`pageSize`, each from 1 through 100. Unknown or repeated query parameters are rejected.

Native success responses include `X-ArrPi-Cache: miss|hit|stale` and `Age` in seconds.
The combined calendar reports those values per source. Healthy sources remain visible
during another source's failure; all sources unavailable returns HTTP 502.
Episode timestamps are UTC, while movie dates retain their calendar day. Movie events
distinguish `cinema`, `digital` and `physical`; these are release dates, not download
or playback availability promises.

## Personal feed subscriptions

Enable personal RSS/iCal in Settings (off by default). Long-lived opaque URLs need
no cookies or SSO session and roll over seven past/sixty future UTC days by
default. Explicit export windows remain bounded to 93 days.

A trusted integration gets a separate client with `feeds:issue` and the relevant
`source:calendar-view` grants. It sends the verified identity in `X-ArrPi-Subject`
to `POST /v1/subscription`; browsers cannot choose that identity or source grants.
`POST /v1/subscription/rotate` replaces its URLs; `DELETE /v1/subscription` revokes
them. Ordinary issuance cannot undo revocation; explicit rotation can. Revoke
subscriptions when retiring an identity. Logout does not revoke feeds.

Capabilities use HMAC-SHA256 with the database's master key, persistent random
namespace, subject and generation. The database stores digests and scoped grants.
Registry loss creates a new namespace, so old URLs cannot resurrect. Feed tokens
grant no JSON/native/status/metrics/issuer/config access. Core and the optional UI expose
the same public paths; Next forwards only a fixed export path and token header,
without browser credentials or redirects. Disable proxy URL logging and retain
no-referrer/no-store headers for these routes.

See [MCP and media requests](mcp.md) for command inputs, pagination, errors and account-bound mutations. [Configuration](configuration.md) describes authentication, limits and backup. The generated [public schema](api/public.schema.json) describes owned API types; exact native version bundles and licenses are under [third_party/native-types](../third_party/native-types/README.md).

Typed library maintenance also uses `POST /v1/tools/{tool}`: `refresh_media`,
`search_releases`, `series_episodes` and `media_command_status`. Each accepts an exact
source name and native library/command IDs. `GET /v1/tools` includes permitted `sources`
for these operations; it does not grant arbitrary command writes. Source grants and
the asynchronous refresh/search workflow are documented in
[library maintenance](mcp.md#refresh-and-search-installed-media). The native passthrough
endpoints above remain read-only.

## Operational summaries

Summary reads work independently of MCP. Add a source in Settings and grant the exact operation needed; ordinary library/calendar access does not grant queue or monitoring data. `?no_cache=true` or `Cache-Control: no-cache` requires fresh upstream reads. Equivalent CLI/MCP tools are `queue_summary`, `library_summary` and `monitoring_status`.

| Projection              | Grant                    | Data                                                                 |
| ----------------------- | ------------------------ | -------------------------------------------------------------------- |
| `/v1/summaries/queue`   | `source:queue-summary`   | Sonarr/Radarr queue counts, statuses, observed total/remaining bytes |
| `/v1/summaries/library` | `source:library-summary` | Native item/file/monitored counts and measured library bytes         |
| `/v1/summaries/status`  | `source:status-view`     | SABnzbd, Cleanuparr, Bazarr or Maintainerr counters                  |

Each source has its own result and freshness. `complete:false` is explicit when a source fails, a queue changes during pagination or a bound stops traversal. Queue reads visit at most twenty 100-item pages within fifteen seconds; pages are not an atomic snapshot. Duplicate IDs and changed totals invalidate completeness. Rows belonging to one download (for example a season pack) count separately for pagination, but byte totals are deduplicated by download client and download ID. Rows lacking that identity stay explicit and make `byte_counts_complete` false. Observed byte counters retain native floating-point precision; they are approximate. Unknown library sizes remain null, with known bytes and the number of unmeasured items reported separately. No titles, native paths, job passwords or user records appear in these projections.

Queue, health and monitoring caches stay fresh for at most fifteen seconds and never serve stale results. Library/calendar caches retain the configured general policy. Every cache shares the aggregate backing-buffer budget and bounded upstream/normalization admission. Reading a cached summary still requires its grant.

The monitoring adapters expose fixed read operations, not arbitrary native APIs:

| Source      | Verified contract                                                                                                                                        | Meaning                                                                                                                                          |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| SABnzbd     | [5.1.3 queue API](https://sabnzbd.org/wiki/configuration/5.1/api)                                                                                        | Total queued jobs, active jobs, pause state, speed in KiB/s and active remaining MiB; paused/checking jobs are excluded from the active counters |
| Cleanuparr  | [v2.10.10 stats v2](https://github.com/Cleanuparr/Cleanuparr/blob/v2.10.10/code/backend/Cleanuparr.Api/Controllers/StatsV2Controller.cs)                 | Seven-day removals and seeker search/completion/failure/grab counters, excluding dry runs                                                        |
| Bazarr      | [v1.6.2 badges](https://github.com/morpheus65535/bazarr/blob/v1.6.2/bazarr/api/badges/badges.py)                                                         | Missing subtitle-language requirements, throttled providers and health issues; requirements are not a count of distinct episodes/movies          |
| Maintainerr | [v3.30.1 storage metrics](https://github.com/Maintainerr/Maintainerr/blob/v3.30.1/apps/server/src/modules/storage-metrics/storage-metrics.controller.ts) | Cumulative handled-action/known-delete-byte totals, reclaimable collection sizes and the upstream fallback flag                                  |

SAB's API key goes in a fixed read-only POST body; its returned job list is limited to one. Other keyed adapters use sensitive `X-Api-Key` headers. Maintainerr's private native endpoint has no API-key requirement; keep it inside the service network. Maintainerr aggregates its own mounts and can report fallback reclaim estimates that overlap: ArrPi preserves those facts, reports unsized collection counts and `reclaimable_size_complete`, and does not reinterpret them as physical disk capacity. Cleanuparr has no exact successful-replacement count; that field remains null rather than treating a recovered strike or a grab as a completed replacement.
