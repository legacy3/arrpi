#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

case "${ARCH:?}" in
  amd64) export ARRPI_PLATFORM=linux-x86_64 ;;
  arm64) export ARRPI_PLATFORM=linux-aarch64 ;;
  *) exit 1 ;;
esac
mkdir -p dist
container=$(docker create "$IMAGE")
trap 'docker rm "$container" >/dev/null' EXIT
docker cp "$container:/usr/local/bin/arrpi" dist/arrpi
export ARRPI_BINARY=dist/arrpi
bash .github/scripts/package-unix.sh
