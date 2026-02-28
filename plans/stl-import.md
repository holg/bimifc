# BayArena STL → IFC Conversion Plan

## Project: bimifc.de Sports Lighting Demo

**Goal:** Convert purchased BayArena 3D print STL files into a semantically enriched IFC4 model with `IfcLightFixture` placements for the LEDVANCE FL Arena floodlight system. The resulting IFC will be displayed in a browser-based viewer on bimifc.de (client-side only, no download).

---

## Source Files

Purchased from Cults3D (`leverkusen.zip`), 4 STL files:

| File | Description | IFC Target Class |
|------|-------------|-----------------|
| `main body.stl` | Stadium bowl / seating structure | `IfcBuildingElementProxy` ("Stadium Structure") |
| `roof.stl` | Roof / canopy | `IfcRoof` ("Stadium Roof") |
| `field.stl` | Playing surface | `IfcSlab` ("Playing Field") |
| `bayarena logo.stl` | BayArena branding | *Exclude or `IfcBuildingElementProxy`* |

**Original scale:** ~220mm long (3D print size)
**Target scale:** ~230m × 185m (real BayArena dimensions)

---

## Step 1: Environment Setup

```bash
pip install numpy numpy-stl trimesh ifcopenshell --break-system-packages
```

### Required Python packages

- `numpy-stl` — read STL files
- `trimesh` — mesh processing, remeshing, decimation
- `ifcopenshell` — IFC4 file creation
- `numpy` — coordinate transforms

---

## Step 2: Mesh Processing (Remesh + Decimate + Rescale)

**Critical:** The mesh must be remeshed before IFC conversion. This serves two purposes:
1. Produces new triangle topology (not traceable to original STL)
2. Reduces polygon count for browser performance

### Processing per STL file

```python
import trimesh
import numpy as np

def process_stl(filepath, target_faces=5000):
    """Load STL, remesh, decimate, and rescale to real-world dimensions."""
    mesh = trimesh.load(filepath)

    # 1. Remesh — creates entirely new vertex/face topology
    #    Subdivide first if mesh is very coarse, then simplify
    if len(mesh.faces) < 1000:
        mesh = mesh.subdivide()

    # 2. Decimate — reduce to target face count
    mesh = mesh.simplify_quadric_decimation(target_faces)

    # 3. Rescale — from ~220mm print to real-world
    #    BayArena real dimensions: ~230m long, ~185m wide, ~30m high (roof peak)
    #    STL is ~220mm long → scale factor ≈ 1045
    bounds = mesh.bounds
    current_length = bounds[1][0] - bounds[0][0]  # X extent
    if current_length == 0:
        current_length = max(bounds[1] - bounds[0])

    # Scale from mm to meters (real world)
    scale_factor = 230.0 / (current_length / 1000.0) if current_length > 1 else 1045.0
    mesh.apply_scale(scale_factor / 1000.0)  # mm → m, then to real size

    # 4. Center on origin
    mesh.apply_translation(-mesh.centroid)

    return mesh
```

### Target polygon counts per part

| Part | Target Faces | Rationale |
|------|-------------|-----------|
| `main body.stl` | 8000–10000 | Largest part, needs detail for bowl shape |
| `roof.stl` | 5000–8000 | Canopy structure |
| `field.stl` | 500–1000 | Flat surface, minimal detail needed |
| `bayarena logo.stl` | Exclude | Not needed for lighting demo, trademark concerns |

---

## Step 3: Create IFC4 Spatial Structure

The IFC file needs a proper spatial hierarchy before geometry can be added.

```python
import ifcopenshell
import ifcopenshell.api
import ifcopenshell.guid

def create_ifc_spatial_structure():
    """Create IFC4 file with project → site → building → storey."""
    model = ifcopenshell.file(schema="IFC4")

    # Project
    project = ifcopenshell.api.run("root.create_entity", model,
        ifc_class="IfcProject", name="BayArena Lighting Demo")

    # Units — meters
    ifcopenshell.api.run("unit.assign_unit", model,
        length={"is_metric": True, "raw": "METRES"})

    # Site — georeferenced to BayArena location
    site = ifcopenshell.api.run("root.create_entity", model,
        ifc_class="IfcSite", name="BayArena Site")
    # BayArena coordinates: 51.0383° N, 6.9878° E
    ifcopenshell.api.run("aggregate.assign_object", model,
        relating_object=project, products=[site])

    # Building
    building = ifcopenshell.api.run("root.create_entity", model,
        ifc_class="IfcBuilding", name="BayArena")
    ifcopenshell.api.run("aggregate.assign_object", model,
        relating_object=site, products=[building])

    # Storey (single storey for stadium)
    storey = ifcopenshell.api.run("root.create_entity", model,
        ifc_class="IfcBuildingStorey", name="Ground Level")
    ifcopenshell.api.run("aggregate.assign_object", model,
        relating_object=building, products=[storey])

    return model, project, site, building, storey
```

---

## Step 4: Convert Mesh → IFC Geometry

Each processed mesh becomes an `IfcTriangulatedFaceSet` representation attached to its IFC entity.

```python
def add_mesh_to_ifc(model, storey, mesh, ifc_class, name):
    """Convert trimesh to IfcTriangulatedFaceSet and add to model."""

    # Extract vertices and faces
    vertices = mesh.vertices.tolist()
    faces = (mesh.faces + 1).tolist()  # IFC uses 1-based indexing

    # Create IfcCartesianPointList3D
    point_list = model.createIfcCartesianPointList3D(vertices)

    # Create IfcTriangulatedFaceSet
    face_set = model.createIfcTriangulatedFaceSet(
        Coordinates=point_list,
        CoordIndex=faces
    )

    # Wrap in shape representation
    shape_rep = model.createIfcShapeRepresentation(
        ContextOfItems=get_body_context(model),
        RepresentationIdentifier="Body",
        RepresentationType="Tessellation",
        Items=[face_set]
    )

    product_shape = model.createIfcProductDefinitionShape(
        Representations=[shape_rep]
    )

    # Create the IFC product entity
    element = ifcopenshell.api.run("root.create_entity", model,
        ifc_class=ifc_class, name=name)
    element.Representation = product_shape

    # Place at origin
    placement = model.createIfcLocalPlacement(
        RelativePlacement=model.createIfcAxis2Placement3D(
            Location=model.createIfcCartesianPoint((0.0, 0.0, 0.0))
        )
    )
    element.ObjectPlacement = placement

    # Assign to storey
    ifcopenshell.api.run("spatial.assign_container", model,
        relating_structure=storey, products=[element])

    return element


def get_body_context(model):
    """Get or create the 3D body representation context."""
    contexts = model.by_type("IfcGeometricRepresentationSubContext")
    for ctx in contexts:
        if ctx.ContextIdentifier == "Body":
            return ctx

    # Create if not exists
    parent = model.by_type("IfcGeometricRepresentationContext")
    if not parent:
        parent = model.createIfcGeometricRepresentationContext(
            ContextType="Model",
            CoordinateSpaceDimension=3,
            Precision=1e-5,
            WorldCoordinateSystem=model.createIfcAxis2Placement3D(
                Location=model.createIfcCartesianPoint((0.0, 0.0, 0.0))
            )
        )
    else:
        parent = parent[0]

    return model.createIfcGeometricRepresentationSubContext(
        ContextIdentifier="Body",
        ContextType="Model",
        ParentContext=parent,
        TargetView="MODEL_VIEW"
    )
```

---

## Step 5: Add IfcLightFixture Placements

The BayArena uses **324 LEDVANCE FL Arena floodlights** on inner rings under the roof, plus **60 Quattro luminaires** for stand lighting.

### Floodlight Ring Layout

The BayArena roof is roughly oval. Floodlights are mounted on inner rings under the roof edge, aimed at the pitch.

```python
import math

def generate_floodlight_positions(n_lights=324, n_rings=3):
    """
    Generate floodlight positions on oval rings under the roof.
    BayArena pitch: ~105m x 68m (standard football)
    Roof inner edge: ~120m x 85m (approximate)
    Mounting height: ~25m (under roof)
    """
    positions = []
    ring_heights = [24.0, 25.0, 26.0]  # slight height variation per ring
    ring_scales = [1.0, 1.05, 1.10]     # inner to outer

    a_base = 60.0   # semi-major axis (half of 120m)
    b_base = 42.5   # semi-minor axis (half of 85m)

    lights_per_ring = n_lights // n_rings

    for ring_idx in range(n_rings):
        a = a_base * ring_scales[ring_idx]
        b = b_base * ring_scales[ring_idx]
        h = ring_heights[ring_idx]

        for i in range(lights_per_ring):
            angle = 2 * math.pi * i / lights_per_ring
            x = a * math.cos(angle)
            y = b * math.sin(angle)
            z = h

            # Aim direction: toward pitch center (0,0,0), tilted down
            aim_x = -x
            aim_y = -y
            aim_z = -h

            positions.append({
                "x": x, "y": y, "z": z,
                "aim_x": aim_x, "aim_y": aim_y, "aim_z": aim_z,
                "ring": ring_idx,
                "index": i
            })

    return positions


def add_light_fixtures(model, storey, positions):
    """Add IfcLightFixture entities at each position."""

    # Create the IfcLightFixtureType (shared by all instances)
    fixture_type = ifcopenshell.api.run("root.create_entity", model,
        ifc_class="IfcLightFixtureType",
        name="LEDVANCE FL Arena")
    fixture_type.PredefinedType = "DIRECTIONSOURCE"

    # Add property set to the type
    pset = ifcopenshell.api.run("pset.add_pset", model,
        product=fixture_type, name="Pset_LightFixtureTypeCommon")
    ifcopenshell.api.run("pset.edit_pset", model,
        pset=pset, properties={
            "Reference": "FL Arena Duo",
            "Status": "NEW",
            "LightFixtureMountingType": "SURFACE",
        })

    # Custom property set for LEDVANCE specifics
    custom_pset = ifcopenshell.api.run("pset.add_pset", model,
        product=fixture_type, name="LEDVANCE_FL_Arena")
    ifcopenshell.api.run("pset.edit_pset", model,
        pset=custom_pset, properties={
            "Manufacturer": "LEDVANCE",
            "ProductFamily": "FL Arena",
            "Wattage": "1200W",
            "Lumens": "71000",
            "CCT": "5700K",
            "CRI": "80",
            "BeamAngle": "15-60°",
            "IP_Rating": "IP66",
            "DMX_Control": True,
            "UEFA_Compliance": "Elite Level A",
        })

    # Add IfcDocumentReference to photometry file
    doc_ref = model.createIfcDocumentReference(
        Location="./photometry/fl_arena.ldt",
        Name="LEDVANCE FL Arena Photometry",
        Description="EULUMDAT photometric data file"
    )
    model.createIfcRelAssociatesDocument(
        GlobalId=ifcopenshell.guid.new(),
        RelatedObjects=[fixture_type],
        RelatingDocument=doc_ref
    )

    # Place individual fixtures
    for pos in positions:
        fixture = ifcopenshell.api.run("root.create_entity", model,
            ifc_class="IfcLightFixture",
            name=f"FL Arena Ring{pos['ring']+1} #{pos['index']+1}")
        fixture.PredefinedType = "DIRECTIONSOURCE"

        # Assign type
        ifcopenshell.api.run("type.assign_type", model,
            related_objects=[fixture], relating_type=fixture_type)

        # Place at position
        placement = model.createIfcLocalPlacement(
            RelativePlacement=model.createIfcAxis2Placement3D(
                Location=model.createIfcCartesianPoint(
                    (pos["x"], pos["y"], pos["z"]))
            )
        )
        fixture.ObjectPlacement = placement

        # Assign to storey
        ifcopenshell.api.run("spatial.assign_container", model,
            relating_structure=storey, products=[fixture])

    return fixture_type
```

---

## Step 6: Main Conversion Script

```python
#!/usr/bin/env python3
"""
BayArena STL → IFC Conversion
Converts 3D print STLs into a semantically enriched IFC4 model
with LEDVANCE FL Arena floodlight placements.
"""

import os

def main():
    stl_dir = "./leverkusen/"  # directory with extracted STLs

    # Files to process (excluding logo for trademark reasons)
    stl_mapping = {
        "main body.stl":  ("IfcBuildingElementProxy", "Stadium Structure", 8000),
        "roof.stl":       ("IfcRoof", "Stadium Roof", 6000),
        "field.stl":      ("IfcSlab", "Playing Field", 800),
    }

    # Step 1: Create IFC spatial structure
    model, project, site, building, storey = create_ifc_spatial_structure()

    # Step 2: Process each STL and add to IFC
    for stl_file, (ifc_class, name, target_faces) in stl_mapping.items():
        filepath = os.path.join(stl_dir, stl_file)
        if not os.path.exists(filepath):
            print(f"WARNING: {filepath} not found, skipping")
            continue

        print(f"Processing {stl_file} → {ifc_class} ({name})")
        mesh = process_stl(filepath, target_faces=target_faces)
        add_mesh_to_ifc(model, storey, mesh, ifc_class, name)
        print(f"  → {len(mesh.faces)} faces, {len(mesh.vertices)} vertices")

    # Step 3: Add floodlight positions
    print("Generating floodlight positions...")
    positions = generate_floodlight_positions(n_lights=324, n_rings=3)
    add_light_fixtures(model, storey, positions)
    print(f"  → {len(positions)} IfcLightFixture entities placed")

    # Step 4: Write IFC file
    output_path = "./bayarena_lighting.ifc"
    model.write(output_path)
    print(f"\nDone! Written to {output_path}")
    print(f"  File size: {os.path.getsize(output_path) / 1024:.1f} KB")


if __name__ == "__main__":
    main()
```

---

## Step 7: Validation & Viewing

### Validate the IFC

```bash
# Check with IfcOpenShell
python -c "
import ifcopenshell
m = ifcopenshell.open('bayarena_lighting.ifc')
print(f'Schema: {m.schema}')
print(f'Entities: {len(list(m))}')
print(f'LightFixtures: {len(m.by_type(\"IfcLightFixture\"))}')
print(f'LightFixtureTypes: {len(m.by_type(\"IfcLightFixtureType\"))}')
print(f'Roofs: {len(m.by_type(\"IfcRoof\"))}')
print(f'Slabs: {len(m.by_type(\"IfcSlab\"))}')
"
```

### View locally

- **Flinker:** Upload to https://viewer.flinker.app (client-side, no server upload)
- **Open IFC Viewer:** https://openifcviewer.com
- **Blender + BonsaiBIM:** For detailed inspection

### Convert to glTF for web viewer

```bash
# Using IfcConvert (from IfcOpenShell)
IfcConvert bayarena_lighting.ifc bayarena_lighting.glb
```

---

## Step 8: Deploy to bimifc.de

The IFC should be displayed in a **client-side web viewer** (no download button). Options:

### Option A: web-ifc + Three.js (recommended)

Use the `web-ifc` WASM library from ThatOpen to load and render IFC4 directly in the browser. This is consistent with your other .icu tools approach (client-side processing).

### Option B: Pre-convert to glTF

Convert IFC → glTF/GLB server-side, serve the GLB. Lighter viewer but loses IFC metadata.

### Option C: IFC.js / That Open Engine

Full IFC viewer toolkit with property inspection, tree navigation, etc.

---

## File Structure for Demo

```
bimifc.de/
├── viewer/
│   ├── index.html          # Three.js + web-ifc viewer
│   ├── bayarena.ifc         # The converted model
│   └── web-ifc/             # WASM loader
└── photometry/
    ├── fl_arena_narrow.ldt  # LEDVANCE photometry (if available)
    └── fl_arena_wide.ldt
```

---

## BayArena Lighting Facts (for Demo Narrative)

| Fact | Detail |
|------|--------|
| **Lighting partner** | LEDVANCE (since 2023) |
| **Floodlight model** | FL Arena (Duo + Quattro variants) |
| **Pitch floodlights** | 324 on inner rings |
| **Stand luminaires** | 60 Quattro (white + RGB chips) |
| **Standard** | UEFA Elite Level A (exceeds requirements) |
| **Control** | DMX, individually addressable |
| **Features** | Color changes, light shows, dynamic scenarios |
| **Also equipped** | Human Centric Lighting in gym + changing rooms |
| **Previous project** | Ulrich-Haberland-Stadion (women's team, 800 lux, UEFA Level D) |
| **Upcoming** | LEDVANCE showing FL Arena at Light + Building 2026 (March 8–13) |

---

## Demo Flow at the Meeting

1. **eulumdat.icu** → Load LEDVANCE FL Arena LDT file, show polar diagram + beam angles
2. **gldf.icu** → Show GLDF container with FL Arena variants, generate PDF datasheet
3. **bimifc.de** → Show the actual BayArena as IFC with 324 light fixtures placed under the roof, linked to the photometric data

**Key message:** Complete open-source pipeline from photometric data → product data → BIM integration, all browser-based, demonstrated on their own stadium.
