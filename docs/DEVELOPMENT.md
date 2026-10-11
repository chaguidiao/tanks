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

Headless rendering (screenshots without a display, Mesa llvmpipe):
```bash
tools/dev/devbox.sh run xvfb-run -a cargo run -p client -- --screenshot out/m0.png
```

## Testing old-GPU compatibility
- **OpenGL 2.1 build**: build the client with the GL 2.1 raylib feature (set up in M0).
- **Software renderer**: `LIBGL_ALWAYS_SOFTWARE=1` (Mesa llvmpipe), or raylib's own software backend.
- **Fake an older GL version on Mesa**: `MESA_GL_VERSION_OVERRIDE=2.1 MESA_GLSL_VERSION_OVERRIDE=120`. This is a smoke test, not a guarantee; real old hardware is still the final check.

## Editor / IDE
rust-analyzer needs the toolchain. Either:
- run your editor inside the distrobox (`devbox.sh enter`, then launch it, or `distrobox-export --app <editor>`), or
- point the host editor's rust-analyzer at a wrapper that calls `devbox.sh run rust-analyzer`. Works, but slower to start.

## Cleanup

```bash
tools/dev/devbox.sh clean   # image, distrobox, ~/.local/share/tanks-dev
rm -rf target/
```
