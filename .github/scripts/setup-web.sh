#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

cd web
npm install --global pnpm@11.17.0
pnpm install --frozen-lockfile
pnpm exec playwright install --with-deps chromium
