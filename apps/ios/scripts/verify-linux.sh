#!/usr/bin/env bash
# Linux checks for the iOS app (no Xcode): builds omcp-ffi for the host, generates the Swift
# bindings, then compiles HoplaKit + the generated OmcpFFI module + RustOmcpCore.swift with
# SwiftPM and runs HoplaKit's tests and RustBridgeTests against the real Rust core. Also
# syntax-checks every Swift file (SwiftUI/UIKit files are parsed, not type-checked).
# Requires a Swift 5.9+ toolchain on PATH.
set -euo pipefail
IOS="$(cd "$(dirname "$0")/.." && pwd)"
ROOT="$(cd "$IOS/../.." && pwd)"
WORK="${OMCP_VERIFY_DIR:-$ROOT/target/ios-linux-verify}"

python3 "$IOS/scripts/strings.py" --check
(cd "$IOS/HoplaKit" && swift test)

cd "$ROOT"
cargo build -p omcp-ffi
rm -rf "$WORK" && mkdir -p "$WORK/gen"
cargo run -q -p omcp-ffi --features bindgen --bin uniffi-bindgen -- generate \
  --library "$ROOT/target/debug/libomcp_ffi.so" --language swift \
  --config "$ROOT/crates/omcp-ffi/uniffi.toml" --out-dir "$WORK/gen" 2>/dev/null

P="$WORK/pkg"
mkdir -p "$P/Sources/omcp_ffiFFI/include" "$P/Sources/OmcpFFI" "$P/Sources/HoplaRust" "$P/Tests/BridgeTests"
cp "$WORK/gen/omcp_ffiFFI.h" "$P/Sources/omcp_ffiFFI/include/"
cp "$WORK/gen/omcp_ffiFFI.modulemap" "$P/Sources/omcp_ffiFFI/include/module.modulemap"
echo "// header-only C target" > "$P/Sources/omcp_ffiFFI/dummy.c"
cp "$WORK/gen/OmcpFFI.swift" "$P/Sources/OmcpFFI/"
ln -s "$IOS/HoplaKit/Sources/HoplaKit" "$P/Sources/HoplaKit"
cp "$IOS/OpenMeshChat/Core/RustOmcpCore.swift" "$P/Sources/HoplaRust/"
cp "$IOS/OpenMeshChatTests/RustBridgeTests.swift" "$P/Tests/BridgeTests/"
cat > "$P/Package.swift" <<SWIFT
// swift-tools-version:5.9
import PackageDescription
let package = Package(
    name: "HoplaLinuxVerify",
    targets: [
        .target(name: "omcp_ffiFFI", linkerSettings: [.unsafeFlags(["-L$ROOT/target/debug", "-lomcp_ffi", "-Xlinker", "-rpath", "-Xlinker", "$ROOT/target/debug"])]),
        .target(name: "OmcpFFI", dependencies: ["omcp_ffiFFI"]),
        .target(name: "HoplaKit"),
        .target(name: "HoplaRust", dependencies: ["HoplaKit", "OmcpFFI"]),
        .testTarget(name: "BridgeTests", dependencies: ["HoplaRust", "HoplaKit"]),
    ]
)
SWIFT
(cd "$P" && swift test)

# Syntax-only parse of everything (catches typos in UIKit/SwiftUI/CoreBluetooth files).
find "$IOS" -name '*.swift' -not -path '*/.build/*' -not -path '*/Generated/*' -not -path '*/build/*' -print0 | xargs -0 -n1 swiftc -parse
echo "verify-linux: OK"
