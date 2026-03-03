# Default: list available recipes
default:
    @just --list

# Build and install pheromone binaries
build-pheromones:
    cargo build --release
    cargo install --path crates/wezel_cli --force --root "$HOME/.wezel"
    mkdir -p "$HOME/.wezel/bin/pheromones"
    @for bin in target/release/pheromone-*; do \
        [ -f "$bin" ] && [ -x "$bin" ] || continue; \
        cp "$bin" "$HOME/.wezel/bin/pheromones/"; \
        echo "  $(basename "$bin")"; \
    done

# Run burrow + anthill dev servers
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    cleanup() {
        echo ""
        echo "Shutting down..."
        kill $BURROW_PID 2>/dev/null || true
        kill $ANTHILL_PID 2>/dev/null || true
        wait
    }
    trap cleanup EXIT INT TERM
    echo "Starting burrow API server..."
    cargo run -p burrow --bin burrow --release -- --port 3001 &
    BURROW_PID=$!
    echo "Starting anthill dev server..."
    cd anthill && npm run dev &
    ANTHILL_PID=$!
    echo ""
    echo "  Burrow:  http://localhost:3001"
    echo "  Anthill: http://localhost:5173"
    echo ""
    echo "Press Ctrl+C to stop both."
    wait

# Seed the burrow database
seed:
    python3 scripts/seed.py