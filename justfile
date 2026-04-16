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

