#!/usr/bin/env bash
set -e

# Build bimifc-ffi as an XCFramework for iOS integration.
# Targets: aarch64-apple-ios (device), aarch64-apple-ios-sim (simulator on Apple Silicon)

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

echo "==> Ensuring iOS targets are installed..."
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# ── Build host target (needed for uniffi-bindgen) ───────────────────
echo "==> Building host target (debug) for uniffi-bindgen..."
cargo build -p bimifc-ffi

# ── Build iOS targets (release) ────────────────────────────────────
echo "==> Building aarch64-apple-ios (device, release)..."
cargo build --release -p bimifc-ffi --target aarch64-apple-ios

echo "==> Building aarch64-apple-ios-sim (simulator, release)..."
cargo build --release -p bimifc-ffi --target aarch64-apple-ios-sim

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
rm -rf target/xcf-ios target/xcf-sim target/BimIfcFFI.xcframework

# --- iOS device ---
mkdir -p target/xcf-ios/Headers target/xcf-ios/Modules
cp target/swift-bindings/bimifc_ffiFFI.h        target/xcf-ios/Headers/
cp target/swift-bindings/bimifc_ffiFFI.modulemap target/xcf-ios/Modules/module.modulemap

# --- iOS simulator ---
mkdir -p target/xcf-sim/Headers target/xcf-sim/Modules
cp target/swift-bindings/bimifc_ffiFFI.h        target/xcf-sim/Headers/
cp target/swift-bindings/bimifc_ffiFFI.modulemap target/xcf-sim/Modules/module.modulemap

# ── Create XCFramework ──────────────────────────────────────────────
echo "==> Creating XCFramework..."

xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libbimifc_ffi.a \
  -headers target/xcf-ios/Headers \
  -library target/aarch64-apple-ios-sim/release/libbimifc_ffi.a \
  -headers target/xcf-sim/Headers \
  -output target/BimIfcFFI.xcframework

# ── Done ────────────────────────────────────────────────────────────
echo ""
echo "==> Build complete!"
echo "    XCFramework: target/BimIfcFFI.xcframework"
echo "    Swift file:  target/swift-bindings/bimifc_ffi.swift"
