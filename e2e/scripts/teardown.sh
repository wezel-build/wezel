#!/usr/bin/env bash
set -euo pipefail

E2E_DB_NAME="${E2E_DB_NAME:-wezel_test}"
PID_FILE="/tmp/wezel_e2e_burrow.pid"

# Stop burrow
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    echo "==> Stopping burrow (PID $PID)..."
    kill "$PID" 2>/dev/null || true
    rm "$PID_FILE"
    echo "==> Burrow stopped."
else
    echo "==> No burrow PID file found, skipping."
fi

# Drop test database (best-effort: CI postgres service is ephemeral)
echo "==> Dropping database '$E2E_DB_NAME'..."
dropdb "$E2E_DB_NAME" 2>/dev/null || echo "    (could not drop database, skipping)"

echo "==> Teardown complete."
