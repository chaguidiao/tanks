#!/usr/bin/env bash
# Build a release package for the current OS: the game (GL 3.3 and GL 2.1 builds), the headless
# tool, and data/ with the maps pre-generated, as one folder plus an archive.
#
#   tools/package.sh <platform-name> [out-dir]      e.g. tools/package.sh linux-x86_64 dist
#
# Run from the repository root with a Rust toolchain (see docs/DEVELOPMENT.md for the container).
# Output: <out-dir>/tanks-<version>-<platform>/ and .tar.gz (.zip on Windows) and .sha256.
set -euo pipefail

name="${1:?usage: tools/package.sh <platform-name> [out-dir]}"
out="${2:-dist}"
cd "$(dirname "${BASH_SOURCE[0]}")/.."

version="$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -1)"
pkg="tanks-${version}-${name}"
exe=""
case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) exe=".exe" ;; esac

# Build id shown on the title screen: the tag for a release ("v0.1.0"), otherwise a describe
# string such as "v0.1.0-3-g1a2b3c4" or a bare hash; "" without git.
TANKS_BUILD="$(git describe --tags --always --dirty 2>/dev/null || true)"
export TANKS_BUILD

cargo build --release -p client -p headless
CARGO_TARGET_DIR=target/gl21 cargo build --release -p client --no-default-features --features gl21

rm -rf "${out:?}/${pkg}"
mkdir -p "${out}/${pkg}"
cp "target/release/client${exe}" "${out}/${pkg}/tanks${exe}"
cp "target/gl21/release/client${exe}" "${out}/${pkg}/tanks-gl21${exe}"
cp "target/release/tanks-headless${exe}" "${out}/${pkg}/tanks-headless${exe}"
cp LICENSE NOTICE THIRD_PARTY.md "${out}/${pkg}/"
# The user guide is built outside this script (docs/guide/build.sh needs the game for its
# screenshots); a local package includes it when it has been built, a CI release attaches it
# to the GitHub release instead (docs/DEVELOPMENT.md Releases).
if [ -f out/guide/tanks-user-guide.pdf ]; then
    cp out/guide/tanks-user-guide.pdf "${out}/${pkg}/USER-GUIDE.pdf"
fi
cp tools/README-release.txt "${out}/${pkg}/README.txt"
cp -r data "${out}/${pkg}/data"
rm -rf "${out}/${pkg}/data/maps"

# Pre-generate the maps the scenarios use (deterministic: same recipe and seed, same bytes), so
# the first start does not wait for generation and the folder can be read-only.
(
    cd "${out}/${pkg}"
    ./tanks-headless gen --recipe rolling-farmland --seed 42 --id demo
    ./tanks-headless gen --recipe wooded-uplands --seed 7 --id uplands
    ./tanks-headless gen --recipe market-town --seed 42 --id town
    # Every scenario must load from the package: a missing file or bad reference fails here.
    for f in data/scenarios/*.ron; do
        id="$(basename "$f" .ron)"
        ./tanks-headless run --scenario "$id" --seconds 5 >/dev/null
    done
)

cd "${out}"
case "$name" in
    # Not tar: Git Bash's GNU tar ignores -a and writes an uncompressed tar named .zip.
    windows*)
        archive="${pkg}.zip"
        powershell.exe -NoProfile -Command "Compress-Archive -Path '${pkg}' -DestinationPath '${archive}' -Force"
        ;;
    # COPYFILE_DISABLE: no AppleDouble (._) entries from macOS tar.
    *) archive="${pkg}.tar.gz"; COPYFILE_DISABLE=1 tar -czf "${archive}" "${pkg}" ;;
esac
if command -v sha256sum >/dev/null; then sha256sum "${archive}" >"${archive}.sha256"; else shasum -a 256 "${archive}" >"${archive}.sha256"; fi
echo "packaged ${out}/${archive}"
