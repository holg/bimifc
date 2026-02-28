# Upstream Update Plan: ifc-lite → bimifc

**Date:** 2026-02-28
**Upstream:** `louistrue/ifc-lite` (119 commits ahead of local `main`, 191 non-merge commits total)
**Target:** `holg/bimifc` (pure Rust, 7-crate workspace)
**Branch:** `own-parser-leptos` (28 commits ahead of upstream/main, significant divergence)

---

## Executive Summary

The upstream ifc-lite project has evolved substantially since the fork. Key improvements span **geometry processing** (modular processors, void handling, mesh deduplication), **Rust core** (model bounds, transforms, GPU geometry, symbolic representations), and **JS/TS packages** (IFC5 export, BabylonJS viewer, mesh batching, federated models). This plan focuses on **Rust-side changes** that directly benefit bimifc's pure-Rust architecture.

---

## 1. Rust Core Changes (HIGH PRIORITY)

### 1.1 Model Bounds (`rust/core/src/model_bounds.rs` — NEW, 427 lines)

**What:** Computes AABB bounds for entire IFC models during parsing. Enables camera fitting, RTC offset calculation, and georeferencing without a full geometry pass.

**Action:** Create `bimifc-model/src/bounds.rs`
- Add `ModelBounds` struct with min/max `Point3<f64>`
- Integrate with parser to accumulate bounds during entity scan
- Use for initial camera placement in bimifc-bevy

### 1.2 Geometry Transform Module (`rust/geometry/src/transform.rs` — NEW, 326 lines)

**What:** Centralized 4×4 matrix operations for IFC placement chains: `IfcLocalPlacement → IfcAxis2Placement3D → transform matrix`. Handles nested placements, unit scaling, and coordinate system conversions.

**Action:** Create `bimifc-geometry/src/transform.rs`
- `placement_to_matrix(entity, resolver) -> Mat4`
- `axis2placement3d_to_matrix(location, axis, ref_direction) -> Mat4`
- Chain parent placements recursively
- Apply `unit_scale` from IFCPROJECT

### 1.3 Unit Scale Extraction

**What:** Upstream extracts length unit from `IFCPROJECT → IFCUNITASSIGNMENT → IFCSIUNIT` and applies uniformly. Many IFC files use millimeters; without this, geometry is 1000× wrong.

**Action:** Add `extract_unit_scale(model) -> f64` to `bimifc-parser`
- Scan for `IfcProject` → `UnitsInContext` → find `IfcSIUnit(LENGTHUNIT)`
- Check for `IfcConversionBasedUnit` (feet, inches)
- Default to 1.0 (meters) if not found

---

## 2. Geometry Pipeline Overhaul (HIGH PRIORITY)

### 2.1 Modular Processor Architecture

**What:** Upstream split the monolithic `processors.rs` (2540 lines) into dedicated modules. Each processor implements a `GeometryProcessor` trait.

**Upstream structure:**
```
rust/geometry/src/processors/
├── mod.rs          — trait + registry (168 lines)
├── extrusion.rs    — IfcExtrudedAreaSolid (186 lines)
├── tessellated.rs  — IfcTriangulatedFaceSet (661 lines, fast-path parsing)
├── brep.rs         — IfcFacetedBrep (766 lines)
├── swept.rs        — IfcSweptDiskSolid (402 lines)
├── advanced.rs     — IfcAdvancedBrep (698 lines)
├── mapped.rs       — IfcMappedItem instancing (115 lines)
├── boolean.rs      — CSG operations (316 lines)
├── surface.rs      — Surface models (241 lines)
├── helpers.rs      — Shared utilities (277 lines)
└── tests.rs        — Integration tests (208 lines)
```

**Action:** Refactor `bimifc-geometry/src/` to match:
- Define `GeometryProcessor` trait in `bimifc-model`
- Split existing processors into separate files
- Add missing processors: `MappedItemProcessor`, `BooleanClippingProcessor`, `AdvancedBrepProcessor`, `SurfaceProcessor`

### 2.2 Fast-Path Parsing for TriangulatedFaceSet

**What:** Upstream's `tessellated.rs` (661 lines) includes a **direct byte-level parser** that extracts coordinate lists and index arrays from raw IFC bytes without going through the tokenizer. This is 3–5× faster than the standard decode path.

**Action:** Add fast-path to `bimifc-parser`:
- `extract_coordinate_list_from_entity(raw_bytes) -> Vec<f32>` — parse `((x,y,z),(x,y,z),...)` directly
- `parse_indices_direct(raw_bytes) -> Vec<u32>` — parse `((i,j,k),(i,j,k),...)` directly
- Fall back to standard tokenization if fast-path fails
- Wire into `TriangulatedFaceSetProcessor`

### 2.3 Geometry Router Refactor

**What:** The router was split into focused modules:

```
rust/geometry/src/router/
├── mod.rs          — public API + processor registry (267 lines)
├── processing.rs   — element → mesh pipeline (710 lines)
├── transforms.rs   — placement chain resolution (395 lines)
├── clipping.rs     — boolean clipping planes (398 lines)
├── voids.rs        — 3D void subtraction (1065 lines)
├── voids_2d.rs     — 2D profile-level void subtraction (496 lines)
├── caching.rs      — geometry deduplication (63 lines)
└── tests.rs        — comprehensive tests (491 lines)
```

**Action:** Restructure `bimifc-geometry/src/router.rs` → `router/` module:
- `mod.rs`: `GeometryRouter` struct with processor HashMap and caches
- `processing.rs`: `process_element()` follows Representation → ShapeRepresentation → Items
- `transforms.rs`: placement chain resolution using transform module
- `caching.rs`: FxHashMap-based geometry deduplication by content hash

### 2.4 Void/Opening Handling (MAJOR PERFORMANCE WIN)

**What:** This is the single biggest performance improvement upstream. Two approaches:

1. **3D CSG** (`voids.rs`, 1065 lines): Extend opening meshes and perform boolean subtraction. Works for all cases but slow.
2. **2D Profile-Level** (`voids_2d.rs`, 496 lines): Classify voids as coplanar or non-planar. Coplanar voids are subtracted at the 2D profile level *before* extrusion — **10–25× faster** than 3D CSG.

**Key types:**
- `VoidIndex` — pre-built map: host entity ID → void entity IDs (from `IfcRelVoidsElement`)
- `VoidClassification` — enum: `Coplanar { profile_hole, depth }`, `NonPlanar { mesh }`, `NonIntersecting`
- `VoidAnalyzer` — classifies voids, extracts coplanar profiles

**Action:** Implement in stages:
1. Build `VoidIndex` from `IfcRelVoidsElement` relationships during parsing
2. Implement `VoidAnalyzer` with coplanarity detection (epsilon-based)
3. Add 2D void subtraction to extrusion processor (subtract hole polygons from profile before extruding)
4. Fall back to 3D CSG for non-planar voids

### 2.5 Mesh Deduplication & Instancing

**What:** Upstream uses content-based hashing (FxHasher) to deduplicate repeated geometry. Buildings with repeated floor plans benefit enormously (e.g., 20-floor building → 1 unique geometry × 20 transforms instead of 20 copies).

Also supports `IfcMappedItem` instancing: shared geometry definition + per-instance transform matrix.

**Action:**
- Add `compute_mesh_hash(mesh) -> u64` using FxHasher on vertex/index data
- Add `geometry_hash_cache: FxHashMap<u64, Arc<Mesh>>` to router
- Implement `MappedItemProcessor` for explicit IFC instancing
- Return `Arc<Mesh>` from processors for zero-copy sharing

---

## 3. Mesh Data Structure (MEDIUM PRIORITY)

### 3.1 Flat Array Mesh Representation

**What:** Upstream uses flat `Vec<f32>` for positions/normals and `Vec<u32>` for indices, with pre-allocated capacity and batch merge operations.

**Current bimifc:** Uses similar but less optimized structure.

**Action:** Align `bimifc-geometry/src/mesh.rs` with upstream:
- `merge(&mut self, other: &Mesh)` — offset-aware combining
- `merge_all(&mut self, meshes: &[Mesh])` — single allocation for batch merge
- `bounds() -> (Vec3, Vec3)` — chunk-based AABB calculation
- Ensure interleaved layout option for GPU path: `[px, py, pz, nx, ny, nz, ...]`

---

## 4. WASM/GPU Optimizations (MEDIUM PRIORITY)

### 4.1 GPU Geometry Module

**What:** Upstream's `gpu_geometry.rs` provides GPU-ready interleaved vertex data with metadata per mesh, designed for zero-copy WASM → GPU transfer.

**Key struct:**
```rust
GpuGeometry {
    vertex_data: Vec<f32>,           // interleaved [px,py,pz, nx,ny,nz, ...]
    indices: Vec<u32>,
    mesh_metadata: Vec<GpuMeshMetadata>,  // per-mesh: express_id, color, offsets
}
```

**Action:** Create `bimifc-geometry/src/gpu.rs`:
- `GpuGeometry` struct with interleaved vertex layout
- `GpuMeshMetadata` with entity ID, type, color, buffer offsets
- Z-up → Y-up coordinate conversion for WebGL/WebGPU
- Raw pointer accessors for zero-copy WASM views
- Wire into bimifc-bevy's mesh creation

### 4.2 Symbolic Representations (`rust/wasm-bindings/src/api/symbolic.rs` — 930 lines)

**What:** Generates simplified symbolic geometry for elements that lack explicit geometry (e.g., doors as rectangles, windows as planes, spaces as bounding boxes). Provides fallback visualization.

**Action:** Consider for bimifc-geometry as optional fallback:
- Detect elements with no geometry representation
- Generate box/plane/cylinder approximations based on IFC type
- Lower priority — real geometry processing comes first

---

## 5. IFC5/IFCX Round-Trip (LOW PRIORITY for now)

### 5.1 Current State in bimifc

bimifc already has:
- ✅ IFCX JSON parser (`bimifc-parser/src/ifcx/`) — 1,320 lines
- ✅ ECS composition flattening
- ✅ Model implementation with ID mappings
- ✅ Geometry extraction from USD mesh data
- ✅ Auto-detection (STEP vs IFCX)

### 5.2 Upstream Additions

Upstream added full IFC5 **export** (TypeScript):
- IFC4 → IFCX schema conversion
- Spatial hierarchy mapping to IFC5 composition model
- USD geometry export
- Entity name resolution

**Action:** Defer. Export is TypeScript-only upstream. When needed, implement `bimifc-export` crate with:
- `IfcxExporter` — serialize bimifc model to IFCX JSON
- `GltfExporter` — export to GLB/glTF
- `StepExporter` — write IFC4 STEP format

---

## 6. Upstream JS/TS Features (AWARENESS ONLY)

These upstream features are TypeScript-only. They don't need direct porting but inform bimifc's roadmap:

| Feature | Upstream Package | bimifc Equivalent |
|---------|-----------------|-------------------|
| BabylonJS viewer | `examples/babylonjs-viewer/` | N/A (using Bevy) |
| Three.js batching | `packages/renderer/` | Bevy batch rendering |
| Mesh color mutation | `packages/renderer/src/scene.ts` | Bevy material system |
| Federated models | `packages/spatial/` | `bimifc-parser/src/spatial.rs` |
| BVH spatial index | `packages/spatial/` | Not implemented |
| IDS validation | `packages/ids/` | Not implemented |
| BCF integration | `packages/bcf/` | Not implemented |
| 2D drawings | `packages/drawing-2d/` | Not implemented |
| Lens (data coloring) | `packages/lens/` | Not implemented |
| Lists (entity queries) | `packages/lists/` | Not implemented |
| Scripting platform | Planned | Not planned |

---

## 7. Implementation Roadmap

### Phase 1: Foundation (Geometry Pipeline)
1. Extract unit scale from IFCPROJECT
2. Implement transform module (placement chains)
3. Refactor processors into modular architecture
4. Add fast-path parsing for TriangulatedFaceSet
5. Restructure router into submodules

### Phase 2: Performance (Voids & Deduplication)
6. Build VoidIndex from IfcRelVoidsElement
7. Implement VoidAnalyzer with coplanarity detection
8. Add 2D void subtraction to extrusion processor
9. Add 3D CSG fallback for non-planar voids
10. Implement content-based mesh deduplication (FxHasher)
11. Implement MappedItem instancing

### Phase 3: Rendering Optimization
12. Implement GPU geometry module (interleaved vertex data)
13. Add model bounds calculation
14. Wire GPU geometry into bimifc-bevy
15. Add Z→Y up coordinate conversion for WebGL path

### Phase 4: Features (As Needed)
16. Symbolic representation fallbacks
17. IFC5 export (IFCX writer)
18. glTF/GLB export
19. BVH spatial indexing

---

## 8. Files to Reference

### Upstream Rust (ifc-lite)
| File | Lines | Purpose |
|------|-------|---------|
| `rust/core/src/model_bounds.rs` | 427 | Model AABB bounds |
| `rust/geometry/src/transform.rs` | 326 | Placement → matrix |
| `rust/geometry/src/mesh.rs` | 398 | Mesh data structure |
| `rust/geometry/src/processors/mod.rs` | 168 | Processor trait + registry |
| `rust/geometry/src/processors/tessellated.rs` | 661 | Fast-path TriangulatedFaceSet |
| `rust/geometry/src/processors/brep.rs` | 766 | FacetedBrep processing |
| `rust/geometry/src/processors/swept.rs` | 402 | SweptDiskSolid |
| `rust/geometry/src/processors/advanced.rs` | 698 | AdvancedBrep |
| `rust/geometry/src/processors/mapped.rs` | 115 | MappedItem instancing |
| `rust/geometry/src/processors/boolean.rs` | 316 | CSG operations |
| `rust/geometry/src/router/mod.rs` | 267 | Router public API |
| `rust/geometry/src/router/processing.rs` | 710 | Element → mesh pipeline |
| `rust/geometry/src/router/voids.rs` | 1065 | 3D void subtraction |
| `rust/geometry/src/router/voids_2d.rs` | 496 | 2D profile-level voids |
| `rust/geometry/src/router/caching.rs` | 63 | Geometry deduplication |
| `rust/geometry/src/router/transforms.rs` | 395 | Placement chain resolution |
| `rust/wasm-bindings/src/api/mod.rs` | 284 | WASM API structure |
| `rust/wasm-bindings/src/api/symbolic.rs` | 930 | Symbolic representations |
| `rust/wasm-bindings/src/gpu_geometry.rs` | 43+ | GPU-ready geometry |

### bimifc (current)
| Crate | Purpose | Key files |
|-------|---------|-----------|
| `bimifc-model` | Traits & types | `lib.rs` |
| `bimifc-parser` | STEP + IFCX parsing | `scanner.rs`, `tokenizer.rs`, `decoder.rs`, `resolver.rs`, `spatial.rs` |
| `bimifc-geometry` | Geometry processing | `router.rs`, `extrusion.rs`, `profile.rs`, `triangulation.rs`, `mesh.rs` |
| `bimifc-bevy` | 3D renderer | Bevy 0.18 + WebGPU/WebGL2 |
| `bimifc-leptos` | Web UI | Leptos 0.7 |
| `bimifc-viewer` | WASM bundle | cdylib |
| `bimifc-viewer-tui` | Terminal viewer | ratatui |

---

## 9. Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| Upstream divergence grows | Hard to cherry-pick later | Prioritize Phase 1–2 now |
| CSG implementation complexity | Void handling is 1500+ lines | Start with 2D-only, add 3D later |
| Bevy vs Three.js rendering differences | GPU geometry layout may differ | Abstract behind trait |
| Unit scale bugs | Geometry 1000× wrong | Test with mm/cm/ft files |
| IFCX format evolution | IFC5 spec is still draft | Keep parser flexible |

---

## 10. Upstream Sync Strategy

**Do NOT rebase or merge upstream/main.** The architectures have diverged too far (TypeScript monorepo vs pure Rust). Instead:

1. **Cherry-pick Rust logic** from upstream `rust/` directory
2. **Port algorithms** to idiomatic Rust in bimifc crates
3. **Track upstream commits** via `git log upstream/main --oneline -- rust/` for Rust-only changes
4. **Periodically fetch** upstream to stay aware: `git fetch upstream`

```bash
# Watch for new Rust-side changes
git fetch upstream
git log upstream/main --oneline -- 'rust/' | head -20
```
