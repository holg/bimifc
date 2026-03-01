#!/bin/bash
# Build the unified BIMIFC viewer (single WASM: Leptos UI + Bevy 3D)
#
# Uses trunk to compile crates/bimifc-viewer into a unified WASM module.
#
# Usage:
#   ./scripts/build-unified.sh          # Build only (release)
#   ./scripts/build-unified.sh serve    # Build and serve on :8083
#   ./scripts/build-unified.sh dev      # Debug build + serve

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
VIEWER_CRATE="$ROOT_DIR/crates/bimifc-viewer"
DIST_DIR="$VIEWER_CRATE/dist"

# Check for trunk
if ! command -v trunk &> /dev/null; then
    echo "ERROR: trunk not found. Install with:"
    echo "  cargo install trunk"
    exit 1
fi

MODE="${1:-build}"

echo "=== Building BIMIFC Unified Viewer ==="
echo "Viewer crate: $VIEWER_CRATE"
echo "Output:       $DIST_DIR"
echo ""

cd "$VIEWER_CRATE"

case "$MODE" in
    dev)
        echo "Debug build + serve..."
        trunk build --release
        echo ""
        echo "=== Build Complete ==="
        echo "Serving on http://127.0.0.1:8083"
        echo "  (Ctrl+C to stop)"
        echo ""
        python3 -m http.server 8083 -d "$DIST_DIR"
        ;;
    serve)
        echo "Release build + serve..."
        trunk build --release
        echo ""
        echo "=== Build Complete ==="
        echo "Serving on http://127.0.0.1:8083"
        echo "  (Ctrl+C to stop)"
        echo ""
        python3 -m http.server 8083 -d "$DIST_DIR"
        ;;
    build)
        echo "Release build..."
        trunk build --release
        echo ""
        echo "=== Build Complete ==="
        echo ""
        echo "To serve locally:"
        echo "  python3 -m http.server 8083 -d $DIST_DIR"
        echo "  # Then open http://127.0.0.1:8083"
        ;;
    *)
        echo "Unknown mode: $MODE"
        echo "Usage: $0 [build|serve|dev]"
        exit 1
        ;;
esac

echo ""
echo "Files:"
ls -lh "$DIST_DIR"/*.wasm "$DIST_DIR"/*.js 2>/dev/null || true
