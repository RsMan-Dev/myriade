#!/bin/sh
# Called inside rust:alpine3.22 by game-build.yml, without release credentials.
set -eu

apk add --no-cache build-base pkgconf linux-headers python3 git bash
git config --global --add safe.directory /work
# Match the shared release/SDK pin, not the image's bundled Rust or the moving stable channel.
bash tools/ci/rustup_install.sh --profile minimal --component llvm-tools
export RUSTUP_TOOLCHAIN="$(cat tools/ci/rust-version)"

# Static musl cannot dlopen the mods' libraries. Link against Alpine's native libraries rather than
# Ubuntu's glibc libraries.
export RUSTFLAGS='-C target-feature=-crt-static'
target=x86_64-unknown-linux-musl
bin="target/$target/release"
cargo build --release --locked --target "$target" --no-default-features -p horizon-game --bin horizon-client
cargo build --release --locked --target "$target" -p horizon-prepare --bin horizon-prepare -p horizon-server --bin horizon-server
cp wasm-std/horizon-wasm-std.wasm "$bin/horizon-wasm-std.wasm"

set -- "$bin/horizon-client" "$bin/horizon-prepare" "$bin/horizon-server"
python3 tools/check_imports.py --musl "$@"
python3 tools/check_cpu_baseline.py "$@"

# Even --help must never touch real player data.
export HORIZON_HOME="$(mktemp -d)"
export XDG_DATA_HOME="$(mktemp -d)"
export XDG_CONFIG_HOME="$(mktemp -d)"
"$bin/horizon-client" --help > "$HORIZON_HOME/client-help"
grep -q -- '--resources' "$HORIZON_HOME/client-help"
"$bin/horizon-prepare" --help > "$HORIZON_HOME/prepare-help"
grep -q -- '--minecraft-dir' "$HORIZON_HOME/prepare-help"
for binary in horizon-prepare horizon-server; do
  test "$("$bin/$binary" --version)" = "$binary $HORIZON_VERSION"
done

cargo run --locked --target "$target" -p xtask -- package-server \
  "$bin/horizon-client" dist/horizon-linux-musl-x86_64
tar -C dist -czf dist/horizon-linux-musl-x86_64.tar.gz horizon-linux-musl-x86_64
