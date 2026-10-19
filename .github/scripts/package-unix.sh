#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

case "$ARRPI_PLATFORM" in
  linux-x86_64 | linux-aarch64 | macos-x86_64 | macos-aarch64) ;;
  *)
    echo 'Unsupported native release platform' >&2
    exit 1
    ;;
esac
bash .github/scripts/smoke-binary.sh "$ARRPI_BINARY"
file "$ARRPI_BINARY"
stage=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/arrpi-package.XXXXXX")
trap 'rm -rf "$stage"' EXIT
cp "$ARRPI_BINARY" "$stage/arrpi"
chmod 755 "$stage/arrpi"
cp LICENSE "$stage/LICENSE"
cp docs/binaries.md "$stage/README.md"
mkdir -p dist
archive="dist/arrpi-$ARRPI_PLATFORM.tar.gz"
tar -czf "$archive" -C "$stage" arrpi LICENSE README.md
if command -v sha256sum > /dev/null; then
  (cd dist && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
else
  (cd dist && shasum -a 256 "$(basename "$archive")" > "$(basename "$archive").sha256")
fi
