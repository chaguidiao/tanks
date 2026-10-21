# Development Environment

All toolchains and build dependencies live in a **podman container**, so the host stays clean. The only host requirements are `podman` and `distrobox`.

## What lives where

| Thing | Location | Removed by |
|---|---|---|
| System build deps (cmake, clang, X11/Wayland/GL/ALSA headers, xvfb) | podman image `localhost/tanks-dev` (Ubuntu 24.04) | `devbox.sh clean` |
| Rust toolchain, cargo registry cache, cargo-nextest, just | `~/.local/share/tanks-dev/rust` (mounted at `/opt/rust`) | `devbox.sh clean` |
| Distrobox home (shell history, editor caches) | `~/.local/share/tanks-dev/home` | `devbox.sh clean` |
| Build output | `<repo>/target/` (gitignored) | `cargo clean` / delete |

Nothing is installed into `~/.cargo`, `~/.rustup` or system paths on the host.

## Two ways to run

- **`devbox.sh run …`**: a throwaway `podman run --rm` container. Use it for builds, tests, clippy, map generation and headless screenshots. This is what Claude and CI use.
- **`devbox.sh enter` / `devbox.sh gui …`**: a persistent **distrobox** built from the same image. Distrobox passes through the display (X11/Wayland), the GPU (`/dev/dri`) and audio (PipeWire/Pulse) automatically, so the raylib client opens a normal window.

Both mount the repo at the same absolute path and share `/opt/rust`, so `target/` is reused between them.

## First-time setup

```bash
tools/dev/devbox.sh setup        # build image (~1 GB), install Rust (~600 MB), create distrobox
tools/dev/devbox.sh status
```

## Daily use

```bash
tools/dev/devbox.sh run cargo build
tools/dev/devbox.sh run cargo nextest run
tools/dev/devbox.sh run cargo clippy --all-targets -- -D warnings
tools/dev/devbox.sh gui cargo run -p client          # needs a desktop session
tools/dev/devbox.sh enter                            # interactive shell
```

Tasks (`justfile`): `just check`, `just gen`, `just play`, `just edit`, `just screenshots`, e.g.
`tools/dev/devbox.sh run just check` or `tools/dev/devbox.sh gui just play`.

Maps are generated, not committed (`data/maps/` is gitignored): the client and `tanks-headless run`
generate and bake a scenario's map from its recipe + seed on first use.

```bash
tools/dev/devbox.sh run cargo run --release -p headless -- gen --recipe rolling-farmland --seed 42 --id demo
tools/dev/devbox.sh run cargo run --release -p headless -- render --map demo --layer tint --downsample 3 --out out/demo.png
tools/dev/devbox.sh run cargo run --release -p headless -- run --scenario m1-move --seconds 600 --move 6000,5200
```

Client: starts at the title screen → main menu (play scenario, scenario editor, terrain editor, database,
options). Every control is a button that shows its hotkey; hover for a tooltip, F7 for the key list of
the current screen, Esc for the screen menu. Game: LMB select (a single vehicle shows its status card; I opens its data), RMB move, Shift+RMB add route checkpoints
(release Shift to send), hold RMB (or RMB on a unit, contact, graphic or plan CP) for the context menu, P plan
(LMB adds CPs, C CP menu, Enter send), Ctrl+1–9 code words, H halt, Space pause, 1–4 speed. Terrain editor: 1–4 brushes, 5 river, 6 road,
7 forest, 8 building, Ctrl+S save. Scenario editor: F9 play test (and back). Options are stored in
`~/.config/tanks/settings.cfg` (override with `TANKS_CONFIG`).

Headless rendering (screenshots without a display, Mesa llvmpipe):
```bash
tools/dev/devbox.sh run xvfb-run -a -s "-screen 0 1600x900x24" cargo run -p client -- --order 1-1:3000,2600 --advance 60 --screenshot out/m1.png
# platoon route with checkpoints: --order <platoon>:cp1x,cp1y,...,destx,desty
tools/dev/devbox.sh run bash -c 'xvfb-run -a -s "-screen 0 1600x900x24" target/debug/client --scenario m5-hold-the-village --order 1:5200,4300,5800,3900,5400,3300 --advance 60 --screenshot out/route.png; true'
```
Menus and flows (screenshot runs use default options and never write the options file):
```bash
# a screen directly: title | main | play | edit-scenario | edit-terrain | database | options
target/debug/client --screen play --screenshot out/play.png
# scripted keys, one per frame (waitN = N idle frames; loads do not count): title → menu → 2nd scenario → game → Esc menu
target/debug/client --keys wait5,enter,wait3,enter,wait3,down,enter,wait5,esc --frames 30 --screenshot out/pause.png
# any game/editor option opens that screen directly (as before); --pause adds its menu
target/debug/client --scenario m5-hold-the-village --scenario-editor --screenshot out/scened.png
# one vehicle selected (status card), then its DATABASE sheet (I)
target/debug/client --scenario m5-hold-the-village --advance 600 --select 1-2 --keys i --screenshot out/data.png
```
If you wrap `xvfb-run` in `bash -c`, end the script with `; true`: otherwise bash execs `xvfb-run` as
the container's PID 1, where it can miss Xvfb's ready signal and hang forever.

## Testing old-GPU compatibility
- **OpenGL 2.1 build**: `cargo build -p client --no-default-features --features gl21` (use a separate `CARGO_TARGET_DIR`, e.g. `target/gl21`, to avoid rebuilding raylib each switch).
- **Software renderer**: `LIBGL_ALWAYS_SOFTWARE=1` (Mesa llvmpipe), or raylib's own software backend.
- **Fake an older GL version on Mesa**: `MESA_GL_VERSION_OVERRIDE=2.1 MESA_GLSL_VERSION_OVERRIDE=120`. This is a smoke test, not a guarantee; real old hardware is still the final check.

## Releases (M7e)
- **Local package** (current OS): `tools/dev/devbox.sh run tools/package.sh linux-x86_64 dist` builds the GL 3.3 game (`tanks`), the GL 2.1 game (`tanks-gl21`) and `tanks-headless`, copies `data/` with the maps pre-generated (`demo` = `rolling-farmland` seed 42, `uplands` = `wooded-uplands` seed 7), checks that every scenario in `data/scenarios` loads from the package, and writes `dist/tanks-<version>-<platform>.tar.gz` (`.zip` on Windows) with a `.sha256`. The package finds `data/` next to the executable, so it can be started from any directory; `TANKS_DATA` overrides.
- **All three OSes**: `.github/workflows/release.yml` runs the script on Linux (Ubuntu 22.04, for glibc compatibility), Windows and macOS (arm64) for a `v*` tag or a manual run, and a tag also publishes a GitHub release with the archives. Not run from this container: the first tag is the first real test of the Windows and macOS legs; Intel macOS is not built.
- **Version**: the Cargo workspace version is the release version; tag `v<version>` (the workflow fails on a mismatch). `package.sh` passes `git describe` as `TANKS_BUILD`, so the title footer shows `0.1.0` for the tagged build, `0.1.0-3-G1A2B3C4` for later commits and `0.1.0 DEV` for a plain `cargo build`.
- Packages are unsigned (macOS Gatekeeper and Windows SmartScreen will warn). Add a map or scenario to the script's `gen` lines when a scenario uses a new map.
- raylib quirk: `--screenshot` with an absolute path prepends the working directory; use a relative path.

## Editor / IDE
rust-analyzer needs the toolchain. Either:
- run your editor inside the distrobox (`devbox.sh enter`, then launch it, or `distrobox-export --app <editor>`), or
- point the host editor's rust-analyzer at a wrapper that calls `devbox.sh run rust-analyzer`. Works, but slower to start.

## Cleanup

```bash
tools/dev/devbox.sh clean   # image, distrobox, ~/.local/share/tanks-dev
rm -rf target/
```
