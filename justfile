# Task runner (inside the tanks-dev container: tools/dev/devbox.sh run just <task>)

default: check

# fmt + clippy + tests (what must pass before a commit)
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo nextest run --workspace

# generate and bake the demo map used by the m1-move scenario
gen seed="42" recipe="rolling-farmland" id="demo":
    cargo run --release -p headless -- gen --recipe {{recipe}} --seed {{seed}} --id {{id}}

# run the client (needs a display: tools/dev/devbox.sh gui just play)
play scenario="m1-move":
    cargo run --release -p client -- --scenario {{scenario}}

# open the terrain editor on a map
edit map="demo":
    cargo run --release -p client -- --map {{map}} --edit

# headless screenshots for both GL backends
screenshots:
    mkdir -p out
    xvfb-run -a -s "-screen 0 1600x900x24" cargo run -p client -- --order 1-1:3000,2600 --advance 60 --screenshot out/gl33.png
    CARGO_TARGET_DIR=target/gl21 MESA_GL_VERSION_OVERRIDE=2.1 MESA_GLSL_VERSION_OVERRIDE=120 xvfb-run -a -s "-screen 0 1600x900x24" cargo run -p client --no-default-features --features gl21 -- --screenshot out/gl21.png
