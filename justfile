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

# Download the latest nightly wezel release into ~/.wezel/bin/wezel
install-latest:
    #!/usr/bin/env bash
    set -euo pipefail
    case "$(uname -sm)" in
        "Darwin arm64")   target="aarch64-apple-darwin" ;;
        "Darwin x86_64")  target="x86_64-apple-darwin" ;;
        "Linux aarch64")  target="aarch64-unknown-linux-gnu" ;;
        "Linux x86_64")   target="x86_64-unknown-linux-gnu" ;;
        *) echo "unsupported platform: $(uname -sm)" >&2; exit 1 ;;
    esac
    tag=$(gh release list --repo wezel-build/wezel --limit 1 --json tagName --jq '.[0].tagName')
    archive="wezel_cli-${target}.tar.xz"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    echo "fetching $tag → $archive"
    gh release download "$tag" --repo wezel-build/wezel \
        --pattern "$archive" --pattern "${archive}.sha256" --dir "$tmp"
    (cd "$tmp" && grep -v '^[[:space:]]*$' "${archive}.sha256" | shasum -a 256 -c -)
    tar -xJf "$tmp/$archive" -C "$tmp"
    mkdir -p "$HOME/.wezel/bin"
    install -m 0755 "$tmp/wezel_cli-${target}/wezel" "$HOME/.wezel/bin/wezel"
    echo "installed $($HOME/.wezel/bin/wezel --version 2>/dev/null || echo wezel) → $HOME/.wezel/bin/wezel"

