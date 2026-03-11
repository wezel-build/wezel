#!/usr/bin/env bash
set -euo pipefail

E2E_DB_NAME="${E2E_DB_NAME:-wezel_test}"
BURROW_PORT="${BURROW_PORT:-3002}"
PID_FILE="/tmp/wezel_e2e_burrow.pid"
BASE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# Set DATABASE_URL if not already provided
export DATABASE_URL="${DATABASE_URL:-postgresql://localhost/$E2E_DB_NAME}"

echo "==> Setting up e2e environment"
echo "    DB:   $E2E_DB_NAME"
echo "    URL:  $DATABASE_URL"
echo "    Port: $BURROW_PORT"

# Create test database (best-effort: already exists in CI, needs creating locally)
echo "==> Creating database '$E2E_DB_NAME'..."
createdb "$E2E_DB_NAME" 2>/dev/null || echo "    (database already exists, continuing)"

# Kill any stale burrow process from a previous run
if [ -f "$PID_FILE" ]; then
    OLD_PID=$(cat "$PID_FILE")
    kill "$OLD_PID" 2>/dev/null || true
    rm "$PID_FILE"
fi

# Start burrow in the background
BURROW_BIN="$BASE_DIR/target/debug/burrow"
if [ ! -x "$BURROW_BIN" ]; then
    echo "ERROR: $BURROW_BIN not found. Run 'cargo build --workspace' first." >&2
    exit 1
fi

echo "==> Starting burrow on port $BURROW_PORT..."
DATABASE_URL="$DATABASE_URL" "$BURROW_BIN" --port "$BURROW_PORT" &
BURROW_PID=$!
echo "$BURROW_PID" > "$PID_FILE"

# Poll until burrow is ready (max 10 seconds)
echo "==> Waiting for burrow to be ready..."
for i in $(seq 1 20); do
    if curl -sf "http://localhost:$BURROW_PORT/health" > /dev/null 2>&1; then
        echo "==> Burrow ready (PID $BURROW_PID)."
        exit 0
    fi
    sleep 0.5
done

echo "ERROR: Burrow did not become ready within 10 seconds." >&2
kill "$BURROW_PID" 2>/dev/null || true
rm -f "$PID_FILE"
exit 1
