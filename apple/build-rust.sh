#!/bin/bash
set -e

# Build bimifc-ffi for macOS and generate Swift bindings
# Run this before opening the Xcode project

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${GREEN}Building bimifc-ffi for macOS...${NC}"

cd "$PROJECT_ROOT"

# Build release static library for macOS
cargo build -p bimifc-ffi --release

echo -e "${GREEN}Generating Swift bindings...${NC}"

# Generate Swift source from the built library
cargo run -p bimifc-ffi --bin uniffi-bindgen -- generate \
    --library target/release/libbimifc_ffi.dylib \
    --language swift \
    --out-dir crates/bimifc-ffi/generated

# Copy generated files into the Xcode project
SWIFT_SRC="$SCRIPT_DIR/BimIFC/BimIFC/Generated"
mkdir -p "$SWIFT_SRC"

cp crates/bimifc-ffi/generated/bimifc_ffi.swift "$SWIFT_SRC/"
cp crates/bimifc-ffi/generated/bimifc_ffiFFI.h "$SWIFT_SRC/"
cp crates/bimifc-ffi/generated/bimifc_ffiFFI.modulemap "$SWIFT_SRC/"

# Copy the static library
LIB_DIR="$SCRIPT_DIR/BimIFC/Libraries"
mkdir -p "$LIB_DIR"
cp target/release/libbimifc_ffi.a "$LIB_DIR/"

echo -e "${GREEN}Done!${NC}"
echo ""
echo "Files generated:"
echo "  Swift bindings: $SWIFT_SRC/bimifc_ffi.swift"
echo "  C header:       $SWIFT_SRC/bimifc_ffiFFI.h"
echo "  Static lib:     $LIB_DIR/libbimifc_ffi.a"
echo ""
echo "To build the app:"
echo "  1. Open apple/BimIFC/BimIFC.xcodeproj in Xcode"
echo "  2. Build & Run (Cmd+R)"
