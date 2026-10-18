#!/bin/bash
set -euo pipefail

core_image=${1:-arrpi-core:test}
ui_image=${2:-arrpi-ui:test}
repo_dir=$(cd "$(dirname "$0")/.." && pwd)
core_name="arrpi-core-lifecycle-$$-$RANDOM"
ui_name="arrpi-ui-lifecycle-$$-$RANDOM"
invalid_name="arrpi-ui-invalid-$$-$RANDOM"
client_name="arrpi-client-lifecycle-$$-$RANDOM"

cleanup() {
  docker rm --force "$core_name" "$ui_name" "$invalid_name" "$client_name" > /dev/null 2>&1 || true
}
trap 'cleanup' EXIT

fail() {
  echo "$*" >&2
  exit 1
}

for invalid_origin in http://invalid:fake-origin-secret@example.invalid fake-origin-secret; do
  docker run --detach --name "$invalid_name" --network none --read-only \
    --cap-drop ALL --security-opt no-new-privileges:true --pids-limit 128 \
    --env "ARRPI_CORE_ORIGIN=$invalid_origin" "$ui_image" > /dev/null
  [[ $(timeout 10 docker wait "$invalid_name") == 1 ]] || fail "Invalid core origin did not fail UI startup"
  startup_log=$(docker logs "$invalid_name" 2>&1)
  grep -Fq 'Invalid ArrPi core origin' <<< "$startup_log" || fail "UI startup failed for an unrelated reason"
  if grep -Fq 'fake-origin-secret' <<< "$startup_log"; then
    fail "Invalid core origin leaked its value in logs"
  fi
  docker rm "$invalid_name" > /dev/null
done

# A UI outage must not affect core, and its own health does not require core.
docker run --detach --name "$core_name" --network none --read-only \
  --cap-drop ALL --security-opt no-new-privileges:true --pids-limit 64 \
  --tmpfs /state:size=4m,uid=1000,gid=1000,mode=0700 "$core_image" > /dev/null
docker run --detach --name "$ui_name" --network none --read-only \
  --cap-drop ALL --security-opt no-new-privileges:true --pids-limit 128 \
  --tmpfs /tmp:size=4m,uid=1000,gid=1000,mode=0700 \
  --tmpfs /run/proxy-test:size=1m,uid=1000,gid=1000,mode=0700 \
  --mount "type=bind,src=$repo_dir/web/tests/fixtures/proxy-server.ts,dst=/test/proxy-server.ts,readonly" \
  --env ARRPI_CORE_ORIGIN=http://127.0.0.1:8780 "$ui_image" > /dev/null

for name in "$core_name" "$ui_name"; do
  for ((attempt = 0; attempt < 15; attempt++)); do
    if [[ $name == "$core_name" ]]; then
      if docker exec "$name" /usr/local/bin/arrpi health --url http://127.0.0.1:8780 > /dev/null 2>&1; then
        break
      fi
    elif docker exec "$name" /nodejs/bin/node -e 'fetch("http://127.0.0.1:3000/ui-health").then(r=>{if(!r.ok)process.exit(1)}).catch(()=>process.exit(1))' > /dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  if ((attempt == 15)); then
    echo "Container failed readiness: $name" >&2
    exit 1
  fi
  [[ $(docker inspect --format '{{.Config.User}}' "$name") == 1000:1000 ]] || fail "Unexpected runtime user: $name"
  if docker exec "$name" /bin/sh -c true > /dev/null 2>&1; then
    echo "Runtime unexpectedly includes a shell: $name" >&2
    exit 1
  fi
done

if docker exec "$core_name" /nodejs/bin/node --version > /dev/null 2>&1 \
  || docker exec "$ui_name" /usr/local/bin/arrpi --version > /dev/null 2>&1; then
  echo "Core and UI runtime contents overlap" >&2
  exit 1
fi

docker exec "$ui_name" /nodejs/bin/node -e 'const fs=require("node:fs"); for(const p of ["/state/arrpi.sqlite","/usr/local/bin/npm","/usr/local/bin/pnpm","/usr/local/bin/arrpi","/nodejs/bin/npm","/nodejs/bin/pnpm","/nodejs/bin/corepack","/nodejs/include"]) if(fs.existsSync(p))process.exit(1); if(!fs.existsSync("/etc/ssl/certs/ca-certificates.crt"))process.exit(1)' || fail "UI contents or TLS closure check failed"

# Start a delayed fake core only after proving independent UI readiness.
docker exec --detach "$ui_name" /nodejs/bin/node /test/proxy-server.ts
timeout 5 docker exec "$ui_name" /nodejs/bin/node -e 'const net=require("node:net"); function ready(){const s=net.connect(8780,"127.0.0.1",()=>s.end()); s.on("error",()=>setTimeout(ready,50))} ready()'
# The proxy must not impose Next's former 30-second cutoff on core batches.
timeout 40 docker exec "$ui_name" /nodejs/bin/node -e 'fetch("http://127.0.0.1:3000/health?slow",{signal:AbortSignal.timeout(38000)}).then(async r=>{if(await r.text()!=="slow")process.exit(1)}).catch(()=>process.exit(1))' || fail "UI proxy interrupted an accepted long request"
# A header deadline must not truncate a feed body that streams after it expires.
timeout 30 docker exec "$ui_name" /nodejs/bin/node -e 'fetch("http://127.0.0.1:3000/feeds/"+"A".repeat(43)+"/calendar.rss",{signal:AbortSignal.timeout(28000)}).then(async r=>{if(r.status!==200||await r.text()!=="streamed-feed")process.exit(1)}).catch(()=>process.exit(1))' || fail "UI truncated a streamed feed body"
# Keep the client alive independently when Docker terminates the server's PID namespace.
docker run --rm --name "$client_name" --network "container:$ui_name" --read-only \
  --cap-drop ALL --security-opt no-new-privileges:true --pids-limit 128 \
  --entrypoint /nodejs/bin/node "$ui_image" \
  -e 'fetch("http://127.0.0.1:3000/health",{signal:AbortSignal.timeout(10000)}).then(async r=>{if(!r.ok||await r.text()!=="drained")process.exit(1)}).catch(()=>process.exit(1))' &
drain_pid=$!
# Docker must start the independent client before its request can reach the proxy.
# Poll inside one process; keep the shutdown/draining assertions below unchanged.
timeout 15 docker exec "$ui_name" /nodejs/bin/node -e 'const fs=require("node:fs"); function ready(){if(fs.existsSync("/run/proxy-test/draining"))process.exit(0);setTimeout(ready,50)} ready()' || fail "UI did not forward the drain request"

# PID 1 receives SIGTERM directly. Both services must stop without SIGKILL.
for name in "$ui_name" "$core_name"; do
  started_at=$(date +%s%N)
  docker stop --timeout 10 "$name" > /dev/null
  elapsed_ms=$((($(date +%s%N) - started_at) / 1000000))
  exit_code=$(docker inspect --format '{{.State.ExitCode}}' "$name")
  expected=0
  if [[ $name == "$ui_name" ]]; then
    # Next's own graceful cleanup uses the standard SIGTERM exit status.
    expected=143
    if ((elapsed_ms < 1000)); then
      echo "UI did not drain its in-flight request" >&2
      exit 1
    fi
  fi
  if [[ $exit_code != "$expected" ]]; then
    echo "Container failed graceful shutdown: $name (exit $exit_code)" >&2
    exit 1
  fi
done
wait "$drain_pid" || fail "Client lost the in-flight response during UI shutdown"

# Daemon image bytes, not compressed registry transfers. Keep generous headroom.
core_bytes=$(docker image inspect --format '{{.Size}}' "$core_image")
ui_bytes=$(docker image inspect --format '{{.Size}}' "$ui_image")
if ((core_bytes > 65000000 || ui_bytes > 240000000)); then
  echo "Runtime image budget exceeded: core=$core_bytes ui=$ui_bytes" >&2
  exit 1
fi
echo "Container checks passed: core=$core_bytes bytes, UI=$ui_bytes bytes."
