#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cleanup() {
    echo ""
    echo "Shutting down..."
    kill $BURROW_PID 2>/dev/null || true
    kill $ANTHILL_PID 2>/dev/null || true
    wait
}
trap cleanup EXIT INT TERM

echo "Starting burrow API server..."
cd "$REPO_ROOT"
cargo run -p burrow --bin burrow -- --port 3001 &
BURROW_PID=$!

echo "Starting anthill dev server..."
cd "$REPO_ROOT/anthill"
npm run dev &
ANTHILL_PID=$!

echo ""
echo "  Burrow:  http://localhost:3001"
echo "  Anthill: http://localhost:5173"
echo ""
echo "Press Ctrl+C to stop both."

wait