#!/usr/bin/env bash
# Serve the self-contained WebCLAP test host with the cross-origin-isolation
# headers the runtime needs, then print the URL that auto-loads the Parametric
# HRTF plugin.
set -euo pipefail

PORT="${1:-8000}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEB="$ROOT/web"

if [ ! -f "$WEB/parametric-hrtf.wclap.tar.gz" ]; then
  echo "Bundle missing — building it first (cargo xtask bundle-webclap --release)…"
  ( cd "$ROOT" && cargo xtask bundle-webclap --release )
fi

URL="http://localhost:$PORT/?module=parametric-hrtf.wclap.tar.gz&audio=audio/loop.mp3"
echo "────────────────────────────────────────────────────────────"
echo " Parametric HRTF — WebCLAP test host"
echo " Open: $URL"
echo " (click once to start audio, then drag the source on the pad)"
echo "────────────────────────────────────────────────────────────"

cd "$WEB"
exec python3 server.py "$PORT"
