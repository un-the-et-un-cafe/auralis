#!/usr/bin/env bash
# macOS: build omcp-ffi for iOS device + Apple-silicon simulator, generate the UniFFI Swift
# bindings, and package build/OmcpFFI.xcframework (static libs + C header + modulemap).
# Outputs (git-ignored): apps/ios/build/OmcpFFI.xcframework, apps/ios/Generated/OmcpFFI/OmcpFFI.swift
# Requires: Xcode, rustup targets aarch64-apple-ios aarch64-apple-ios-sim.
set -euo pipefail
IOS="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$IOS/../.." && pwd)"
PROFILE="${OMCP_RUST_PROFILE:-release}"
TARGETS="${OMCP_IOS_TARGETS:-aarch64-apple-ios aarch64-apple-ios-sim}"
flag=""; [ "$PROFILE" = "release" ] && flag="--release"
export IPHONEOS_DEPLOYMENT_TARGET=17.0
cd "$ROOT"

for t in $TARGETS; do
  cargo rustc --locked -p omcp-ffi --lib $flag --target "$t" --crate-type staticlib
done

# Bindings from a host build of the same crate (library mode).
cargo build --locked -p omcp-ffi
HOSTLIB="$ROOT/target/debug/libomcp_ffi.dylib"; [ -f "$HOSTLIB" ] || HOSTLIB="$ROOT/target/debug/libomcp_ffi.so"
GEN="$IOS/build/uniffi"
rm -rf "$GEN" "$IOS/Generated/OmcpFFI" "$IOS/build/OmcpFFI.xcframework"
mkdir -p "$GEN/include" "$IOS/Generated/OmcpFFI"
cargo run --locked -q -p omcp-ffi --features bindgen --bin uniffi-bindgen -- generate --library "$HOSTLIB" \
  --language swift --config "$ROOT/crates/omcp-ffi/uniffi.toml" --out-dir "$GEN"
cp "$GEN/OmcpFFI.swift" "$IOS/Generated/OmcpFFI/"
cp "$GEN/omcp_ffiFFI.h" "$GEN/include/"
cp "$GEN/omcp_ffiFFI.modulemap" "$GEN/include/module.modulemap"

args=()
for t in $TARGETS; do
  args+=(-library "$ROOT/target/$t/$PROFILE/libomcp_ffi.a" -headers "$GEN/include")
done
xcodebuild -create-xcframework "${args[@]}" -output "$IOS/build/OmcpFFI.xcframework"
echo "omcp-ffi: $IOS/build/OmcpFFI.xcframework + Generated/OmcpFFI/OmcpFFI.swift"
