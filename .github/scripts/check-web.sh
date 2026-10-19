#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

echo '::group::Python helpers'
uvx --from black==26.10.1 black --check scripts
uv lock --script scripts/screenshot.py --check
uv lock --script scripts/demo.py --check
echo '::endgroup::'

cd web
echo '::group::Documentation formatting'
pnpm exec prettier --check ../AGENTS.md ../README.md '../docs/**/*.md' ../third_party/native-types/README.md ../.github/workflows/check.yml
pnpm exec prettier --check ../README.md.in --parser markdown
pnpm exec prettier --plugin prettier-plugin-sh --check '../.github/scripts/*.sh'
echo '::endgroup::'
echo '::group::Frontend checks'
pnpm format:check
pnpm lint
pnpm check
pnpm types:check
echo '::endgroup::'
