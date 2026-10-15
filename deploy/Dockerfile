# syntax=docker/dockerfile:1
FROM rust:1.96.1-slim-bookworm@sha256:e18a79fc84dfcfc3ab5ba72290398a644c135c97eaa881447fddc354ee4701a3 AS core-build
RUN apt-get update && apt-get install -y --no-install-recommends build-essential pkg-config && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml clippy.toml ./
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/usr/local/cargo/git --mount=type=cache,target=/build/target cargo build --locked --release -p arrpi && cp target/release/arrpi /build/arrpi
RUN mkdir /state && chown 1000:1000 /state && chmod 700 /state \
    && ldd /build/arrpi > /tmp/arrpi-linkage \
    && ! grep -q 'not found' /tmp/arrpi-linkage \
    && awk '$2 == "=>" {print $1}' /tmp/arrpi-linkage | sort > /tmp/arrpi-libraries \
    && printf 'libc.so.6\nlibgcc_s.so.1\nlibm.so.6\n' | cmp - /tmp/arrpi-libraries

FROM node:24.22.0-bookworm-slim@sha256:a5f4366e834695d5f363b920c104953a535d3a9864d1bdf74566a0ebe01daee5 AS web-deps
RUN npm install --global --ignore-scripts pnpm@11.17.0
WORKDIR /build/web
COPY web/package.json web/pnpm-lock.yaml ./
RUN --mount=type=cache,target=/root/.local/share/pnpm/store pnpm install --frozen-lockfile

FROM web-deps AS web-build
ENV NEXT_TELEMETRY_DISABLED=1
COPY web ./
RUN pnpm build

FROM node:24.22.0-bookworm-slim@sha256:a5f4366e834695d5f363b920c104953a535d3a9864d1bdf74566a0ebe01daee5 AS fixtures
WORKDIR /fixtures
COPY web/tests/fixtures ./
USER node
HEALTHCHECK --interval=2s --timeout=3s --retries=10 CMD ["node", "-e", "Promise.all([8989,7878,5055].map(port=>fetch('http://127.0.0.1:'+port+'/api/'+(port===5055?'v1/status':'v3/health'),{headers:{'X-Api-Key':'fake-upstream-api-key'}}).then(r=>{if(!r.ok)throw Error()}))).catch(()=>process.exit(1))"]
CMD ["node", "server.ts"]

FROM gcr.io/distroless/nodejs24-debian13:nonroot@sha256:59c479c9b8c123e66047315c968aa2d35e31ae5149c856e4110a79c8b3563f0b AS ui-runtime

# Preserve the complete official native/TLS closure, dropping only Node symbols.
# Copy the resulting filesystem to a fresh layer so the original binary is absent.
FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587 AS ui-trim
RUN apt-get update && apt-get install -y --no-install-recommends binutils && rm -rf /var/lib/apt/lists/*
COPY --from=ui-runtime / /runtime
RUN strip --strip-unneeded /runtime/nodejs/bin/node \
    && readelf -SW /runtime/nodejs/bin/node > /tmp/node-sections \
    && ! grep -Eq '[.]debug_|[.]symtab' /tmp/node-sections

FROM scratch AS ui
LABEL org.opencontainers.image.source="https://github.com/legacy3/arrpi" org.opencontainers.image.licenses="MIT"
COPY --from=ui-trim /runtime /
ENV NODE_ENV=production NEXT_TELEMETRY_DISABLED=1 PORT=3000 HOSTNAME=0.0.0.0 ARRPI_CORE_ORIGIN=http://arrpi:8780
WORKDIR /app/web
COPY --from=web-build /build/web/.next/standalone ./
COPY --from=web-build /build/web/.next/static ./.next/static
COPY --from=web-build /build/web/public ./public
COPY web/runtime ./runtime
USER 1000:1000
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 CMD ["/nodejs/bin/node", "-e", "fetch('http://127.0.0.1:'+Number(process.env.PORT??3000)+'/ui-health').then(r=>{if(!r.ok)process.exit(1)}).catch(()=>process.exit(1))"]
ENTRYPOINT ["/nodejs/bin/node"]
CMD ["runtime/start.ts"]

# Last/default target builds only Rust; no frontend dependency installation.
FROM gcr.io/distroless/cc-debian13:nonroot@sha256:e792ab3d241a468a4fd7519ddbbebe66b49b5f365771716ea688ad40b6c6f1c2 AS core
LABEL org.opencontainers.image.source="https://github.com/legacy3/arrpi" org.opencontainers.image.licenses="MIT"
COPY --from=core-build /build/arrpi /usr/local/bin/arrpi
COPY --from=core-build --chown=1000:1000 /state /state
ENV ARRPI_LISTEN=0.0.0.0:8780
USER 1000:1000
EXPOSE 8780
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 CMD ["/usr/local/bin/arrpi", "health", "--url", "http://127.0.0.1:8780"]
ENTRYPOINT ["/usr/local/bin/arrpi"]
CMD ["serve", "--database", "/state/arrpi.sqlite"]
