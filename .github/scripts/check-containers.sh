#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

export ARRPI_PREVIEW_BIND="${ARRPI_PREVIEW_BIND:-127.0.0.2}"
export ARRPI_CORE_IMAGE=arrpi-core:test
export ARRPI_UI_IMAGE=arrpi-ui:test

cleanup() {
  local arrpi_result=$?
  if ((arrpi_result != 0)); then
    docker compose -p arrpi-test -f deploy/compose.test.yaml logs || true
    docker compose -p arrpi-smoke -f deploy/compose.yaml -f deploy/compose.web.yaml logs || true
  fi
  docker compose -p arrpi-test -f deploy/compose.test.yaml down --volumes || true
  docker compose -p arrpi-smoke -f deploy/compose.yaml -f deploy/compose.web.yaml down --volumes || true
  return "$arrpi_result"
}
trap cleanup EXIT

echo '::group::Validate deployment manifests'
docker compose -f deploy/compose.yaml config --quiet
docker compose -f deploy/compose.yaml -f deploy/compose.web.yaml config --quiet
docker compose -f deploy/compose.test.yaml config --quiet
docker compose -f deploy/compose.test.yaml -f deploy/compose.preview.yaml config --quiet
echo '::endgroup::'

echo '::group::Build and start images'
docker compose -p arrpi-test -f deploy/compose.test.yaml down --volumes --timeout 10
docker compose -p arrpi-test -f deploy/compose.test.yaml up --build -d --wait
echo '::endgroup::'

echo '::group::Identical UI artifact at independent core origins'
primary=$(docker compose -p arrpi-test -f deploy/compose.test.yaml ps -q arrpi-ui)
secondary=$(docker compose -p arrpi-test -f deploy/compose.test.yaml ps -q ui-secondary)
test "$(docker inspect --format '{{.Image}}' "$primary")" = "$(docker inspect --format '{{.Image}}' "$secondary")"
echo '::endgroup::'

echo '::group::Browser and proxy integration tests'
(
  cd web
  pnpm test
)
echo '::endgroup::'

echo '::group::Container CLI'
docker compose -p arrpi-test -f deploy/compose.test.yaml exec -T arrpi arrpi health --url http://arrpi-ui:3000
docker compose -p arrpi-test -f deploy/compose.test.yaml exec -T -e ARRPI_TEST_CLIENT_KEY=fake-browser-key-00000000000000000000 arrpi arrpi get sonarr series --url http://arrpi-ui:3000 --key-env ARRPI_TEST_CLIENT_KEY --no-cache --metadata
docker compose -p arrpi-test -f deploy/compose.test.yaml exec -T arrpi arrpi health --url http://127.0.0.1:8780
echo '::endgroup::'

echo '::group::Container test harness lint'
curl -fsSL https://github.com/koalaman/shellcheck/releases/download/v0.11.0/shellcheck-v0.11.0.linux.x86_64.tar.xz -o /tmp/shellcheck.tar.xz
printf '%s  %s\n' 8c3be12b05d5c177a04c29e3c78ce89ac86f1595681cab149b65b97c4e227198 /tmp/shellcheck.tar.xz | sha256sum -c
tar -xJf /tmp/shellcheck.tar.xz -C /tmp
/tmp/shellcheck-v0.11.0/shellcheck .github/scripts/*.sh
echo '::endgroup::'

echo '::group::Runtime contents, image budgets and lifecycle'
bash .github/scripts/test-runtime.sh
echo '::endgroup::'

echo '::group::Standalone and optional UI smoke'
docker compose -p arrpi-smoke -f deploy/compose.yaml up --no-build --pull never -d --wait
curl -fsS http://127.0.0.1:8780/health
docker compose -p arrpi-smoke -f deploy/compose.yaml -f deploy/compose.web.yaml up --no-build --pull never -d --wait
curl -fsS http://127.0.0.1:3000/ui-health
curl -fsS http://127.0.0.1:3000/health
test "$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:3000/v1/status)" = 401
echo '::endgroup::'
