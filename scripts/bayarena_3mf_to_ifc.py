#!/usr/bin/env python3
"""
BayArena 3MF → IFC4 Upgrade Script

Replaces the decimated STL-based geometry in bayarena_lighting.ifc with
full-resolution colored meshes from the BambuStudio 3MF project file.

Inputs:
    tests/import/colored.3mf          — 3MF with 4 meshes + per-triangle paint
    (generates IFC from scratch, keeping all BIM structure from bayarena_stl_to_ifc.py)

Output:
    tests/models/bayarena_lighting.ifc — upgraded IFC4 with 85k triangles + colors

Requirements:
    uv run --with ifcopenshell --with numpy python3 scripts/bayarena_3mf_to_ifc.py
"""

import math
import os
import sys
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

import ifcopenshell
import ifcopenshell.api
import ifcopenshell.guid
import numpy as np


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT_DIR = os.path.dirname(SCRIPT_DIR)

THREE_MF_PATH = os.path.join(ROOT_DIR, "tests", "import", "colored.3mf")
OUTPUT_PATH = os.path.join(ROOT_DIR, "tests", "models", "bayarena_lighting.ifc")

# 3MF mesh files → IFC mapping
# Order matters: field, body, roof, logo
MESH_MAPPING = [
    {
        "model_file": "3D/Objects/field.stl_1.model",
        "ifc_class": "IfcSlab",
        "name": "Playing Field",
        "description": "UEFA regulation playing field 105x68m",
        "base_extruder": 6,     # White in 3MF (override to green for IFC)
        "base_color": (0.13, 0.55, 0.13),  # Green (better than 3MF white)
        "use_colour_map": False,  # Field is uniform green
    },
    {
        "model_file": "3D/Objects/main body.stl_2.model",
        "ifc_class": "IfcBuildingElementProxy",
        "name": "Stadium Structure",
        "description": "BayArena main stadium structure",
        "base_extruder": 1,     # Beige
        "base_color": (0.867, 0.851, 0.643),  # #DDD9A4 beige
        "use_colour_map": True,  # Has multi-color paint (beige + silver + red)
    },
    {
        "model_file": "3D/Objects/roof.stl_3.model",
        "ifc_class": "IfcRoof",
        "name": "Stadium Roof",
        "description": "BayArena roof canopy — translucent membrane over steel frame",
        "base_extruder": 2,     # Silver
        "base_color": (0.9, 0.9, 0.92),  # Light grey-white (membrane)
        "use_colour_map": False,  # Uniform color
        "transparency": 0.5,     # Semi-transparent canopy membrane
    },
    {
        "model_file": "3D/Objects/bayarena logo.stl_4.model",
        "ifc_class": "IfcBuildingElementProxy",
        "name": "BayArena Logo",
        "description": "BayArena entrance logo on north facade",
        "base_extruder": 4,     # Black
        "base_color": (0.0, 0.0, 0.0),  # Black
        "use_colour_map": True,
    },
]

# Per-mesh transforms applied after Z-offset and elevation, before global scaling.
# All offsets are in model mm (before the global SCALE_FACTOR conversion).
MESH_TRANSFORMS = {
    "bayarena logo.stl_4.model": {
        "keep_front_face": True,   # Remove back-face embossing (Z < 0)
        "close_x_gap": True,       # Close the split-halves gap at X=0
        "scale": 0.8,              # → ~74m wide × 18m tall (both halves merged at same height)
        "rotate_z_90": True,       # Rotate text horizontal: (x, y, z) → (-y, x, z)
        "rotate_x_90": True,       # Stand upright
        "xy_offset": (0.0, -97.0), # Centered X, at front roof rim
        "elevation": 54.6,         # Half above canopy, half behind: center at roof top (66.4m)
        # Color override: "Arena" text (orig Y < -4) to red (colour index 3)
        "color_override_y_lt": -4.0,
    },
}

# BambuStudio 7-extruder palette (1-indexed)
EXTRUDER_PALETTE = {
    1: (0.867, 0.851, 0.643),  # #DDD9A4 Beige — stadium exterior
    2: (0.753, 0.753, 0.753),  # #C0C0C0 Silver — roof, structural
    3: (1.0, 0.0, 0.0),        # #FF0000 Red — Bayer 04 seating
    4: (0.0, 0.0, 0.0),        # #000000 Black — logo, accents
    5: (0.471, 0.471, 0.471),  # #787878 Grey — details
    6: (1.0, 1.0, 1.0),        # #FFFFFF White — print placeholder
    7: (0.133, 0.545, 0.133),  # #00FF00 Green — pitch (print)
}

# IFC colour table indices (1-based, matching IfcColourRgbList order)
COLOUR_TABLE = [
    (0.867, 0.851, 0.643),  # 1: beige
    (0.753, 0.753, 0.753),  # 2: silver
    (1.0, 0.0, 0.0),        # 3: red (Bayer 04)
    (0.0, 0.0, 0.0),        # 4: black
    (0.471, 0.471, 0.471),  # 5: grey
    (1.0, 1.0, 1.0),        # 6: white
    (0.133, 0.545, 0.133),  # 7: green
]

# Extruder number → colour table index
EXTRUDER_TO_COLOUR_INDEX = {
    1: 1,  # beige
    2: 2,  # silver
    3: 3,  # red
    4: 4,  # black
    5: 5,  # grey
    6: 6,  # white
    7: 7,  # green
}

# Assembly Z-offsets (mm, from model_settings.config)
# These bring each part from centered-at-origin to Z_min=0.
ASSEMBLY_Z_OFFSETS = {
    "field.stl_1.model": 0.80,
    "main body.stl_2.model": 20.775,
    "roof.stl_3.model": 12.176,
    "bayarena logo.stl_4.model": 0.942,
}

# Additional assembly offsets for correct stacking (mm, in model coordinates).
# The 3MF stores each part independently on the build plate (Z=0).
# These offsets position parts correctly in the assembled stadium.
# Roof: sits ON TOP of the body bowl as a canopy/lid with ~2.5mm overlap.
#   → roof Z=0 at body Z = body_height - overlap = 41.55 - 2.5 = 39.05mm
# Logo: placed on the body's outer wall; source_offset from 3MF metadata.
ASSEMBLY_ELEVATION = {
    "roof.stl_3.model": 39.05,     # Roof base elevation (mm)
}
ASSEMBLY_XY_OFFSETS = {
    # Logo positioning is handled in MESH_TRANSFORMS (not 3D-print assembly offsets)
}

# Scale: 3MF main body X span = 191.9mm, real BayArena ~200m
# Using 1048× as average of X/Y calculations
SCALE_FACTOR = 1048.0

# Floodlight configuration
N_RINGS = 3
PANELS_PER_RING = 18       # 18 panel positions per ring (every 20°)
HEADS_PER_PANEL = 6        # 6 luminaire heads per panel (2×3 grid)
N_LIGHTS = N_RINGS * PANELS_PER_RING * HEADS_PER_PANEL  # 324

# Stadium side mapping (oval layout, 0° = North)
SIDES = ['NORTH', 'NE', 'EAST', 'SE', 'SOUTH', 'SW', 'WEST', 'NW']

# Head layout within a panel: 2 rows × 3 columns
# Offsets relative to panel center in panel-local coordinates (meters)
# Row 1 (top): narrow, medium, wide    Row 2 (bottom): narrow, medium, wide
HEAD_OFFSETS = [
    (-0.40, 0.0,  0.15),   # H1: top-left, narrow beam
    ( 0.00, 0.0,  0.15),   # H2: top-center, medium beam
    ( 0.40, 0.0,  0.15),   # H3: top-right, wide beam
    (-0.40, 0.0, -0.15),   # H4: bottom-left, narrow beam
    ( 0.00, 0.0, -0.15),   # H5: bottom-center, medium beam
    ( 0.40, 0.0, -0.15),   # H6: bottom-right, wide beam
]

# Beam type per head position (index into fixture type variants)
HEAD_BEAM_TYPE = ["narrow", "medium", "wide", "narrow", "medium", "wide"]


# ---------------------------------------------------------------------------
# 3MF Parser
# ---------------------------------------------------------------------------

def parse_3mf_mesh(zip_file, model_path):
    """Parse a 3MF model XML file, returning vertices, triangles, and paint colors.

    Returns:
        vertices: list of (x, y, z) floats
        triangles: list of (v1, v2, v3) ints (0-based)
        paint_colors: list of paint_color strings (one per triangle, "" if unpainted)
    """
    ns = {"m": "http://schemas.microsoft.com/3dmanufacturing/core/2015/02"}

    with zip_file.open(model_path) as f:
        tree = ET.parse(f)

    root = tree.getroot()
    mesh_elem = root.find(".//m:object/m:mesh", ns)
    if mesh_elem is None:
        raise ValueError(f"No mesh found in {model_path}")

    # Parse vertices
    vertices = []
    for v in mesh_elem.findall("m:vertices/m:vertex", ns):
        x = float(v.get("x"))
        y = float(v.get("y"))
        z = float(v.get("z"))
        vertices.append((x, y, z))

    # Parse triangles + paint_color
    triangles = []
    paint_colors = []

    for t in mesh_elem.findall("m:triangles/m:triangle", ns):
        v1 = int(t.get("v1"))
        v2 = int(t.get("v2"))
        v3 = int(t.get("v3"))
        triangles.append((v1, v2, v3))

        # paint_color is an unprefixed attribute (BambuStudio extension)
        pc = t.get("paint_color", "")
        paint_colors.append(pc)

    return vertices, triangles, paint_colors


def parse_paint_color(paint_str, base_extruder):
    """Map a BambuStudio paint_color string to a colour table index (1-based).

    paint_color encoding:
    - "" or "8" → base extruder color
    - "{N}C" → extruder N+1 color
    - Long string → pick most frequent "{N}C" pattern
    """
    if not paint_str or paint_str == "8":
        return EXTRUDER_TO_COLOUR_INDEX.get(base_extruder, 0)

    # Simple case: single "{N}C"
    if len(paint_str) == 2 and paint_str[1] == 'C' and paint_str[0].isdigit():
        ext = int(paint_str[0]) + 1
        return EXTRUDER_TO_COLOUR_INDEX.get(ext, EXTRUDER_TO_COLOUR_INDEX.get(base_extruder, 0))

    # Complex case: count occurrences of each "{N}C" pattern
    counts = {}
    i = 0
    while i < len(paint_str):
        if i + 1 < len(paint_str) and paint_str[i].isdigit() and paint_str[i + 1] == 'C':
            ext = int(paint_str[i]) + 1
            counts[ext] = counts.get(ext, 0) + 1
            i += 2
        elif paint_str[i] == 'A':
            # 'A' sometimes appears as separator, skip
            i += 1
        elif paint_str[i].isdigit():
            # Bare digit (like '8') = base color
            counts[base_extruder] = counts.get(base_extruder, 0) + 1
            i += 1
        else:
            i += 1

    if counts:
        dominant = max(counts, key=counts.get)
        return EXTRUDER_TO_COLOUR_INDEX.get(dominant, EXTRUDER_TO_COLOUR_INDEX.get(base_extruder, 0))

    return EXTRUDER_TO_COLOUR_INDEX.get(base_extruder, 0)


def process_3mf_mesh(zip_file, mesh_info):
    """Parse and transform a 3MF mesh for IFC output.

    Returns:
        vertices: numpy array (N, 3) in real-world meters
        triangles: list of (v1, v2, v3) 0-based
        colour_indices: list of colour table indices (1-based) per triangle
    """
    model_file = mesh_info["model_file"]
    base_extruder = mesh_info["base_extruder"]

    vertices, triangles, paint_colors = parse_3mf_mesh(zip_file, model_file)
    verts = np.array(vertices, dtype=np.float64)

    # Filter to front face only (remove back-face embossing)
    model_name = os.path.basename(model_file)
    transform = MESH_TRANSFORMS.get(model_name)
    if transform and transform.get("keep_front_face"):
        tris_arr = np.array(triangles)
        # Keep triangles where all vertices have Z >= 0 (front embossing)
        mask = np.all(verts[tris_arr, 2] >= 0, axis=1)
        # Filter triangles and paint colors
        triangles = [triangles[i] for i in range(len(triangles)) if mask[i]]
        paint_colors = [paint_colors[i] for i in range(len(paint_colors)) if mask[i]]
        # Re-index: find used vertices and compact
        used = set()
        for v1, v2, v3 in triangles:
            used.update([v1, v2, v3])
        old_to_new = {}
        new_verts = []
        for old_idx in sorted(used):
            old_to_new[old_idx] = len(new_verts)
            new_verts.append(verts[old_idx])
        verts = np.array(new_verts, dtype=np.float64)
        triangles = [(old_to_new[v1], old_to_new[v2], old_to_new[v3])
                     for v1, v2, v3 in triangles]
        print(f"    Front face filter: {mask.sum()} of {len(mask)} triangles kept")
        # Flatten to a single plane (Z=0) to prevent ghost back-face through transparent roof
        verts[:, 2] = 0.0

    if transform and transform.get("close_x_gap"):
        # The two mesh halves are MIRROR COPIES of the full logo (front + back for 3D printing).
        # Keep only the right half (X > 0) — the left half is a mirrored duplicate.
        tris_arr = np.array(triangles)
        mask = np.all(verts[tris_arr, 0] >= 0, axis=1)
        triangles = [triangles[i] for i in range(len(triangles)) if mask[i]]
        paint_colors = [paint_colors[i] for i in range(len(paint_colors)) if mask[i]]
        # Re-index vertices
        used = set()
        for v1, v2, v3 in triangles:
            used.update([v1, v2, v3])
        old_to_new = {}
        new_verts = []
        for old_idx in sorted(used):
            old_to_new[old_idx] = len(new_verts)
            new_verts.append(verts[old_idx])
        verts = np.array(new_verts, dtype=np.float64)
        triangles = [(old_to_new[v1], old_to_new[v2], old_to_new[v3])
                     for v1, v2, v3 in triangles]
        # Shift to X=0 origin
        x_min = verts[:, 0].min()
        verts[:, 0] -= x_min
        print(f"    Kept right half only: {mask.sum()} of {len(mask)} triangles, {len(verts)} verts")

    # Apply assembly Z-offset (center each part at Z_min=0)
    z_offset = ASSEMBLY_Z_OFFSETS.get(model_name, 0.0)
    verts[:, 2] += z_offset

    # Apply stacking elevation (roof sits inside body bowl, not at ground)
    elevation = ASSEMBLY_ELEVATION.get(model_name, 0.0)
    verts[:, 2] += elevation

    # Apply XY position offset (logo on facade wall)
    xy_offset = ASSEMBLY_XY_OFFSETS.get(model_name, None)
    if xy_offset is not None:
        verts[:, 0] += xy_offset[0]
        verts[:, 1] += xy_offset[1]

    # Compute original-space triangle centroids (before transforms) for color override
    orig_centroids_y = None
    transform = MESH_TRANSFORMS.get(model_name)
    if transform and transform.get("color_override_y_lt") is not None:
        tris_arr = np.array(triangles)
        orig_centroids_y = verts[tris_arr].mean(axis=1)[:, 1]

    # Apply per-mesh transforms (scale, rotation, repositioning)
    if transform:
        scale = transform.get("scale", 1.0)
        if scale != 1.0:
            verts *= scale

        if transform.get("rotate_z_90"):
            # Rotate 90° around Z-axis: (x, y, z) → (-y, x, z)
            x_old = verts[:, 0].copy()
            y_old = verts[:, 1].copy()
            verts[:, 0] = -y_old
            verts[:, 1] = x_old

        if transform.get("rotate_x_90"):
            # Rotate 90° around X-axis: (x, y, z) → (x, -z, y)
            y_old = verts[:, 1].copy()
            z_old = verts[:, 2].copy()
            verts[:, 1] = -z_old
            verts[:, 2] = y_old

        t_xy = transform.get("xy_offset")
        if t_xy:
            verts[:, 0] += t_xy[0]
            verts[:, 1] += t_xy[1]

        t_elev = transform.get("elevation")
        if t_elev:
            verts[:, 2] += t_elev

    # Scale from mm to real-world meters
    # 3MF is in mm, scale factor converts to real-world mm, then /1000 to meters
    verts *= SCALE_FACTOR / 1000.0

    # Center all meshes on XY origin (using body mesh bounds as reference)
    # We'll do this globally after loading all meshes

    # Parse paint colors → colour indices
    colour_indices = []
    base_idx = EXTRUDER_TO_COLOUR_INDEX.get(base_extruder, 1)
    for pc in paint_colors:
        if not pc:
            colour_indices.append(base_idx)
        else:
            colour_indices.append(parse_paint_color(pc, base_extruder))

    # Color override: make "Arena" portion red
    if orig_centroids_y is not None:
        y_threshold = transform["color_override_y_lt"]
        red_idx = EXTRUDER_TO_COLOUR_INDEX[3]  # red = colour index 3
        count = 0
        for i in range(len(colour_indices)):
            if orig_centroids_y[i] < y_threshold:
                colour_indices[i] = red_idx
                count += 1
        print(f"    Color override: {count} triangles set to red (Y < {y_threshold})")

    return verts, triangles, colour_indices


# ---------------------------------------------------------------------------
# IFC4 Spatial Structure (same as bayarena_stl_to_ifc.py)
# ---------------------------------------------------------------------------

def create_ifc_spatial_structure():
    """Create IFC4 file with IfcProject → IfcSite → IfcBuilding → 2 Storeys."""
    model = ifcopenshell.file(schema="IFC4")

    project = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class="IfcProject", name="BayArena Lighting Demo",
    )

    ifcopenshell.api.run(
        "unit.assign_unit", model,
        length={"is_metric": True, "raw": "METRES"},
    )

    context = ifcopenshell.api.run("context.add_context", model, context_type="Model")
    body_context = ifcopenshell.api.run(
        "context.add_context", model,
        context_type="Model",
        context_identifier="Body",
        target_view="MODEL_VIEW",
        parent=context,
    )

    site = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class="IfcSite", name="BayArena",
    )
    site.Description = "Bismarckstra\u00dfe 122-124, 51373 Leverkusen"
    ifcopenshell.api.run(
        "aggregate.assign_object", model,
        relating_object=project, products=[site],
    )

    building = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class="IfcBuilding", name="BayArena",
    )
    building.Description = "Bayer 04 Leverkusen football stadium, 30,210 capacity"
    ifcopenshell.api.run(
        "aggregate.assign_object", model,
        relating_object=site, products=[building],
    )

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
# IFC Geometry + Color
# ---------------------------------------------------------------------------

def add_mesh_to_ifc(model, storey, body_context, vertices, triangles,
                    colour_indices, mesh_info):
    """Add a mesh to IFC with base color and optional per-triangle colour map."""
    ifc_class = mesh_info["ifc_class"]
    name = mesh_info["name"]
    base_color = mesh_info["base_color"]
    use_colour_map = mesh_info["use_colour_map"]

    # IfcCartesianPointList3D — round to 6 decimals for reasonable file size
    vert_list = [[round(float(v), 6) for v in row] for row in vertices]
    point_list = model.createIfcCartesianPointList3D(vert_list)

    # IfcTriangulatedFaceSet — 1-based indexing
    face_list = [[int(t[0]) + 1, int(t[1]) + 1, int(t[2]) + 1] for t in triangles]
    face_set = model.createIfcTriangulatedFaceSet(
        Coordinates=point_list,
        CoordIndex=face_list,
    )

    # Base surface style
    colour = model.createIfcColourRgb(
        None, float(base_color[0]), float(base_color[1]), float(base_color[2])
    )
    transparency = float(mesh_info.get("transparency", 0.0))
    rendering = model.createIfcSurfaceStyleRendering(
        SurfaceColour=colour,
        Transparency=transparency,
        ReflectanceMethod="NOTDEFINED",
    )
    surface_style = model.createIfcSurfaceStyle(
        Name=name,
        Side="BOTH",
        Styles=[rendering],
    )
    model.createIfcStyledItem(
        Item=face_set,
        Styles=[surface_style],
    )

    # Per-triangle IfcIndexedColourMap
    if use_colour_map and colour_indices:
        # Check if there's actually color variation
        unique_colours = set(colour_indices)
        if len(unique_colours) > 1:
            # Build IfcColourRgbList
            colour_rgb_list = model.createIfcColourRgbList(
                ColourList=COLOUR_TABLE
            )

            # ColourIndex — one per triangle
            model.createIfcIndexedColourMap(
                MappedTo=face_set,
                Opacity=None,
                Colours=colour_rgb_list,
                ColourIndex=colour_indices,
            )
            n_unique = len(unique_colours)
            print(f"    IfcIndexedColourMap: {len(colour_indices)} indices, {n_unique} unique colors")

    # Shape representation
    shape_rep = model.createIfcShapeRepresentation(
        ContextOfItems=body_context,
        RepresentationIdentifier="Body",
        RepresentationType="Tessellation",
        Items=[face_set],
    )
    product_shape = model.createIfcProductDefinitionShape(
        Representations=[shape_rep],
    )

    # Create element
    element = ifcopenshell.api.run(
        "root.create_entity", model,
        ifc_class=ifc_class, name=name,
    )
    if "description" in mesh_info:
        element.Description = mesh_info["description"]
    if "object_type" in mesh_info:
        element.ObjectType = mesh_info["object_type"]
    element.Representation = product_shape

    # Placement at origin
    placement = model.createIfcLocalPlacement(
        RelativePlacement=model.createIfcAxis2Placement3D(
            Location=model.createIfcCartesianPoint((0.0, 0.0, 0.0)),
        ),
    )
    element.ObjectPlacement = placement

    # Assign to storey
    ifcopenshell.api.run(
        "spatial.assign_container", model,
        relating_structure=storey, products=[element],
    )

    return element


# ---------------------------------------------------------------------------
# Floodlight Placement — Panel Assembly Model
# ---------------------------------------------------------------------------

def get_panel_side(angle_deg):
    """Map panel angle (0°=North, clockwise) to stadium side name."""
    sector_index = int((angle_deg + 22.5) % 360 / 45)
    return SIDES[sector_index]


def compute_aiming_placement(panel_x, panel_y, panel_z):
    """Compute IfcAxis2Placement3D with Axis pointing toward pitch center (0,0,0).

    The Axis represents the panel's aiming direction (downward toward pitch).
    RefDirection is the panel's local X axis (tangent to the ring).

    Returns (axis_x, axis_y, axis_z, ref_x, ref_y, ref_z).
    """
    # Aim vector: from panel position toward pitch center at ground level
    dx = 0.0 - panel_x
    dy = 0.0 - panel_y
    dz = 0.0 - panel_z
    length = math.sqrt(dx * dx + dy * dy + dz * dz)
    if length < 1e-6:
        return (0.0, 0.0, -1.0, 1.0, 0.0, 0.0)

    aim_x = dx / length
    aim_y = dy / length
    aim_z = dz / length

    # RefDirection: tangent to the oval ring (perpendicular to radial, in XY plane)
    # Radial direction in XY
    rad_len = math.sqrt(panel_x * panel_x + panel_y * panel_y)
    if rad_len < 1e-6:
        return (aim_x, aim_y, aim_z, 1.0, 0.0, 0.0)

    # Tangent = cross(Z_up, radial) = (-ry, rx, 0) normalized
    ref_x = -panel_y / rad_len
    ref_y = panel_x / rad_len
    ref_z = 0.0

    return (aim_x, aim_y, aim_z, ref_x, ref_y, ref_z)


def generate_panel_positions():
    """Generate panel positions on 3 oval rings.

    Returns list of dicts with panel center position + metadata.
    18 panels per ring, evenly spaced every 20°.
    """
    panels = []
    ring_heights = [24.0, 25.0, 26.0]
    ring_scales = [1.0, 1.05, 1.10]
    a_base = 60.0   # semi-major axis
    b_base = 42.5   # semi-minor axis

    for ring_idx in range(N_RINGS):
        a = a_base * ring_scales[ring_idx]
        b = b_base * ring_scales[ring_idx]
        h = ring_heights[ring_idx]

        sector_counts = {}

        for panel_idx in range(PANELS_PER_RING):
            angle_deg = (panel_idx / PANELS_PER_RING) * 360.0
            angle_rad = math.radians(angle_deg)

            x = a * math.cos(angle_rad)
            y = b * math.sin(angle_rad)

            side = get_panel_side(angle_deg)
            sector_counts[side] = sector_counts.get(side, 0) + 1

            panels.append({
                "x": x, "y": y, "z": h,
                "ring": ring_idx,
                "panel_idx": panel_idx,
                "angle_deg": angle_deg,
                "side": side,
                "side_idx": sector_counts[side],
            })

    return panels


def create_fixture_types(model):
    """Create 3 IfcLightFixtureType variants (narrow, medium, wide beam).

    All share the same base properties; only beam angle differs.
    """
    beam_configs = {
        "narrow": {"angle": 15.0, "desc": "Narrow beam (15°) for far-side pitch"},
        "medium": {"angle": 30.0, "desc": "Medium beam (30°) for mid-field"},
        "wide":   {"angle": 60.0, "desc": "Wide flood (60°) for near-side and stands"},
    }

    types = {}
    for beam_name, cfg in beam_configs.items():
        ft = ifcopenshell.api.run(
            "root.create_entity", model,
            ifc_class="IfcLightFixtureType",
            name=f"LEDVANCE FL Arena 1200W {cfg['angle']:.0f}\u00b0",
        )
        ft.PredefinedType = "DIRECTIONSOURCE"
        ft.Description = cfg["desc"]
        ft.Tag = f"urn:ledvance:product:fl-arena-1200-{beam_name}"
        types[beam_name] = ft

    # Shared Pset_LightFixtureTypeCommon on all three
    for ft in types.values():
        pset = ifcopenshell.api.run(
            "pset.add_pset", model,
            product=ft, name="Pset_LightFixtureTypeCommon",
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

    # LEDVANCE property set with typed measures — on all three types
    all_types = list(types.values())
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
        RelatedObjects=all_types,
        RelatingPropertyDefinition=custom_pset,
    )

    # Per-type beam angle properties
    for beam_name, ft in types.items():
        angle = beam_configs[beam_name]["angle"]
        beam_props = [
            model.createIfcPropertySingleValue(
                Name="BeamAngle",
                NominalValue=model.create_entity("IfcPlaneAngleMeasure", math.radians(angle)),
            ),
        ]
        beam_pset = model.createIfcPropertySet(
            GlobalId=ifcopenshell.guid.new(),
            Name="Pset_BeamAngle",
            HasProperties=beam_props,
        )
        model.createIfcRelDefinesByProperties(
            GlobalId=ifcopenshell.guid.new(),
            RelatedObjects=[ft],
            RelatingPropertyDefinition=beam_pset,
        )

    # Embed photometric LDT data as property sets per fixture type
    ldt_dir = Path(__file__).parent.parent / "crates" / "bimifc-viewer" / "photometry"
    beam_ldt_files = {
        "narrow": ldt_dir / "fl_arena_narrow.ldt",
        "medium": ldt_dir / "fl_arena_medium.ldt",
        "wide":   ldt_dir / "fl_arena_wide.ldt",
    }
    for beam_name, ft in types.items():
        ldt_path = beam_ldt_files[beam_name]
        ldt_content = ldt_path.read_text(encoding="utf-8", errors="replace")
        photometry_props = [
            model.createIfcPropertySingleValue(
                Name="EulumdatData",
                NominalValue=model.create_entity("IfcText", ldt_content),
            ),
            model.createIfcPropertySingleValue(
                Name="PhotometryFormat",
                NominalValue=model.create_entity("IfcLabel", "EULUMDAT"),
            ),
            model.createIfcPropertySingleValue(
                Name="SourceFile",
                NominalValue=model.create_entity("IfcLabel", ldt_path.name),
            ),
        ]
        photometry_pset = model.createIfcPropertySet(
            GlobalId=ifcopenshell.guid.new(),
            Name="Pset_Photometry",
            HasProperties=photometry_props,
        )
        model.createIfcRelDefinesByProperties(
            GlobalId=ifcopenshell.guid.new(),
            RelatedObjects=[ft],
            RelatingPropertyDefinition=photometry_pset,
        )

    return types


def add_light_fixtures(model, roof_storey):
    """Add panel assemblies with fixture heads, aiming vectors, and ring grouping."""
    panels = generate_panel_positions()
    fixture_types = create_fixture_types(model)

    # IfcSystem per ring
    ring_descriptions = [
        "Inner ring - narrow beam aiming",
        "Middle ring - medium beam",
        "Outer ring - wide flood fill",
    ]
    ring_systems = []
    ring_panels = [[] for _ in range(N_RINGS)]

    for ring_idx in range(N_RINGS):
        system = ifcopenshell.api.run(
            "root.create_entity", model,
            ifc_class="IfcSystem",
            name=f"Lighting Ring {ring_idx + 1}",
        )
        system.Description = ring_descriptions[ring_idx]
        ring_systems.append(system)

    total_heads = 0

    # Collect fixtures per beam type for batched type assignment
    fixtures_by_beam = {"narrow": [], "medium": [], "wide": []}

    for panel_info in panels:
        ring = panel_info["ring"] + 1
        side = panel_info["side"]
        side_idx = panel_info["side_idx"]
        px, py, pz = panel_info["x"], panel_info["y"], panel_info["z"]
        angle_deg = panel_info["angle_deg"]

        # Panel naming
        panel_name = f"FLA-R{ring}-{side}-P{side_idx:02d}"
        panel_desc = (
            f"Ring {ring}, {side} sector, panel {panel_info['panel_idx'] + 1}/{PANELS_PER_RING}, "
            f"angle {angle_deg:.1f}\u00b0, {HEADS_PER_PANEL} heads"
        )

        # Create IfcElementAssembly for the panel
        assembly = model.createIfcElementAssembly(
            GlobalId=ifcopenshell.guid.new(),
            Name=panel_name,
            Description=panel_desc,
            ObjectType="Luminaire Panel",
            PredefinedType="USERDEFINED",
        )

        # Panel placement with aiming direction
        aim = compute_aiming_placement(px, py, pz)
        panel_placement = model.createIfcLocalPlacement(
            RelativePlacement=model.createIfcAxis2Placement3D(
                Location=model.createIfcCartesianPoint((px, py, pz)),
                Axis=model.createIfcDirection((aim[0], aim[1], aim[2])),
                RefDirection=model.createIfcDirection((aim[3], aim[4], aim[5])),
            ),
        )
        assembly.ObjectPlacement = panel_placement

        # Panel property set (shared definition, per-panel instance)
        panel_props = [
            model.createIfcPropertySingleValue(
                Name="NumberOfHeads",
                NominalValue=model.create_entity("IfcInteger", HEADS_PER_PANEL),
            ),
            model.createIfcPropertySingleValue(
                Name="TotalPower",
                NominalValue=model.create_entity("IfcPowerMeasure",
                                                 1200.0 * HEADS_PER_PANEL),
            ),
        ]
        panel_pset = model.createIfcPropertySet(
            GlobalId=ifcopenshell.guid.new(),
            Name="Pset_LuminairePanel",
            HasProperties=panel_props,
        )
        model.createIfcRelDefinesByProperties(
            GlobalId=ifcopenshell.guid.new(),
            RelatedObjects=[assembly],
            RelatingPropertyDefinition=panel_pset,
        )

        # Assign panel to roof storey via IfcRelContainedInSpatialStructure
        model.createIfcRelContainedInSpatialStructure(
            GlobalId=ifcopenshell.guid.new(),
            RelatingStructure=roof_storey,
            RelatedElements=[assembly],
        )

        # Create 6 fixture heads as children of the panel
        head_fixtures = []
        for head_idx in range(HEADS_PER_PANEL):
            beam_type = HEAD_BEAM_TYPE[head_idx]

            head_name = f"{panel_name}-H{head_idx + 1}"
            head_desc = f"Head {head_idx + 1}, {beam_type} beam"

            fixture = model.createIfcLightFixture(
                GlobalId=ifcopenshell.guid.new(),
                Name=head_name,
                Description=head_desc,
                ObjectType=f"LEDVANCE FL Arena 1200W 5700K {beam_type}",
                PredefinedType="DIRECTIONSOURCE",
            )

            # Head placement relative to panel
            ox, oy, oz = HEAD_OFFSETS[head_idx]
            head_placement = model.createIfcLocalPlacement(
                PlacementRelTo=panel_placement,
                RelativePlacement=model.createIfcAxis2Placement3D(
                    Location=model.createIfcCartesianPoint((ox, oy, oz)),
                ),
            )
            fixture.ObjectPlacement = head_placement

            head_fixtures.append(fixture)
            fixtures_by_beam[beam_type].append(fixture)
            total_heads += 1

        # IfcRelAggregates: panel → heads
        model.createIfcRelAggregates(
            GlobalId=ifcopenshell.guid.new(),
            Name=f"{panel_name} Heads",
            RelatingObject=assembly,
            RelatedObjects=head_fixtures,
        )

        # Collect panel for ring grouping
        ring_panels[panel_info["ring"]].append(assembly)

    # Batched type assignment: one IfcRelDefinesByType per beam type
    for beam_type, fixtures in fixtures_by_beam.items():
        if fixtures:
            model.createIfcRelDefinesByType(
                GlobalId=ifcopenshell.guid.new(),
                Name=f"Type Assignment {beam_type}",
                RelatedObjects=fixtures,
                RelatingType=fixture_types[beam_type],
            )

    # IfcRelAssignsToGroup: panels per ring
    for ring_idx in range(N_RINGS):
        if ring_panels[ring_idx]:
            model.createIfcRelAssignsToGroup(
                GlobalId=ifcopenshell.guid.new(),
                Name=f"Ring {ring_idx + 1} Assignment",
                RelatedObjects=ring_panels[ring_idx],
                RelatingGroup=ring_systems[ring_idx],
            )

    return fixture_types, total_heads


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    print(f"3MF source: {THREE_MF_PATH}")
    print(f"Output:     {OUTPUT_PATH}")
    print()

    if not os.path.exists(THREE_MF_PATH):
        print(f"ERROR: {THREE_MF_PATH} not found")
        sys.exit(1)

    # Step 1: Parse all meshes from 3MF
    print("=== Parsing 3MF meshes ===")
    all_meshes = []

    with zipfile.ZipFile(THREE_MF_PATH, 'r') as zf:
        for mesh_info in MESH_MAPPING:
            print(f"  {mesh_info['name']}...")
            verts, tris, colours = process_3mf_mesh(zf, mesh_info)
            all_meshes.append((verts, tris, colours))
            print(f"    {len(verts)} vertices, {len(tris)} triangles")

            # Color stats
            unique = set(colours)
            color_names = {1: "beige", 2: "silver", 3: "red", 4: "black",
                          5: "grey", 6: "white", 7: "green"}
            for ci in sorted(unique):
                count = colours.count(ci)
                pct = count / len(colours) * 100
                print(f"    colour {ci} ({color_names.get(ci, '?')}): {count} ({pct:.0f}%)")

    # Step 2: Center all meshes on XY using body mesh as reference
    print("\n=== Centering meshes ===")
    body_verts = all_meshes[1][0]  # main body is index 1
    center_x = (body_verts[:, 0].min() + body_verts[:, 0].max()) / 2
    center_y = (body_verts[:, 1].min() + body_verts[:, 1].max()) / 2
    print(f"  Body center: ({center_x:.1f}, {center_y:.1f})")

    for i in range(len(all_meshes)):
        verts, tris, colours = all_meshes[i]
        verts[:, 0] -= center_x
        verts[:, 1] -= center_y
        all_meshes[i] = (verts, tris, colours)

    # Step 3: Create IFC spatial structure
    print("\n=== Creating IFC spatial structure ===")
    model, project, site, building, ground_storey, roof_storey, body_context = (
        create_ifc_spatial_structure()
    )

    # Step 4: Add meshes to IFC
    print("\n=== Adding meshes to IFC ===")
    # Assign roof and logo to roof storey, rest to ground
    roof_classes = {"IfcRoof"}
    roof_names = {"BayArena Logo"}
    total_tris = 0
    for i, mesh_info in enumerate(MESH_MAPPING):
        verts, tris, colours = all_meshes[i]
        storey = roof_storey if (
            mesh_info["ifc_class"] in roof_classes or mesh_info["name"] in roof_names
        ) else ground_storey
        print(f"  {mesh_info['name']}: {len(tris)} triangles → {storey.Name}")
        add_mesh_to_ifc(
            model, storey, body_context,
            verts, tris, colours, mesh_info,
        )
        total_tris += len(tris)
    print(f"  Total: {total_tris} triangles")

    # Step 5: Add floodlight panels
    print("\n=== Adding floodlight panels ===")
    fixture_types, total_heads = add_light_fixtures(model, roof_storey)
    n_panels = N_RINGS * PANELS_PER_RING
    print(f"  {n_panels} panels ({PANELS_PER_RING}/ring) × {HEADS_PER_PANEL} heads = {total_heads} IfcLightFixture")
    print(f"  {len(fixture_types)} IfcLightFixtureType variants (narrow/medium/wide)")

    # Step 6: Write
    print("\n=== Writing IFC ===")
    os.makedirs(os.path.dirname(OUTPUT_PATH), exist_ok=True)
    model.write(OUTPUT_PATH)
    file_size = os.path.getsize(OUTPUT_PATH)
    print(f"  Written to {OUTPUT_PATH}")
    print(f"  File size: {file_size / 1024:.1f} KB ({file_size / 1024 / 1024:.1f} MB)")

    # Step 7: Validation
    print("\n=== Validation ===")
    m = ifcopenshell.open(OUTPUT_PATH)
    print(f"  Schema:               {m.schema}")
    print(f"  Total entities:       {len(list(m))}")
    print(f"  IfcBuildingStorey:    {len(m.by_type('IfcBuildingStorey'))}")
    print(f"  IfcBuildingElementProxy: {len(m.by_type('IfcBuildingElementProxy'))}")
    print(f"  IfcRoof:              {len(m.by_type('IfcRoof'))}")
    print(f"  IfcSlab:              {len(m.by_type('IfcSlab'))}")
    print(f"  IfcElementAssembly:   {len(m.by_type('IfcElementAssembly'))}")
    print(f"  IfcLightFixture:      {len(m.by_type('IfcLightFixture'))}")
    print(f"  IfcLightFixtureType:  {len(m.by_type('IfcLightFixtureType'))}")
    print(f"  IfcSystem:            {len(m.by_type('IfcSystem'))}")
    print(f"  IfcRelAggregates:     {len(m.by_type('IfcRelAggregates'))}")
    print(f"  IfcTriangulatedFaceSet: {len(m.by_type('IfcTriangulatedFaceSet'))}")
    print(f"  IfcIndexedColourMap:  {len(m.by_type('IfcIndexedColourMap'))}")
    print(f"  IfcStyledItem:        {len(m.by_type('IfcStyledItem'))}")

    # Sample panels
    assemblies = m.by_type("IfcElementAssembly")
    print(f"\n  Sample panels:")
    seen_sides = set()
    for a in assemblies:
        parts = a.Name.split('-')
        side = parts[2] if len(parts) >= 4 else '?'
        if side not in seen_sides and parts[1] == 'R1':
            seen_sides.add(side)
            print(f"    {a.Name:25s} | {a.ObjectType} | {a.Description}")

    # Sample fixture heads
    fixtures = m.by_type("IfcLightFixture")
    print(f"\n  Sample fixture heads (from first panel):")
    first_panel = assemblies[0].Name if assemblies else ""
    for f in fixtures[:HEADS_PER_PANEL]:
        print(f"    {f.Name:30s} | {f.ObjectType}")

    # Fixture types
    print(f"\n  Fixture types:")
    for ft in m.by_type("IfcLightFixtureType"):
        print(f"    {ft.Name} | {ft.Description}")

    # Geometry elements
    print(f"\n  Geometry elements:")
    for proxy in m.by_type("IfcBuildingElementProxy"):
        print(f"    {proxy.Name} ({proxy.is_a()})")
    for roof in m.by_type("IfcRoof"):
        print(f"    {roof.Name} ({roof.is_a()})")
    for slab in m.by_type("IfcSlab"):
        print(f"    {slab.Name} ({slab.is_a()})")


if __name__ == "__main__":
    main()
