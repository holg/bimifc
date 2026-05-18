//! IFC 2x3 STEP file writer.
//!
//! Generates a valid IFC2X3 file from [`RoomData`] containing:
//! - Spatial hierarchy: Project → Site → Building → Storey
//! - Building elements: IfcWallStandardCase, IfcDoor, IfcWindow, IfcOpeningElement
//! - Furnishing elements: IfcFurnishingElement
//! - Geometry: Extruded area solids (rectangular profiles)

use crate::guid::new_global_id;
use crate::model::*;
use crate::step::*;
use chrono::Utc;

/// Default heights in meters when the input doesn't specify.
const DEFAULT_WALL_HEIGHT: f64 = 2.8;
const DEFAULT_DOOR_HEIGHT: f64 = 2.1;
const DEFAULT_WINDOW_HEIGHT: f64 = 1.5;
const DEFAULT_OPENING_HEIGHT: f64 = 2.4;
const DEFAULT_FURNITURE_HEIGHT: f64 = 0.8;

/// Generate an IFC 2x3 STEP file from room data.
pub fn write_ifc(data: &RoomData) -> String {
    let mut ids = IdAlloc::new();
    let mut b = StepBuilder::new();

    // ── Shared geometry primitives ──────────────────────────────────

    let origin3d = ids.next();
    b.entity(origin3d, "IFCCARTESIANPOINT((0.,0.,0.))");

    let origin2d = ids.next();
    b.entity(origin2d, "IFCCARTESIANPOINT((0.,0.))");

    let dir_z = ids.next();
    b.entity(dir_z, "IFCDIRECTION((0.,0.,1.))");

    let dir_x = ids.next();
    b.entity(dir_x, "IFCDIRECTION((1.,0.,0.))");

    let dir_y = ids.next();
    b.entity(dir_y, "IFCDIRECTION((0.,1.,0.))");

    // World coordinate system
    let wcs = ids.next();
    b.entity(
        wcs,
        &format!("IFCAXIS2PLACEMENT3D(#{origin3d},#{dir_z},#{dir_x})"),
    );

    // Geometric representation context
    let geom_ctx = ids.next();
    b.entity(
        geom_ctx,
        &format!("IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#{wcs},$)"),
    );

    // Sub-context for body geometry
    let body_ctx = ids.next();
    b.entity(
        body_ctx,
        &format!(
            "IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Body','Model',*,*,*,*,#{geom_ctx},$,.MODEL_VIEW.,$)"
        ),
    );

    // ── Units (SI, meters) ─────────────────────────────────────────

    let u_length = ids.next();
    b.entity(u_length, "IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.)");

    let u_area = ids.next();
    b.entity(u_area, "IFCSIUNIT(*,.AREAUNIT.,$,.SQUARE_METRE.)");

    let u_volume = ids.next();
    b.entity(u_volume, "IFCSIUNIT(*,.VOLUMEUNIT.,$,.CUBIC_METRE.)");

    let u_angle = ids.next();
    b.entity(u_angle, "IFCSIUNIT(*,.PLANEANGLEUNIT.,$,.RADIAN.)");

    let u_solid_angle = ids.next();
    b.entity(u_solid_angle, "IFCSIUNIT(*,.SOLIDANGLEUNIT.,$,.STERADIAN.)");

    let units = ids.next();
    b.entity(
        units,
        &format!(
            "IFCUNITASSIGNMENT((#{u_length},#{u_area},#{u_volume},#{u_angle},#{u_solid_angle}))"
        ),
    );

    // ── Owner history ──────────────────────────────────────────────

    let person = ids.next();
    b.entity(
        person,
        &format!(
            "IFCPERSON($,'{}','',(''),(''),(''),(''),())",
            escape(&data.project.author)
        ),
    );

    let org = ids.next();
    b.entity(
        org,
        &format!(
            "IFCORGANIZATION($,'{}',$,$,$)",
            escape(&data.project.organization)
        ),
    );

    let person_org = ids.next();
    b.entity(
        person_org,
        &format!("IFCPERSONANDORGANIZATION(#{person},#{org},$)"),
    );

    let app = ids.next();
    b.entity(
        app,
        &format!("IFCAPPLICATION(#{org},'0.2.0','bimifc-writer','bimifc-writer')"),
    );

    let owner = ids.next();
    b.entity(
        owner,
        &format!("IFCOWNERHISTORY(#{person_org},#{app},$,.NOCHANGE.,$,$,$,0)"),
    );

    // ── Spatial hierarchy ──────────────────────────────────────────

    // Project
    let project = ids.next();
    b.entity(
        project,
        &format!(
            "IFCPROJECT('{}',#{owner},'{}',$,$,$,$,(#{geom_ctx}),#{units})",
            new_global_id(),
            escape(&data.project.project_name),
        ),
    );

    // Site
    let site_placement = ids.next();
    b.entity(site_placement, &format!("IFCLOCALPLACEMENT($,#{wcs})"));

    let site = ids.next();
    b.entity(
        site,
        &format!(
            "IFCSITE('{}',#{owner},'{}',$,$,#{site_placement},$,$,.ELEMENT.,$,$,$,$,$)",
            new_global_id(),
            escape(&data.project.site_name),
        ),
    );

    // Building
    let bldg_placement = ids.next();
    b.entity(
        bldg_placement,
        &format!("IFCLOCALPLACEMENT(#{site_placement},#{wcs})"),
    );

    let building = ids.next();
    b.entity(
        building,
        &format!(
            "IFCBUILDING('{}',#{owner},'{}',$,$,#{bldg_placement},$,$,.ELEMENT.,$,$,$)",
            new_global_id(),
            escape(&data.project.building_name),
        ),
    );

    // Aggregation: Project -> Site -> Building
    b.entity(
        ids.next(),
        &format!(
            "IFCRELAGGREGATES('{}',#{owner},$,$,#{project},(#{site}))",
            new_global_id()
        ),
    );
    b.entity(
        ids.next(),
        &format!(
            "IFCRELAGGREGATES('{}',#{owner},$,$,#{site},(#{building}))",
            new_global_id()
        ),
    );

    // ── Group elements by storey ────────────────────────────────────

    // Build storey elevations from explicit list or derive from elements
    let mut storey_order: Vec<String> = Vec::new();
    let mut storey_elevations: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();

    // First: use explicit storey elevations if provided
    for (name, elev) in &data.storey_elevations {
        storey_elevations.insert(name.clone(), *elev);
        if !storey_order.contains(name) {
            storey_order.push(name.clone());
        }
    }

    // Then: add any storeys from elements not already listed
    for elem in &data.elements {
        let name = if elem.storey.is_empty() {
            data.project.storey_name.clone()
        } else {
            elem.storey.clone()
        };
        // Only auto-derive elevation if not explicitly set
        if !storey_elevations.contains_key(&name) {
            storey_elevations.insert(name.clone(), elem.z_offset);
        }
        if !storey_order.contains(&name) {
            storey_order.push(name);
        }
    }

    // If no elements, create one default storey
    if storey_order.is_empty() {
        storey_order.push(data.project.storey_name.clone());
        storey_elevations.insert(data.project.storey_name.clone(), 0.0);
    }

    // Create storeys and assign elements
    let mut storey_ids: Vec<u32> = Vec::new();

    for storey_name in &storey_order {
        let elevation = storey_elevations.get(storey_name).copied().unwrap_or(0.0);

        // Storey placement with elevation offset
        let storey_origin = ids.next();
        b.entity(
            storey_origin,
            &format!("IFCCARTESIANPOINT((0.,0.,{elevation}))"),
        );
        let storey_axis = ids.next();
        b.entity(
            storey_axis,
            &format!("IFCAXIS2PLACEMENT3D(#{storey_origin},#{dir_z},#{dir_x})"),
        );
        let storey_placement = ids.next();
        b.entity(
            storey_placement,
            &format!("IFCLOCALPLACEMENT(#{bldg_placement},#{storey_axis})"),
        );

        let storey_id = ids.next();
        b.entity(
            storey_id,
            &format!(
                "IFCBUILDINGSTOREY('{}',#{owner},'{}',$,$,#{storey_placement},$,$,.ELEMENT.,{elevation})",
                new_global_id(),
                escape(storey_name),
            ),
        );
        storey_ids.push(storey_id);

        // Collect elements for this storey
        let mut contained_ids: Vec<u32> = Vec::new();
        for elem in &data.elements {
            let elem_storey = if elem.storey.is_empty() {
                &data.project.storey_name
            } else {
                &elem.storey
            };
            if elem_storey != storey_name {
                continue;
            }

            // Adjust z_offset relative to storey elevation
            let mut elem_adjusted = elem.clone();
            elem_adjusted.z_offset -= elevation;

            let height = effective_height(&elem_adjusted);
            let elem_id = write_element(
                &elem_adjusted,
                &data.bounding_box,
                height,
                &mut ids,
                &mut b,
                owner,
                storey_placement,
                body_ctx,
                dir_z,
                dir_x,
            );
            contained_ids.push(elem_id);
        }

        if !contained_ids.is_empty() {
            b.entity(
                ids.next(),
                &format!(
                    "IFCRELCONTAINEDINSPATIALSTRUCTURE('{}',#{owner},$,$,{},#{storey_id})",
                    new_global_id(),
                    ref_list(&contained_ids),
                ),
            );
        }
    }

    // Aggregate storeys under building
    b.entity(
        ids.next(),
        &format!(
            "IFCRELAGGREGATES('{}',#{owner},$,$,#{building},{})",
            new_global_id(),
            ref_list(&storey_ids),
        ),
    );

    // ── Assemble STEP file ─────────────────────────────────────────

    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let data_section = b.into_data_section();

    format!(
        "ISO-10303-21;\n\
         HEADER;\n\
         FILE_DESCRIPTION(('ViewDefinition [CoordinationView_V2.0]'),'2;1');\n\
         FILE_NAME('roomplan_export.ifc','{timestamp}',('{}'),('{}'),\
         'bimifc-writer 0.2.0','bimifc-writer','');\n\
         FILE_SCHEMA(('IFC2X3'));\n\
         ENDSEC;\n\
         DATA;\n\
         {data_section}\n\
         ENDSEC;\n\
         END-ISO-10303-21;\n",
        escape(&data.project.author),
        escape(&data.project.organization),
    )
}

/// Determine the extrusion height for an element.
fn effective_height(elem: &RoomElement) -> f64 {
    if elem.height > 0.0 {
        return elem.height;
    }
    match &elem.kind {
        ElementKind::Wall => DEFAULT_WALL_HEIGHT,
        ElementKind::Door => DEFAULT_DOOR_HEIGHT,
        ElementKind::Window => DEFAULT_WINDOW_HEIGHT,
        ElementKind::Opening => DEFAULT_OPENING_HEIGHT,
        ElementKind::Slab => 0.3,
        ElementKind::RoofGable | ElementKind::RoofHip | ElementKind::RoofBarrel => 3.0,
        ElementKind::Column => 2.8,
        ElementKind::Beam => 0.4,
        ElementKind::Stair => 2.8,
        ElementKind::Railing => 1.0,
        ElementKind::Furniture(_) => DEFAULT_FURNITURE_HEIGHT,
    }
}

/// Write a single element's geometry + entity and return its STEP ID.
#[allow(clippy::too_many_arguments)]
fn write_element(
    elem: &RoomElement,
    bbox: &Rect,
    height: f64,
    ids: &mut IdAlloc,
    b: &mut StepBuilder,
    owner: u32,
    storey_placement: u32,
    body_ctx: u32,
    dir_z: u32,
    dir_x: u32,
) -> u32 {
    // Position: center of rect relative to bounding box origin
    let cx = (elem.rect.x - bbox.x) + elem.rect.width / 2.0;
    let cy = (elem.rect.y - bbox.y) + elem.rect.height / 2.0;
    let cz = elem.z_offset;

    // Element placement
    let elem_origin = ids.next();
    b.entity(elem_origin, &format!("IFCCARTESIANPOINT(({cx},{cy},{cz}))"));

    // Handle rotation: if non-zero, compute rotated X-direction
    let placement_3d = ids.next();
    if elem.rotation.abs() > 1e-6 {
        let cos_r = elem.rotation.cos();
        let sin_r = elem.rotation.sin();
        let rot_dir_x = ids.next();
        b.entity(rot_dir_x, &format!("IFCDIRECTION(({cos_r},{sin_r},0.))"));
        b.entity(
            placement_3d,
            &format!("IFCAXIS2PLACEMENT3D(#{elem_origin},#{dir_z},#{rot_dir_x})"),
        );
    } else {
        b.entity(
            placement_3d,
            &format!("IFCAXIS2PLACEMENT3D(#{elem_origin},#{dir_z},#{dir_x})"),
        );
    }

    let local_placement = ids.next();
    b.entity(
        local_placement,
        &format!("IFCLOCALPLACEMENT(#{storey_placement},#{placement_3d})"),
    );

    let solid = if matches!(elem.kind, ElementKind::RoofGable | ElementKind::RoofBarrel) {
        // Gable/Barrel: 2D profile extruded along depth
        let hw = elem.rect.width / 2.0;
        let hd = elem.rect.height / 2.0;
        let overhang = 0.3;

        let mut pts: Vec<u32> = Vec::new();
        match &elem.kind {
            ElementKind::RoofGable => {
                let p = ids.next();
                b.entity(p, &format!("IFCCARTESIANPOINT(({},0.))", -(hw + overhang)));
                pts.push(p);
                let p = ids.next();
                b.entity(p, &format!("IFCCARTESIANPOINT((0.,{height}))"));
                pts.push(p);
                let p = ids.next();
                b.entity(p, &format!("IFCCARTESIANPOINT(({},0.))", hw + overhang));
                pts.push(p);
            }
            ElementKind::RoofBarrel => {
                let segments = 12;
                let radius = hw + overhang;
                for i in 0..=segments {
                    let angle = std::f64::consts::PI * (i as f64) / (segments as f64);
                    let x = -radius * angle.cos();
                    let y = height * angle.sin();
                    let p = ids.next();
                    b.entity(p, &format!("IFCCARTESIANPOINT(({x},{y}))"));
                    pts.push(p);
                }
            }
            _ => unreachable!(),
        }

        let first = pts[0];
        let pt_refs: String = pts
            .iter()
            .map(|p| format!("#{p}"))
            .collect::<Vec<_>>()
            .join(",");
        let polyline = ids.next();
        b.entity(polyline, &format!("IFCPOLYLINE(({pt_refs},#{first}))"));
        let profile = ids.next();
        b.entity(
            profile,
            &format!("IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#{polyline})"),
        );

        let depth = elem.rect.height + 2.0 * overhang;
        let extrude_dir = ids.next();
        b.entity(extrude_dir, "IFCDIRECTION((0.,0.,1.))");

        let solid_origin = ids.next();
        b.entity(
            solid_origin,
            &format!("IFCCARTESIANPOINT((0.,{},0.))", -(hd + overhang)),
        );
        let solid_axis = ids.next();
        b.entity(solid_axis, "IFCDIRECTION((0.,1.,0.))");
        let solid_ref = ids.next();
        b.entity(solid_ref, "IFCDIRECTION((-1.,0.,0.))");
        let solid_placement = ids.next();
        b.entity(
            solid_placement,
            &format!("IFCAXIS2PLACEMENT3D(#{solid_origin},#{solid_axis},#{solid_ref})"),
        );

        let solid = ids.next();
        b.entity(
            solid,
            &format!("IFCEXTRUDEDAREASOLID(#{profile},#{solid_placement},#{extrude_dir},{depth})"),
        );
        solid
    } else if matches!(elem.kind, ElementKind::RoofHip) {
        // Hip/pyramid roof: trapezoid profile extruded along depth
        // The trapezoid narrows toward the top to approximate a hip shape
        // For square plans, the ridge is very short → nearly a pyramid
        let hw = elem.rect.width / 2.0;
        let hd = elem.rect.height / 2.0;
        let overhang = 0.3;

        // Ridge width: proportional to aspect ratio difference
        // Square plan → very narrow ridge (near pyramid)
        // Rectangular plan → ridge along the longer axis
        let aspect = (elem.rect.width / elem.rect.height).min(elem.rect.height / elem.rect.width);
        let ridge_ratio = 0.05 + (1.0 - aspect) * 0.4; // 5% for square, up to 45% for elongated
        let ridge_hw = hw * ridge_ratio;

        // Trapezoid: wide base → narrow top
        let p1 = ids.next();
        b.entity(p1, &format!("IFCCARTESIANPOINT(({},0.))", -(hw + overhang)));
        let p2 = ids.next();
        b.entity(p2, &format!("IFCCARTESIANPOINT(({},{height}))", -ridge_hw));
        let p3 = ids.next();
        b.entity(p3, &format!("IFCCARTESIANPOINT(({},{height}))", ridge_hw));
        let p4 = ids.next();
        b.entity(p4, &format!("IFCCARTESIANPOINT(({},0.))", hw + overhang));
        let polyline = ids.next();
        b.entity(
            polyline,
            &format!("IFCPOLYLINE((#{p1},#{p2},#{p3},#{p4},#{p1}))"),
        );
        let profile = ids.next();
        b.entity(
            profile,
            &format!("IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#{polyline})"),
        );

        // Extrusion depth: shorter than full depth to create hip effect on ends
        // The trapezoid handles the side slopes, but ends are still vertical
        // To improve: extrude the full depth but accept the limitation
        let depth = elem.rect.height + 2.0 * overhang;
        let extrude_dir = ids.next();
        b.entity(extrude_dir, "IFCDIRECTION((0.,0.,1.))");

        let solid_origin = ids.next();
        b.entity(
            solid_origin,
            &format!("IFCCARTESIANPOINT((0.,{},0.))", -(hd + overhang)),
        );
        let solid_axis = ids.next();
        b.entity(solid_axis, "IFCDIRECTION((0.,1.,0.))");
        let solid_ref = ids.next();
        b.entity(solid_ref, "IFCDIRECTION((-1.,0.,0.))");
        let solid_placement = ids.next();
        b.entity(
            solid_placement,
            &format!("IFCAXIS2PLACEMENT3D(#{solid_origin},#{solid_axis},#{solid_ref})"),
        );

        let solid = ids.next();
        b.entity(
            solid,
            &format!("IFCEXTRUDEDAREASOLID(#{profile},#{solid_placement},#{extrude_dir},{depth})"),
        );
        solid
    } else {
        // Standard rectangular profile extruded upward
        let hw = elem.rect.width / 2.0;
        let hh = elem.rect.height / 2.0;

        let p1 = ids.next();
        b.entity(p1, &format!("IFCCARTESIANPOINT(({hw},{hh}))"));
        let p2 = ids.next();
        b.entity(p2, &format!("IFCCARTESIANPOINT(({},{hh}))", -hw));
        let p3 = ids.next();
        b.entity(p3, &format!("IFCCARTESIANPOINT(({},{}))", -hw, -hh));
        let p4 = ids.next();
        b.entity(p4, &format!("IFCCARTESIANPOINT(({hw},{}))", -hh));

        let polyline = ids.next();
        b.entity(
            polyline,
            &format!("IFCPOLYLINE((#{p1},#{p2},#{p3},#{p4},#{p1}))"),
        );

        let profile = ids.next();
        b.entity(
            profile,
            &format!("IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#{polyline})"),
        );

        let extrude_dir = ids.next();
        b.entity(extrude_dir, "IFCDIRECTION((0.,0.,1.))");

        let solid_origin = ids.next();
        b.entity(solid_origin, "IFCCARTESIANPOINT((0.,0.,0.))");

        let solid_placement = ids.next();
        b.entity(
            solid_placement,
            &format!("IFCAXIS2PLACEMENT3D(#{solid_origin},#{dir_z},#{dir_x})"),
        );

        let solid = ids.next();
        b.entity(
            solid,
            &format!("IFCEXTRUDEDAREASOLID(#{profile},#{solid_placement},#{extrude_dir},{height})"),
        );
        solid
    };

    // Surface style (optional color)
    if let Some([r, g, b_col, a]) = elem.color {
        let transparency = 1.0 - a;
        let colour = ids.next();
        b.entity(colour, &format!("IFCCOLOURRGB($,{},{},{})", r, g, b_col));
        let rendering = ids.next();
        b.entity(
            rendering,
            &format!(
                "IFCSURFACESTYLERENDERING(#{colour},{transparency},.BOTH.,$,$,$,$,$,.NOTDEFINED.)"
            ),
        );
        let surface_style = ids.next();
        b.entity(
            surface_style,
            &format!("IFCSURFACESTYLE('Surface',.BOTH.,(#{rendering}))"),
        );
        // Point StyledItem directly to SurfaceStyle (matching viewer parser expectations)
        b.entity(
            ids.next(),
            &format!("IFCSTYLEDITEM(#{solid},(#{surface_style}),$)"),
        );
    }

    // Shape representation
    let shape_rep = ids.next();
    b.entity(
        shape_rep,
        &format!("IFCSHAPEREPRESENTATION(#{body_ctx},'Body','SweptSolid',(#{solid}))"),
    );

    let product_shape = ids.next();
    b.entity(
        product_shape,
        &format!("IFCPRODUCTDEFINITIONSHAPE($,$,(#{shape_rep}))"),
    );

    // The IFC entity itself
    let guid = new_global_id();
    let name = elem
        .label
        .as_deref()
        .map(|l| format!("'{}'", escape(l)))
        .unwrap_or_else(|| "$".to_string());

    let entity = ids.next();

    match &elem.kind {
        ElementKind::Wall => {
            b.entity(
                entity,
                &format!(
                    "IFCWALLSTANDARDCASE('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$)"
                ),
            );
        }
        ElementKind::Door => {
            let w = elem.rect.width;
            b.entity(
                entity,
                &format!(
                    "IFCDOOR('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,{height},{w})"
                ),
            );
        }
        ElementKind::Window => {
            let w = elem.rect.width;
            b.entity(
                entity,
                &format!(
                    "IFCWINDOW('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,{height},{w})"
                ),
            );
        }
        ElementKind::Opening => {
            b.entity(
                entity,
                &format!(
                    "IFCOPENINGELEMENT('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$)"
                ),
            );
        }
        ElementKind::Slab => {
            b.entity(
                entity,
                &format!(
                    "IFCSLAB('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.FLOOR.)"
                ),
            );
        }
        ElementKind::RoofGable => {
            b.entity(
                entity,
                &format!(
                    "IFCROOF('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.GABLE_ROOF.)"
                ),
            );
        }
        ElementKind::RoofHip => {
            b.entity(
                entity,
                &format!(
                    "IFCROOF('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.HIP_ROOF.)"
                ),
            );
        }
        ElementKind::RoofBarrel => {
            b.entity(
                entity,
                &format!(
                    "IFCROOF('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.BARREL_ROOF.)"
                ),
            );
        }
        ElementKind::Column => {
            b.entity(
                entity,
                &format!(
                    "IFCCOLUMN('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$)"
                ),
            );
        }
        ElementKind::Beam => {
            b.entity(
                entity,
                &format!(
                    "IFCBEAM('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$)"
                ),
            );
        }
        ElementKind::Stair => {
            b.entity(
                entity,
                &format!(
                    "IFCSTAIR('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.STRAIGHT_RUN_STAIR.)"
                ),
            );
        }
        ElementKind::Railing => {
            b.entity(
                entity,
                &format!(
                    "IFCRAILING('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.HANDRAIL.)"
                ),
            );
        }
        ElementKind::Furniture(category) => {
            let obj_type = map_furniture_type(category);
            b.entity(
                entity,
                &format!(
                    "IFCFURNISHINGELEMENT('{guid}',#{owner},{name},'{obj_type}',$,#{local_placement},#{product_shape},$)"
                ),
            );
        }
    }

    entity
}

/// Map RoomPlan object categories to IFC object type descriptions.
fn map_furniture_type(category: &str) -> &str {
    match category.to_lowercase().as_str() {
        "bed" => "Bed",
        "table" => "Table",
        "chair" => "Chair",
        "sofa" => "Sofa",
        "storage" => "Storage",
        "toilet" => "Toilet",
        "bathtub" => "Bathtub",
        "sink" => "Sink",
        "refrigerator" => "Refrigerator",
        "stove" | "oven" => "Stove",
        "washerdryer" => "WasherDryer",
        "dishwasher" => "Dishwasher",
        "television" => "Television",
        "fireplace" => "Fireplace",
        _ => "FurnishingElement",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_room() -> RoomData {
        RoomData::new(
            vec![
                // North wall
                RoomElement {
                    kind: ElementKind::Wall,
                    rect: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 5.0,
                        height: 0.2,
                    },
                    rotation: 0.0,
                    label: Some("North Wall".into()),
                    height: 2.8,
                    z_offset: 0.0,
                    storey: String::new(),
                    color: None,
                },
                // East wall
                RoomElement {
                    kind: ElementKind::Wall,
                    rect: Rect {
                        x: 4.8,
                        y: 0.0,
                        width: 0.2,
                        height: 4.0,
                    },
                    rotation: 0.0,
                    label: None,
                    height: 2.8,
                    z_offset: 0.0,
                    storey: String::new(),
                    color: None,
                },
                // Door in north wall
                RoomElement {
                    kind: ElementKind::Door,
                    rect: Rect {
                        x: 1.0,
                        y: 0.0,
                        width: 0.9,
                        height: 0.2,
                    },
                    rotation: 0.0,
                    label: Some("Main Door".into()),
                    height: 2.1,
                    z_offset: 0.0,
                    storey: String::new(),
                    color: None,
                },
                // Window in east wall
                RoomElement {
                    kind: ElementKind::Window,
                    rect: Rect {
                        x: 4.8,
                        y: 1.5,
                        width: 0.2,
                        height: 1.2,
                    },
                    rotation: 0.0,
                    label: None,
                    height: 1.5,
                    z_offset: 0.0,
                    storey: String::new(),
                    color: None,
                },
                // Bed
                RoomElement {
                    kind: ElementKind::Furniture("bed".into()),
                    rect: Rect {
                        x: 1.0,
                        y: 1.5,
                        width: 2.0,
                        height: 1.6,
                    },
                    rotation: 0.0,
                    label: Some("Double Bed".into()),
                    height: 0.6,
                    z_offset: 0.0,
                    storey: String::new(),
                    color: None,
                },
                // Rotated table
                RoomElement {
                    kind: ElementKind::Furniture("table".into()),
                    rect: Rect {
                        x: 3.0,
                        y: 2.0,
                        width: 1.0,
                        height: 0.6,
                    },
                    rotation: 0.785, // ~45 degrees
                    label: Some("Desk".into()),
                    height: 0.75,
                    z_offset: 0.0,
                    storey: String::new(),
                    color: None,
                },
            ],
            Rect {
                x: 0.0,
                y: 0.0,
                width: 5.0,
                height: 4.0,
            },
            RoomDimensions {
                width: 5.0,
                height: 2.8,
                depth: 4.0,
            },
        )
    }

    #[test]
    fn produces_valid_step_structure() {
        let ifc = write_ifc(&sample_room());

        assert!(ifc.starts_with("ISO-10303-21;"));
        assert!(ifc.ends_with("END-ISO-10303-21;\n"));
        assert!(ifc.contains("HEADER;"));
        assert!(ifc.contains("DATA;"));
        assert!(ifc.contains("FILE_SCHEMA(('IFC2X3'))"));
    }

    #[test]
    fn contains_spatial_hierarchy() {
        let ifc = write_ifc(&sample_room());

        assert!(ifc.contains("IFCPROJECT("));
        assert!(ifc.contains("IFCSITE("));
        assert!(ifc.contains("IFCBUILDING("));
        assert!(ifc.contains("IFCBUILDINGSTOREY("));
        assert!(ifc.contains("IFCRELAGGREGATES("));
    }

    #[test]
    fn contains_building_elements() {
        let ifc = write_ifc(&sample_room());

        assert!(ifc.contains("IFCWALLSTANDARDCASE("));
        assert!(ifc.contains("IFCDOOR("));
        assert!(ifc.contains("IFCWINDOW("));
        assert!(ifc.contains("IFCFURNISHINGELEMENT("));
        assert!(ifc.contains("IFCRELCONTAINEDINSPATIALSTRUCTURE("));
    }

    #[test]
    fn contains_geometry() {
        let ifc = write_ifc(&sample_room());

        assert!(ifc.contains("IFCEXTRUDEDAREASOLID("));
        assert!(ifc.contains("IFCARBITRARYCLOSEDPROFILEDEF("));
        assert!(ifc.contains("IFCSHAPEREPRESENTATION("));
        assert!(ifc.contains("IFCPRODUCTDEFINITIONSHAPE("));
    }

    #[test]
    fn handles_rotation() {
        let ifc = write_ifc(&sample_room());
        // The 45° rotated table should produce a custom direction
        assert!(ifc.contains("0.707")); // cos(0.785) ≈ 0.707
    }

    #[test]
    fn handles_labels() {
        let ifc = write_ifc(&sample_room());

        assert!(ifc.contains("'North Wall'"));
        assert!(ifc.contains("'Main Door'"));
        assert!(ifc.contains("'Double Bed'"));
        assert!(ifc.contains("'Desk'"));
    }

    #[test]
    fn default_heights_applied() {
        let data = RoomData::new(
            vec![RoomElement {
                kind: ElementKind::Wall,
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 3.0,
                    height: 0.2,
                },
                rotation: 0.0,
                label: None,
                height: 0.0, // Should get default 2.8
                z_offset: 0.0,
                storey: String::new(),
                color: None,
            }],
            Rect {
                x: 0.0,
                y: 0.0,
                width: 3.0,
                height: 3.0,
            },
            RoomDimensions {
                width: 3.0,
                height: 2.8,
                depth: 3.0,
            },
        );

        let ifc = write_ifc(&data);
        // The extruded area solid should use 2.8
        assert!(ifc.contains(",2.8)"));
    }
}
