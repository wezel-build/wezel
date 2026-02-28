#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$HOME/.wezel/bin/pheromones"

cd "$REPO_ROOT"

echo "Building workspace (release)..."
cargo build --release

echo ""
echo "Installing wezel CLI..."
cargo install --path crates/wezel_cli --force

echo ""
mkdir -p "$DEST"

echo "Copying pheromone binaries to $DEST..."
for bin in target/release/pheromone-*; do
    [ -f "$bin" ] && [ -x "$bin" ] || continue
    cp "$bin" "$DEST/"
    echo "  $(basename "$bin")"
done

echo ""
echo "Done."