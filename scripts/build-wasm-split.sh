#!/bin/bash
# WASM split bundle builder with Brotli pre-compression
#
# Reads configuration from build-config.toml in the same directory.
#
# Usage:
#   ./build-wasm-split.sh          # Build only
#   ./build-wasm-split.sh deploy   # Build and deploy via rsync
#   ./build-wasm-split.sh serve    # Build and serve locally
#   ./build-wasm-split.sh --help   # Show help
#
# The split architecture ensures fast initial page load while still
# providing full 3D visualization capabilities when needed.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
CONFIG_FILE="$SCRIPT_DIR/build-config.toml"

# =============================================================================
# TOML Parser (simple key=value extraction)
# =============================================================================

# Read a value from TOML config
# Usage: toml_get "section.key" [default_value]
toml_get() {
    local key="$1"
    local default="$2"
    local section=""
    local field=""

    # Split key into section and field (e.g., "project.name" -> "project" "name")
    if [[ "$key" == *.* ]]; then
        section="${key%.*}"
        field="${key##*.}"
    else
        field="$key"
    fi

    local in_section=false
    local current_section=""

    while IFS= read -r line || [[ -n "$line" ]]; do
        # Skip comments and empty lines
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "${line// }" ]] && continue

        # Check for section header [section] or [section.subsection]
        if [[ "$line" =~ ^\[([a-zA-Z0-9._-]+)\] ]]; then
            current_section="${BASH_REMATCH[1]}"
            if [[ -z "$section" ]] || [[ "$current_section" == "$section" ]] || [[ "$current_section" == "$section."* ]]; then
                in_section=true
            else
                in_section=false
            fi
            continue
        fi

        # If we're in the right section (or no section specified), look for the field
        if [[ "$in_section" == true ]] || [[ -z "$section" && -z "$current_section" ]]; then
            # Match key = value or key = "value"
            if [[ "$line" =~ ^[[:space:]]*${field}[[:space:]]*=[[:space:]]*\"([^\"]*)\" ]]; then
                echo "${BASH_REMATCH[1]}"
                return 0
            elif [[ "$line" =~ ^[[:space:]]*${field}[[:space:]]*=[[:space:]]*([^[:space:]#]+) ]]; then
                echo "${BASH_REMATCH[1]}"
                return 0
            fi
        fi
    done < "$CONFIG_FILE"

    echo "$default"
}

# Read array from TOML (simple format: key = ["a", "b"])
toml_get_array() {
    local key="$1"
    local section="${key%.*}"
    local field="${key##*.}"

    local in_section=false
    local current_section=""

    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ "$line" =~ ^[[:space:]]*# ]] && continue
        [[ -z "${line// }" ]] && continue

        if [[ "$line" =~ ^\[([a-zA-Z0-9._-]+)\] ]]; then
            current_section="${BASH_REMATCH[1]}"
            [[ "$current_section" == "$section" ]] && in_section=true || in_section=false
            continue
        fi

        if [[ "$in_section" == true ]]; then
            if [[ "$line" =~ ^[[:space:]]*${field}[[:space:]]*=[[:space:]]*\[(.+)\] ]]; then
                # Extract array elements
                local array_content="${BASH_REMATCH[1]}"
                # Remove quotes and split by comma
                echo "$array_content" | tr ',' '\n' | sed 's/[" ]//g'
                return 0
            fi
        fi
    done < "$CONFIG_FILE"
}

# =============================================================================
# Load Configuration
# =============================================================================

if [[ ! -f "$CONFIG_FILE" ]]; then
    echo "ERROR: Configuration file not found: $CONFIG_FILE"
    echo ""
    echo "Create a build-config.toml file with your project settings."
    exit 1
fi

# Project settings
PROJECT_NAME=$(toml_get "project.name" "app")
PROJECT_DISPLAY=$(toml_get "project.display_name" "$PROJECT_NAME")

# Paths (relative to ROOT_DIR)
WASM_CRATE=$(toml_get "paths.wasm_crate" "crates/wasm")
BEVY_CRATE=$(toml_get "paths.bevy_crate" "crates/bevy")
BEVY_OUTPUT_REL=$(toml_get "paths.bevy_output" "target/wasm32-unknown-unknown/web-release")
DIST_OUTPUT_REL=$(toml_get "paths.dist_output" "$WASM_CRATE/dist")
WATCH_PATHS_RAW=$(toml_get_array "paths.watch_paths")

# Absolute paths
WASM_DIR="$ROOT_DIR/$WASM_CRATE"
BEVY_DIR="$ROOT_DIR/$BEVY_CRATE"
BEVY_OUTPUT="$ROOT_DIR/$BEVY_OUTPUT_REL"
DIST_DIR="$ROOT_DIR/$DIST_OUTPUT_REL"

# Build array of watch directories
WATCH_DIRS=()
while IFS= read -r path; do
    [[ -n "$path" ]] && WATCH_DIRS+=("$ROOT_DIR/$path")
done <<< "$WATCH_PATHS_RAW"

# Bevy settings
BEVY_LIBRARY=$(toml_get "bevy.library_name" "${PROJECT_NAME}_bevy")
BEVY_BINARY=$(toml_get "bevy.binary_name" "${PROJECT_NAME}-3d")
BEVY_FEATURES=$(toml_get_array "bevy.features" | tr '\n' ',' | sed 's/,$//')

# Bundle flags
BUILD_LEPTOS=$(toml_get "bundles.leptos" "true")
BUILD_BEVY=$(toml_get "bundles.bevy" "true")

# Deploy settings
DEPLOY_TARGET=$(toml_get "deploy.target" "")
RSYNC_FLAGS=$(toml_get "deploy.rsync_flags" "-avz")
LOCAL_PORT=$(toml_get "deploy.local.port" "8083")

# =============================================================================
# Check for tools
# =============================================================================

HAVE_BROTLI=false
if command -v brotli &> /dev/null; then
    HAVE_BROTLI=true
fi

# =============================================================================
# Hash caching for incremental builds
# =============================================================================

CACHE_FILE="$ROOT_DIR/target/.wasm-build-cache"
FORCE_REBUILD=false

# Calculate hash of source files for a crate
calculate_source_hash() {
    local crate_dir="$1"
    if [[ ! -d "$crate_dir" ]]; then
        echo "0"
        return
    fi
    find "$crate_dir/src" -name "*.rs" -type f 2>/dev/null | sort | xargs cat 2>/dev/null | \
        cat - "$crate_dir/Cargo.toml" 2>/dev/null | \
        if command -v md5sum &> /dev/null; then md5sum | cut -c1-16; else md5 -q | cut -c1-16; fi
}

get_cached_hash() {
    local component="$1"
    if [[ -f "$CACHE_FILE" ]]; then
        grep "^${component}=" "$CACHE_FILE" 2>/dev/null | cut -d'=' -f2
    fi
}

save_hash() {
    local component="$1"
    local hash="$2"
    mkdir -p "$(dirname "$CACHE_FILE")"
    if [[ -f "$CACHE_FILE" ]]; then
        grep -v "^${component}=" "$CACHE_FILE" > "${CACHE_FILE}.tmp" 2>/dev/null || true
        mv "${CACHE_FILE}.tmp" "$CACHE_FILE"
    fi
    echo "${component}=${hash}" >> "$CACHE_FILE"
}

needs_rebuild() {
    local component="$1"
    local crate_dir="$2"

    if [[ "$FORCE_REBUILD" == "true" ]]; then
        return 0
    fi

    local current_hash=$(calculate_source_hash "$crate_dir")
    local cached_hash=$(get_cached_hash "$component")

    if [[ "$current_hash" == "$cached_hash" ]] && [[ -n "$cached_hash" ]]; then
        return 1  # No rebuild needed
    fi
    return 0  # Rebuild needed
}

# =============================================================================
# Command line handling
# =============================================================================

ACTION="build"
if [[ "$1" == "deploy" ]]; then
    ACTION="deploy"
elif [[ "$1" == "serve" ]]; then
    ACTION="serve"
elif [[ "$1" == "serve-only" ]]; then
    ACTION="serve-only"
elif [[ "$1" == "force" ]]; then
    FORCE_REBUILD=true
    ACTION="build"
elif [[ "$1" == "bevy" ]]; then
    FORCE_REBUILD=true
    BUILD_LEPTOS="false"
    ACTION="build"
elif [[ "$1" == "leptos" ]]; then
    FORCE_REBUILD=true
    BUILD_BEVY="false"
    ACTION="build"
elif [[ "$1" == "clean" ]]; then
    echo "Cleaning build cache..."
    rm -f "$CACHE_FILE"
    rm -rf "$DIST_DIR"
    echo "Done."
    exit 0
elif [[ "$1" == "--help" ]] || [[ "$1" == "-h" ]]; then
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  (none)      Build WASM bundles (incremental - skips unchanged)"
    echo "  force       Force rebuild all bundles (ignore cache)"
    echo "  bevy        Force rebuild only Bevy WASM"
    echo "  leptos      Force rebuild only Leptos WASM"
    echo "  deploy      Build and deploy via rsync to configured target"
    echo "  serve       Build and start local development server"
    echo "  serve-only  Start local server without rebuilding"
    echo "  clean       Remove build cache and dist directory"
    echo "  --help      Show this help"
    echo ""
    echo "Configuration: $CONFIG_FILE"
    echo ""
    echo "Project: $PROJECT_DISPLAY"
    echo "Output:  $DIST_DIR"
    if [[ -n "$DEPLOY_TARGET" ]]; then
        echo "Deploy:  $DEPLOY_TARGET"
    fi
    exit 0
fi

# serve-only: skip build process entirely
if [[ "$ACTION" == "serve-only" ]]; then
    echo "=== Starting local server on port $LOCAL_PORT (no rebuild) ==="
    echo ""
    if [[ ! -d "$DIST_DIR" ]]; then
        echo "ERROR: Dist directory not found: $DIST_DIR"
        echo "Run '$0' first to build."
        exit 1
    fi
    echo "Running: python3 -m http.server $LOCAL_PORT -d $DIST_DIR"
    echo "Open: http://localhost:$LOCAL_PORT"
    echo ""
    python3 -m http.server "$LOCAL_PORT" -d "$DIST_DIR"
    exit 0
fi

# =============================================================================
# Build Process
# =============================================================================

echo "=== Building $PROJECT_DISPLAY Split WASM ==="
echo ""
if [[ "$BUILD_LEPTOS" == "true" ]]; then
    echo "  Bundle 1: Leptos viewer UI (loads immediately)"
fi
if [[ "$BUILD_BEVY" == "true" ]]; then
    echo "  Bundle 2: Bevy 3D renderer (loads on demand)"
fi
if [[ "$HAVE_BROTLI" == "true" ]]; then
    echo ""
    echo "  Brotli pre-compression: enabled"
fi
echo ""

STEP=1
TOTAL_STEPS=6

# Track what was built
BEVY_BUILT=false
LEPTOS_BUILT=false

# -----------------------------------------------------------------------------
# Step 1: Build Bevy 3D viewer
# -----------------------------------------------------------------------------
if [[ "$BUILD_BEVY" == "true" ]]; then
    # Check all watch directories for changes
    BEVY_NEEDS_BUILD=false
    if needs_rebuild "bevy" "$BEVY_DIR"; then
        BEVY_NEEDS_BUILD=true
    else
        for i in "${!WATCH_DIRS[@]}"; do
            watch_dir="${WATCH_DIRS[$i]}"
            if [[ -d "$watch_dir" ]] && needs_rebuild "watch-$i" "$watch_dir"; then
                BEVY_NEEDS_BUILD=true
                break
            fi
        done
    fi
    if [[ "$BEVY_NEEDS_BUILD" != "true" ]] && [[ ! -f "$BEVY_OUTPUT/${BEVY_LIBRARY}.js" ]]; then
        BEVY_NEEDS_BUILD=true
    fi

    if [[ "$BEVY_NEEDS_BUILD" == "true" ]]; then
        echo "[$STEP/$TOTAL_STEPS] Building Bevy 3D viewer..."
        cd "$BEVY_DIR"

        FEATURE_FLAG=""
        if [[ -n "$BEVY_FEATURES" ]]; then
            FEATURE_FLAG="--features $BEVY_FEATURES"
        fi

        # Build the library (cdylib) for WASM — no default features (external-ui mode)
        echo "  Building library: $BEVY_LIBRARY"
        cargo build --lib --release $FEATURE_FLAG --no-default-features --target wasm32-unknown-unknown
        mkdir -p "$BEVY_OUTPUT"
        wasm-bindgen --out-dir "$BEVY_OUTPUT" --target web \
            "$ROOT_DIR/target/wasm32-unknown-unknown/release/${BEVY_LIBRARY}.wasm"

        if command -v wasm-opt &> /dev/null && [[ -f "$BEVY_OUTPUT/${BEVY_LIBRARY}_bg.wasm" ]]; then
            echo "  Running wasm-opt..."
            wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals \
                -o "$BEVY_OUTPUT/${BEVY_LIBRARY}_bg_opt.wasm" "$BEVY_OUTPUT/${BEVY_LIBRARY}_bg.wasm" || {
                echo "  wasm-opt failed, using unoptimized WASM"
            }
            [[ -f "$BEVY_OUTPUT/${BEVY_LIBRARY}_bg_opt.wasm" ]] && \
                mv "$BEVY_OUTPUT/${BEVY_LIBRARY}_bg_opt.wasm" "$BEVY_OUTPUT/${BEVY_LIBRARY}_bg.wasm"
        fi

        # Save hashes after successful build
        save_hash "bevy" "$(calculate_source_hash "$BEVY_DIR")"
        for i in "${!WATCH_DIRS[@]}"; do
            watch_dir="${WATCH_DIRS[$i]}"
            [[ -d "$watch_dir" ]] && save_hash "watch-$i" "$(calculate_source_hash "$watch_dir")"
        done
        BEVY_BUILT=true
        echo ""
    else
        echo "[$STEP/$TOTAL_STEPS] Bevy 3D viewer: unchanged, skipping build"
    fi
fi
((STEP++))

# -----------------------------------------------------------------------------
# Step 2: Build Leptos viewer
# -----------------------------------------------------------------------------
if [[ "$BUILD_LEPTOS" == "true" ]]; then
    LEPTOS_NEEDS_BUILD=false
    if needs_rebuild "leptos" "$WASM_DIR"; then
        LEPTOS_NEEDS_BUILD=true
    else
        for i in "${!WATCH_DIRS[@]}"; do
            watch_dir="${WATCH_DIRS[$i]}"
            if [[ -d "$watch_dir" ]] && needs_rebuild "watch-$i" "$watch_dir"; then
                LEPTOS_NEEDS_BUILD=true
                break
            fi
        done
    fi
    if [[ "$LEPTOS_NEEDS_BUILD" != "true" ]] && { [[ ! -d "$DIST_DIR" ]] || [[ -z "$(ls -A "$DIST_DIR"/*.wasm 2>/dev/null)" ]]; }; then
        LEPTOS_NEEDS_BUILD=true
    fi

    if [[ "$LEPTOS_NEEDS_BUILD" == "true" ]]; then
        echo "[$STEP/$TOTAL_STEPS] Building Leptos viewer..."

        # Save bevy directory if it exists (trunk wipes dist/)
        BEVY_BACKUP=""
        if [[ -d "$DIST_DIR/bevy" ]]; then
            BEVY_BACKUP=$(mktemp -d)
            cp -r "$DIST_DIR/bevy" "$BEVY_BACKUP/"
            echo "  Backed up bevy/ before trunk build"
        fi

        cd "$WASM_DIR"

        # Use the split-mode index.html (no isUnifiedMode, includes bevy-loader.js)
        ORIG_INDEX="$WASM_DIR/index.html"
        SPLIT_INDEX="$WASM_DIR/index-split.html"
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
            rm -rf "$BEVY_BACKUP"
            echo "  Restored bevy/ after trunk build"
        fi

        # Save hash after successful build
        save_hash "leptos" "$(calculate_source_hash "$WASM_DIR")"
        for i in "${!WATCH_DIRS[@]}"; do
            watch_dir="${WATCH_DIRS[$i]}"
            [[ -d "$watch_dir" ]] && save_hash "watch-$i" "$(calculate_source_hash "$watch_dir")"
        done
        LEPTOS_BUILT=true
        echo ""
    else
        echo "[$STEP/$TOTAL_STEPS] Leptos viewer: unchanged, skipping build"
    fi
fi
((STEP++))

# -----------------------------------------------------------------------------
# Step 3: Add content hashes to Bevy files
# -----------------------------------------------------------------------------
if [[ "$BUILD_BEVY" == "true" ]]; then
    if [[ "$BEVY_BUILT" == "true" ]] || [[ -z "$(ls -A "$DIST_DIR/bevy/"*.wasm 2>/dev/null)" ]]; then
        echo "[$STEP/$TOTAL_STEPS] Adding content hashes to Bevy files..."
        mkdir -p "$DIST_DIR/bevy"

        rm -f "$DIST_DIR/bevy/"*.js "$DIST_DIR/bevy/"*.wasm "$DIST_DIR/bevy/"*.br

        if command -v md5sum &> /dev/null; then
            JS_HASH=$(md5sum "$BEVY_OUTPUT/${BEVY_LIBRARY}.js" | cut -c1-16)
            WASM_HASH=$(md5sum "$BEVY_OUTPUT/${BEVY_LIBRARY}_bg.wasm" | cut -c1-16)
        else
            JS_HASH=$(md5 -q "$BEVY_OUTPUT/${BEVY_LIBRARY}.js" | cut -c1-16)
            WASM_HASH=$(md5 -q "$BEVY_OUTPUT/${BEVY_LIBRARY}_bg.wasm" | cut -c1-16)
        fi

        cp "$BEVY_OUTPUT/${BEVY_LIBRARY}.js" "$DIST_DIR/bevy/${BEVY_LIBRARY}-${JS_HASH}.js"
        cp "$BEVY_OUTPUT/${BEVY_LIBRARY}_bg.wasm" "$DIST_DIR/bevy/${BEVY_LIBRARY}-${WASM_HASH}_bg.wasm"

        # Update JS to reference hashed WASM
        if [[ "$(uname)" == "Darwin" ]]; then
            sed -i '' "s/${BEVY_LIBRARY}_bg.wasm/${BEVY_LIBRARY}-${WASM_HASH}_bg.wasm/g" "$DIST_DIR/bevy/${BEVY_LIBRARY}-${JS_HASH}.js"
        else
            sed -i "s/${BEVY_LIBRARY}_bg.wasm/${BEVY_LIBRARY}-${WASM_HASH}_bg.wasm/g" "$DIST_DIR/bevy/${BEVY_LIBRARY}-${JS_HASH}.js"
        fi
        echo "  ${BEVY_LIBRARY}-${JS_HASH}.js"
        echo "  ${BEVY_LIBRARY}-${WASM_HASH}_bg.wasm"
        echo ""
    else
        echo "[$STEP/$TOTAL_STEPS] Bevy files: unchanged, using cached hashes"
        # Extract existing hashes from dist/bevy filenames
        JS_HASH=$(ls "$DIST_DIR/bevy/${BEVY_LIBRARY}"-*.js 2>/dev/null | head -1 | sed "s/.*${BEVY_LIBRARY}-\([^.]*\)\.js/\1/")
        WASM_HASH=$(ls "$DIST_DIR/bevy/${BEVY_LIBRARY}"-*_bg.wasm 2>/dev/null | head -1 | sed "s/.*${BEVY_LIBRARY}-\([^_]*\)_bg\.wasm/\1/")
    fi
fi
((STEP++))

# -----------------------------------------------------------------------------
# Step 4: Generate bevy-loader.js
# -----------------------------------------------------------------------------
if [[ "$BUILD_BEVY" == "true" ]]; then
    # Check if loader already exists with correct hash reference
    EXISTING_LOADER=$(ls "$DIST_DIR/bevy-loader-"*.js 2>/dev/null | head -1)
    if [[ -n "$EXISTING_LOADER" ]] && grep -q "${BEVY_LIBRARY}-${JS_HASH}.js" "$EXISTING_LOADER" 2>/dev/null; then
        echo "[$STEP/$TOTAL_STEPS] bevy-loader.js: unchanged, skipping"
        BEVY_LOADER_HASH=$(echo "$EXISTING_LOADER" | sed 's/.*bevy-loader-\([^.]*\)\.js/\1/')
    else
        echo "[$STEP/$TOTAL_STEPS] Generating bevy-loader.js..."
        # Remove old loaders
        rm -f "$DIST_DIR/bevy-loader-"*.js "$DIST_DIR/bevy-loader-"*.js.br "$DIST_DIR/bevy-loader.js"

        cat > "$DIST_DIR/bevy-loader-temp.js" << 'JSEOF'
// Lazy loader for Bevy 3D Scene Viewer
// Auto-generated with content hashes for cache busting

let bevyLoaded = false;
let bevyLoading = false;
let bevyLoadPromise = null;

JSEOF
        # Append the dynamic part with variable substitution
        cat >> "$DIST_DIR/bevy-loader-temp.js" << EOF
async function loadBevyViewer() {
    if (bevyLoaded) {
        console.log("[Bevy] Already loaded");
        return;
    }
    if (bevyLoading && bevyLoadPromise) {
        console.log("[Bevy] Loading in progress, waiting...");
        return bevyLoadPromise;
    }

    bevyLoading = true;
    console.log("[Bevy] Loading 3D viewer...");

    bevyLoadPromise = (async () => {
        try {
            const bevy = await import('./bevy/${BEVY_LIBRARY}-${JS_HASH}.js');
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

console.log("[Bevy] Loader ready (JS: ${JS_HASH}, WASM: ${WASM_HASH})");
EOF

        if command -v md5sum &> /dev/null; then
            BEVY_LOADER_HASH=$(md5sum "$DIST_DIR/bevy-loader-temp.js" | cut -c1-16)
        else
            BEVY_LOADER_HASH=$(md5 -q "$DIST_DIR/bevy-loader-temp.js" | cut -c1-16)
        fi
        mv "$DIST_DIR/bevy-loader-temp.js" "$DIST_DIR/bevy-loader-${BEVY_LOADER_HASH}.js"
        echo "  bevy-loader-${BEVY_LOADER_HASH}.js"
    fi
fi
((STEP++))

# -----------------------------------------------------------------------------
# Step 5: Update index.html with hashed loader filename
# -----------------------------------------------------------------------------
echo "[$STEP/$TOTAL_STEPS] Updating index.html with hashed loader filenames..."

SED_INPLACE=(-i '')
[[ "$(uname)" != "Darwin" ]] && SED_INPLACE=(-i)

if [[ "$BUILD_BEVY" == "true" ]] && [[ -n "$BEVY_LOADER_HASH" ]]; then
    # Handle all cases: unhashed, already hashed with old hash, with query string
    sed "${SED_INPLACE[@]}" "s|bevy-loader\(-[a-f0-9]*\)\{0,1\}\.js\(\?v=[0-9]*\)\{0,1\}\"|bevy-loader-${BEVY_LOADER_HASH}.js\"|g" "$DIST_DIR/index.html"
    sed "${SED_INPLACE[@]}" "s|bevy-loader\(-[a-f0-9]*\)\{0,1\}\.js\(\?v=[0-9]*\)\{0,1\}\">|bevy-loader-${BEVY_LOADER_HASH}.js\">|g" "$DIST_DIR/index.html"
    echo "  bevy-loader -> bevy-loader-${BEVY_LOADER_HASH}.js"
fi

echo "  Updated loader references in index.html"
echo ""
((STEP++))

# -----------------------------------------------------------------------------
# Step 6: Pre-compress with Brotli
# -----------------------------------------------------------------------------
echo "[$STEP/$TOTAL_STEPS] Pre-compressing with Brotli..."

if [[ "$HAVE_BROTLI" == "true" ]]; then
    if command -v nproc &> /dev/null; then
        NCPU=$(nproc)
    elif command -v sysctl &> /dev/null; then
        NCPU=$(sysctl -n hw.ncpu 2>/dev/null || echo 4)
    else
        NCPU=4
    fi
    echo "  Using $NCPU parallel jobs..."

    FILES_TO_COMPRESS=()

    # Root level files
    for f in "$DIST_DIR/"*.wasm "$DIST_DIR/"*.js "$DIST_DIR/"*.css; do
        [[ -f "$f" ]] && FILES_TO_COMPRESS+=("$f")
    done

    # Subdirectory files (bevy/)
    for f in "$DIST_DIR/bevy/"*.wasm "$DIST_DIR/bevy/"*.js; do
        [[ -f "$f" ]] && FILES_TO_COMPRESS+=("$f")
    done

    echo "  Compressing ${#FILES_TO_COMPRESS[@]} files in parallel..."
    printf '%s\n' "${FILES_TO_COMPRESS[@]}" | xargs -P "$NCPU" -I {} brotli -f -q 11 {}

    echo "  Done!"
else
    echo "  brotli not found, skipping pre-compression."
    echo "  Install with: brew install brotli"
fi
echo ""

# =============================================================================
# Summary
# =============================================================================

echo "=== Build Complete ==="
echo ""

# Show what was rebuilt vs cached
echo "Build status:"
if [[ "$BEVY_BUILT" == "true" ]]; then
    echo "  Bevy 3D viewer:     REBUILT"
elif [[ "$BUILD_BEVY" == "true" ]]; then
    echo "  Bevy 3D viewer:     cached (unchanged)"
fi
if [[ "$LEPTOS_BUILT" == "true" ]]; then
    echo "  Leptos viewer:      REBUILT"
elif [[ "$BUILD_LEPTOS" == "true" ]]; then
    echo "  Leptos viewer:      cached (unchanged)"
fi
echo ""

# Bundle sizes
LEPTOS_WASM=$(ls "$DIST_DIR/"*_bg.wasm 2>/dev/null | grep -v bevy | head -1)
BEVY_WASM_FILE=$(ls "$DIST_DIR/bevy/"*_bg.wasm 2>/dev/null | head -1)

LEPTOS_SIZE=$(ls -lh "$LEPTOS_WASM" 2>/dev/null | awk '{print $5}')
BEVY_SIZE=$(ls -lh "$BEVY_WASM_FILE" 2>/dev/null | awk '{print $5}')

echo "Bundle sizes (raw / brotli):"
if [[ "$HAVE_BROTLI" == "true" ]]; then
    LEPTOS_BR=$(ls -lh "${LEPTOS_WASM}.br" 2>/dev/null | awk '{print $5}')
    BEVY_BR=$(ls -lh "${BEVY_WASM_FILE}.br" 2>/dev/null | awk '{print $5}')

    [[ -n "$LEPTOS_SIZE" ]] && echo "  Leptos viewer:      $LEPTOS_SIZE -> $LEPTOS_BR (loads immediately)"
    [[ -n "$BEVY_SIZE" ]] && echo "  Bevy 3D viewer:     $BEVY_SIZE -> $BEVY_BR (loads on demand)"
else
    [[ -n "$LEPTOS_SIZE" ]] && echo "  Leptos viewer:      $LEPTOS_SIZE (loads immediately)"
    [[ -n "$BEVY_SIZE" ]] && echo "  Bevy 3D viewer:     $BEVY_SIZE (loads on demand)"
fi
echo ""

if [[ "$BUILD_BEVY" == "true" ]]; then
    echo "Hashed filenames:"
    echo "  Bevy:  ${BEVY_LIBRARY}-${JS_HASH}.js / ${BEVY_LIBRARY}-${WASM_HASH}_bg.wasm"
fi
echo ""
echo "Output: $DIST_DIR"
echo ""

# =============================================================================
# Deploy / Serve
# =============================================================================

if [[ "$ACTION" == "deploy" ]]; then
    echo ""
    if [[ -z "$DEPLOY_TARGET" ]]; then
        echo "ERROR: No deploy target configured in build-config.toml"
        echo "Add [deploy] section with target = \"user@host:/path/\""
        exit 1
    fi

    echo "=== Deploying to $DEPLOY_TARGET ==="

    # Optional: clean up old hashed files on server before deploying
    if [[ "$2" == "clean" ]]; then
        DEPLOY_HOST="${DEPLOY_TARGET%%:*}"
        DEPLOY_PATH="${DEPLOY_TARGET#*:}"

        echo "Cleaning up old hashed files on server..."
        ssh "$DEPLOY_HOST" "
            cd '$DEPLOY_PATH' 2>/dev/null || exit 0
            find . -maxdepth 1 -name 'bevy-loader-*.js' -type f -delete 2>/dev/null
            find . -maxdepth 1 -name 'bevy-loader-*.js.br' -type f -delete 2>/dev/null
            rm -rf bevy/ 2>/dev/null
            echo 'Old files cleaned'
        " || echo "  (cleanup failed, continuing with deploy)"
    fi

    echo "Running: rsync $RSYNC_FLAGS $DIST_DIR/ $DEPLOY_TARGET"
    echo ""
    rsync $RSYNC_FLAGS "$DIST_DIR/" "$DEPLOY_TARGET"
    echo ""
    echo "Deploy complete!"

elif [[ "$ACTION" == "serve" ]]; then
    echo ""
    echo "=== Starting local server on port $LOCAL_PORT ==="
    echo ""
    echo "Running: python3 -m http.server $LOCAL_PORT -d $DIST_DIR"
    echo "Open: http://localhost:$LOCAL_PORT"
    echo ""
    python3 -m http.server "$LOCAL_PORT" -d "$DIST_DIR"

else
    # Just show instructions
    if [[ -n "$DEPLOY_TARGET" ]]; then
        echo "To deploy to $DEPLOY_TARGET:"
        echo "  $0 deploy"
        echo "  $0 deploy clean   # clean old files first"
        echo ""
    fi
    echo "To serve locally:"
    echo "  $0 serve"
    echo "  # or: python3 -m http.server $LOCAL_PORT -d $DIST_DIR"
    echo "  open http://localhost:$LOCAL_PORT"
fi
