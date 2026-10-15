# Native API type bundles

These complete official schemas and generated TypeScript declarations are pinned to
application releases, not just the API's own `3.0.0` label:

| Bundle                          | Application release | Native API | Paths | Schema source                                                                                                                           |
| ------------------------------- | ------------------- | ---------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `sonarr-4.0.20.3014-apiv3.d.ts` | Sonarr 4.0.20.3014  | `/api/v3`  | 162   | [Pinned upstream schema](https://github.com/Sonarr/Sonarr/blob/cab419ade8ac7fcab5bf80394ee492abd35d5f5a/src/Sonarr.Api.V3/openapi.json) |
| `radarr-6.4.4.10685-apiv3.d.ts` | Radarr 6.4.4.10685  | `/api/v3`  | 164   | [Pinned upstream schema](https://github.com/Radarr/Radarr/blob/a96bf7c3ecb23cb76993770727285dc17a613fbc/src/Radarr.Api.V3/openapi.json) |
| `seerr-3.5.0-apiv1.d.ts`        | Seerr 3.5.0         | `/api/v1`  | 168   | [Pinned upstream schema](https://github.com/seerr-team/seerr/blob/e2f24cb46079746936516c723b09820360f95113/seerr-api.yml)               |

Import their `paths`, `components` or `operations` types directly. They are declarations,
not HTTP clients or permission grants. The specifications contain mutations and
configuration operations that ArrPi intentionally does not expose.

Sonarr/Radarr schemas and declarations retain GPL-3.0 attribution; Seerr retains MIT attribution. See `LICENSE.Sonarr`, `LICENSE.Radarr`, `LICENSE.Seerr` and `manifest.json`.
ArrPi's authored Rust code uses the repository's MIT license. Native schema/type bundles retain their upstream licenses and are not compiled into the Rust executable.

## Regenerate and verify

The frontend's locked tooling provides the generator:

```console
cd web
pnpm install --frozen-lockfile
pnpm types
pnpm types:check
```

Normal generation and drift checks are offline. `pnpm types:fetch` deliberately
redownloads the immutable sources and rejects a hash mismatch before writing them.
The manifest records exact commits, source paths, generator version and SHA-256 hashes.
Do not format or hand-edit schemas/declarations: schema bytes are hashed and
declarations must match the pinned generator's output.

For a new app release, add a distinct manifest entry and schema rather than overwriting
an older version bundle. Review upstream differences, preserve its license, regenerate,
and run drift/type checks. OpenAPI completeness follows the official specification;
missing or inaccurate upstream response schemas are not invented by ArrPi.

An exact Sonarr v3 bundle is not provided. The Sonarr v4 bundle includes v4-specific functionality and must not be treated as exact v3 typing.
