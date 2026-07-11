#!/usr/bin/env bash
#
# build.sh — render Amber's README/website demo GIFs.
#
# Privacy model: nothing in this pipeline hardcodes a host path, username, or
# personal setup. The containerised path runs every command under /work inside
# the image and mounts only the output directory. The host fallback runs the
# same tapes with a neutral prompt so no host layout appears in the frames.
#
# Usage:
#   docs/vhs/build.sh           # prefer the container, fall back to host
#   docs/vhs/build.sh --host    # force the host fallback
#   docs/vhs/build.sh --docker  # require the container (fail if unavailable)
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="$ROOT/docs/vhs/out"
WEB="$ROOT/website/assets/vhs"
IMAGE="amber-vhs"

MODE="${1:-auto}"

copy_website_gifs() {
    mkdir -p "$WEB"
    for name in amber-emoji-analyze amber-emoji-score; do
        if [ -f "$OUT/$name.gif" ]; then
            cp "$OUT/$name.gif" "$WEB/$name.gif"
        fi
    done
}

run_docker() {
    command -v docker >/dev/null 2>&1 || return 1
    # Build amber on the host; the image (Debian trixie, glibc 2.41) runs the
    # host binary unchanged. Compiling inside the image is avoided because it
    # would need the developer's sibling-crate layout in the build context.
    cargo build --release --locked --manifest-path "$ROOT/Cargo.toml"
    docker build -t "$IMAGE" -f "$ROOT/docs/vhs/Containerfile" "$ROOT"
    mkdir -p "$OUT"
    # seccomp=unconfined lets headless Chromium create its user-namespace
    # sandbox inside Docker; scoped to this local recorder only.
    docker run --rm --security-opt seccomp=unconfined -v "$OUT:/work/out" "$IMAGE"
}

run_host() {
    command -v vhs >/dev/null 2>&1 || {
        echo "error: vhs is not installed and the container is unavailable" >&2
        echo "       install vhs (https://github.com/charmbracelet/vhs) or use docker" >&2
        exit 1
    }
    cargo build --release --locked --manifest-path "$ROOT/Cargo.toml"
    mkdir -p "$OUT"
    (
        export PATH="$ROOT/target/release:$PATH"
        export RUST_LOG=error
        cd "$ROOT/docs/vhs"
        for t in tapes/*.tape; do
            vhs "$t"
        done
    )
}

case "$MODE" in
    --docker)
        run_docker
        ;;
    --host)
        run_host
        ;;
    auto)
        if ! run_docker; then
            echo "container path unavailable; falling back to host vhs" >&2
            run_host
        fi
        ;;
    *)
        echo "usage: $0 [--docker|--host|auto]" >&2
        exit 2
        ;;
esac

copy_website_gifs
echo "GIFs written to docs/vhs/out and (emoji set) website/assets/vhs"
