#!/bin/bash
# Build split WASM architecture for BIMIFC viewer
#
# Produces two separate WASM modules:
#   1. Leptos WASM (UI + IFC parsing) — built by trunk from bimifc-viewer
#   2. Bevy WASM (3D renderer) — built by wasm-pack from bimifc-bevy
#
# The Bevy WASM is loaded lazily when the user opens a 3D view.
#
# Usage:
#   ./scripts/build-wasm-split.sh          # Build only
#   ./scripts/build-wasm-split.sh serve    # Build and serve on :8083
#   ./scripts/build-wasm-split.sh bevy     # Build only the Bevy WASM
#   ./scripts/build-wasm-split.sh leptos   # Build only the Leptos WASM

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
VIEWER_CRATE="$ROOT_DIR/crates/bimifc-viewer"
BEVY_CRATE="$ROOT_DIR/crates/bimifc-bevy"
DIST_DIR="$VIEWER_CRATE/dist"

MODE="${1:-build}"

# ---------------------------------------------------------------------------
# Prerequisites
# ---------------------------------------------------------------------------

check_tool() {
    if ! command -v "$1" &> /dev/null; then
        echo "ERROR: $1 not found. Install with:"
        echo "  $2"
        exit 1
    fi
}

# ---------------------------------------------------------------------------
# Build Bevy WASM (3D renderer)
# ---------------------------------------------------------------------------

build_bevy() {
    check_tool wasm-pack "cargo install wasm-pack"

    echo "=== Building Bevy WASM (3D renderer) ==="
    cd "$BEVY_CRATE"

    wasm-pack build \
        --target web \
        --release \
        --out-dir "$DIST_DIR/bevy" \
        --out-name bimifc_bevy \
        -- --no-default-features

    # Remove wasm-pack boilerplate
    rm -f "$DIST_DIR/bevy/.gitignore" "$DIST_DIR/bevy/package.json" "$DIST_DIR/bevy/README.md"

    # Hash the WASM for cache busting
    BEVY_JS="$DIST_DIR/bevy/bimifc_bevy.js"
    BEVY_WASM="$DIST_DIR/bevy/bimifc_bevy_bg.wasm"

    if [[ -f "$BEVY_WASM" ]]; then
        WASM_SIZE=$(wc -c < "$BEVY_WASM" | tr -d ' ')
        echo "  Bevy WASM: $(( WASM_SIZE / 1024 / 1024 ))MB ($WASM_SIZE bytes)"
    fi

    # Generate bevy-loader.js
    BEVY_JS_NAME=$(basename "$BEVY_JS")
    cat > "$DIST_DIR/bevy-loader.js" << LOADEREOF
// Lazy loader for Bevy 3D Scene Viewer
let bevyLoaded = false;
let bevyLoading = false;
let bevyLoadPromise = null;

async function loadBevyViewer() {
    if (bevyLoaded) return;
    if (bevyLoading && bevyLoadPromise) return bevyLoadPromise;

    bevyLoading = true;
    console.log("[Bevy] Loading 3D viewer...");

    bevyLoadPromise = (async () => {
        try {
            const bevy = await import('./bevy/${BEVY_JS_NAME}');
            await bevy.default();
            bevy.run_on_canvas("#bevy-canvas");
            bevyLoaded = true;
            bevyLoading = false;
            console.log("[Bevy] 3D viewer loaded successfully");
        } catch (error) {
            const errorStr = error.toString();
            if (errorStr.includes("Using exceptions for control flow") ||
                errorStr.includes("don't mind me")) {
                console.log("[Bevy] Ignoring control flow exception (not a real error)");
                bevyLoaded = true;
                bevyLoading = false;
                return;
            }
            console.error("[Bevy] Failed to load 3D viewer:", error);
            bevyLoading = false;
            bevyLoadPromise = null;
            throw error;
        }
    })();

    return bevyLoadPromise;
}

function isBevyLoaded() { return bevyLoaded; }
function isBevyLoading() { return bevyLoading; }

window.loadBevyViewer = loadBevyViewer;
window.isBevyLoaded = isBevyLoaded;
window.isBevyLoading = isBevyLoading;

console.log("[Bevy] Loader ready");
LOADEREOF

    echo "  Generated bevy-loader.js"
    echo ""
}

# ---------------------------------------------------------------------------
# Build Leptos WASM (UI + parsing)
# ---------------------------------------------------------------------------

build_leptos() {
    check_tool trunk "cargo install trunk"

    echo "=== Building Leptos WASM (UI + parsing) ==="

    # Save bevy directory if it exists (trunk wipes dist/)
    BEVY_BACKUP=""
    if [[ -d "$DIST_DIR/bevy" ]]; then
        BEVY_BACKUP=$(mktemp -d)
        cp -r "$DIST_DIR/bevy" "$BEVY_BACKUP/"
        if [[ -f "$DIST_DIR/bevy-loader.js" ]]; then
            cp "$DIST_DIR/bevy-loader.js" "$BEVY_BACKUP/"
        fi
        echo "  Backed up bevy/ before trunk build"
    fi

    cd "$VIEWER_CRATE"

    # Use the split-mode index.html (no isUnifiedMode, includes bevy-loader.js)
    ORIG_INDEX="$VIEWER_CRATE/index.html"
    SPLIT_INDEX="$VIEWER_CRATE/index-split.html"
    if [[ -f "$SPLIT_INDEX" ]]; then
        cp "$ORIG_INDEX" "$ORIG_INDEX.unified.bak"
        cp "$SPLIT_INDEX" "$ORIG_INDEX"
        echo "  Using split-mode index.html"
    fi

    trunk build --release

    # Restore original index.html
    if [[ -f "$ORIG_INDEX.unified.bak" ]]; then
        mv "$ORIG_INDEX.unified.bak" "$ORIG_INDEX"
    fi

    # Restore bevy directory after trunk build
    if [[ -n "$BEVY_BACKUP" && -d "$BEVY_BACKUP/bevy" ]]; then
        cp -r "$BEVY_BACKUP/bevy" "$DIST_DIR/"
        if [[ -f "$BEVY_BACKUP/bevy-loader.js" ]]; then
            cp "$BEVY_BACKUP/bevy-loader.js" "$DIST_DIR/"
        fi
        rm -rf "$BEVY_BACKUP"
        echo "  Restored bevy/ after trunk build"
    fi

    LEPTOS_WASM=$(ls "$DIST_DIR"/*.wasm 2>/dev/null | grep -v bevy | head -1)
    if [[ -f "$LEPTOS_WASM" ]]; then
        WASM_SIZE=$(wc -c < "$LEPTOS_WASM" | tr -d ' ')
        echo "  Leptos WASM: $(( WASM_SIZE / 1024 / 1024 ))MB ($WASM_SIZE bytes)"
    fi
    echo ""
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

echo "============================================"
echo "  BIMIFC Split WASM Builder"
echo "============================================"
echo "Dist: $DIST_DIR"
echo ""

case "$MODE" in
    bevy)
        build_bevy
        ;;
    leptos)
        build_leptos
        ;;
    build)
        build_bevy
        build_leptos
        echo "=== Build Complete ==="
        echo ""
        echo "To serve locally:"
        echo "  python3 -m http.server 8083 -d $DIST_DIR"
        ;;
    serve)
        build_bevy
        build_leptos
        echo "=== Build Complete ==="
        echo "Serving on http://127.0.0.1:8083"
        echo ""
        python3 -m http.server 8083 -d "$DIST_DIR"
        ;;
    *)
        echo "Usage: $0 [build|serve|bevy|leptos]"
        exit 1
        ;;
esac

echo ""
echo "Files in dist/:"
ls -lh "$DIST_DIR"/*.wasm "$DIST_DIR"/*.js "$DIST_DIR"/bevy/*.wasm 2>/dev/null || true
