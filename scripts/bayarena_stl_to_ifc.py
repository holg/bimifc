#!/usr/bin/env python3
"""
BayArena STL → IFC4 Conversion Script

Converts purchased BayArena 3D-print STL files into a semantically enriched
IFC4 model with 324 LEDVANCE FL Arena floodlight placements.

Pipeline: eulumdat.icu → gldf.icu → bimifc.de

Requirements:
    pip install numpy numpy-stl trimesh ifcopenshell

Usage:
    python3 scripts/bayarena_stl_to_ifc.py
"""

import math
import os
import sys

import ifcopenshell
import ifcopenshell.api
import ifcopenshell.guid
import numpy as np
import trimesh


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

STL_DIR = os.path.join(os.path.dirname(__file__), "..", "tests", "import", "leverkusen")
OUTPUT_PATH = os.path.join(os.path.dirname(__file__), "..", "tests", "models", "bayarena_lighting.ifc")

# STL → IFC mapping: filename → (ifc_class, name, target_faces)
STL_MAPPING = {
    "main body.stl": ("IfcBuildingElementProxy", "Stadium Structure", 8000),
    "roof.stl": ("IfcRoof", "Stadium Roof", 6000),
    "field.stl": ("IfcSlab", "Playing Field", 800),
    # "bayarena logo.stl" excluded — not needed for lighting demo
}

# Surface style colors: ifc_class → (R, G, B) in 0..1
SURFACE_COLORS = {
    "IfcBuildingElementProxy": (0.65, 0.65, 0.68),  # Gray concrete structure
    "IfcRoof": (0.72, 0.73, 0.75),                   # Metallic roof
    "IfcSlab": (0.20, 0.55, 0.20),                   # Green playing field
}

# BayArena real-world dimensions
REAL_LENGTH_M = 230.0  # meters along X

# Floodlight configuration
N_LIGHTS = 324
N_RINGS = 3

# Stadium side mapping for fixture naming (oval layout, 0° = North)
SIDES = ['NORTH', 'NE', 'EAST', 'SE', 'SOUTH', 'SW', 'WEST', 'NW']


# Per-ring sector counters, reset per call to generate_fixture_names()
_sector_counters = {}


def generate_fixture_names(total_per_ring: int = 108):
    """Pre-compute (Name, Description, ObjectType) for all fixtures in a ring.

    Returns a list of (name, description, object_type) tuples, one per fixture.
    Uses angular sectors to assign stadium side references with sequential
    per-sector numbering.
    """
    results = []
    sector_counts = {}  # side → running count

    for i in range(total_per_ring):
        angle_deg = (i / total_per_ring) * 360.0

        # Determine stadium side from angle (8 sectors of 45°)
        sector_index = int((angle_deg + 22.5) % 360 / 45)
        side = SIDES[sector_index]

        # Sequential index within this sector
        sector_counts[side] = sector_counts.get(side, 0) + 1
        local_index = sector_counts[side]

        results.append((side, local_index, i, angle_deg))

    return results


def fixture_naming(ring: int, side: str, local_index: int,
                   global_index: int, angle_deg: float,
                   total_per_ring: int = 108):
    """Generate (Name, Description, ObjectType) for an IfcLightFixture."""
    name = f"FLA-R{ring}-{side}-{local_index:02d}"
    description = (
        f"Ring {ring}, {side} sector, "
        f"position {global_index + 1}/{total_per_ring}, "
        f"angle {angle_deg:.1f}\u00b0"
    )
    object_type = "LEDVANCE FL Arena 1200W 5700K"

    return name, description, object_type


# ---------------------------------------------------------------------------
# Mesh Processing
# ---------------------------------------------------------------------------

def process_stl(filepath: str, target_faces: int) -> trimesh.Trimesh:
    """Load STL, remesh for IP protection, decimate, rescale to real-world."""
    mesh = trimesh.load(filepath)

    # Remesh: subdivide first if very coarse, then simplify — creates new topology
    if len(mesh.faces) < 1000:
        mesh = mesh.subdivide()

    # Decimate to target face count
    if len(mesh.faces) > target_faces:
        mesh = mesh.simplify_quadric_decimation(face_count=target_faces)

    # Rescale from ~220mm print to real-world ~230m
    bounds = mesh.bounds
    current_length = bounds[1][0] - bounds[0][0]  # X extent
    if current_length < 1e-6:
        current_length = max(bounds[1] - bounds[0])

    # STL is in mm; target is meters
    scale_factor = REAL_LENGTH_M / (current_length / 1000.0) if current_length > 1 else 1045.0
    mesh.apply_scale(scale_factor / 1000.0)

    # Center on origin
    mesh.apply_translation(-mesh.centroid)

    return mesh


# ---------------------------------------------------------------------------
# IFC4 Spatial Structure
# ---------------------------------------------------------------------------

def create_ifc_spatial_structure():
    """Create IFC4 file with IfcProject → IfcSite → IfcBuilding → IfcBuildingStorey."""
    model = ifcopenshell.file(schema="IFC4")

    # Project
    project = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class="IfcProject", name="BayArena Lighting Demo",
    )

    # Units — metres
    ifcopenshell.api.run(
        "unit.assign_unit", model,
        length={"is_metric": True, "raw": "METRES"},
    )

    # Representation context
    context = ifcopenshell.api.run("context.add_context", model, context_type="Model")
    body_context = ifcopenshell.api.run(
        "context.add_context", model,
        context_type="Model",
        context_identifier="Body",
        target_view="MODEL_VIEW",
        parent=context,
    )

    # Site (BayArena: 51.0383°N, 6.9878°E)
    site = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class="IfcSite", name="BayArena",
    )
    site.Description = "Bismarckstra\u00dfe 122-124, 51373 Leverkusen"
    ifcopenshell.api.run(
        "aggregate.assign_object", model,
        relating_object=project, products=[site],
    )

    # Building
    building = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class="IfcBuilding", name="BayArena",
    )
    building.Description = "Bayer 04 Leverkusen football stadium, 30,210 capacity"
    ifcopenshell.api.run(
        "aggregate.assign_object", model,
        relating_object=site, products=[building],
    )

    # Ground Level Storey (structural elements)
    ground_storey = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class="IfcBuildingStorey", name="Ground Level",
    )
    ground_storey.Description = "Ground level containing stadium structure and playing field"
    ground_storey.Elevation = 0.0
    ifcopenshell.api.run(
        "aggregate.assign_object", model,
        relating_object=building, products=[ground_storey],
    )

    # Roof Ring Level Storey (lighting fixtures)
    roof_storey = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class="IfcBuildingStorey", name="Roof Ring Level",
    )
    roof_storey.Description = "Lighting ring structure at +24-26m"
    roof_storey.Elevation = 24.0
    ifcopenshell.api.run(
        "aggregate.assign_object", model,
        relating_object=building, products=[roof_storey],
    )

    return model, project, site, building, ground_storey, roof_storey, body_context


# ---------------------------------------------------------------------------
# Geometry: Mesh → IfcTriangulatedFaceSet
# ---------------------------------------------------------------------------

def get_body_context(model):
    """Get the Body sub-context."""
    for ctx in model.by_type("IfcGeometricRepresentationSubContext"):
        if ctx.ContextIdentifier == "Body":
            return ctx
    # Fallback: first context
    contexts = model.by_type("IfcGeometricRepresentationContext")
    return contexts[0] if contexts else None


def add_surface_style(model, face_set, rgb):
    """Attach an IfcSurfaceStyleRendering to a face set."""
    colour = model.createIfcColourRgb(None, float(rgb[0]), float(rgb[1]), float(rgb[2]))
    rendering = model.createIfcSurfaceStyleRendering(
        SurfaceColour=colour,
        Transparency=0.0,
        ReflectanceMethod="NOTDEFINED",
    )
    surface_style = model.createIfcSurfaceStyle(
        Name=None,
        Side="BOTH",
        Styles=[rendering],
    )
    model.createIfcStyledItem(
        Item=face_set,
        Styles=[surface_style],
    )


def add_mesh_to_ifc(model, storey, body_context, mesh, ifc_class, name, color_rgb):
    """Convert a trimesh to IfcTriangulatedFaceSet and add to the model."""
    vertices = mesh.vertices.tolist()
    faces = (mesh.faces + 1).tolist()  # 1-based indexing for IFC

    point_list = model.createIfcCartesianPointList3D(vertices)
    face_set = model.createIfcTriangulatedFaceSet(
        Coordinates=point_list,
        CoordIndex=faces,
    )

    # Surface style for viewer colors
    add_surface_style(model, face_set, color_rgb)

    shape_rep = model.createIfcShapeRepresentation(
        ContextOfItems=body_context,
        RepresentationIdentifier="Body",
        RepresentationType="Tessellation",
        Items=[face_set],
    )
    product_shape = model.createIfcProductDefinitionShape(
        Representations=[shape_rep],
    )

    element = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class=ifc_class, name=name,
    )
    element.Representation = product_shape

    placement = model.createIfcLocalPlacement(
        RelativePlacement=model.createIfcAxis2Placement3D(
            Location=model.createIfcCartesianPoint((0.0, 0.0, 0.0)),
        ),
    )
    element.ObjectPlacement = placement

    ifcopenshell.api.run(
        "spatial.assign_container", model,
        relating_structure=storey, products=[element],
    )
    return element


# ---------------------------------------------------------------------------
# Floodlight Placement (324 LEDVANCE FL Arena)
# ---------------------------------------------------------------------------

def generate_floodlight_positions(n_lights=N_LIGHTS, n_rings=N_RINGS):
    """Generate oval-ring positions under roof for floodlights.

    BayArena pitch: ~105m × 68m (standard football)
    Roof inner edge: ~120m × 85m (approximate)
    Mounting height: ~25m
    """
    positions = []
    ring_heights = [24.0, 25.0, 26.0]
    ring_scales = [1.0, 1.05, 1.10]

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

            positions.append({
                "x": x, "y": y, "z": z,
                "ring": ring_idx,
                "index": i,
            })

    return positions


def add_light_fixtures(model, roof_storey, positions):
    """Add IfcLightFixture entities with type, property sets, and ring grouping."""
    lights_per_ring = N_LIGHTS // N_RINGS

    # Pre-compute fixture naming for one ring (all rings have same count)
    ring_naming = generate_fixture_names(lights_per_ring)

    # --- IfcLightFixtureType (improved) ---
    fixture_type = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class="IfcLightFixtureType", name="LEDVANCE FL Arena 1200W",
    )
    fixture_type.PredefinedType = "DIRECTIONSOURCE"
    fixture_type.Description = "High-power LED floodlight for sports venue lighting"
    fixture_type.Tag = "urn:ledvance:product:fl-arena-1200"

    # Pset_LightFixtureTypeCommon
    pset = ifcopenshell.api.run(
        "pset.add_pset", model,
        product=fixture_type, name="Pset_LightFixtureTypeCommon",
    )
    ifcopenshell.api.run(
        "pset.edit_pset", model,
        pset=pset,
        properties={
            "Reference": "FL Arena Duo",
            "Status": "NEW",
            "LightFixtureMountingType": "SURFACE",
        },
    )

    # Custom LEDVANCE property set — typed measures via createIfcPropertySingleValue
    ledvance_props = []
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="Manufacturer", NominalValue=model.create_entity("IfcLabel", "LEDVANCE"),
    ))
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="ProductFamily", NominalValue=model.create_entity("IfcLabel", "FL Arena"),
    ))
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="NominalPower", NominalValue=model.create_entity("IfcPowerMeasure", 1200.0),
    ))
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="LuminousFlux", NominalValue=model.create_entity("IfcLuminousFluxMeasure", 71000.0),
    ))
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="ColourTemperature",
        NominalValue=model.create_entity("IfcThermodynamicTemperatureMeasure", 5700.0),
    ))
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="ColourRenderingIndex", NominalValue=model.create_entity("IfcReal", 80.0),
    ))
    # Beam angle as min/max in radians
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="BeamAngleMin",
        NominalValue=model.create_entity("IfcPlaneAngleMeasure", math.radians(15.0)),
    ))
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="BeamAngleMax",
        NominalValue=model.create_entity("IfcPlaneAngleMeasure", math.radians(60.0)),
    ))
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="IP_Rating", NominalValue=model.create_entity("IfcLabel", "IP66"),
    ))
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="DMX_Control", NominalValue=model.create_entity("IfcBoolean", True),
    ))
    ledvance_props.append(model.createIfcPropertySingleValue(
        Name="UEFA_Compliance", NominalValue=model.create_entity("IfcLabel", "Elite Level A"),
    ))

    custom_pset = model.createIfcPropertySet(
        GlobalId=ifcopenshell.guid.new(),
        Name="LEDVANCE_FL_Arena",
        HasProperties=ledvance_props,
    )
    model.createIfcRelDefinesByProperties(
        GlobalId=ifcopenshell.guid.new(),
        RelatedObjects=[fixture_type],
        RelatingPropertyDefinition=custom_pset,
    )

    # Photometry document reference
    doc_ref = model.createIfcDocumentReference(
        Location="./photometry/fl_arena.ldt",
        Name="LEDVANCE FL Arena Photometry",
        Description="EULUMDAT photometric data file",
    )
    model.createIfcRelAssociatesDocument(
        GlobalId=ifcopenshell.guid.new(),
        RelatedObjects=[fixture_type],
        RelatingDocument=doc_ref,
    )

    # --- IfcSystem per ring ---
    ring_descriptions = [
        "Inner ring - narrow beam aiming",
        "Middle ring - medium beam",
        "Outer ring - wide flood fill",
    ]
    ring_systems = []
    ring_fixtures = [[] for _ in range(N_RINGS)]  # collect fixtures per ring

    for ring_idx in range(N_RINGS):
        system = ifcopenshell.api.run(
            "root.create_entity", model,
            ifc_class="IfcSystem",
            name=f"Lighting Ring {ring_idx + 1}",
        )
        system.Description = ring_descriptions[ring_idx]
        ring_systems.append(system)

    # --- Place individual fixtures ---
    for pos in positions:
        ring = pos["ring"] + 1  # 1-based ring number
        index = pos["index"]

        side, local_idx, global_idx, angle_deg = ring_naming[index]
        name, description, object_type = fixture_naming(
            ring, side, local_idx, global_idx, angle_deg, lights_per_ring
        )

        fixture = ifcopenshell.api.run(
            "root.create_entity", model,
            ifc_class="IfcLightFixture",
            name=name,
        )
        fixture.Description = description
        fixture.PredefinedType = "DIRECTIONSOURCE"

        # Assign type (must happen before setting ObjectType, as it may clear it)
        ifcopenshell.api.run(
            "type.assign_type", model,
            related_objects=[fixture], relating_type=fixture_type,
        )
        fixture.ObjectType = object_type

        # Place at position
        placement = model.createIfcLocalPlacement(
            RelativePlacement=model.createIfcAxis2Placement3D(
                Location=model.createIfcCartesianPoint(
                    (pos["x"], pos["y"], pos["z"]),
                ),
            ),
        )
        fixture.ObjectPlacement = placement

        # Assign to roof storey
        ifcopenshell.api.run(
            "spatial.assign_container", model,
            relating_structure=roof_storey, products=[fixture],
        )

        # Collect for ring grouping
        ring_fixtures[pos["ring"]].append(fixture)

    # --- IfcRelAssignsToGroup per ring ---
    for ring_idx in range(N_RINGS):
        if ring_fixtures[ring_idx]:
            model.createIfcRelAssignsToGroup(
                GlobalId=ifcopenshell.guid.new(),
                Name=f"Ring {ring_idx + 1} Assignment",
                RelatedObjects=ring_fixtures[ring_idx],
                RelatingGroup=ring_systems[ring_idx],
            )

    return fixture_type


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    stl_dir = os.path.abspath(STL_DIR)
    output_path = os.path.abspath(OUTPUT_PATH)

    print(f"STL source: {stl_dir}")
    print(f"Output:     {output_path}")
    print()

    # Step 1: Spatial structure
    model, project, site, building, ground_storey, roof_storey, body_context = (
        create_ifc_spatial_structure()
    )

    # Step 2: Process each STL → assign to ground storey
    for stl_file, (ifc_class, name, target_faces) in STL_MAPPING.items():
        filepath = os.path.join(stl_dir, stl_file)
        if not os.path.exists(filepath):
            print(f"WARNING: {filepath} not found, skipping")
            continue

        print(f"Processing {stl_file} \u2192 {ifc_class} ({name})")
        mesh = process_stl(filepath, target_faces=target_faces)
        color_rgb = SURFACE_COLORS.get(ifc_class, (0.7, 0.7, 0.7))
        add_mesh_to_ifc(model, ground_storey, body_context, mesh, ifc_class, name, color_rgb)
        print(f"  \u2192 {len(mesh.faces)} faces, {len(mesh.vertices)} vertices")

    # Step 3: Floodlight placements → assign to roof storey
    print("\nGenerating floodlight positions...")
    positions = generate_floodlight_positions()
    add_light_fixtures(model, roof_storey, positions)
    print(f"  \u2192 {len(positions)} IfcLightFixture entities placed")

    # Step 4: Write
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    model.write(output_path)
    file_size = os.path.getsize(output_path) / 1024
    print(f"\nDone! Written to {output_path}")
    print(f"  File size: {file_size:.1f} KB")

    # Quick validation
    print("\nValidation:")
    m = ifcopenshell.open(output_path)
    print(f"  Schema:               {m.schema}")
    print(f"  Total entities:       {len(list(m))}")
    print(f"  IfcBuildingStorey:    {len(m.by_type('IfcBuildingStorey'))}")
    print(f"  IfcLightFixture:      {len(m.by_type('IfcLightFixture'))}")
    print(f"  IfcLightFixtureType:  {len(m.by_type('IfcLightFixtureType'))}")
    print(f"  IfcSystem:            {len(m.by_type('IfcSystem'))}")
    print(f"  IfcRoof:              {len(m.by_type('IfcRoof'))}")
    print(f"  IfcSlab:              {len(m.by_type('IfcSlab'))}")
    print(f"  IfcBuildingElementProxy: {len(m.by_type('IfcBuildingElementProxy'))}")

    # Sample fixture names
    fixtures = m.by_type("IfcLightFixture")
    print(f"\n  Sample fixture names:")
    for f in fixtures[:6]:
        print(f"    {f.Name:25s}  {f.Description}")
    if len(fixtures) > 6:
        print(f"    ... ({len(fixtures) - 6} more)")

    # Ring grouping
    systems = m.by_type("IfcSystem")
    for s in systems:
        print(f"  System: {s.Name} — {s.Description}")


if __name__ == "__main__":
    main()
