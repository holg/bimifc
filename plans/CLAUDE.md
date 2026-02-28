# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build                              # Build all crates
cargo build --release                    # Release build (LTO enabled)
cargo test                               # Run all tests
cargo test -p bimifc-parser              # Test a specific crate
cargo test -p bimifc-parser test_name    # Run a single test
cargo clippy --workspace                 # Lint all crates
cargo run -p bimifc-viewer-tui -- model.ifc  # Run TUI viewer
```

## Architecture

Seven-crate workspace for parsing IFC (Industry Foundation Classes) BIM files and rendering them in 3D.

### Crate Dependency Layers

```
bimifc-model          ← Foundation: traits (IfcParser, IfcModel, EntityResolver, SpatialQuery,
                         PropertyReader, GeometrySource) and types (EntityId, IfcType, AttributeValue)

bimifc-parser         ← Implements model traits; IFC4 STEP parser + IFC5 IFCX JSON parser
bimifc-geometry       ← Converts IFC geometry to triangle meshes using EntityResolver trait

bimifc-bevy           ← Bevy 0.18 renderer (WebGPU/WebGL2), depends on parser + geometry
bimifc-leptos         ← Leptos 0.7 reactive UI, depends on parser + geometry

bimifc-viewer         ← Web app combining Leptos + Bevy (cdylib for WASM)
bimifc-viewer-tui     ← Terminal viewer using ratatui, depends on parser + geometry
```

### Key Design Patterns

- **Trait-based abstraction**: `bimifc-model` defines all interfaces; parser and geometry crates code against traits, not concrete types. This keeps geometry processing backend-agnostic.
- **Lazy entity decoding**: The STEP parser scans entity locations with SIMD (`memchr`) during initial parse, but only decodes entity attributes on demand via `EntityResolver`.
- **Arc-based sharing**: Decoded entities and mesh geometry are wrapped in `Arc` for zero-copy sharing across components.
- **Batch rendering**: `bimifc-bevy` merges individual entity meshes into 2-3 draw calls (opaque/transparent batches) rather than one draw call per entity.
- **Unified mode**: Optional feature where Leptos and Bevy share memory directly in WASM instead of serializing through JS.

### Parser Pipeline (bimifc-parser)

`scanner.rs` (SIMD entity location) → `tokenizer.rs` (nom-based tokenization) → `decoder.rs` (attribute extraction) → `resolver.rs` (FxHashMap entity index) → `spatial.rs` (hierarchy tree) → `properties.rs` (property sets)

Format auto-detection between STEP and IFCX is in `lib.rs` via `parse_auto()`.

### Geometry Pipeline (bimifc-geometry)

`router.rs` dispatches to type-specific processors → `profile.rs` builds 2D profiles → `extrusion.rs` generates 3D meshes → `triangulation.rs` handles polygon triangulation (via `earcutr`) → `mesh.rs` computes normals and produces GPU-ready data.

### Platform Compilation

`bimifc-bevy` uses conditional compilation (`#[cfg(target_arch = "wasm32")]`, platform-specific features) for WASM, macOS/iOS native, and desktop targets. The viewer crate builds as a `cdylib` for WASM deployment.

## Test Data

IFC test models live in `tests/models/` (IFC4 STEP files) and `tests/models/ifc5/` (IFCX JSON files). Integration tests are in `crates/bimifc-parser/tests/`.
