#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

version=$(jq -r .version web/package.json)
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
tag="v$version"
assets=(arrpi-windows-x86_64.zip arrpi-windows-x86_64.zip.sha256)
for platform in linux-x86_64 linux-aarch64 macos-x86_64 macos-aarch64; do
  archive="arrpi-$platform.tar.gz"
  assets+=("$archive" "$archive.sha256")
done
for asset in "${assets[@]}"; do test -s "$asset"; done
for checksum in ./*.sha256; do sha256sum -c "$checksum"; done

# Immutable releases require every asset to be uploaded before publication.
if gh release view "$tag" --json isDraft --jq .isDraft > /tmp/arrpi-release-draft; then
  if [ "$(cat /tmp/arrpi-release-draft)" = false ]; then
    echo "Retaining published release $tag"
    exit 0
  fi
  gh release upload "$tag" "${assets[@]}" --clobber
else
  gh release create "$tag" "${assets[@]}" --draft --target "$GITHUB_SHA" \
    --title "ArrPi $tag" --generate-notes
fi
gh release edit "$tag" --draft=false
gh release view "$tag" --json url --jq .url >> "$GITHUB_STEP_SUMMARY"
