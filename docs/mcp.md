# MCP and media requests

## Setup

Configure Seerr's internal URL/API key and catalog language in **Settings**. Give each
account its own client key with its Seerr user ID and stable Plex user ID. Only Plex-bound Seerr accounts are supported; local, Jellyfin and Emby users cannot be bound. The catalog language defaults to English (`en`); change it in the Seerr connection settings. Open **Settings → MCP**
at `/settings/mcp` to enable the server and tune tools/limits. The **MCP** page at `/mcp` contains connections, account access, usage statistics and write history. MCP transport requires a saved Seerr connection.
The page has separate **Copy Claude Code**, **Copy Codex** and **Copy Gemini CLI** buttons. Each copies one complete install command with the selected account token. Claude/Gemini use native HTTP with user scope. The generated Codex command uses `mcp-remote` through `npx` to supply a literal Bearer header; Node/npx is required on that client machine, not in core.

The activity page shows per-account grants; global tool switches live in **Settings → MCP**. Admin/configuration keys cannot be copied as MCP credentials. A deleted or mismatched native account fails closed; tools recheck Seerr permissions on every call. Tokens, bindings and connection settings live in SQLite.

`/v1/mcp` is a stateless Streamable HTTP endpoint. Configure a public
HTTPS endpoint in Settings, route that exact path to ArrPi, and supply the account's
`Authorization: Bearer` token. [External OAuth](oauth.md) is optional; account-token clients need no SSO browser session. Keep normal UI/admin routes protected.
The shared command API is `GET /v1/tools` and `POST /v1/tools/{tool}`. It uses the
same account credential, schemas and checks. Only these explicit routes belong in
a token-authenticated ingress exception; never expose an unrestricted API prefix.

```console
# Both deployments use the same account-bound API.
export ARRPI_API_KEY='<account token>'
arrpi mcp --url https://arrpi.example.com
arrpi tool search_media --url https://arrpi.example.com --arguments '{"query":"Moonberry Station"}'
arrpi tool list_series --url https://arrpi.example.com --no-cache
```

Stdio emits MCP messages only. HTTPS is required except for loopback. `--no-cache`
forces fresh library/calendar reads; Seerr account/catalog/request/issue reads are
always fresh and never share the native source cache.

## Tools

Only permitted tools are advertised.

| Workflow             | Tools                                                                                                                           |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Installed library    | `list_movies`, `list_series`, `search_library`                                                                                  |
| Catalog and details  | `search_media`, `media_details`, `media_details_batch`                                                                          |
| Requests             | `my_requests`, `request_status`, `request_profiles`, `request_movie`, `request_series`, `request_media_batch`, `cancel_request` |
| Issues               | `list_issues`, `report_issue`, `close_issue`, `close_issues_batch`                                                              |
| Quota and health     | `my_quota`, `service_status`                                                                                                    |
| Operational counters | `queue_summary`, `library_summary`, `monitoring_status`                                                                         |
| Calendar             | `upcoming_releases`                                                                                                             |

Library pages contain at most the configured page limit (default 100) and report totals, completeness and source freshness. Catalog results contain TMDB IDs and public links; native paths and user objects are omitted.

## Requests and profiles

Requests go through Seerr with the server-resolved account in X-API-User. Seerr
keeps approval, quotas, requester tags and default services/profiles. Fresh duplicate preflight returns already_available/already_requested without creating a duplicate; TV checks each explicitly requested season. Inputs cannot
select another user, 4K, a raw profile/server/root folder ID, tags or quota bypass. TV requests require
explicit distinct positive seasons and report accepted/skipped seasons. Fresh preflight
checks identity, availability, seasons and quotas. Accepted requests are not completed
downloads. Supported write contract: Seerr 3.5.0, pinned in the native type bundles;
other versions retain validated reads but disable mutations until tested.

`request_profiles` lists safe named movie/TV profiles from Seerr's current default non-4K service. Omit `profile` or pass null to keep Default; a real profile named "Default" is still a normal named choice. Single requests and each batch item accept the same optional exact name. Explicit selection requires native REQUEST_ADVANCED, MANAGE_REQUESTS or ADMIN permission, rechecked fresh. Set the associated source's **Seerr server ID (profile selection)** in general Settings; IDs may be zero, are scoped by app kind and cannot be assigned twice. ArrPi never guesses this association. Missing mapping leaves Default available.

For an explicit choice, ArrPi reads the linked Arr instance's full `qualityprofile` definitions fresh and rejects unknown, empty or 4K-enabled profiles. An enabled group enables its member qualities regardless of child flags; a disabled group remains disabled. Seerr and native profile IDs/names must agree. Server/profile IDs go only to Seerr, with requester/root/tag policy preserved. The receipt confirms the selected profile was accepted by Seerr; it does not prove the asynchronous Arr download succeeded. Existing library media is a no-op and returns `profile_applied: false`.

## Library search and links

`search_library` accepts structured `media_type` (movie/tv, optional), `query` over title/plot, any-of `genres`, `available`, `year_from`, `year_to`, `min_rating` (0-10), `sort` (title/rating/newest/recently_added), `offset`, `limit` and `no_cache`. It searches installed media, independently of the requestable Seerr catalog. Existing list tools share its schema and metadata.

```console
arrpi tool search_library --arguments '{"media_type":"movie","genres":["Comedy"],"available":true,"min_rating":7,"sort":"rating","limit":20}'
```

Results include plot, genres, ratings/provider, runtime, certification, IDs, dates, original language and file availability. The primary community rating prefers IMDb, then TMDB, Trakt and Sonarr-reported ratings; critic ratings remain separately labeled, including their raw values/scales even when native vote counts are zero. Sonarr does not identify a rating provider in its response, so that score is labeled Sonarr rather than an invented database origin. Series `available` means at least one episode file exists; episode counts, `availability.coverage_scope` and `availability.status` distinguish partial coverage from `monitored_available`, which covers the monitored aired count rather than the entire series. Movie `release_date` is native `inCinemas`, TV uses `firstAired`; digital and physical movie releases have their own fields. Nothing in this normalized output exposes native paths or file objects. `total` counts filtered visible items, `next_offset` is the next page or null, `complete` and source statuses identify failures. Sorting has a stable source/ID tie-breaker. Pages use the configured limit, with no 10,000-entry or offset cutoff. Native APIs return complete arrays, still bounded by `cache.max_body_bytes` (supported up to 64 MiB); increase that setting for large libraries. An oversized/malformed source is marked unavailable, never silently truncated; `upstream_response_too_large` identifies the byte-cap case. Cached pages share the current native snapshot; a forced refresh or source update can change results between pages.

`media_details` and `media_details_batch` return `links` for the Seerr entry, playback, TMDB, IMDb, TVDB, Trakt, Simkl and movie-only Letterboxd. Seerr entry URLs use its public Application URL (including a configured base path); playback uses native `mediaInfo.mediaUrl`, generated by Seerr from the media server Web App URL and actual item identity. No deployment domain is hardcoded. Missing IDs, unavailable media or absent public URLs produce null links/reasons. Credentials, token parameters and internal URLs are discarded. Details batches read public settings once, retaining healthy metadata if those settings fail.

## Batches, freshness and errors

Writes have bounded admission and no automatic HTTP retries. Timeout/ambiguous
responses return `unknown_outcome`; inspect media details and your requests before
resubmitting. No mutation journal or exactly-once guarantee is added. Pending-only
cancellation is disabled for Seerr admin/manager actors because their native DELETE
can remove approved requests during a concurrent approval. Ordinary users can cancel
their own pending non-4K requests. Issues resolve Seerr's internal media ID, not TMDB
ID, and validate optional season/episode coordinates. Users can close only their own issues.

Request/details/issue-closing batches accept twenty items by default (adjustable to one hundred).
Requests run sequentially and return ordered per-item outcomes. Batch/page/concurrent-write
limits, queue wait and the total write-batch budget (45 seconds by default) are configurable under MCP.
Budget expiry leaves remaining items unsubmitted and reports the active item conservatively
as `unknown_outcome`; reconcile it before retrying. Overlapping writes queue instead of
failing immediately; a before-dispatch queue timeout is explicitly retry-safe.

Read tools accept `no_cache`; cached library/calendar reads then require fresh upstream data.
Seerr reads are always uncached by ArrPi. Media responses include `available`,
`partially_available`, `status_name`, `reported_updated_at` and freshness metadata.
Seerr can still lag its latest Plex sync; fresh reads do not trigger library scans.

`my_quota` exposes only the bound account's native movie/TV limits, rolling days, used/remaining allowance, restricted state and units (movies vs seasons). Limit0 is unlimited. `service_status` independently reports authorized Seerr/Arr reachability, native health issue count and cache age, with `no_cache` for a current probe. Stale health is not claimed as current reachability. Source health remains discoverable when Seerr is down; unverified personalized/write tools remain hidden. `close_issues_batch` accepts distinct `issue_ids` and uses the single-tool creator/native permission checks plus configured write-batch limits.

Search uses RFC3986 encoding for every query value. Seerr rejects form `+` spaces but accepts `%20`, so multi-word/punctuation queries must not use form encoding. Candidates return `tmdb_id`, title, year, type and availability. `availability` describes the overall media status; `season_availability` contains individual TV seasons and is empty for movies. `requested` means pending/processing in Seerr, not historical requests or absence from the library. Search is also the title-to-ID candidate resolver, with no guessing among remakes. Errors identify upstream invalid arguments, rate limiting, unavailability and response byte-cap overflow, while never forwarding private error bodies.

Read detail batches use the same total deadline. A failed item does not suppress later independent reads; an expired deadline returns retry-safe `batch_timeout` and explicit unattempted tails. Write batches stop on infrastructure failures and retain `unknown_outcome` for dispatched mutations.

## Statistics and write history

The MCP table shows per-account/tool calls, errors and last-used time. Aggregate counts,
outcome categories and durations persist in the same SQLite database, survive token
rotation/restarts and contain no arguments or credentials. The protected statistics API is
`GET /v1/config/mcp/stats?client=<name>`. A batch counts as one outer tool call; errors
include partial batches. Tools/list and invalid credentials are not counted.

The protected MCP page/API shows paginated **Write history** per selected account. `GET /v1/config/mcp/audit?client=<name>&offset=0&limit=20` requires config management. The same SQLite file records time, client name, original bound Seerr actor, tool, numeric media/request/issue IDs, season coordinates and outcome. It never stores issue messages, search text, titles, arbitrary arguments or credentials. Request/issue batches record attempted items independently, including uncertain outcomes, and skip unsubmitted items. Retention defaults to 10,000 entries **per account**, configurable through **Settings → MCP** and applied on each write. Key rotation preserves history; client rename/removal does not rewrite old rows, and reusing a name retains its past actor IDs. This is bounded best-effort activity history, not an exactly-once/retry journal; audit storage failure never causes an already-issued write to be retried.
Cancellation after upstream acceptance can also leave an attempt without its final
history row. Reconcile native request state before retrying an uncertain write.

## Native metadata limits

Installed movies include audio/subtitle summaries, video codec and file quality when Radarr reports them in its native library snapshot. Sonarr series snapshots do not contain individual episode-file media info; those fields stay absent rather than triggering an episode-by-episode scan. Native language summaries are not inferred from a title's original language.

`request_profiles` reports the configured ordinary and anime defaults as IDs/names, even when the account cannot choose an explicit profile. Submitting `profile:null` still delegates selection to Seerr. Request summaries include native creation/update times and optional titles/release dates where supplied. The pinned Seerr request-list API does not provide titles; ArrPi does not fan out to TMDB for each request. Submit/duplicate receipts reuse title/date metadata already fetched by their preflight.

Seerr 3.5.0 has no reliable ordinary-account endpoint for the last completed full Plex sync. Its [admin-only sync status](https://github.com/seerr-team/seerr/blob/v3.5.0/server/routes/settings/index.ts) exposes progress, and its [scanner library lastScan](https://github.com/seerr-team/seerr/blob/v3.5.0/server/lib/scanners/plex/index.ts) is updated by recent-item scans rather than all full scans. ArrPi therefore does not label either value as a completed full-sync timestamp. Fresh ArrPi reads cannot force Seerr's Plex sync.
