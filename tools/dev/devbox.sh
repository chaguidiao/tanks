#!/usr/bin/env bash
# Tanks dev environment wrapper. Everything lives in:
#   - podman image  localhost/tanks-dev
#   - distrobox     tanks-dev            (interactive shell + GUI)
#   - host dir      $TANKS_DEV_DATA      (rust toolchain, cargo cache, box home)
#   - repo dir      target/              (build output, gitignored)
# `devbox.sh clean` removes all of it.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
IMAGE="localhost/tanks-dev:latest"
BOX="tanks-dev"
DATA="${TANKS_DEV_DATA:-$HOME/.local/share/tanks-dev}"

mounts=(-v "$REPO:$REPO" -v "$DATA/rust:/opt/rust")

usage() {
    cat <<EOF
usage: tools/dev/devbox.sh <command>

  setup        build image, install Rust toolchain + tools, create distrobox
  build-image  (re)build the podman image only
  run CMD...   run CMD headless in a throwaway container (builds, tests, CI-style)
  enter        interactive shell in the distrobox (GUI apps work here)
  gui CMD...   run CMD in the distrobox (e.g. cargo run -p client)
  status       show what exists and disk usage
  clean        remove image, distrobox and $DATA
EOF
}

build_image() {
    podman build -t "$IMAGE" -f "$REPO/tools/dev/Containerfile" "$REPO/tools/dev"
}

run() {
    mkdir -p "$DATA/rust"
    podman run --rm -i $([ -t 0 ] && echo -t) \
        --userns=keep-id "${mounts[@]}" -w "$PWD" \
        "$IMAGE" bash -lc "$(printf '%q ' "$@")"
}

install_toolchain() {
    run bash -c '
        set -e
        if ! command -v rustup >/dev/null; then
            curl --proto =https --tlsv1.2 -sSf https://sh.rustup.rs \
                | sh -s -- -y --no-modify-path --profile minimal \
                    --default-toolchain stable --component rustfmt,clippy,rust-analyzer
        fi
        rustup show active-toolchain
        command -v cargo-nextest >/dev/null || cargo install --locked cargo-nextest
        command -v just >/dev/null || cargo install --locked just
    '
}

create_box() {
    mkdir -p "$DATA/home"
    distrobox create --yes --no-entry --name "$BOX" --image "$IMAGE" \
        --home "$DATA/home" \
        --volume "$REPO:$REPO" --volume "$DATA/rust:/opt/rust"
}

cmd="${1:-}"; shift || true
case "$cmd" in
    setup)        build_image; install_toolchain; create_box ;;
    build-image)  build_image ;;
    run)          run "$@" ;;
    enter)        distrobox enter "$BOX" ;;
    gui)          distrobox enter "$BOX" -- bash -lc "cd $(printf '%q' "$PWD") && $(printf '%q ' "$@")" ;;
    status)
        podman image ls "$IMAGE"
        distrobox list | grep -E "NAME|$BOX" || true
        du -sh "$DATA" "$REPO/target" 2>/dev/null || true ;;
    clean)
        distrobox rm --force "$BOX" 2>/dev/null || true
        podman rmi -f "$IMAGE" 2>/dev/null || true
        podman unshare rm -rf "$DATA" ;;
    *) usage; exit 1 ;;
esac
