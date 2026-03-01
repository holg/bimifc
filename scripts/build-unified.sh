#!/bin/bash
# Build and deploy the unified BIMIFC viewer (single WASM: Leptos UI + Bevy 3D)
#
# Uses trunk to compile crates/bimifc-viewer into a unified WASM module.
#
# Usage:
#   ./scripts/build-unified.sh              # Build only (release)
#   ./scripts/build-unified.sh serve        # Build and serve on :8083
#   ./scripts/build-unified.sh serve-only   # Serve without rebuilding
#   ./scripts/build-unified.sh deploy       # Build and deploy via rsync
#   ./scripts/build-unified.sh deploy clean # Deploy + clean old files on server first
#   ./scripts/build-unified.sh dev          # Debug build + serve

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
VIEWER_CRATE="$ROOT_DIR/crates/bimifc-viewer"
DIST_DIR="$VIEWER_CRATE/dist"
LOCAL_PORT=8083

# Deploy target (rsync format: user@host:/path/)
DEPLOY_TARGET="iesna.eu:/var/www/bimifc.de/html"
RSYNC_FLAGS="-avz"

# Check for tools
check_tool() {
    if ! command -v "$1" &> /dev/null; then
        echo "ERROR: $1 not found. Install with:"
        echo "  $2"
        exit 1
    fi
}

HAVE_BROTLI=false
command -v brotli &> /dev/null && HAVE_BROTLI=true

MODE="${1:-build}"

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------
if [[ "$MODE" == "--help" ]] || [[ "$MODE" == "-h" ]]; then
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  build         Build release WASM (default)"
    echo "  serve         Build and serve locally on :$LOCAL_PORT"
    echo "  serve-only    Serve without rebuilding"
    echo "  deploy        Build and deploy to $DEPLOY_TARGET"
    echo "  deploy clean  Build, clean old server files, then deploy"
    echo "  dev           Debug build + serve"
    echo "  --help        Show this help"
    echo ""
    echo "Output: $DIST_DIR"
    exit 0
fi

# ---------------------------------------------------------------------------
# serve-only: skip build
# ---------------------------------------------------------------------------
if [[ "$MODE" == "serve-only" ]]; then
    if [[ ! -d "$DIST_DIR" ]]; then
        echo "ERROR: Dist directory not found: $DIST_DIR"
        echo "Run '$0' first to build."
        exit 1
    fi
    echo "Serving on http://127.0.0.1:$LOCAL_PORT (no rebuild)"
    python3 -m http.server "$LOCAL_PORT" -d "$DIST_DIR"
    exit 0
fi

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
check_tool trunk "cargo install trunk"

echo "=== Building BIMIFC Unified Viewer ==="
echo "Viewer crate: $VIEWER_CRATE"
echo "Output:       $DIST_DIR"
echo ""

cd "$VIEWER_CRATE"

if [[ "$MODE" == "dev" ]]; then
    echo "Debug build..."
    trunk build
else
    echo "Release build..."
    trunk build --release
fi

# ---------------------------------------------------------------------------
# Brotli pre-compression
# ---------------------------------------------------------------------------
echo ""
if [[ "$HAVE_BROTLI" == "true" ]]; then
    echo "Pre-compressing with Brotli..."
    if command -v nproc &> /dev/null; then
        NCPU=$(nproc)
    elif command -v sysctl &> /dev/null; then
        NCPU=$(sysctl -n hw.ncpu 2>/dev/null || echo 4)
    else
        NCPU=4
    fi

    FILES_TO_COMPRESS=()
    for f in "$DIST_DIR/"*.wasm "$DIST_DIR/"*.js "$DIST_DIR/"*.css; do
        [[ -f "$f" ]] && FILES_TO_COMPRESS+=("$f")
    done

    if [[ ${#FILES_TO_COMPRESS[@]} -gt 0 ]]; then
        printf '%s\n' "${FILES_TO_COMPRESS[@]}" | xargs -P "$NCPU" -I {} brotli -f -q 11 {}
        echo "  Compressed ${#FILES_TO_COMPRESS[@]} files"
    fi
else
    echo "Brotli not found, skipping pre-compression."
    echo "  Install with: brew install brotli"
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "=== Build Complete ==="
echo ""

WASM_FILE=$(ls "$DIST_DIR/"*_bg.wasm 2>/dev/null | head -1)
JS_FILE=$(ls "$DIST_DIR/"*.js 2>/dev/null | head -1)
CSS_FILE=$(ls "$DIST_DIR/"*.css 2>/dev/null | head -1)

echo "Files:"
if [[ -n "$WASM_FILE" ]]; then
    WASM_SIZE=$(ls -lh "$WASM_FILE" | awk '{print $5}')
    echo "  WASM: $WASM_SIZE  $(basename "$WASM_FILE")"
    if [[ -f "${WASM_FILE}.br" ]]; then
        BR_SIZE=$(ls -lh "${WASM_FILE}.br" | awk '{print $5}')
        echo "        $BR_SIZE  $(basename "${WASM_FILE}.br")"
    fi
fi
if [[ -n "$JS_FILE" ]]; then
    echo "  JS:   $(ls -lh "$JS_FILE" | awk '{print $5}')  $(basename "$JS_FILE")"
fi
if [[ -n "$CSS_FILE" ]]; then
    echo "  CSS:  $(ls -lh "$CSS_FILE" | awk '{print $5}')  $(basename "$CSS_FILE")"
fi
echo ""

# ---------------------------------------------------------------------------
# Deploy / Serve
# ---------------------------------------------------------------------------
case "$MODE" in
    deploy)
        if [[ -z "$DEPLOY_TARGET" ]]; then
            echo "ERROR: No deploy target configured"
            exit 1
        fi

        # Optional: clean up old files on server
        if [[ "$2" == "clean" ]]; then
            DEPLOY_HOST="${DEPLOY_TARGET%%:*}"
            DEPLOY_PATH="${DEPLOY_TARGET#*:}"

            echo "Cleaning old files on server..."
            ssh "$DEPLOY_HOST" "
                cd '$DEPLOY_PATH' 2>/dev/null || exit 0
                rm -f *.wasm *.wasm.br *.js *.js.br *.css *.css.br 2>/dev/null
                rm -rf ifc/ 2>/dev/null
                echo 'Old files cleaned'
            " || echo "  (cleanup failed, continuing with deploy)"
        fi

        echo "=== Deploying to $DEPLOY_TARGET ==="
        echo ""
        rsync $RSYNC_FLAGS "$DIST_DIR/" "$DEPLOY_TARGET"
        echo ""
        echo "Deploy complete!"
        ;;
    dev|serve)
        echo "Serving on http://127.0.0.1:$LOCAL_PORT"
        echo "  (Ctrl+C to stop)"
        echo ""
        python3 -m http.server "$LOCAL_PORT" -d "$DIST_DIR"
        ;;
    build)
        echo "To serve locally:"
        echo "  $0 serve"
        echo "  # or: python3 -m http.server $LOCAL_PORT -d $DIST_DIR"
        echo ""
        echo "To deploy:"
        echo "  $0 deploy"
        ;;
esac
