#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "Seeding burrow database..."
python3 "$REPO_ROOT/scripts/seed.py"