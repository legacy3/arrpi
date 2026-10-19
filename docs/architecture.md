# Architecture

ArrPi has four Rust crates and an optional Next.js frontend. Dependency direction is acyclic: the application depends on core, adapters and MCP; adapters and MCP depend on core.

| Location                | Owns                                                                                                                 |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `crates/arrpi-core`     | Validated configuration, permissions, source capabilities, command use cases, public contracts and library selection |
| `crates/arrpi-adapters` | Native media/monitoring sources, bounded HTTP/cache, OAuth verification, SQLite and remote ArrPi clients             |
| `crates/arrpi-mcp`      | rmcp HTTP/stdio transport and SDK tool presentation                                                                  |
| `crates/arrpi`          | CLI, HTTP authentication/routes, settings application and startup                                                    |
| `web`                   | Next.js App Router UI, runtime core proxy and generated TypeScript consumers                                         |

## Sources and commands

`ApiSource` advertises optional native, library, community ratings, calendar, summary, catalog, account, request and issue capabilities. A provider implements only the capabilities it supports. Source registration is independent of command orchestration; capability tests use fake providers to exercise these interfaces independently of native apps.

Core uses semantic `ActorRights`. The account adapter resolves the bound native account and translates its permissions. Local grants and capability availability are checked before identity I/O. Native account rights remain authoritative and are read fresh.

The typed command registry declares names, arguments, effects and configurable schemas once per settings revision. HTTP, CLI and MCP share command execution through `CommandExecutor`. Core owns operation effects, while `arrpi-mcp` projects them into SDK schemas and annotations. `EventSink` receives typed, bounded observations without making core depend on SQLite or a metrics transport.

To add a provider, implement the relevant capability traits in adapters, register its descriptor during application composition and add fake-upstream contract tests. Add native endpoint allowlists and version checks at the adapter boundary. Preserve source identity and freshness beside data. Change shared command dispatch only when adding an operation, not when adding another implementation of an existing capability.

## Reads and memory

Native JSON stays native. A shared HTTP implementation limits status/body collection, disables redirects and automatic retries, and classifies failures according to read versus mutation intent. Native reads use per-source caches with canonical query identity, coalesced refreshes and explicit stale-if-error behavior.

Body reservations track backing allocations, not only visible slice lengths. Eviction cannot release a reservation while a response consumer still holds the bytes. Maintenance prunes expired entries; pressure reclaims idle fresh entries in bounded global LRU order. Active refreshes remain protected. Resource pools belong to settings revisions, so draining requests can retain an older pool after an apply. These limits are not an absolute process RSS ceiling.

Library selection parses borrowed native data, keeps the requested top-K page and projects only selected items. Optional `RatingsSource` snapshots join community metadata by TMDB/type, TVDB or IMDb before filtering and sorting. Plex pages the inventory and reads full metadata in matching batches of at most 100 IDs into one sanitized cached snapshot; it never projects personal scores or playback history. Library reads do not fan out into per-title catalog requests. Seerr detail batches retrieve its additional IMDb/Rotten Tomatoes scores separately. A bounded CPU pool admits library/calendar and operational summary normalization after network reads. Queue records use an ArrayVec with a 100-row deserialization cap; traversal keeps at most twenty pages of provenance and bounded deduplication state. Healthy sources survive another source's failure; incomplete data is explicit. [Benchmarking](performance.md) covers allocation tests and runtime profiling.

## Mutations and identity

Media requests and issue mutations go through Seerr as the server-resolved account. Inputs cannot choose another actor, request 4K, inject native IDs for request routing, replace tags or bypass quotas. Default services, profiles, requester tags and approvals remain Seerr policy.

Existing library maintenance is a separate `MaintenanceSource` capability. Explicit per-source grants authorize typed metadata refresh and automatic release searches; the native passthrough stays read-only. Maintenance never adds media or changes profiles, monitoring or tags. Search preflight checks fresh native state and a non-4K profile. TV searches exclude unmonitored and unaired episodes and default to missing files. Only fixed command names and typed IDs reach the native command endpoint. Admission and response reservations precede dispatch; ambiguous transport, server or receipt failures remain `unknown_outcome` with no retry. Episode/progress reads are uncached, and command receipts never imply completed downloads. Native maintenance does not impersonate a Seerr user or depend on its write-version gate; account requests retain those checks.

Write admission is bounded. After waiting, adapters recheck native identity, permissions, request state and selected profile/service safety before dispatch. An ambiguous dispatched mutation returns `unknown_outcome`; automatic retries are disabled. Activity history is best effort, not an exactly-once journal. [MCP](mcp.md) documents the public workflow and uncertainty contract.

Optional [HTTP OAuth](oauth.md) verifies externally issued, resource-bound access tokens before resolving an exact issuer/subject binding to an existing requester. Verified credentials stay in trusted request extensions; claim roles and email addresses never select grants. The adapter owns library-backed discovery/JWKS validation, while the application owns HTTP admission and runtime maintenance.

Feed capabilities are separate from account/admin credentials. They permit only scoped RSS/iCalendar reads, remain valid without browser SSO, and are revocable through a trusted issuer. HMAC-SHA256 and a persistent namespace prevent old URLs from resurrecting after registry loss. [API and feeds](api.md) describes those routes.

## Configuration and UI

One SQLite file stores settings, secrets, account bindings, feed registrations, statistics and write history. A validated immutable runtime is built from that revision; SQLite is the authoritative configuration store. Core alone mounts state.

Next.js serves the UI in its own optional image; Rust serves API/CLI only. A runtime `ARRPI_CORE_ORIGIN` lets one UI image target different core deployments. Shared dashboard layout and a per-session TanStack Query client preserve navigation and isolate data across accounts. Ahooks supplies the elapsed timer; local React state handles form interaction. Secrets stay in memory or behind an authenticated proxy, never browser persistent storage.

An SSO proxy must preserve Next's RSC/router headers for client navigation while filtering credentials and enforcing exact-origin settings writes. Dropping those headers makes Next return HTML to navigation fetches and forces document reloads.

## Contracts and extension rules

Owned Rust DTOs and numeric limits generate the UI contracts:

```sh
cargo run --locked -q -p arrpi-core --bin export-schema > docs/api/public.schema.json
pnpm --dir web types
pnpm --dir web types:check
```

Exact native version bundles remain separately licensed under `third_party/native-types`; they are not relabeled as another app version.

Extension tests should cover authorization before caching, native response compatibility, body lifetime and cancellation. Mutating adapters must distinguish before-dispatch failures from ambiguous dispatched writes and verify account rights again after queue admission.

Container-visible disk APIs do not establish physical filesystem identity. ArrPi does not deduplicate disks by equal capacity or decide what media to delete. Queue summaries report page/item budgets and explicit completeness; collection counters preserve their native coverage and fallback flags.
