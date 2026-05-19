#!/usr/bin/env python3
"""
BayArena MEP overlay — adds electrical / plumbing / HVAC elements to the
existing lighting IFC so the discipline-view demo has something to show.

Input:
    crates/bimifc-viewer/ifc/bayarena_lighting.ifc

Output:
    crates/bimifc-viewer/ifc/bayarena_mep_demo.ifc

Layout (stadium is roughly X: -66..66, Y: -46..46, Z: 0..26):

  Electrical (orange):
    - 6 IfcCableCarrierSegment along the +X concourse wall, ceiling height
    - 4 IfcCableCarrierFitting at corners
    - 6 IfcCableSegment dropping vertically inside each cable tray

  Plumbing (blue):
    - 8 IfcPipeSegment along the -X concourse wall, mid-height
    - 4 IfcPipeFitting joining them (T-joints + elbows)

  HVAC (cyan / red):
    - 6 IfcAirTerminal mounted on the roof-ring storey, distributed
    - 4 IfcSpaceHeater along the south wall

Everything is simple extruded box / cylinder geometry so the viewer
renders without needing a CSG kernel. The placement intentionally
spreads each discipline along a different wall so the MEP toggle
buttons in the UI show visually distinct subsets.

Usage:
    uv run --with ifcopenshell --with numpy python3 scripts/bayarena_add_mep.py
"""

import os
import sys
from pathlib import Path

import ifcopenshell
import ifcopenshell.api
import ifcopenshell.guid


SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT_DIR = os.path.dirname(SCRIPT_DIR)
SOURCE_IFC = os.path.join(ROOT_DIR, "crates", "bimifc-viewer", "ifc", "bayarena_lighting.ifc")
OUTPUT_IFC = os.path.join(ROOT_DIR, "crates", "bimifc-viewer", "ifc", "bayarena_mep_demo.ifc")


# ----------------------------------------------------------------------
# Geometry helpers — minimal IfcExtrudedAreaSolid construction
# ----------------------------------------------------------------------

def make_local_placement(model, x, y, z, parent_placement=None, rotation_z=0.0):
    """Create an IfcLocalPlacement at (x,y,z) with optional Z rotation."""
    import math
    location = model.create_entity("IfcCartesianPoint", Coordinates=(float(x), float(y), float(z)))
    if rotation_z != 0.0:
        cos_r = math.cos(rotation_z)
        sin_r = math.sin(rotation_z)
        axis = model.create_entity("IfcDirection", DirectionRatios=(0.0, 0.0, 1.0))
        ref_dir = model.create_entity("IfcDirection", DirectionRatios=(cos_r, sin_r, 0.0))
        axis2 = model.create_entity(
            "IfcAxis2Placement3D",
            Location=location,
            Axis=axis,
            RefDirection=ref_dir,
        )
    else:
        axis2 = model.create_entity("IfcAxis2Placement3D", Location=location)
    return model.create_entity(
        "IfcLocalPlacement",
        PlacementRelTo=parent_placement,
        RelativePlacement=axis2,
    )


def make_box_extrusion(model, width, depth, height, body_context):
    """Extruded rectangle: width × depth at the base, extruded `height` upward.

    Returns an IfcProductDefinitionShape with one Body representation.
    """
    profile = model.create_entity(
        "IfcRectangleProfileDef",
        ProfileType="AREA",
        XDim=float(width),
        YDim=float(depth),
    )
    direction = model.create_entity("IfcDirection", DirectionRatios=(0.0, 0.0, 1.0))
    solid = model.create_entity(
        "IfcExtrudedAreaSolid",
        SweptArea=profile,
        Position=model.create_entity(
            "IfcAxis2Placement3D",
            Location=model.create_entity("IfcCartesianPoint", Coordinates=(0.0, 0.0, 0.0)),
        ),
        ExtrudedDirection=direction,
        Depth=float(height),
    )
    rep = model.create_entity(
        "IfcShapeRepresentation",
        ContextOfItems=body_context,
        RepresentationIdentifier="Body",
        RepresentationType="SweptSolid",
        Items=[solid],
    )
    return model.create_entity("IfcProductDefinitionShape", Representations=[rep])


def make_cylinder_extrusion(model, radius, height, body_context):
    """Extruded circle: cylinder of `radius` extruded `height` upward."""
    profile = model.create_entity(
        "IfcCircleProfileDef",
        ProfileType="AREA",
        Radius=float(radius),
    )
    direction = model.create_entity("IfcDirection", DirectionRatios=(0.0, 0.0, 1.0))
    solid = model.create_entity(
        "IfcExtrudedAreaSolid",
        SweptArea=profile,
        Position=model.create_entity(
            "IfcAxis2Placement3D",
            Location=model.create_entity("IfcCartesianPoint", Coordinates=(0.0, 0.0, 0.0)),
        ),
        ExtrudedDirection=direction,
        Depth=float(height),
    )
    rep = model.create_entity(
        "IfcShapeRepresentation",
        ContextOfItems=body_context,
        RepresentationIdentifier="Body",
        RepresentationType="SweptSolid",
        Items=[solid],
    )
    return model.create_entity("IfcProductDefinitionShape", Representations=[rep])


def find_body_context(model):
    """Locate the existing Body subcontext from the source file."""
    for ctx in model.by_type("IfcGeometricRepresentationSubContext"):
        if ctx.ContextIdentifier == "Body":
            return ctx
    # Fallback: any model-type context
    for ctx in model.by_type("IfcGeometricRepresentationContext"):
        if ctx.ContextType == "Model":
            return ctx
    raise RuntimeError("No Body representation context found in source IFC")


def get_storey(model, name_contains):
    """Find a building storey by name substring."""
    for s in model.by_type("IfcBuildingStorey"):
        if s.Name and name_contains.lower() in s.Name.lower():
            return s
    raise RuntimeError(f"No storey containing {name_contains!r}")


# ----------------------------------------------------------------------
# Element creation
# ----------------------------------------------------------------------

def create_mep_element(model, ifc_class, name, storey, x, y, z, shape, rotation_z=0.0):
    """Create one product entity of `ifc_class`, place it, contain it in storey."""
    placement = make_local_placement(
        model, x, y, z,
        parent_placement=storey.ObjectPlacement,
        rotation_z=rotation_z,
    )
    elem = model.create_entity(
        ifc_class,
        GlobalId=ifcopenshell.guid.new(),
        Name=name,
        ObjectPlacement=placement,
        Representation=shape,
    )
    # Contain in storey via IfcRelContainedInSpatialStructure
    ifcopenshell.api.run(
        "spatial.assign_container",
        model,
        products=[elem],
        relating_structure=storey,
    )
    return elem


def add_electrical(model, storey, body_ctx):
    """6 cable carrier segments along +X wall at z=4.5m, with 4 fittings + 6 cables."""
    elements = []
    # Cable trays: 6 segments running along +X wall, each 8m long
    tray_shape = make_box_extrusion(model, 0.4, 0.15, 8.0, body_ctx)
    for i in range(6):
        # Place at X=42, Y from -40 to +40, Z=4.5 (overhead at concourse)
        y = -40 + i * 16
        elem = create_mep_element(
            model, "IfcCableCarrierSegment",
            f"Cable Tray Run {i + 1}",
            storey, 42.0, y, 4.5, tray_shape,
        )
        elements.append(elem)

    # Fittings at corners — slightly larger boxes
    fit_shape = make_box_extrusion(model, 0.5, 0.5, 0.25, body_ctx)
    for i, (x, y) in enumerate([(42.0, -40.0), (42.0, -20.0), (42.0, 20.0), (42.0, 40.0)]):
        elem = create_mep_element(
            model, "IfcCableCarrierFitting",
            f"Cable Carrier Fitting {i + 1}",
            storey, x, y, 4.5, fit_shape,
        )
        elements.append(elem)

    # Cable segments — thin cylinders dropping from each tray
    cable_shape = make_cylinder_extrusion(model, 0.025, 3.0, body_ctx)  # 5cm diameter, 3m drop
    for i in range(6):
        y = -40 + i * 16
        elem = create_mep_element(
            model, "IfcCableSegment",
            f"Power Cable {i + 1}",
            storey, 42.5, y, 1.5, cable_shape,
        )
        elements.append(elem)

    return elements


def add_plumbing(model, storey, body_ctx):
    """8 pipe segments along -X wall at z=2.5m, with 4 fittings."""
    elements = []
    pipe_shape = make_cylinder_extrusion(model, 0.06, 6.0, body_ctx)  # 12cm dia, 6m long
    for i in range(8):
        y = -40 + i * 12
        elem = create_mep_element(
            model, "IfcPipeSegment",
            f"Water Pipe {i + 1}",
            storey, -42.0, y, 2.5, pipe_shape,
        )
        elements.append(elem)

    fit_shape = make_cylinder_extrusion(model, 0.10, 0.3, body_ctx)
    for i, y in enumerate([-30, -10, 10, 30]):
        elem = create_mep_element(
            model, "IfcPipeFitting",
            f"Pipe Fitting {i + 1}",
            storey, -42.0, float(y), 2.5, fit_shape,
        )
        elements.append(elem)

    return elements


def add_hvac(model, ground_storey, roof_storey, body_ctx):
    """6 air terminals at roof-ring level, 4 space heaters on ground south wall."""
    elements = []
    at_shape = make_box_extrusion(model, 0.6, 0.6, 0.15, body_ctx)
    for i in range(6):
        x = -40 + i * 16
        elem = create_mep_element(
            model, "IfcAirTerminal",
            f"Air Diffuser {i + 1}",
            roof_storey, float(x), 30.0, -0.2, at_shape,
        )
        elements.append(elem)

    heater_shape = make_box_extrusion(model, 1.2, 0.18, 0.6, body_ctx)
    for i, x in enumerate([-30, -10, 10, 30]):
        elem = create_mep_element(
            model, "IfcSpaceHeater",
            f"Radiator {i + 1}",
            ground_storey, float(x), -44.0, 0.3, heater_shape,
        )
        elements.append(elem)

    return elements


# ----------------------------------------------------------------------
# Main
# ----------------------------------------------------------------------

def main():
    if not os.path.exists(SOURCE_IFC):
        print(f"ERROR: source IFC not found: {SOURCE_IFC}", file=sys.stderr)
        sys.exit(1)

    print(f"Loading {SOURCE_IFC} …")
    model = ifcopenshell.open(SOURCE_IFC)

    body_ctx = find_body_context(model)
    ground = get_storey(model, "Ground")
    roof = get_storey(model, "Roof")

    print("Adding electrical (cable trays, fittings, cables) …")
    elec = add_electrical(model, ground, body_ctx)
    print(f"  +{len(elec)} electrical elements")

    print("Adding plumbing (pipes, fittings) …")
    plumb = add_plumbing(model, ground, body_ctx)
    print(f"  +{len(plumb)} plumbing elements")

    print("Adding HVAC (air terminals, space heaters) …")
    hvac = add_hvac(model, ground, roof, body_ctx)
    print(f"  +{len(hvac)} HVAC elements")

    print(f"Writing {OUTPUT_IFC} …")
    Path(os.path.dirname(OUTPUT_IFC)).mkdir(parents=True, exist_ok=True)
    model.write(OUTPUT_IFC)

    size_mb = os.path.getsize(OUTPUT_IFC) / 1024 / 1024
    print(f"Done. Output: {size_mb:.2f} MB")

    # Quick verification: count each new entity type
    out = ifcopenshell.open(OUTPUT_IFC)
    print("\nEntity counts in output:")
    for cls in [
        "IfcLightFixture",
        "IfcCableSegment", "IfcCableCarrierSegment", "IfcCableCarrierFitting",
        "IfcPipeSegment", "IfcPipeFitting",
        "IfcSpaceHeater", "IfcAirTerminal",
    ]:
        count = len(out.by_type(cls))
        print(f"  {cls}: {count}")


if __name__ == "__main__":
    main()
