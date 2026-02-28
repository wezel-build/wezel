#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Seeding burrow database..."
cd "$REPO_ROOT"
cargo run --release -p burrow --bin burrow-seed