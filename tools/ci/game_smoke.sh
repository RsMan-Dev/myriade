#!/usr/bin/env bash
# Start the built dedicated server with private files, never in a player's game folder, then stop it cleanly.
set -euo pipefail
home=$(mktemp -d "$RUNNER_TEMP/horizon-smoke.XXXXXX")
version=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' research-lock.json | head -1)
server="$(cd "$BIN" && pwd)/horizon-client"
resources="$PWD/.local/horizon-client/$version"
port="${SMOKE_PORT:-25599}"
log="$home/server.log"
cd "$home"
coproc SERVER { HORIZON_HOME="$home" XDG_DATA_HOME="$home/data" XDG_CONFIG_HOME="$home/config" \
  "$server" --resources "$resources" --dedicated --nogui --port "$port" --universe "$home/universe" 2>&1; }
cat <&"${SERVER[0]}" > "$log" &
reader=$!
deadline=$((SECONDS + 180))
until grep -q "Done" "$log" 2>/dev/null; do
  if (( SECONDS > deadline )) || ! kill -0 "$SERVER_PID" 2>/dev/null; then
    tail -40 "$log"
    exit 1
  fi
  sleep 1
done
echo stop >&"${SERVER[1]}"
wait "$SERVER_PID" || { tail -40 "$log"; exit 1; }
wait "$reader" 2>/dev/null || true
printf '{"error": null, "started": true}\n' > "$RUNNER_TEMP/smoke-test.json"
cat "$RUNNER_TEMP/smoke-test.json"
