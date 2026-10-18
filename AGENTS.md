# Repository rules

- Read README.md. Keep API and configuration changes documented.
- Use the pinned Rust toolchain, Rust 2024 and MSRV 1.95. Commit Cargo.lock.
- Keep dependencies acyclic: app → core/adapters/MCP; adapters/MCP → core. Core owns traits and policy; adapters own HTTP and storage.
- The Next.js/TypeScript UI lives in `web/`, uses pnpm, shadcn and `@wowlab/eslint-config`, and runs separately from Rust. Only core mounts SQLite.
- Check the shadcn catalogue before adding interactive controls. Compose its local primitives for app-specific workflows; preserve keyboard behavior and accessible labels.
- Store settings and secrets in SQLite. Append numbered SQL migrations; never edit released migrations. Never log credentials, native URLs, query strings or response bodies.
- Send media requests through the bound Seerr account. Sonarr/Radarr allow only typed metadata-refresh and release-search commands with explicit source grants; never expose arbitrary writes or change profiles, monitoring or tags. Authenticate before cache lookup; include source and query identity in cache keys.
- Bound bodies, caches, requests and concurrency. Keep healthy source results when another source fails.
- Tests use local fixtures and fake credentials. Do not modify owner configuration or real media.
- Format Rust with rustfmt, Python with Black through uv, and supported files with Prettier. Commit Python helper locks.
- Edit README.md.in and regenerate README.md with Markupwright 0.1.3. Edit other Markdown directly. Rulewright 0.1.2 and Markupwright are development tools, not runtime dependencies.
- Run checks appropriate to the change; `.github/workflows/check.yml` defines them. Do not weaken lint rules. Runtime changes must pass both container modes.
- Keep deployment files in `deploy/`, fixtures in `tests/`, benchmarks in their crates and reference docs in `docs/`. Keep state, credentials and raw recordings out of Git.
- Use work branches. Merge or rewrite published history only when the owner asks.
