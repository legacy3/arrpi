# MCP and media requests

## Setup

Configure Seerr's internal URL/API key and catalog language in **Settings**. Give each
account its own client key and select its verified requester account in **Settings → Access**. The picker binds both the Seerr ID and stable Plex ID. Only Plex-bound Seerr accounts are supported; local, Jellyfin and Emby users cannot be bound. The catalog language defaults to English (`en`); change it in the Seerr connection settings. Open **Settings → MCP**
at `/settings/mcp` to enable the server and tune tools/limits. The **MCP** page at `/mcp` contains connections, account access, usage statistics and write history. MCP transport requires a saved Seerr connection.
The page has separate **Copy Claude Code**, **Copy Codex** and **Copy Gemini CLI** buttons. Each copies one complete install command with the selected account token. Claude/Gemini use native HTTP with user scope. The generated Codex command uses `mcp-remote` through `npx` to supply a literal Bearer header; Node/npx is required on that client machine, not in core.

The activity page separates searchable write history from tools and usage. History searches all retained entries for the selected account and filters by action, outcome and time. Accepted commands remain distinct from completed commands; the progress dialog only reads their status. Older records without receipt titles retain their coordinates.

The instance title and public website in **Settings → General** identify the stack in MCP initialization, together with refresh/search workflow instructions.

The activity page shows effective per-account access and its reason; global tool switches live in **Settings → MCP**. Admin/configuration keys cannot be copied as MCP credentials. Account tools fail closed for deleted/mismatched Seerr accounts and recheck native permissions on every call. Library maintenance uses separate source grants. Tokens, bindings and connection settings live in SQLite.

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
| Library maintenance  | `refresh_media`, `series_episodes`, `search_releases`, `media_command_status`                                                   |

Library pages contain at most the configured page limit (default 100) and report totals, completeness and source freshness. Catalog results contain TMDB IDs and public links; native paths and user objects are omitted.

## Refresh and search installed media

For “check whether this series has a new episode we are missing”:

1. Resolve the exact title with `search_library` or `list_series`. Keep its `source` and native `id`; a TMDB ID is not interchangeable.
2. Call `refresh_media` with `{"source":"shows","item_id":7}`. This queues metadata refresh and a file scan, including updated episode metadata. It does not search indexers.
3. Wait briefly and check `media_command_status` with the returned `source` and `command_id`. A queued/started command has not finished. Continue only after `successful:true`.
4. Call `series_episodes` with `{"source":"shows","item_id":7,"missing_only":true,"aired_only":true}`. Follow `next_offset` to inspect all matching episodes; `monitored` identifies those eligible for automatic search. Unknown air dates remain unknown.
5. Call `search_releases` with the same source/item ID to search monitored aired missing episodes. Add `season` or explicit native `episode_ids` to narrow the search; these scopes cannot be combined.

Movie refresh and automatic searches use the same tools with a Radarr source. Existing files are skipped by default. `missing_only:false` explicitly permits profile-governed upgrades. Neither mode changes quality profiles, monitoring or tags, adds media, or performs a manual release grab. Searches refuse unknown or 4K-enabled quality profiles; adding new media still goes through Seerr requests. Whole-series missing searches use Sonarr's native bulk command, which groups seasons and excludes queued episodes. Narrow episode selections use native episode searches.

Grant `source:library-refresh` for metadata refresh and `source:library-search` for automatic searches. Either permits safe command-status reads. Episode listing requires `source:library-view`. These grants are separate from read access and Seerr request permissions; an existing read-only client gains no write access. Global tool switches apply, and the UI Explorer lists maintenance under each permitted source. Maintenance needs the shared MCP/command layer enabled but does not require a requester binding or a compatible Seerr write version.

Episode pages use the configured page limit. Individual episode searches—including narrowed season selections and upgrade searches—use the configured batch limit (20 by default, up to 100), because Sonarr searches each selected episode separately. Whole-series missing searches use native season grouping and the separate **Episodes per automatic search** limit under **Settings → MCP** (1,000 by default, up to 10,000). Select smaller scopes or tune those limits when `search_too_large` is returned. Unmonitored series and searches without eligible items return `nothing_to_search` and an explicit reason without submitting a command. A metadata refresh can still inspect an unmonitored item. Concurrent write admission is per source.

Maintenance writes support stable Sonarr 4 and Radarr 3–6, including compatible patch/minor updates. ArrPi checks the native version before dispatch and verifies that the decoded command receipt retained its requested scope. Unsupported versions leave reads available and reject mutations. There is no older-version fallback that could turn an ignored scoping field into a library-wide refresh.

`refresh_media` and `search_releases` return asynchronous native command receipts. `media_command_status` reads current command state without exposing native messages, paths or command bodies. `completed` means the command finished, not that a suitable release was found, downloaded or imported; inspect episode availability and the queue afterward. Native command records can expire. Do not poll continuously or retry an `unknown_outcome` automatically: inspect current commands/queue/media first. Use `no_cache:true` on library reads after a refresh; episode and command-status reads are always fresh.

## Requests and profiles

Requests go through Seerr with the server-resolved account in X-API-User. Seerr
keeps approval, quotas, requester tags and default services/profiles. Fresh duplicate preflight returns already_available/already_requested without creating a duplicate; TV checks each explicitly requested season. Inputs cannot
select another user, 4K, a raw profile/server/root folder ID, tags or quota bypass. TV requests require
explicit distinct positive seasons and report accepted/skipped seasons. Fresh preflight
checks identity, availability, seasons and quotas. Accepted requests are not completed
downloads. Writes support stable Seerr releases `>=3.5.0, <4.0.0`. The pinned 3.5.0 native
bundle records the tested baseline; compatible patch and minor updates do not
disable requests. Older, prerelease, unknown and new-major versions retain reads
but disable mutations pending adapter validation. Every write still verifies the
account identity and current native permissions.

`request_profiles` lists safe named movie/TV profiles from Seerr's current default non-4K service. Omit `profile` or pass null to keep Default; a real profile named "Default" is still a normal named choice. Single requests and each batch item accept the same optional exact name. Explicit selection requires native REQUEST_ADVANCED, MANAGE_REQUESTS or ADMIN permission, rechecked fresh. Set the associated source's **Seerr server ID (profile selection)** in general Settings; IDs may be zero, are scoped by app kind and cannot be assigned twice. ArrPi never guesses this association. Missing mapping leaves Default available.

For an explicit choice, ArrPi reads the linked Arr instance's full `qualityprofile` definitions fresh and rejects unknown, empty or 4K-enabled profiles. An enabled group enables its member qualities regardless of child flags; a disabled group remains disabled. Seerr and native profile IDs/names must agree. Server/profile IDs go only to Seerr, with requester/root/tag policy preserved. The receipt confirms the selected profile was accepted by Seerr; it does not prove the asynchronous Arr download succeeded. Existing library media is a no-op and returns `profile_applied: false`.

## Library search and links

`search_library` accepts structured `media_type` (movie/tv, optional), `query` over title/plot, any-of `genres`, `available`, `year_from`, `year_to`, `min_rating` (0-10), `sort` (title/rating/newest/recently_added), `offset`, `limit` and `no_cache`. It searches installed media, independently of the requestable Seerr catalog. Existing list tools share its schema and metadata.

```console
arrpi tool search_library --arguments '{"media_type":"movie","genres":["Comedy"],"available":true,"min_rating":7,"sort":"rating","limit":20}'
```

Results include plot, genres, ratings/provider, runtime, certification, IDs, dates, original language and file availability. Each `ratings` entry names its provider and reporting `origin`, normalized 0–10 score, raw value/scale and votes when known. Configure Plex as a source with `ratings-view` to join its community scores (bulk inventory plus full-metadata batches, since the inventory omits additional providers) by external database ID before filtering and pagination. Personal Plex ratings and watch history are excluded. `rating_provider`, `min_votes` and `max_runtime_minutes` select recommendations without treating missing votes or runtimes as qualifying. `rating_sources` and `ratings_complete` report external-source freshness and partial failures; `no_cache` refreshes both native and Plex snapshots. The primary community rating prefers IMDb, then TMDB, Trakt and Sonarr-reported ratings; critic ratings remain separately labeled, including their raw values/scales even when native vote counts are zero. Sonarr does not identify a rating provider in its response, so that score is labeled Sonarr rather than an invented database origin. Series `available` means at least one episode file exists; episode counts, `availability.coverage_scope` and `availability.status` distinguish partial coverage from `monitored_available`, which covers the monitored aired count rather than the entire series. Movie `release_date` is native `inCinemas`, TV uses `firstAired`; digital and physical movie releases have their own fields. Nothing in this normalized output exposes native paths or file objects. `total` counts filtered visible items, `next_offset` is the next page or null, `complete` and source statuses identify failures. Sorting has a stable source/ID tie-breaker. Pages use the configured limit, with no 10,000-entry or offset cutoff. Native APIs return complete arrays, still bounded by `cache.max_body_bytes` (supported up to 64 MiB); increase that setting for large libraries. An oversized/malformed source is marked unavailable, never silently truncated; `upstream_response_too_large` identifies the byte-cap case. Cached pages share the current native snapshot; a forced refresh or source update can change results between pages.

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

Library maintenance history also records the confirmed source, native item ID and
command ID when known. These entries share the existing audit retention and statistics;
no separate database or native credentials are exposed.

## Native metadata limits

Installed movies include audio/subtitle summaries, video codec and file quality when Radarr reports them in its native library snapshot. Sonarr series snapshots do not contain individual episode-file media info; those fields stay absent rather than triggering an episode-by-episode scan. Native language summaries are not inferred from a title's original language.

`request_profiles` reports the configured ordinary and anime defaults as IDs/names, even when the account cannot choose an explicit profile. Submitting `profile:null` still delegates selection to Seerr. Request summaries include native creation/update times and optional titles/release dates where supplied. The pinned Seerr request-list API does not provide titles; ArrPi does not fan out to TMDB for each request. Submit/duplicate receipts reuse title/date metadata already fetched by their preflight.

Seerr 3.5.0 has no reliable ordinary-account endpoint for the last completed full Plex sync. Its [admin-only sync status](https://github.com/seerr-team/seerr/blob/v3.5.0/server/routes/settings/index.ts) exposes progress, and its [scanner library lastScan](https://github.com/seerr-team/seerr/blob/v3.5.0/server/lib/scanners/plex/index.ts) is updated by recent-item scans rather than all full scans. ArrPi therefore does not label either value as a completed full-sync timestamp. Fresh ArrPi reads cannot force Seerr's Plex sync.

Operator history endpoint `GET /v1/config/mcp/audit` accepts `client`, `offset`, `limit`, optional fuzzy `query` (128 characters), exact `tool` and `outcome`, and RFC3339 `since`. Filtering spans all retained entries within one account; pagination stays newest first. Read access requires `config:manage`; this does not expose history to MCP requesters.

Movie details also read Seerr’s IMDb/Rotten Tomatoes endpoint; TV details read its available Rotten Tomatoes ratings. Use `media_details_batch` to compare those with library-query scores. Missing external scores remain absent; failures mark `ratings_complete: false` without losing usable metadata.
