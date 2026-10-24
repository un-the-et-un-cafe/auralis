#!/usr/bin/env bash
# Build omcp-ffi for Android (cargo-ndk) and generate the UniFFI Kotlin bindings.
# Outputs (git-ignored): app/build/rust/jniLibs/<abi>/libomcp_ffi.so, app/build/rust/kotlin/
# Requires: ANDROID_NDK_HOME (or ANDROID_HOME/ndk/<ver>), cargo-ndk, rustup targets
#   aarch64-linux-android x86_64-linux-android armv7-linux-androideabi
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="$HERE/app/build/rust"
PROFILE="${OMCP_RUST_PROFILE:-release}"
ABIS="${OMCP_ABIS:-arm64-v8a x86_64}"
mkdir -p "$OUT/jniLibs" "$OUT/kotlin"
cd "$ROOT"
targets=()
for a in $ABIS; do targets+=(-t "$a"); done
flag=""; [ "$PROFILE" = "release" ] && flag="--release"
cargo ndk "${targets[@]}" --platform 31 -o "$OUT/jniLibs" build -p omcp-ffi $flag
# Bindings are generated from a host build of the same crate (library mode).
cargo build -p omcp-ffi
cargo run -q -p omcp-ffi --features bindgen --bin uniffi-bindgen -- \
  generate --library "$ROOT/target/debug/libomcp_ffi.so" --language kotlin \
  --config "$ROOT/crates/omcp-ffi/uniffi.toml" --out-dir "$OUT/kotlin" --no-format
echo "omcp-ffi: jniLibs + Kotlin bindings in $OUT"
