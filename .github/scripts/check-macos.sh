#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

case "${ARRPI_TARGET:?}" in
  x86_64-apple-darwin) export ARRPI_PLATFORM=macos-x86_64 ;;
  aarch64-apple-darwin) export ARRPI_PLATFORM=macos-aarch64 ;;
  *) exit 1 ;;
esac
cargo build --release --locked -p arrpi --target "$ARRPI_TARGET"
export ARRPI_BINARY="target/$ARRPI_TARGET/release/arrpi"
bash .github/scripts/package-unix.sh
