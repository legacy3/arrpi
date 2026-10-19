#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

docker pull "$IMAGE"
trap 'docker rm --force arrpi-image-smoke >/dev/null 2>&1 || true' EXIT
mounts=(--tmpfs "/tmp:size=32m,mode=1777")
if [ "$TARGET" = core ]; then
  mounts+=(--tmpfs "/state:size=16m,uid=1000,gid=1000,mode=0700")
else
  mounts+=(--tmpfs "/app/web/.next/cache:size=32m,uid=1000,gid=1000,mode=0700")
fi
docker run -d --name arrpi-image-smoke --network none --read-only \
  --cap-drop ALL --security-opt no-new-privileges:true --pids-limit 128 \
  "${mounts[@]}" \
  --health-start-period 0s --health-interval 1s --health-timeout 5s --health-retries 10 "$IMAGE"
for ((attempt = 0; attempt < 30; attempt++)); do
  if [ "$(docker inspect --format '{{.State.Running}}' arrpi-image-smoke)" != true ]; then break; fi
  health=$(docker inspect --format '{{.State.Health.Status}}' arrpi-image-smoke) || break
  if [ "$health" = healthy ]; then exit 0; fi
  if [ "$health" = unhealthy ]; then break; fi
  sleep 1
done
docker logs arrpi-image-smoke
exit 1
