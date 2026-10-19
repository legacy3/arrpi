#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

version=$(jq -r .version web/package.json)
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
tag="v$version"
assets=(arrpi-windows-x86_64.zip arrpi-windows-x86_64.zip.sha256 docs/images/mcp-demo.mp4)
for asset in "${assets[@]}"; do test -s "$asset"; done
sha256sum -c arrpi-windows-x86_64.zip.sha256

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
