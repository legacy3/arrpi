#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

for target in core ui; do
  image="ghcr.io/${GITHUB_REPOSITORY_OWNER,,}/arrpi-$target"
  reference="$image:sha-$GITHUB_SHA"
  if docker buildx imagetools inspect "$reference" > /dev/null 2> /tmp/arrpi-existing-manifest.err; then
    echo "Retaining existing commit manifest: $reference"
    continue
  fi
  if ! grep -Fq -- "$reference: not found" /tmp/arrpi-existing-manifest.err && ! grep -Fq 'manifest unknown' /tmp/arrpi-existing-manifest.err; then
    cat /tmp/arrpi-existing-manifest.err
    exit 1
  fi
  sources=()
  for file in /tmp/digests/digests-"$target"-*/*; do
    digest="${file##*/}"
    [[ "$digest" =~ ^[0-9a-f]{64}$ ]] || exit 1
    sources+=("$image@sha256:$digest")
  done
  test "${#sources[@]}" = 2
  docker buildx imagetools create -t "$reference" \
    --annotation "index:org.opencontainers.image.source=https://github.com/$GITHUB_REPOSITORY" \
    --annotation "index:org.opencontainers.image.revision=$GITHUB_SHA" "${sources[@]}"
  docker buildx imagetools inspect "$reference"
done
# Commit tags are retained; an old re-run must not roll latest backwards.
current=$(gh api "repos/$GITHUB_REPOSITORY/git/ref/heads/main" --jq .object.sha)
if [ "$current" = "$GITHUB_SHA" ]; then
  for target in core ui; do
    image="ghcr.io/${GITHUB_REPOSITORY_OWNER,,}/arrpi-$target"
    promoted=false
    for ((attempt = 0; attempt < 3; attempt++)); do
      if docker buildx imagetools create -t "$image:latest" "$image:sha-$GITHUB_SHA"; then
        promoted=true
        break
      fi
      sleep 2
    done
    test "$promoted" = true
  done
fi
echo "Published core/UI commit manifests for $GITHUB_SHA" >> "$GITHUB_STEP_SUMMARY"
