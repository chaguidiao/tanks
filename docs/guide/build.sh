#!/usr/bin/env bash
# Build out/guide/tanks-user-guide.pdf from docs/guide/chapters (see docs/guide/README.md).
#   docs/guide/build.sh            render the PDF from the existing screenshots
#   docs/guide/build.sh --shots    retake every screenshot first (several minutes)
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO"
RUST="${TANKS_DEV_DATA:-$HOME/.local/share/tanks-dev}/rust"
if [[ "${1:-}" == "--shots" ]]; then
    tools/dev/devbox.sh run cargo build -p client
    tools/dev/devbox.sh run bash docs/guide/shots-menus.sh
    tools/dev/devbox.sh run bash docs/guide/shots-cli.sh
    podman image exists localhost/tanks-shots || podman build -t localhost/tanks-shots -f docs/guide/containers/Containerfile.shots docs/guide/containers
    podman run --rm -v "$REPO:$REPO" -v "$RUST:/opt/rust" -w "$REPO" localhost/tanks-shots bash docs/guide/shots-live.sh
fi
podman image exists localhost/tanks-guide || podman build -t localhost/tanks-guide -f docs/guide/containers/Containerfile.render docs/guide/containers
podman run --rm --userns=keep-id -v "$REPO:$REPO" -w "$REPO" localhost/tanks-guide python3 docs/guide/build.py
