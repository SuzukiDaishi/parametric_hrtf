#!/usr/bin/env bash
# Serve the self-contained WebCLAP test host with the cross-origin-isolation
# headers the runtime needs, then print the URL that auto-loads the Parametric
# HRTF plugin.
set -euo pipefail

PORT="${1:-8000}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if command -v python3 >/dev/null 2>&1; then
  PYTHON=python3
else
  PYTHON=python
fi

exec "$PYTHON" "$ROOT/scripts/serve.py" "$PORT"
