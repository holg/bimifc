# Upstream draft: photometric data extraction for `rust/core`

**Target:** [louistrue/ifc-lite](https://github.com/louistrue/ifc-lite) — Issue with `enhancement` label
**Status:** Draft, not yet submitted
**License:** both projects MPL-2.0, direct port is permitted

---

## Title

`feat(parser): extract IfcLightSourceGoniometric → goniometric distribution`

## Body

### Summary

`IfcLightFixture` and `IfcLightSourceGoniometric` are present in the
generated schema but no parser code reads the photometric payload they
carry. Proposing to add a `build_photometric_index` walker in the same
shape as `build_geometry_style_index` (rust/wasm-bindings/src/api/styling.rs)
that surfaces the goniometric C-plane / γ-angle / luminous intensity table
keyed by light-source express ID.

### Entity chain to resolve

The non-obvious part — almost all IFC tooling stops at `IfcLightFixture`
and treats it as opaque geometry:

```
IfcLightFixture
  └── Representation  → IfcProductDefinitionShape
                          └── Representations[] → IfcShapeRepresentation
                                                    └── Items[] → IfcMappedItem
                                                                    └── MappingSource → IfcRepresentationMap
                                                                                          └── MappedRepresentation → IfcShapeRepresentation
                                                                                                                      └── Items[] → IfcLightSourceGoniometric
                                                                                                                                      └── LightDistributionDataSource → IfcLightIntensityDistribution
                                                                                                                                                                          └── DistributionData[] → IfcLightDistributionData
                                                                                                                                                                                                     ├── MainPlaneAngle (C-plane)
                                                                                                                                                                                                     ├── SecondaryPlaneAngle[] (γ angles)
                                                                                                                                                                                                     └── LuminousIntensity[] (candela)
```

Some exporters skip the `IfcMappedItem` indirection and put the
`IfcLightSourceGoniometric` directly under the shape representation;
both paths need handling.

### Proposed data shape

Two structs, mirroring the style index that already lives in
`api/styling.rs`:

```rust
pub struct GoniometricDistribution {
    pub name: String,
    pub emitter_type: String,      // STEP enum string, e.g. "LIGHTEMITTINGDIODE"
    pub distribution_type: String, // "TYPE_C" | "TYPE_B" | "TYPE_A"
    pub colour_temperature_k: f64,
    pub luminous_flux_lm: f64,
    pub planes: Vec<DistributionPlane>,
}

pub struct DistributionPlane {
    pub c_angle_deg: f64,
    pub gamma_angles_deg: Vec<f64>,
    pub intensities_cd: Vec<f64>,  // candela; index-aligned with gamma_angles
}
```

Public API mirroring the existing styling pattern:

```rust
pub(crate) fn build_photometric_index(
    content: &str,
    decoder: &mut ifc_lite_core::EntityDecoder,
) -> rustc_hash::FxHashMap<u32, GoniometricDistribution>
```

Returns: map from `IfcLightFixture` express ID → its resolved
distribution, or empty entry if the fixture's representation has no
goniometric source.

### Implementation notes

- Single pass over `EntityScanner::new(content)`, branching on
  `"IFCLIGHTFIXTURE"` (same idiom as the `IFCSTYLEDITEM` /
  `IFCINDEXEDCOLOURMAP` branches in styling.rs).
- Attribute-index lookups via `decoder.decode_at_with_id` then
  `entity.get_ref(idx)` / `entity.get_attr(idx)` — no new decoder
  capabilities needed.
- Returns owned `Vec<f64>` per plane. For an average architectural
  luminaire (24 C-planes × 19 γ-angles) that's ~3.5KB per fixture;
  worth keeping owned given how few fixtures most models have.
- No new C/C++ deps. No geometry kernel involvement. Sits cleanly in
  `rust/core` or as a new `api/photometric.rs` under `rust/wasm-bindings`,
  preference to be confirmed.

### Reference

We have a working implementation against our own parser (different
internals, but the same IFC entity chain): see `bimifc-parser/src/lighting.rs`
in [holg/bimifc](https://github.com/holg/bimifc). Same MPL-2.0 license,
so the resolution logic can be ported directly; only the
`EntityScanner`/`EntityDecoder` call shape changes.

### Out of scope for this proposal

- IES / EULUMDAT format conversion (downstream concern; can be a
  follow-up if there's interest).
- `Pset_Photometry.EulumdatData` embedded `.ldt` lookup — also a
  follow-up. Property-set extraction already works via existing
  parser machinery; this would just be a documented convention.
- Anything UI / rendering. This PR ships only the data extraction
  Rust-side; viewers consume it however they want.

### Test plan

- Round-trip against a manufacturer-exported luminaire `.ifc` (any
  DIALux-exported file works as a fixture).
- Snapshot test of the FxHashMap output for a known fixture.
- Regression check that fixtures without `IfcLightSourceGoniometric`
  return an empty entry (not absent) so callers can distinguish
  "fixture exists, no photometric data" from "fixture not in map".

---

## Submission checklist

- [ ] Confirm with Holger before posting publicly
- [ ] Choose: Issue with `enhancement` label (recommended, technical
      scope is concrete) vs. Discussion under Ideas
- [ ] Include a 20-line sample `.ifc` excerpt showing the entity chain
      so a maintainer can mentally trace the walker on the issue
- [ ] If accepted, follow-up PR ports `bimifc-parser/src/lighting.rs`
      logic against `ifc-lite-core`'s `EntityDecoder` API
