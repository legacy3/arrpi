# Configuration and operations

## First startup

First startup creates `/state/arrpi.sqlite` in the private persistent state volume.
This one database contains settings, upstream API keys, client credentials, the
feed master key, subscription registry and aggregate tool statistics. SQLite is the authoritative settings store. The web process receives no native credentials. The
JSON files under `tests/` contain fake credentials and serve only as test fixtures.

The Compose quick start prints the generated admin key with `docker compose -f deploy/compose.yaml exec arrpi config key`. Connect the UI with that key, then add your upstream connections in **Settings**. Native API keys belong in those connections; dashboards and agents use separate ArrPi client keys with selected permissions.

For a source build, `ARRPI_DATABASE` (or `--database`) selects the SQLite file for `serve`, `config key` and `config import`. It defaults to `/state/arrpi.sqlite`; use a writable local path outside Docker. `ARRPI_LISTEN` (or `serve --listen`) overrides the saved listener address. The core image sets `ARRPI_LISTEN=0.0.0.0:8780` for container networking; published Compose ports remain loopback-only.

## Editing settings

The UI separates **Connections** (`/settings`), **Access** (`/settings/access`), **Feeds** (`/settings/feeds`) **Server** (`/settings/server`) and **OAuth** (`/settings/oauth`). Connections and clients have individual editors; each save updates that item or section while preserving the rest of the document. Blank credential fields keep existing keys. Source removal is blocked while clients still reference it; edit their permissions in Access first.

[External OAuth](oauth.md) has a separate settings page for its authorization server and account bindings. It is disabled by default.

Configuration clients (`admin` or `config:manage`) must remain unbound. Use a
separate requester client for Seerr actions.

UI and CLI edits use the same admin API. `GET /v1/config` returns a revision and
redacted settings; `PUT /v1/config` validates the complete document, preserves null
secret fields by source/client identity, and rejects an outdated revision with 409.
Source URLs/keys, client permissions/keys, feed settings and cache policy apply
immediately. Listener address and connection-lifetime/count changes require a
restart. Docker's fixed listen override controls container networking.

```console
export ARRPI_API_KEY='<your admin client key>'
arrpi config get --url http://127.0.0.1:8780 --pretty > arrpi-import.json
# Edit the revisioned document. Null credentials retain the existing value.
arrpi config set --url http://127.0.0.1:8780 --file arrpi-import.json
```

`config key --database /state/arrpi.sqlite --name admin` reads a stored client key
for local setup/recovery. An offline `config import --database ... --file ...`
accepts one JSON settings document with concrete credentials; stop the server
before importing. It can replace invalid stored settings too. The import file is
used once, then discarded; it is not a runtime configuration source.

## Backups

ArrPi applies versioned SQLite migrations automatically when opening its database. Schema changes are transactional; failed migrations roll back. Downgrades require restoring a database backup with its matching image version.

Back up **`/state/arrpi.sqlite`** and the image/source revision. Stop ArrPi before
copying its database; restore UID 1000 ownership and mode 0600 in a private state
directory. Nothing else is needed to restore ArrPi settings, credentials or feed
URLs. Cache and build/test output are disposable. Restoring an older snapshot
restores its old revocation state too; rotate the feed master if those revoked
links must stay invalid.

## Remote access

Compose publishes core on `127.0.0.1:8780` and the optional UI on `127.0.0.1:3000`. A reverse proxy running on the same host can reach those addresses directly. Create a TLS site for your domain and forward all paths to **`http://127.0.0.1:3000`** for the UI deployment, or **`http://127.0.0.1:8780`** for core alone. Preserve the request path, query, Authorization and Origin headers, disable response buffering for streamed responses and allow up to 660 seconds for long-running batches. The UI forwards API requests to core.

A proxy in another container cannot reach the host's loopback ports. Attach it to the same Docker network as ArrPi and use `http://arrpi-ui:3000` or `http://arrpi:8780` as the upstream instead. Keep the database and internal service ports private; public access goes through the TLS site.

Set **Settings → MCP → Public endpoint** to `https://arrpi.example.com/v1/mcp`, using your domain. This adds the exact public Host and Origin to MCP's allowlists. Keep proxy Host handling consistent with that public authority. The optional UI overlay separately allows its internal core authority through `ARRPI_MCP_PROXY_HOST`; this is not a wildcard host exception.

ArrPi requires bearer credentials for API access. If you also use browser SSO, configure route-specific authentication:

| Route                                          | Authentication                                           |
| ---------------------------------------------- | -------------------------------------------------------- |
| UI and admin routes                            | Browser SSO, or ArrPi's manual client-key connection     |
| `/v1/mcp`                                      | Account bearer token; bypass browser redirects           |
| `GET /v1/tools` and `POST /v1/tools/{tool}`    | Account bearer token when exposed to remote CLI clients  |
| `/feeds/{token}/rss` and `/feeds/{token}/ical` | Personal feed token in the URL; bypass browser redirects |

Do not replace account bearer credentials with an admin key on token-authenticated routes. [API and feeds](api.md) and [MCP](mcp.md) describe their permissions and revocation.

### Automatic browser sign-in

The standalone web UI accepts a scoped client key. An SSO reverse proxy can provide
automatic sign-in on protected browser routes: authenticate and authorize each request, strip
incoming authorization and identity headers, inject an ArrPi bearer credential
server-side, and set `X-ArrPi-Web-Auth: proxy`. This header only selects the UI's
automatic connection screen; it grants no API access. Never expose the injected key
in HTML, browser storage or frontend configuration. Cookie-based proxies must check
the exact trusted Origin on PUT settings requests and allowlisted POST command
paths, and reject other writes. Select the signed-in user's requester credential
for command routes while retaining the admin credential for configuration.
Derive that mapping from verified identity, and leave unknown users unbound.

## Container runtime

The UI reads the nonsecret `ARRPI_CORE_ORIGIN` at startup, so the same image can
connect to different core deployments without rebuilding. `deploy/compose.web.yaml`
uses `ARRPI_CORE_AUTHORITY` to keep that destination and core's exact allowed MCP
proxy Host aligned. Core still checks bearer credentials and MCP Origins. Both
services run with read-only filesystems, dropped capabilities and no privilege
escalation; the UI has no database mount, Rust binary, shell or package manager.

## Limits and outages

Defaults limit each source to 32 cache entries of at most 1 MiB each and four
upstream requests. Upstream requests time out after ten seconds; admission waits
at most 250 ms. The server admits 64 API response bodies and 64 accepted connections.
A shared `cache.max_retained_bytes` budget defaults to 128 MiB. Reservations follow
the native body backing allocation through cache eviction, slices and outgoing
responses, and release after its final reference drops. Expired bodies are pruned
every five seconds and on allocation pressure. `cache.max_normalizations` defaults
to four concurrent library/calendar projections. These settings also apply through
the UI; cache entry count alone cannot bypass the shared byte limit.
Pressure also evicts idle fresh entries in global LRU order; active refreshes and
outgoing body leases stay protected. Resource pools belong to an immutable settings
revision. Requests draining after a configuration apply can retain the previous
revision's pool alongside the replacement; this is not a process-wide RSS ceiling.

Request leases last until the body is dropped. Connections have a 30-second idle
deadline and 300-second maximum lifetime; Rust supports HTTP/1 only.
Concurrent reads of one canonical query share its refresh. Unused cache entries are
evicted; in-flight entries remain protected. Exhausted request admission returns
429; exhausted source/cache capacity returns 503 with `Retry-After`.

Successful data stays fresh for 60 seconds. Transport errors, timeouts and native 500/502/503/504
can reuse the last success for another 900 seconds, with a five-second failure backoff.
Authentication rejection, redirects, oversized responses and malformed JSON never
fall back to old data. Restarting drops all cached data. These are cache and fetch
bounds, not a measured whole-process memory guarantee for Rust plus Next.

Calendar parsing accepts at most 4096 native items per source and 8192 output events
in total. Overlarge source results fail explicitly. Dates are not a filesystem-space
or retention signal; ArrPi never deletes media.

## Discover existing services

Discovery is a local CLI operation, not a server-side network-scanning API:

```sh
arrpi discover --ip 192.0.2.10
arrpi discover --ip 2001:db8::10 --https
arrpi discover --docker-network media
```

An explicit IP or one named Docker network is required; CIDR ranges and implicit network scans are not accepted. Docker inspection has a five-second timeout and a 256 KiB response cap. Docker discovery requires Docker CLI/socket access on the machine running the command; the runtime images do not mount the Docker socket or include that CLI. For container networks run the command from a host or container that can route to the reported IPs.

Probes use common ports for Sonarr, Radarr, Prowlarr, SABnzbd, Seerr, Bazarr, Maintainerr, Cleanuparr, Plex, Tautulli and ArrPi. At most 64 addresses and eight concurrent requests are permitted, with two-second request and thirty-second overall deadlines. The report includes every completed probe and marks deadline truncation incomplete with a failing exit status. TLS probes use normal certificate validation.

No credentials, cookies, redirects or mutations are used. A listening port is only a hint: `expected` comes from the port convention, while `detected` requires an identifying native response. An authentication challenge or generic liveness response stays unidentified. Discovery neither guesses API keys nor saves connections; inspect the report and add verified services through Settings.
