# Native API type bundles

Pinned official schemas and TypeScript models for the applications ArrPi supports:

| Bundle                          | Application release | Native API | Coverage                                             |
| ------------------------------- | ------------------- | ---------- | ---------------------------------------------------- |
| `sonarr-4.0.20.3014-apiv3.d.ts` | Sonarr 4.0.20.3014  | `/api/v3`  | Official OpenAPI, 162 paths                          |
| `radarr-6.4.4.10685-apiv3.d.ts` | Radarr 6.4.4.10685  | `/api/v3`  | Official OpenAPI, 164 paths                          |
| `seerr-3.5.0-apiv1.d.ts`        | Seerr 3.5.0         | `/api/v1`  | Official OpenAPI, 168 paths                          |
| `bazarr-1.6.2-api.d.ts`         | Bazarr 1.6.2        | `/api`     | Official Swagger, 56 paths; upstream badge model     |
| `maintainerr-3.30.0-api.d.ts`   | Maintainerr 3.30.0  | `/api`     | Official OpenAPI, 192 paths; upstream storage models |
| `cleanuparr-2.10.10-apiv2.d.ts` | Cleanuparr 2.10.10  | `/api/v2`  | Official stats interfaces; no full OpenAPI available |

Import `paths`, `components` or `operations` from schema bundles, or the named interfaces from model bundles. These are declarations, not HTTP clients or permission grants. Native specifications include operations that ArrPi intentionally does not expose.

## Provenance and limitations

`manifest.json` records immutable upstream commits, paths, hashes and licenses. Bazarr and Maintainerr publish runtime-generated schemas rather than checked-in specifications. Their captured bytes are pinned to exact container image digests and export endpoints in the manifest. Bazarr's API is unversioned; Maintainerr's schema reports a generic `1.0.0`, independently of its application version.

Those two specifications omit the badge/storage response models ArrPi consumes. Their declarations therefore include the corresponding official TypeScript interfaces, extracted with the TypeScript compiler from separately pinned upstream files. Cleanuparr supplies its official stats interfaces directly. Missing upstream response schemas are not invented.

SABnzbd has no bundled official OpenAPI. Its consumed queue response is typed in `crates/arrpi-adapters/src/monitoring/projection/native.rs`, following [the native API documentation](https://sabnzbd.org/wiki/advanced/api) and [SABnzbd 5.1.3's API implementation](https://github.com/sabnzbd/sabnzbd/blob/a647f73426cb70ad60ae7964b96c2d5e939b4727/sabnzbd/api.py). This is a consumed contract, not a complete native SDK.

Fixtures under `web/tests/fixtures/native/` are shared by Rust adapter tests and the fake HTTP services. TypeScript checks them against the pinned upstream interfaces where supplied; browser integration tests validate normalized responses against Rust's generated public schema.

Sonarr v4 types must not be treated as exact Sonarr v3 types. Release bundles describe their pinned upstream contracts, not every older or newer installation.

## Regenerate and verify

```console
cd web
pnpm install --frozen-lockfile
pnpm types
pnpm types:check
```

Generation and drift checks are offline. `pnpm types:fetch` redownloads immutable checked-in upstream sources and verifies hashes before writing. It retains and hash-checks runtime captures. To capture a new runtime schema, start the manifest's pinned image in an isolated test environment, wait for readiness, and GET its recorded loopback export endpoint. Do not use an authenticated owner's state as a fixture.

For a new application release, add a distinct manifest entry and source file rather than overwriting an older bundle. Record the exact commit/image digest and SHA-256, preserve its license, regenerate, and run drift/type and adapter tests. Do not format or hand-edit pinned sources or generated declarations.

## Licensing

Sonarr, Radarr, Bazarr and Cleanuparr bundles retain GPL-3.0 attribution; Seerr and Maintainerr retain MIT attribution. See the corresponding `LICENSE.*` files and manifest. ArrPi's authored Rust code is MIT. Native schema/type bundles are not compiled into the Rust executable.
