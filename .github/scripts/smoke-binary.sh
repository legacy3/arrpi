#!/usr/bin/env bash
set -euo pipefail

binary=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
state=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/arrpi-native.XXXXXX")
server_pid=
cleanup() {
  if [ -n "$server_pid" ]; then
    kill "$server_pid" 2> /dev/null || true
    wait "$server_pid" 2> /dev/null || true
  fi
  rm -rf "$state"
}
trap cleanup EXIT

"$binary" --version
database="$state/state with spaces/arrpi.sqlite"
origin=http://127.0.0.1:18780
"$binary" serve --listen 127.0.0.1:18780 --database "$database" > "$state/stdout" 2> "$state/stderr" &
server_pid=$!
ready=false
for ((attempt = 0; attempt < 30; attempt++)); do
  if ! kill -0 "$server_pid" 2> /dev/null; then break; fi
  if curl -fsS --max-time 1 "$origin/health" > /dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 0.5
done
if [ "$ready" != true ]; then
  cat "$state/stderr" >&2
  exit 1
fi
key=$("$binary" config key --database "$database")
curl -fsS --max-time 5 -H "Authorization: Bearer $key" "$origin/v1/status" \
  | python3 -c 'import json,sys; assert json.load(sys.stdin)["sources"] == []'
"$binary" health --url "$origin"
test -s "$database"
