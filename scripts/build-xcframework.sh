#!/usr/bin/env bash
set -e

# Build bimifc-ffi as an XCFramework for iOS + macOS integration.
# Targets: aarch64-apple-ios (device), aarch64-apple-ios-sim (simulator),
#          aarch64/x86_64-apple-darwin (macOS universal, lipo'd)

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# ── Pre-flight checks ───────────────────────────────────────────────
echo "==> Checking required tools..."

for tool in cargo xcodebuild rustup; do
  if ! command -v "$tool" &>/dev/null; then
    echo "ERROR: '$tool' is not installed or not in PATH." >&2
    exit 1
  fi
done

echo "==> Ensuring Apple targets are installed..."
rustup target add aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin x86_64-apple-darwin

# ── Build host target (needed for uniffi-bindgen) ───────────────────
echo "==> Building host target (debug) for uniffi-bindgen..."
cargo build -p bimifc-ffi

# ── Build iOS targets (release) ────────────────────────────────────
echo "==> Building aarch64-apple-ios (device, release)..."
cargo build --release -p bimifc-ffi --target aarch64-apple-ios

echo "==> Building aarch64-apple-ios-sim (simulator, release)..."
cargo build --release -p bimifc-ffi --target aarch64-apple-ios-sim

# macOS carries the embedded Bevy viewer (feature `viewer`), so the Mac
# app renders with the real bimifc viewer instead of a second SceneKit
# scene. iOS stays without it: the viewer roughly triples the slice and
# the phone has no 3D editing UI. Set VIEWER=0 to build macOS without it.
MACOS_FEATURES=()
if [[ "${VIEWER:-1}" != "0" ]]; then
  MACOS_FEATURES=(--features viewer)
  echo "==> macOS slices include the embedded Bevy viewer"
fi

echo "==> Building aarch64-apple-darwin (macOS, release)..."
cargo build --release -p bimifc-ffi --target aarch64-apple-darwin "${MACOS_FEATURES[@]}"

echo "==> Building x86_64-apple-darwin (macOS, release)..."
cargo build --release -p bimifc-ffi --target x86_64-apple-darwin "${MACOS_FEATURES[@]}"

# ── Generate Swift bindings via uniffi-bindgen ──────────────────────
echo "==> Generating Swift bindings..."
mkdir -p target/swift-bindings

cargo run --bin uniffi-bindgen -p bimifc-ffi -- generate \
  --library target/debug/libbimifc_ffi.dylib \
  --language swift \
  --out-dir target/swift-bindings

# Verify expected files exist
for f in target/swift-bindings/bimifc_ffi.swift \
         target/swift-bindings/bimifc_ffiFFI.h \
         target/swift-bindings/bimifc_ffiFFI.modulemap; do
  if [[ ! -f "$f" ]]; then
    echo "ERROR: Expected generated file not found: $f" >&2
    exit 1
  fi
done

# ── Prepare XCFramework directory structure ─────────────────────────
echo "==> Preparing XCFramework staging directories..."

# Clean previous artifacts
rm -rf target/xcf-ios target/xcf-sim target/xcf-macos target/BimIfcFFI.xcframework

# --- iOS device ---
mkdir -p target/xcf-ios/Headers
cp target/swift-bindings/bimifc_ffiFFI.h        target/xcf-ios/Headers/
# module.modulemap must live inside Headers/ — xcodebuild -create-xcframework
# only copies the -headers directory, and Clang finds the module there.
cp target/swift-bindings/bimifc_ffiFFI.modulemap target/xcf-ios/Headers/module.modulemap

# --- iOS simulator ---
mkdir -p target/xcf-sim/Headers
cp target/swift-bindings/bimifc_ffiFFI.h        target/xcf-sim/Headers/
cp target/swift-bindings/bimifc_ffiFFI.modulemap target/xcf-sim/Headers/module.modulemap

# --- macOS (universal) ---
mkdir -p target/xcf-macos/Headers
cp target/swift-bindings/bimifc_ffiFFI.h        target/xcf-macos/Headers/
cp target/swift-bindings/bimifc_ffiFFI.modulemap target/xcf-macos/Headers/module.modulemap
# The embedded viewer's C API is hand-written (not UniFFI), so ship its
# header and name it in the modulemap or Swift cannot see the symbols.
if [[ "${VIEWER:-1}" != "0" ]]; then
  cp crates/bimifc-bevy/include/bimifc_viewer.h target/xcf-macos/Headers/
  python3 - "target/xcf-macos/Headers/module.modulemap" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
if "bimifc_viewer.h" not in s:
    s = s.replace('header "bimifc_ffiFFI.h"',
                  'header "bimifc_ffiFFI.h"\n    header "bimifc_viewer.h"', 1)
    open(p, "w").write(s)
PY
fi

echo "==> Creating universal macOS library (lipo)..."
lipo -create \
  target/aarch64-apple-darwin/release/libbimifc_ffi.a \
  target/x86_64-apple-darwin/release/libbimifc_ffi.a \
  -output target/xcf-macos/libbimifc_ffi.a

# ── Create XCFramework ──────────────────────────────────────────────
echo "==> Creating XCFramework..."

xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libbimifc_ffi.a \
  -headers target/xcf-ios/Headers \
  -library target/aarch64-apple-ios-sim/release/libbimifc_ffi.a \
  -headers target/xcf-sim/Headers \
  -library target/xcf-macos/libbimifc_ffi.a \
  -headers target/xcf-macos/Headers \
  -output target/BimIfcFFI.xcframework

# ── Done ────────────────────────────────────────────────────────────
echo ""
echo "==> Build complete!"
echo "    XCFramework: target/BimIfcFFI.xcframework"
echo "    Swift file:  target/swift-bindings/bimifc_ffi.swift"
