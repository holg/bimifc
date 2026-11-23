//! IFC 2x3 STEP file writer.
//!
//! Generates a valid IFC2X3 file from [`RoomData`] containing:
//! - Spatial hierarchy: Project → Site → Building → Storey → Space
//! - Building elements: IfcWallStandardCase, IfcDoor, IfcWindow, IfcSlab, …
//! - Openings: doors/windows cut their host wall via IfcOpeningElement +
//!   IfcRelVoidsElement, and fill it via IfcRelFillsElement
//! - Type objects: IfcWallType, IfcDoorStyle, IfcWindowStyle,
//!   IfcFurnitureType linked with IfcRelDefinesByType
//! - Property sets and base quantities: Pset_WallCommon / Pset_DoorCommon /
//!   Pset_WindowCommon / Pset_SpaceCommon and `BaseQuantities`
//! - Materials: IfcMaterialLayerSetUsage for walls, IfcMaterial elsewhere
//! - Space boundaries: first-level IfcRelSpaceBoundary between each
//!   IfcSpace and the walls, doors, windows and slabs that enclose it
//! - Geometry: extruded area solids (rectangular / polygon profiles)

use crate::guid::{derived_global_id, global_id_from_identifier, new_global_id};
use crate::model::*;
use crate::pset::{
    fmt_real as r, write_element_quantities, write_property_set, IfcProperty, IfcQuantity, IfcValue,
};
use crate::step::*;
use chrono::Utc;
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::FRAC_PI_2;

/// Default heights in meters when the input doesn't specify.
const DEFAULT_WALL_HEIGHT: f64 = 2.8;
const DEFAULT_DOOR_HEIGHT: f64 = 2.1;
const DEFAULT_WINDOW_HEIGHT: f64 = 1.5;
const DEFAULT_OPENING_HEIGHT: f64 = 2.4;
const DEFAULT_FURNITURE_HEIGHT: f64 = 0.8;

/// Positional slack when matching a door/window to its host wall (meters).
const HOST_TOLERANCE: f64 = 0.05;
/// Slack when deciding that a wall lines a space outline (meters).
const BOUNDARY_TOLERANCE: f64 = 0.1;
/// Name written into `IfcElementQuantity.Name`.
///
/// IFC2x3 convention. IFC4 prefers a per-entity `Qto_` name
/// (`Qto_WallBaseQuantities`, `Qto_DoorBaseQuantities`, …); this writer
/// still emits the neutral 2x3 name in both schemas, which parses
/// everywhere but is not the IFC4 convention. Tracked as a known gap —
/// fixing it needs a name per element kind, not one constant.
const BASE_QUANTITIES: &str = "BaseQuantities";

/// Shared STEP output state for one export.
struct Ctx {
    ids: IdAlloc,
    b: StepBuilder,
    owner: u32,
    body_ctx: u32,
    axis_ctx: u32,
    dir_z: u32,
    dir_x: u32,
    /// Which schema this file is being written for; a handful of entity
    /// classes differ between IFC2x3 and IFC4.
    schema: IfcSchema,
}

impl Ctx {
    /// Allocate an id and write the entity in one go.
    fn entity(&mut self, content: &str) -> u32 {
        let id = self.ids.next();
        self.b.entity(id, content);
        id
    }

    /// `IFCLOCALPLACEMENT` relative to `parent`, rotated about Z.
    fn placement(&mut self, parent: u32, x: f64, y: f64, z: f64, rot: f64) -> u32 {
        let origin = self.entity(&format!("IFCCARTESIANPOINT(({},{},{}))", r(x), r(y), r(z)));
        let ref_dir = if rot.abs() > 1e-6 {
            let (s, c) = rot.sin_cos();
            self.entity(&format!("IFCDIRECTION(({},{},0.))", r(c), r(s)))
        } else {
            self.dir_x
        };
        let dir_z = self.dir_z;
        let axis = self.entity(&format!(
            "IFCAXIS2PLACEMENT3D(#{origin},#{dir_z},#{ref_dir})"
        ));
        self.entity(&format!("IFCLOCALPLACEMENT(#{parent},#{axis})"))
    }

    /// Closed 2D polyline (first point repeated at the end).
    fn polyline_2d(&mut self, pts: &[(f64, f64)]) -> u32 {
        let mut ids: Vec<u32> = Vec::with_capacity(pts.len());
        for (x, y) in pts {
            ids.push(self.entity(&format!("IFCCARTESIANPOINT(({},{}))", r(*x), r(*y))));
        }
        let mut refs = ref_list(&ids);
        // close the loop by repeating the first point
        refs.pop();
        refs.push_str(&format!(",#{})", ids[0]));
        self.entity(&format!("IFCPOLYLINE({refs})"))
    }

    /// Closed polyline profile from 2D points.
    fn polygon_profile(&mut self, pts: &[(f64, f64)]) -> u32 {
        let polyline = self.polyline_2d(pts);
        self.entity(&format!(
            "IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#{polyline})"
        ))
    }

    /// Profile with one inner void (a frame / ring).
    fn ring_profile(&mut self, outer: &[(f64, f64)], inner: &[(f64, f64)]) -> u32 {
        let outer = self.polyline_2d(outer);
        let inner = self.polyline_2d(inner);
        self.entity(&format!(
            "IFCARBITRARYPROFILEDEFWITHVOIDS(.AREA.,$,#{outer},(#{inner}))"
        ))
    }

    /// Extrude a profile drawn in the vertical (along, up) plane across the
    /// element's thickness direction, from `t0` to `t0 + depth`. With
    /// `along_x` the profile's X maps to local X and the extrusion runs
    /// along local Y; otherwise profile X maps to local Y and the
    /// extrusion runs along local X. Profile Y is always local +Z.
    fn extrude_across(&mut self, profile: u32, along_x: bool, t0: f64, depth: f64) -> u32 {
        let dir = self.entity("IFCDIRECTION((0.,0.,1.))");
        let (origin, axis, ref_dir) = if along_x {
            // Axis -Y, RefDirection +X  →  profile Y = (-Y) × (+X) = +Z
            let origin = self.entity(&format!("IFCCARTESIANPOINT((0.,{},0.))", r(t0 + depth)));
            let axis = self.entity("IFCDIRECTION((0.,-1.,0.))");
            let ref_dir = self.entity("IFCDIRECTION((1.,0.,0.))");
            (origin, axis, ref_dir)
        } else {
            // Axis +X, RefDirection +Y  →  profile Y = (+X) × (+Y) = +Z
            let origin = self.entity(&format!("IFCCARTESIANPOINT(({},0.,0.))", r(t0)));
            let axis = self.entity("IFCDIRECTION((1.,0.,0.))");
            let ref_dir = self.entity("IFCDIRECTION((0.,1.,0.))");
            (origin, axis, ref_dir)
        };
        let placement = self.entity(&format!(
            "IFCAXIS2PLACEMENT3D(#{origin},#{axis},#{ref_dir})"
        ));
        self.entity(&format!(
            "IFCEXTRUDEDAREASOLID(#{profile},#{placement},#{dir},{})",
            r(depth)
        ))
    }

    /// `IFCFACETEDBREP` from a triangle mesh; `None` if the mesh is
    /// invalid or has no usable faces. Vertices are de-duplicated.
    fn faceted_brep(&mut self, mesh: &ElementMesh) -> Option<u32> {
        if !mesh.is_valid() {
            return None;
        }
        let mut point_ids: HashMap<[u64; 3], u32> = HashMap::new();
        let mut point_of = |ctx: &mut Ctx, v: [f64; 3]| -> u32 {
            let key = [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()];
            if let Some(id) = point_ids.get(&key) {
                return *id;
            }
            let id = ctx.entity(&format!(
                "IFCCARTESIANPOINT(({},{},{}))",
                r(v[0]),
                r(v[1]),
                r(v[2])
            ));
            point_ids.insert(key, id);
            id
        };
        let mut faces: Vec<u32> = Vec::new();
        for tri in mesh.indices.as_chunks::<3>().0 {
            let a = point_of(self, mesh.vertices[tri[0] as usize]);
            let b = point_of(self, mesh.vertices[tri[1] as usize]);
            let c = point_of(self, mesh.vertices[tri[2] as usize]);
            if a == b || b == c || a == c {
                continue; // degenerate
            }
            let polyloop = self.entity(&format!("IFCPOLYLOOP((#{a},#{b},#{c}))"));
            let bound = self.entity(&format!("IFCFACEOUTERBOUND(#{polyloop},.T.)"));
            faces.push(self.entity(&format!("IFCFACE((#{bound}))")));
        }
        if faces.is_empty() {
            return None;
        }
        let shell = self.entity(&format!("IFCCLOSEDSHELL({})", ref_list(&faces)));
        Some(self.entity(&format!("IFCFACETEDBREP(#{shell})")))
    }

    /// Planar boundary surface for `IfcRelSpaceBoundary.ConnectionGeometry`:
    /// an `IFCCURVEBOUNDEDPLANE` on the plane through `origin` with normal
    /// `axis` and in-plane X `ref_dir`, bounded by `outline` (plane 2D
    /// coordinates), wrapped in `IFCCONNECTIONSURFACEGEOMETRY`.
    fn boundary_surface(
        &mut self,
        origin: (f64, f64, f64),
        axis: (f64, f64, f64),
        ref_dir: (f64, f64, f64),
        outline: &[(f64, f64)],
    ) -> u32 {
        let o = self.entity(&format!(
            "IFCCARTESIANPOINT(({},{},{}))",
            r(origin.0),
            r(origin.1),
            r(origin.2)
        ));
        let a = self.entity(&format!(
            "IFCDIRECTION(({},{},{}))",
            r(axis.0),
            r(axis.1),
            r(axis.2)
        ));
        let d = self.entity(&format!(
            "IFCDIRECTION(({},{},{}))",
            r(ref_dir.0),
            r(ref_dir.1),
            r(ref_dir.2)
        ));
        let placement = self.entity(&format!("IFCAXIS2PLACEMENT3D(#{o},#{a},#{d})"));
        let plane = self.entity(&format!("IFCPLANE(#{placement})"));
        let curve = self.polyline_2d(outline);
        let bounded = self.entity(&format!("IFCCURVEBOUNDEDPLANE(#{plane},#{curve},())"));
        self.entity(&format!("IFCCONNECTIONSURFACEGEOMETRY(#{bounded},$)"))
    }

    /// Rectangle profile centred on the origin.
    fn rect_profile(&mut self, width: f64, depth: f64) -> u32 {
        let hw = width / 2.0;
        let hd = depth / 2.0;
        self.polygon_profile(&[(hw, hd), (-hw, hd), (-hw, -hd), (hw, -hd)])
    }

    /// Extrude a profile upward from the local origin.
    fn extrude(&mut self, profile: u32, height: f64) -> u32 {
        let dir = self.entity("IFCDIRECTION((0.,0.,1.))");
        let origin = self.entity("IFCCARTESIANPOINT((0.,0.,0.))");
        let (dz, dx) = (self.dir_z, self.dir_x);
        let placement = self.entity(&format!("IFCAXIS2PLACEMENT3D(#{origin},#{dz},#{dx})"));
        self.entity(&format!(
            "IFCEXTRUDEDAREASOLID(#{profile},#{placement},#{dir},{})",
            r(height)
        ))
    }

    /// `IFCPRODUCTDEFINITIONSHAPE` with a swept-solid body representation
    /// and an optional extra representation (e.g. a wall axis).
    fn product_shape(&mut self, solid: u32, extra_rep: Option<u32>) -> u32 {
        self.product_shape_typed(solid, "SweptSolid", extra_rep)
    }

    /// As [`Ctx::product_shape`] with an explicit body representation
    /// type (`SweptSolid`, `Brep`, …).
    fn product_shape_typed(&mut self, item: u32, rep_type: &str, extra_rep: Option<u32>) -> u32 {
        self.product_shape_items(&[item], rep_type, extra_rep)
    }

    /// As [`Ctx::product_shape_typed`] with several body items (e.g. a
    /// door frame plus its leaf) in one `IFCSHAPEREPRESENTATION`.
    fn product_shape_items(
        &mut self,
        items: &[u32],
        rep_type: &str,
        extra_rep: Option<u32>,
    ) -> u32 {
        let body_ctx = self.body_ctx;
        let items = ref_list(items);
        let body = self.entity(&format!(
            "IFCSHAPEREPRESENTATION(#{body_ctx},'Body','{rep_type}',{items})"
        ));
        let reps = match extra_rep {
            Some(extra) => format!("(#{extra},#{body})"),
            None => format!("(#{body})"),
        };
        self.entity(&format!("IFCPRODUCTDEFINITIONSHAPE($,$,{reps})"))
    }

    /// Wall axis: a 2D line along local X from `-half_len` to `+half_len`.
    fn axis_rep(&mut self, half_len: f64) -> u32 {
        let p1 = self.entity(&format!("IFCCARTESIANPOINT(({},0.))", r(-half_len)));
        let p2 = self.entity(&format!("IFCCARTESIANPOINT(({},0.))", r(half_len)));
        let line = self.entity(&format!("IFCPOLYLINE((#{p1},#{p2}))"));
        let axis_ctx = self.axis_ctx;
        self.entity(&format!(
            "IFCSHAPEREPRESENTATION(#{axis_ctx},'Axis','Curve2D',(#{line}))"
        ))
    }

    /// Surface colour on a solid (glass = alpha < 1).
    fn style(&mut self, solid: u32, [red, green, blue, alpha]: [f32; 4]) {
        let transparency = 1.0 - alpha;
        let colour = self.entity(&format!("IFCCOLOURRGB($,{red},{green},{blue})"));
        let rendering = self.entity(&format!(
            "IFCSURFACESTYLERENDERING(#{colour},{transparency},.BOTH.,$,$,$,$,$,.NOTDEFINED.)"
        ));
        let surface_style =
            self.entity(&format!("IFCSURFACESTYLE('Surface',.BOTH.,(#{rendering}))"));
        // Point StyledItem directly to SurfaceStyle (matching viewer parser expectations)
        self.entity(&format!("IFCSTYLEDITEM(#{solid},(#{surface_style}),$)"));
    }

    fn pset(&mut self, elements: &[u32], name: &str, props: &[IfcProperty]) {
        let owner = self.owner;
        write_property_set(&mut self.ids, &mut self.b, owner, elements, name, props);
    }

    fn quantities(&mut self, elements: &[u32], quantities: &[IfcQuantity]) {
        let owner = self.owner;
        let schema = self.schema;
        write_element_quantities(
            &mut self.ids,
            &mut self.b,
            owner,
            elements,
            BASE_QUANTITIES,
            quantities,
            schema,
        );
    }

    fn rel_defines_by_type(&mut self, occurrences: &[u32], type_id: u32) {
        let owner = self.owner;
        self.entity(&format!(
            "IFCRELDEFINESBYTYPE('{}',#{owner},$,$,{},#{type_id})",
            new_global_id(),
            ref_list(occurrences)
        ));
    }

    fn rel_associates_material(&mut self, objects: &[u32], material: u32) {
        let owner = self.owner;
        self.entity(&format!(
            "IFCRELASSOCIATESMATERIAL('{}',#{owner},$,$,{},#{material})",
            new_global_id(),
            ref_list(objects)
        ));
    }
}

/// A wall's local frame: centre, run direction and half extents. Local X
/// runs along the wall, local Y across its thickness.
#[derive(Debug, Clone, Copy)]
struct WallFrame {
    cx: f64,
    cy: f64,
    rot: f64,
    half_len: f64,
    half_thick: f64,
}

impl WallFrame {
    /// Build the frame, normalising so that the longer rect side is the
    /// run direction (a 0.2 × 4.0 rect at rotation 0 runs along +Y and is
    /// therefore rotated by 90°).
    fn new(elem: &RoomElement, bbox: &Rect) -> Self {
        let (cx, cy) = center_of(elem, bbox);
        let (len, thick, rot) = if elem.rect.width >= elem.rect.height {
            (elem.rect.width, elem.rect.height, elem.rotation)
        } else {
            (elem.rect.height, elem.rect.width, elem.rotation + FRAC_PI_2)
        };
        Self {
            cx,
            cy,
            rot,
            half_len: len / 2.0,
            half_thick: thick / 2.0,
        }
    }

    /// World (bounding-box relative) → wall-local coordinates.
    fn to_local(self, x: f64, y: f64) -> (f64, f64) {
        let (s, c) = self.rot.sin_cos();
        let dx = x - self.cx;
        let dy = y - self.cy;
        (dx * c + dy * s, -dx * s + dy * c)
    }

    /// A point on the centreline at parameter `t` ∈ [-1, 1].
    fn along(&self, t: f64) -> (f64, f64) {
        let (s, c) = self.rot.sin_cos();
        (
            self.cx + c * t * self.half_len,
            self.cy + s * t * self.half_len,
        )
    }

    /// Unit run direction.
    fn dir(&self) -> (f64, f64) {
        let (s, c) = self.rot.sin_cos();
        (c, s)
    }

    /// A wall whose centreline lies on the room's bounding box is external.
    fn is_external(&self, bbox: &Rect) -> bool {
        let tol = self.half_thick + HOST_TOLERANCE;
        self.cx <= tol
            || self.cy <= tol
            || self.cx >= bbox.width - tol
            || self.cy >= bbox.height - tol
    }
}

/// Centre of an element's rect relative to the bounding-box origin.
/// `IfcMaterial` line. IFC4 added Description and Category (1 → 3
/// attributes), so a 2x3-shaped `IFCMATERIAL('Concrete')` is invalid
/// IFC4.
fn ctx_material(schema: IfcSchema, material: &str) -> String {
    if schema.is_ifc4() {
        format!("IFCMATERIAL('{}',$,$)", escape(material))
    } else {
        format!("IFCMATERIAL('{}')", escape(material))
    }
}

fn center_of(elem: &RoomElement, bbox: &Rect) -> (f64, f64) {
    (
        (elem.rect.x - bbox.x) + elem.rect.width / 2.0,
        (elem.rect.y - bbox.y) + elem.rect.height / 2.0,
    )
}

/// The four rect corners in world (bounding-box relative) coordinates.
fn corners_of(elem: &RoomElement, bbox: &Rect) -> [(f64, f64); 4] {
    let (cx, cy) = center_of(elem, bbox);
    let (s, c) = elem.rotation.sin_cos();
    let hw = elem.rect.width / 2.0;
    let hh = elem.rect.height / 2.0;
    let mut out = [(0.0, 0.0); 4];
    for (i, (lx, ly)) in [(hw, hh), (-hw, hh), (-hw, -hh), (hw, -hh)]
        .into_iter()
        .enumerate()
    {
        out[i] = (cx + lx * c - ly * s, cy + lx * s + ly * c);
    }
    out
}

/// Millimetre key for grouping type objects.
fn mm(x: f64) -> i64 {
    (x * 1000.0).round() as i64
}

/// Type-object grouping key; one type entity per distinct key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum TypeKey {
    Wall { thick_mm: i64, material: String },
    Door { w_mm: i64, h_mm: i64 },
    Window { w_mm: i64, h_mm: i64 },
    Furniture(String),
}

impl TypeKey {
    fn name(&self) -> String {
        match self {
            Self::Wall { thick_mm, material } => format!("Wall {thick_mm}mm {material}"),
            Self::Door { w_mm, h_mm } => format!("Door {w_mm}x{h_mm}"),
            Self::Window { w_mm, h_mm } => format!("Window {w_mm}x{h_mm}"),
            Self::Furniture(cat) => cat.clone(),
        }
    }
}

/// An element that has been written for the current storey.
struct Written {
    id: u32,
    placement: u32,
    height: f64,
    /// Storey-relative copy of the input element.
    elem: RoomElement,
    wall: Option<WallFrame>,
    is_external: bool,
}

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

    // Geometric representation context (TrueNorth when georeferenced)
    let true_north_ref = data.project.true_north.and_then(|(x, y)| {
        let len = (x * x + y * y).sqrt();
        if !len.is_finite() || len < 1e-9 {
            return None;
        }
        let dir = ids.next();
        b.entity(
            dir,
            &format!("IFCDIRECTION(({},{}))", r(x / len), r(y / len)),
        );
        Some(format!("#{dir}"))
    });
    let geom_ctx = ids.next();
    b.entity(
        geom_ctx,
        &format!(
            "IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#{wcs},{})",
            true_north_ref.as_deref().unwrap_or("$")
        ),
    );

    // Sub-context for body geometry
    let body_ctx = ids.next();
    b.entity(
        body_ctx,
        &format!(
            "IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Body','Model',*,*,*,*,#{geom_ctx},$,.MODEL_VIEW.,$)"
        ),
    );

    // Sub-context for wall axes
    let axis_ctx = ids.next();
    b.entity(
        axis_ctx,
        &format!(
            "IFCGEOMETRICREPRESENTATIONSUBCONTEXT('Axis','Model',*,*,*,*,#{geom_ctx},$,.GRAPH_VIEW.,$)"
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
        &format!(
            "IFCAPPLICATION(#{org},'{}','{}','{}')",
            escape(&data.project.application_version),
            escape(&data.project.application_name),
            escape(&data.project.application_name),
        ),
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
    let ref_lat = data
        .project
        .site_latitude
        .map(compound_plane_angle)
        .unwrap_or_else(|| "$".into());
    let ref_long = data
        .project
        .site_longitude
        .map(compound_plane_angle)
        .unwrap_or_else(|| "$".into());
    let ref_elev = data
        .project
        .site_elevation
        .map(r)
        .unwrap_or_else(|| "$".into());
    b.entity(
        site,
        &format!(
            "IFCSITE('{}',#{owner},'{}',$,$,#{site_placement},$,$,.ELEMENT.,{ref_lat},{ref_long},{ref_elev},$,$)",
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

    let mut ctx = Ctx {
        ids,
        b,
        owner,
        body_ctx,
        axis_ctx,
        dir_z,
        dir_x,
        schema: data.project.schema,
    };

    // ── Group elements by storey ────────────────────────────────────

    // Build storey elevations from explicit list or derive from elements
    let mut storey_order: Vec<String> = Vec::new();
    let mut storey_elevations: HashMap<String, f64> = HashMap::new();

    // First: use explicit storey elevations if provided
    for (name, elev) in &data.storey_elevations {
        storey_elevations.insert(name.clone(), *elev);
        if !storey_order.contains(name) {
            storey_order.push(name.clone());
        }
    }

    let storey_name_of = |storey: &str| -> String {
        if storey.is_empty() {
            data.project.storey_name.clone()
        } else {
            storey.to_string()
        }
    };

    // Then: add any storeys from elements / spaces not already listed
    let storey_refs = data
        .elements
        .iter()
        .map(|e| (e.storey.as_str(), e.z_offset))
        .chain(data.spaces.iter().map(|s| (s.storey.as_str(), s.z_offset)));
    for (storey, z) in storey_refs {
        let name = storey_name_of(storey);
        // Only auto-derive elevation if not explicitly set
        if !storey_elevations.contains_key(&name) {
            storey_elevations.insert(name.clone(), z);
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

    // Light fixtures go into the project's default storey when it exists,
    // otherwise into the first storey.
    let fixture_storey = if storey_order.contains(&data.project.storey_name) {
        data.project.storey_name.clone()
    } else {
        storey_order[0].clone()
    };

    // Type objects and materials are project-wide: collect occurrences
    // while walking the storeys, emit once at the end.
    let mut type_occurrences: BTreeMap<TypeKey, Vec<u32>> = BTreeMap::new();
    let mut material_occurrences: BTreeMap<String, Vec<u32>> = BTreeMap::new();

    // Create storeys and assign elements
    let mut storey_ids: Vec<u32> = Vec::new();

    for storey_name in &storey_order {
        let elevation = storey_elevations.get(storey_name).copied().unwrap_or(0.0);

        // Storey placement with elevation offset
        let storey_placement = ctx.placement(bldg_placement, 0.0, 0.0, elevation, 0.0);

        let storey_id = ctx.entity(&format!(
            "IFCBUILDINGSTOREY('{}',#{owner},'{}',$,$,#{storey_placement},$,$,.ELEMENT.,{})",
            new_global_id(),
            escape(storey_name),
            r(elevation),
        ));
        storey_ids.push(storey_id);

        // ── Elements ───────────────────────────────────────────────
        // Storey-relative copies of this storey's elements.
        let storey_elems: Vec<RoomElement> = data
            .elements
            .iter()
            .filter(|e| storey_name_of(&e.storey) == *storey_name)
            .map(|e| {
                let mut adjusted = e.clone();
                adjusted.z_offset -= elevation;
                adjusted
            })
            .collect();
        // Wall frames first: corners are mitered against the neighbours.
        let wall_frames: Vec<WallFrame> = storey_elems
            .iter()
            .filter(|e| matches!(e.kind, ElementKind::Wall))
            .map(|e| WallFrame::new(e, &data.bounding_box))
            .collect();

        let mut written: Vec<Written> = Vec::with_capacity(storey_elems.len());
        for elem in storey_elems {
            let profile = matches!(elem.kind, ElementKind::Wall).then(|| {
                let f = WallFrame::new(&elem, &data.bounding_box);
                miter_profile(&f, &wall_frames)
            });
            let height = effective_height(&elem);
            written.push(write_element(
                &mut ctx,
                elem,
                &data.bounding_box,
                height,
                storey_placement,
                profile.as_deref(),
            ));
        }

        // ── Openings: doors / windows / openings cut their host wall ──
        let n = written.len();
        let mut host_of: Vec<Option<usize>> = vec![None; n];
        // (side area, volume) removed from each wall by its openings
        let mut deductions: Vec<(f64, f64)> = vec![(0.0, 0.0); n];
        // standalone `Opening` elements that voided a wall are related
        // through IfcRelVoidsElement and must not be spatially contained
        let mut voided: Vec<bool> = vec![false; n];
        // (centre offset, extent) of each door/window along its host wall
        let mut spans: Vec<Option<(f64, f64)>> = vec![None; n];

        for i in 0..n {
            let fills = match written[i].elem.kind {
                ElementKind::Door | ElementKind::Window => true,
                ElementKind::Opening => false,
                _ => continue,
            };
            let Some(h) = find_host_wall(&written, i, &data.bounding_box) else {
                continue;
            };
            let frame = written[h].wall.expect("host is a wall");
            let (along, extent) = along_wall_extent(&frame, &written[i].elem, &data.bounding_box);
            spans[i] = Some((along, extent));
            let opening_id = if fills {
                write_opening(&mut ctx, &written[h], &written[i], along, extent)
            } else {
                written[i].id
            };
            let wall_id = written[h].id;
            ctx.entity(&format!(
                "IFCRELVOIDSELEMENT('{}',#{owner},$,$,#{wall_id},#{opening_id})",
                new_global_id()
            ));
            if fills {
                let filler = written[i].id;
                ctx.entity(&format!(
                    "IFCRELFILLSELEMENT('{}',#{owner},$,$,#{opening_id},#{filler})",
                    new_global_id()
                ));
            } else {
                voided[i] = true;
            }
            host_of[i] = Some(h);
            written[i].is_external = written[h].is_external;
            let area = extent * written[i].height;
            deductions[h].0 += area;
            deductions[h].1 += area * frame.half_thick * 2.0;
        }

        // ── Property sets, quantities, type + material registration ──
        for (i, w) in written.iter().enumerate() {
            let key = register_element(&mut ctx, w, deductions[i], &data.bounding_box);
            if let Some(key) = key {
                type_occurrences.entry(key).or_default().push(w.id);
            }
            if let Some(material) = material_of(&w.elem) {
                // Walls carry their material through the layer set of
                // their type (see `write_types`).
                if !matches!(w.elem.kind, ElementKind::Wall) {
                    material_occurrences.entry(material).or_default().push(w.id);
                }
            }
        }

        let mut contained_ids: Vec<u32> = written
            .iter()
            .zip(&voided)
            .filter(|(_, v)| !**v)
            .map(|(w, _)| w.id)
            .collect();

        // ── Spaces ─────────────────────────────────────────────────
        let mut space_ids: Vec<u32> = Vec::new();
        for sp in &data.spaces {
            if storey_name_of(&sp.storey) != *storey_name {
                continue;
            }
            if sp.slab_only {
                if let (Some(t), Some(polygon)) = (
                    sp.slab_thickness.filter(|t| *t > 0.0),
                    space_polygon(sp, &data.bounding_box),
                ) {
                    let slab = write_space_slab(
                        &mut ctx,
                        sp,
                        &polygon,
                        sp.z_offset - elevation,
                        t,
                        storey_placement,
                    );
                    contained_ids.push(slab);
                    material_occurrences
                        .entry("Concrete".to_string())
                        .or_default()
                        .push(slab);
                }
                continue;
            }
            let Some((space_id, polygon)) = write_space(
                &mut ctx,
                sp,
                &data.bounding_box,
                sp.z_offset - elevation,
                effective_space_height(sp, data),
                storey_placement,
            ) else {
                continue;
            };
            space_ids.push(space_id);
            let z_space = sp.z_offset - elevation;
            write_space_boundaries(
                &mut ctx,
                space_id,
                &polygon,
                &written,
                &host_of,
                &spans,
                &data.bounding_box,
                z_space,
            );
            if let Some(t) = sp.slab_thickness.filter(|t| *t > 0.0) {
                let slab = write_space_slab(&mut ctx, sp, &polygon, z_space, t, storey_placement);
                contained_ids.push(slab);
                material_occurrences
                    .entry("Concrete".to_string())
                    .or_default()
                    .push(slab);
                // Floor face: the polygon at the space's own floor level.
                let surface = ctx.boundary_surface(
                    (0.0, 0.0, 0.0),
                    (0.0, 0.0, 1.0),
                    (1.0, 0.0, 0.0),
                    &polygon,
                );
                ctx.entity(&format!(
                    "IFCRELSPACEBOUNDARY('{}',#{owner},'1stLevel',$,#{space_id},#{slab},#{surface},.PHYSICAL.,.INTERNAL.)",
                    new_global_id()
                ));
            }
        }
        if !space_ids.is_empty() {
            ctx.entity(&format!(
                "IFCRELAGGREGATES('{}',#{owner},$,$,#{storey_id},{})",
                new_global_id(),
                ref_list(&space_ids),
            ));
        }

        // ── Light fixtures ─────────────────────────────────────────
        // Each fixture goes into its own storey when it names one, so a
        // ceiling luminaire on the ground floor is not filed under the
        // cellar (and drawn 2.6 m too low).
        {
            for fx in data.light_fixtures.iter().filter(|fx| {
                let target = if fx.storey.is_empty() {
                    &fixture_storey
                } else {
                    &fx.storey
                };
                storey_name_of(target) == *storey_name
            }) {
                let fx_id = write_light_fixture(&mut ctx, fx, &data.bounding_box, storey_placement);
                contained_ids.push(fx_id);
            }
        }

        if !contained_ids.is_empty() {
            ctx.entity(&format!(
                "IFCRELCONTAINEDINSPATIALSTRUCTURE('{}',#{owner},$,$,{},#{storey_id})",
                new_global_id(),
                ref_list(&contained_ids),
            ));
        }
    }

    // Aggregate storeys under building
    ctx.entity(&format!(
        "IFCRELAGGREGATES('{}',#{owner},$,$,#{building},{})",
        new_global_id(),
        ref_list(&storey_ids),
    ));

    // ── Type objects + materials ───────────────────────────────────
    write_types(&mut ctx, &type_occurrences);
    for (material, occurrences) in &material_occurrences {
        let mat = ctx.entity(&ctx_material(ctx.schema, material));
        ctx.rel_associates_material(occurrences, mat);
    }

    // ── Assemble STEP file ─────────────────────────────────────────

    let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let data_section = ctx.b.into_data_section();

    format!(
        "ISO-10303-21;\n\
         HEADER;\n\
         FILE_DESCRIPTION(('ViewDefinition [{view}]'),'2;1');\n\
         FILE_NAME('roomplan_export.ifc','{timestamp}',('{}'),('{}'),\
         'bimifc-writer {}','{} {}','');\n\
         FILE_SCHEMA(('{schema}'));\n\
         ENDSEC;\n\
         DATA;\n\
         {data_section}\n\
         ENDSEC;\n\
         END-ISO-10303-21;\n",
        escape(&data.project.author),
        escape(&data.project.organization),
        env!("CARGO_PKG_VERSION"),
        escape(&data.project.application_name),
        escape(&data.project.application_version),
        view = data.project.schema.view_definition(),
        schema = data.project.schema.file_schema(),
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
        ElementKind::Furniture(_)
        | ElementKind::SanitaryTerminal(_)
        | ElementKind::LightFixture(_)
        | ElementKind::Proxy(_) => DEFAULT_FURNITURE_HEIGHT,
    }
}

/// Space height: explicit, else the room height, else the wall default.
fn effective_space_height(sp: &SpaceData, data: &RoomData) -> f64 {
    if sp.height > 0.0 {
        sp.height
    } else if data.dimensions.height > 0.0 {
        data.dimensions.height
    } else {
        DEFAULT_WALL_HEIGHT
    }
}

/// Material name for an element: explicit, else a per-kind default.
fn material_of(elem: &RoomElement) -> Option<String> {
    if let Some(m) = elem.material.as_deref() {
        if !m.trim().is_empty() {
            return Some(m.to_string());
        }
    }
    let default = match &elem.kind {
        ElementKind::Wall => "Masonry",
        ElementKind::Door => "Wood",
        ElementKind::Window => "Glass",
        ElementKind::Slab | ElementKind::Column | ElementKind::Beam | ElementKind::Stair => {
            "Concrete"
        }
        ElementKind::RoofGable | ElementKind::RoofHip | ElementKind::RoofBarrel => "Roof Tiles",
        ElementKind::Railing => "Steel",
        ElementKind::Furniture(_) => "Wood",
        ElementKind::SanitaryTerminal(_) => "Ceramic",
        ElementKind::LightFixture(_) => "Aluminium",
        ElementKind::Opening | ElementKind::Proxy(_) => return None,
    };
    Some(default.to_string())
}

/// GlobalId for an element: derived from its stable identifier when the
/// caller supplied one, random otherwise.
fn element_global_id(elem: &RoomElement) -> String {
    match elem.identifier.as_deref().map(str::trim) {
        Some(id) if !id.is_empty() => global_id_from_identifier(id),
        _ => new_global_id(),
    }
}

/// GlobalId for something derived from an element (opening, slab, …):
/// stable when the element has an identifier, random otherwise.
fn derived_id(identifier: Option<&str>, role: &str) -> String {
    match identifier.map(str::trim) {
        Some(id) if !id.is_empty() => derived_global_id(id, role),
        _ => new_global_id(),
    }
}

/// Fixed palette by kind, used when an element has no explicit colour.
/// Returns `None` for kinds that should stay unstyled (openings).
fn default_color(kind: &ElementKind) -> Option<[f32; 4]> {
    Some(match kind {
        ElementKind::Wall => [0.85, 0.85, 0.85, 1.0],
        ElementKind::Door => [0.55, 0.36, 0.20, 1.0],
        ElementKind::Window => [0.60, 0.80, 0.95, 0.35],
        ElementKind::Slab => [0.70, 0.70, 0.70, 1.0],
        ElementKind::RoofGable | ElementKind::RoofHip | ElementKind::RoofBarrel => {
            [0.60, 0.30, 0.25, 1.0]
        }
        ElementKind::Column | ElementKind::Beam | ElementKind::Stair => [0.75, 0.75, 0.75, 1.0],
        ElementKind::Railing => [0.40, 0.40, 0.40, 1.0],
        ElementKind::Furniture(_) | ElementKind::SanitaryTerminal(_) | ElementKind::Proxy(_) => {
            [0.60, 0.60, 0.60, 1.0]
        }
        ElementKind::LightFixture(_) => [0.95, 0.93, 0.80, 1.0],
        ElementKind::Opening => return None,
    })
}

/// Door / window clear width: the longer rect side (the short one is the
/// wall-thickness direction).
fn leaf_width(elem: &RoomElement) -> f64 {
    elem.rect.width.max(elem.rect.height)
}

/// Write a single element's geometry + entity.
fn write_element(
    ctx: &mut Ctx,
    elem: RoomElement,
    bbox: &Rect,
    height: f64,
    storey_placement: u32,
    wall_profile: Option<&[(f64, f64)]>,
) -> Written {
    let (cx, cy) = center_of(&elem, bbox);
    let cz = elem.z_offset;

    // Walls are normalised so local X runs along the wall (needed for the
    // axis representation, layer-set usage and opening placement).
    let wall = matches!(elem.kind, ElementKind::Wall).then(|| WallFrame::new(&elem, bbox));
    let rotation = wall.map_or(elem.rotation, |f| f.rot);

    let local_placement = ctx.placement(storey_placement, cx, cy, cz, rotation);

    // A caller-supplied mesh replaces the box body with a faceted BRep.
    let brep = elem.mesh.as_ref().and_then(|m| ctx.faceted_brep(m));

    // Surface style: explicit colour or the per-kind palette
    let base_color = elem.color.or_else(|| default_color(&elem.kind));

    #[allow(clippy::type_complexity)]
    let (solids, extra_rep, rep_type): (Vec<(u32, Option<[f32; 4]>)>, Option<u32>, &str) =
        if let Some(brep) = brep {
            let axis = wall.map(|f| ctx.axis_rep(f.half_len));
            (vec![(brep, base_color)], axis, "Brep")
        } else {
            match elem.kind {
                ElementKind::Door => (write_door_body(ctx, &elem, height), None, "SweptSolid"),
                ElementKind::Window => (write_window_body(ctx, &elem, height), None, "SweptSolid"),
                _ => {
                    let (solid, extra) = write_body(ctx, &elem, wall, wall_profile, height);
                    (vec![(solid, base_color)], extra, "SweptSolid")
                }
            }
        };

    // One IFCSTYLEDITEM per solid so sub-parts keep their own colour.
    for (solid, color) in &solids {
        if let Some(color) = color {
            ctx.style(*solid, *color);
        }
    }

    let items: Vec<u32> = solids.iter().map(|(solid, _)| *solid).collect();
    let product_shape = ctx.product_shape_items(&items, rep_type, extra_rep);

    write_entity(
        ctx,
        elem,
        local_placement,
        product_shape,
        height,
        wall,
        bbox,
    )
}

/// Frame member width (jambs, head, sill, mullion) for doors and windows.
const FRAME_WIDTH: f64 = 0.06;
/// Door leaf panel thickness.
const DOOR_LEAF_THICKNESS: f64 = 0.04;
/// Gap between the door leaf and the floor.
const DOOR_LEAF_FLOOR_GAP: f64 = 0.01;
/// Clearance between the door leaf and the inside of its frame.
const DOOR_LEAF_CLEARANCE: f64 = 0.005;
/// Glazing pane thickness.
const GLAZING_THICKNESS: f64 = 0.01;
/// Frames stand proud of both wall faces by this much: viewers that do
/// not cut openings would otherwise z-fight the frame against the wall.
const FRAME_PROUD: f64 = 0.008;
/// Windows wider than this get a mullion splitting the glazing.
const MULLION_MIN_WIDTH: f64 = 1.2;
/// Door frame: a darker wood than the leaf.
const DOOR_FRAME_COLOR: [f32; 4] = [0.42, 0.27, 0.15, 1.0];
/// Window frame: opaque white-grey.
const WINDOW_FRAME_COLOR: [f32; 4] = [0.92, 0.92, 0.90, 1.0];

/// Geometry of a door/window in its own placement: whether the clear
/// width runs along local X (else local Y), the clear width, the wall
/// thickness (the rect's short side) and the frame member width, clamped
/// so tiny elements still get a valid frame.
struct FillerFrame {
    along_x: bool,
    width: f64,
    thickness: f64,
    frame: f64,
}

impl FillerFrame {
    fn new(elem: &RoomElement, height: f64) -> Self {
        let along_x = elem.rect.width >= elem.rect.height;
        let width = leaf_width(elem);
        let thickness = elem.rect.width.min(elem.rect.height);
        let frame = FRAME_WIDTH.min(width / 4.0).min(height / 4.0);
        Self {
            along_x,
            width,
            thickness,
            frame,
        }
    }
}

/// Door body: a U-shaped frame (two jambs + head, full wall thickness)
/// and a leaf panel centred in the wall, inset inside the frame and
/// sitting just above the floor. Two solids with their own colours.
fn write_door_body(ctx: &mut Ctx, elem: &RoomElement, height: f64) -> Vec<(u32, Option<[f32; 4]>)> {
    let f = FillerFrame::new(elem, height);
    let hw = f.width / 2.0;
    let fw = f.frame;

    // Frame: U profile in the (along, up) plane, extruded across the wall.
    let frame_profile = ctx.polygon_profile(&[
        (-hw, 0.0),
        (-hw + fw, 0.0),
        (-hw + fw, height - fw),
        (hw - fw, height - fw),
        (hw - fw, 0.0),
        (hw, 0.0),
        (hw, height),
        (-hw, height),
    ]);
    let frame = ctx.extrude_across(
        frame_profile,
        f.along_x,
        -f.thickness / 2.0 - FRAME_PROUD,
        f.thickness + 2.0 * FRAME_PROUD,
    );

    // Leaf: panel filling the frame opening with a small clearance.
    let lx = (hw - fw - DOOR_LEAF_CLEARANCE).max(hw * 0.5);
    let z0 = DOOR_LEAF_FLOOR_GAP.min(height * 0.1);
    let z1 = (height - fw - DOOR_LEAF_CLEARANCE).max(z0 + height * 0.5);
    let leaf_profile = ctx.polygon_profile(&[(-lx, z0), (lx, z0), (lx, z1), (-lx, z1)]);
    let leaf_t = DOOR_LEAF_THICKNESS.min(f.thickness);
    let leaf = ctx.extrude_across(leaf_profile, f.along_x, -leaf_t / 2.0, leaf_t);

    let (frame_color, leaf_color) = match elem.color {
        Some(c) => (c, c),
        None => (
            DOOR_FRAME_COLOR,
            default_color(&ElementKind::Door).expect("door palette colour"),
        ),
    };
    vec![(frame, Some(frame_color)), (leaf, Some(leaf_color))]
}

/// Window body: a four-sided frame (full wall thickness), one centred
/// glazing pane, and a horizontal mullion at mid-height for wide
/// windows. Frame and mullion are opaque, the pane keeps the glass
/// colour's transparency.
fn write_window_body(
    ctx: &mut Ctx,
    elem: &RoomElement,
    height: f64,
) -> Vec<(u32, Option<[f32; 4]>)> {
    let f = FillerFrame::new(elem, height);
    let hw = f.width / 2.0;
    let fw = f.frame;
    let t = f.thickness;

    // Frame: rectangular ring in the (along, up) plane.
    let frame_profile = ctx.ring_profile(
        &[(-hw, 0.0), (hw, 0.0), (hw, height), (-hw, height)],
        &[
            (-hw + fw, fw),
            (-hw + fw, height - fw),
            (hw - fw, height - fw),
            (hw - fw, fw),
        ],
    );
    let frame = ctx.extrude_across(
        frame_profile,
        f.along_x,
        -t / 2.0 - FRAME_PROUD,
        t + 2.0 * FRAME_PROUD,
    );

    // Pane: overlaps half a frame width into the frame so its edges stay
    // buried (no coplanar faces with the frame's inner sides).
    let px = hw - fw / 2.0;
    let (pz0, pz1) = (fw / 2.0, height - fw / 2.0);
    let pane_profile = ctx.polygon_profile(&[(-px, pz0), (px, pz0), (px, pz1), (-px, pz1)]);
    let pane_t = GLAZING_THICKNESS.min(t);
    let pane = ctx.extrude_across(pane_profile, f.along_x, -pane_t / 2.0, pane_t);

    let pane_color = elem
        .color
        .or_else(|| default_color(&ElementKind::Window))
        .expect("window palette colour");
    let frame_color = match elem.color {
        Some([red, green, blue, _]) => [red, green, blue, 1.0],
        None => WINDOW_FRAME_COLOR,
    };

    let mut solids = vec![(frame, Some(frame_color)), (pane, Some(pane_color))];

    if f.width > MULLION_MIN_WIDTH {
        let (mz0, mz1) = (height / 2.0 - fw / 2.0, height / 2.0 + fw / 2.0);
        let mullion_profile = ctx.polygon_profile(&[(-px, mz0), (px, mz0), (px, mz1), (-px, mz1)]);
        let mullion = ctx.extrude_across(
            mullion_profile,
            f.along_x,
            -t / 2.0 - FRAME_PROUD,
            t + 2.0 * FRAME_PROUD,
        );
        solids.push((mullion, Some(frame_color)));
    }
    solids
}

/// The extruded body for an element (roof profiles, mitered wall
/// profile, or plain box) plus an optional axis representation.
fn write_body(
    ctx: &mut Ctx,
    elem: &RoomElement,
    wall: Option<WallFrame>,
    wall_profile: Option<&[(f64, f64)]>,
    height: f64,
) -> (u32, Option<u32>) {
    match &elem.kind {
        ElementKind::RoofGable | ElementKind::RoofBarrel => {
            // Gable/Barrel: 2D profile extruded along depth
            let hw = elem.rect.width / 2.0;
            let hd = elem.rect.height / 2.0;
            let overhang = 0.3;

            let pts: Vec<(f64, f64)> = match &elem.kind {
                ElementKind::RoofGable => {
                    vec![(-(hw + overhang), 0.0), (0.0, height), (hw + overhang, 0.0)]
                }
                _ => {
                    let segments = 12;
                    let radius = hw + overhang;
                    (0..=segments)
                        .map(|i| {
                            let angle = std::f64::consts::PI * (i as f64) / (segments as f64);
                            (-radius * angle.cos(), height * angle.sin())
                        })
                        .collect()
                }
            };
            let profile = ctx.polygon_profile(&pts);
            let depth = elem.rect.height + 2.0 * overhang;
            (
                write_roof_extrusion(ctx, profile, hd + overhang, depth),
                None,
            )
        }
        ElementKind::RoofHip => {
            // Hip/pyramid roof: trapezoid profile extruded along depth
            // The trapezoid narrows toward the top to approximate a hip shape
            // For square plans, the ridge is very short → nearly a pyramid
            let hw = elem.rect.width / 2.0;
            let hd = elem.rect.height / 2.0;
            let overhang = 0.3;

            // Ridge width: proportional to aspect ratio difference
            // Square plan → very narrow ridge (near pyramid)
            // Rectangular plan → ridge along the longer axis
            let aspect =
                (elem.rect.width / elem.rect.height).min(elem.rect.height / elem.rect.width);
            let ridge_ratio = 0.05 + (1.0 - aspect) * 0.4; // 5% for square, up to 45% for elongated
            let ridge_hw = hw * ridge_ratio;

            let profile = ctx.polygon_profile(&[
                (-(hw + overhang), 0.0),
                (-ridge_hw, height),
                (ridge_hw, height),
                (hw + overhang, 0.0),
            ]);
            let depth = elem.rect.height + 2.0 * overhang;
            (
                write_roof_extrusion(ctx, profile, hd + overhang, depth),
                None,
            )
        }
        ElementKind::Wall => {
            let f = wall.expect("wall frame");
            let profile = match wall_profile {
                Some(pts) => ctx.polygon_profile(pts),
                None => ctx.rect_profile(f.half_len * 2.0, f.half_thick * 2.0),
            };
            let solid = ctx.extrude(profile, height);
            let axis = ctx.axis_rep(f.half_len);
            (solid, Some(axis))
        }
        _ => {
            // Standard rectangular profile extruded upward
            let profile = ctx.rect_profile(elem.rect.width, elem.rect.height);
            (ctx.extrude(profile, height), None)
        }
    }
}

/// The IFC product entity for an element whose placement and shape are
/// already written.
fn write_entity(
    ctx: &mut Ctx,
    elem: RoomElement,
    local_placement: u32,
    product_shape: u32,
    height: f64,
    wall: Option<WallFrame>,
    bbox: &Rect,
) -> Written {
    let owner = ctx.owner;
    let schema = ctx.schema;
    let guid = element_global_id(&elem);
    let name = elem
        .label
        .as_deref()
        .map(|l| format!("'{}'", escape(l)))
        .unwrap_or_else(|| "$".to_string());

    let entity = match &elem.kind {
        ElementKind::Wall => {
            // IFC4 deprecates IfcWallStandardCase in favour of IfcWall
            // with a PredefinedType; 2x3 has no such attribute.
            let wall = if schema.is_ifc4() {
                format!("IFCWALL('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.STANDARD.)")
            } else {
                format!("IFCWALLSTANDARDCASE('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$)")
            };
            ctx.entity(&wall)
        }
        ElementKind::Door => {
            let w = leaf_width(&elem);
            ctx.entity(&format!(
                "IFCDOOR('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,{},{})",
                r(height),
                r(w)
            ))
        }
        ElementKind::Window => {
            let w = leaf_width(&elem);
            ctx.entity(&format!(
                "IFCWINDOW('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,{},{})",
                r(height),
                r(w)
            ))
        }
        ElementKind::Opening => ctx.entity(&format!(
            "IFCOPENINGELEMENT('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$)"
        )),
        ElementKind::Slab => ctx.entity(&format!(
            "IFCSLAB('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.FLOOR.)"
        )),
        ElementKind::RoofGable => ctx.entity(&format!(
            "IFCROOF('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.GABLE_ROOF.)"
        )),
        ElementKind::RoofHip => ctx.entity(&format!(
            "IFCROOF('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.HIP_ROOF.)"
        )),
        ElementKind::RoofBarrel => ctx.entity(&format!(
            "IFCROOF('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.BARREL_ROOF.)"
        )),
        ElementKind::Column => ctx.entity(&format!(
            "IFCCOLUMN('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$)"
        )),
        ElementKind::Beam => ctx.entity(&format!(
            "IFCBEAM('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$)"
        )),
        ElementKind::Stair => ctx.entity(&format!(
            "IFCSTAIR('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.STRAIGHT_RUN_STAIR.)"
        )),
        ElementKind::Railing => ctx.entity(&format!(
            "IFCRAILING('{guid}',#{owner},{name},$,$,#{local_placement},#{product_shape},$,.HANDRAIL.)"
        )),
        ElementKind::Furniture(category) => {
            let obj_type = map_furniture_type(category);
            ctx.entity(&format!(
                "IFCFURNISHINGELEMENT('{guid}',#{owner},{name},'{obj_type}',$,#{local_placement},#{product_shape},$)"
            ))
        }
        ElementKind::SanitaryTerminal(category) => {
            // IFC2X3: sanitary fixtures are IfcFlowTerminal occurrences
            // typed via IfcSanitaryTerminalType (same pattern as lights).
            let obj_type = map_furniture_type(category);
            let predefined = map_sanitary_type(category);
            // IFC4 has a real occurrence class; 2x3 only the flow terminal.
            let entity = ctx.entity(&if schema.is_ifc4() {
                format!(
                    "IFCSANITARYTERMINAL('{guid}',#{owner},{name},$,'{obj_type}',#{local_placement},#{product_shape},$,.{predefined}.)"
                )
            } else {
                format!(
                    "IFCFLOWTERMINAL('{guid}',#{owner},{name},$,'{obj_type}',#{local_placement},#{product_shape},$)"
                )
            });
            let terminal_type = ctx.entity(&format!(
                "IFCSANITARYTERMINALTYPE('{}',#{owner},{name},$,$,$,$,$,$,.{predefined}.)",
                new_global_id()
            ));
            ctx.rel_defines_by_type(&[entity], terminal_type);
            entity
        }
        ElementKind::LightFixture(_) => {
            // IFC2X3: no IfcLightFixture occurrence — IfcFlowTerminal typed
            // by IfcLightFixtureType. A scanned fixture has no photometry,
            // so PredefinedType stays NOTDEFINED (POINTSOURCE/DIRECTIONSOURCE
            // are reserved for fixtures with a known distribution).
            let entity = ctx.entity(&if schema.is_ifc4() {
                format!(
                    "IFCLIGHTFIXTURE('{guid}',#{owner},{name},$,'LightFixture',#{local_placement},#{product_shape},$,.NOTDEFINED.)"
                )
            } else {
                format!(
                    "IFCFLOWTERMINAL('{guid}',#{owner},{name},$,'LightFixture',#{local_placement},#{product_shape},$)"
                )
            });
            let fixture_type = ctx.entity(&format!(
                "IFCLIGHTFIXTURETYPE('{}',#{owner},{name},$,$,$,$,$,$,.NOTDEFINED.)",
                new_global_id()
            ));
            ctx.rel_defines_by_type(&[entity], fixture_type);
            let (placing, mounting) = light_fixture_placing(&elem);
            ctx.pset(
                &[fixture_type],
                "Pset_LightFixtureTypeCommon",
                &[
                    IfcProperty::new("NumberOfSources", IfcValue::Integer(1)),
                    IfcProperty::new("LightFixturePlacingType", IfcValue::Label(placing.into())),
                    IfcProperty::new("LightFixtureMountingType", IfcValue::Label(mounting.into())),
                ],
            );
            entity
        }
        ElementKind::Proxy(category) => {
            let obj_type = map_furniture_type(category);
            ctx.entity(&format!(
                "IFCBUILDINGELEMENTPROXY('{guid}',#{owner},{name},$,'{obj_type}',#{local_placement},#{product_shape},$,$)"
            ))
        }
    };

    let is_external = wall.is_some_and(|f| f.is_external(bbox));

    Written {
        id: entity,
        placement: local_placement,
        height,
        elem,
        wall,
        is_external,
    }
}

/// Roof profiles are drawn in the XZ plane and extruded along +Y for
/// `depth`, starting `half_depth` behind the element centre.
fn write_roof_extrusion(ctx: &mut Ctx, profile: u32, half_depth: f64, depth: f64) -> u32 {
    let extrude_dir = ctx.entity("IFCDIRECTION((0.,0.,1.))");
    let solid_origin = ctx.entity(&format!("IFCCARTESIANPOINT((0.,{},0.))", r(-half_depth)));
    let solid_axis = ctx.entity("IFCDIRECTION((0.,1.,0.))");
    let solid_ref = ctx.entity("IFCDIRECTION((-1.,0.,0.))");
    let solid_placement = ctx.entity(&format!(
        "IFCAXIS2PLACEMENT3D(#{solid_origin},#{solid_axis},#{solid_ref})"
    ));
    ctx.entity(&format!(
        "IFCEXTRUDEDAREASOLID(#{profile},#{solid_placement},#{extrude_dir},{})",
        r(depth)
    ))
}

/// Wall profile in wall-local coordinates with both ends mitered against
/// neighbouring walls. Each end looks for another wall whose axis
/// endpoint meets this one (corner) or whose axis runs across it (tee);
/// the two face lines are then cut at their intersections with the
/// neighbour's face lines, so the corner closes without overlap or gap.
/// Falls back to the plain rectangle when nothing meets an end.
fn miter_profile(f: &WallFrame, walls: &[WallFrame]) -> Vec<(f64, f64)> {
    const MAX_EXTENSION: f64 = 1.0;
    let ht = f.half_thick;
    let hl = f.half_len;
    // [+end, -end] cut positions for the top (+y) and bottom (-y) faces
    let mut x_top = [hl, -hl];
    let mut x_bot = [hl, -hl];

    for (ei, e) in [1.0_f64, -1.0].into_iter().enumerate() {
        let end = (e * hl, 0.0);
        let mut best: Option<(f64, f64, f64)> = None; // (distance, x_top, x_bot)
        for o in walls {
            let same = (o.cx - f.cx).abs() < 1e-9
                && (o.cy - f.cy).abs() < 1e-9
                && (o.rot - f.rot).abs() < 1e-9
                && (o.half_len - f.half_len).abs() < 1e-9;
            if same {
                continue;
            }
            let (ax, ay) = o.along(-1.0);
            let (bx, by) = o.along(1.0);
            let b0 = f.to_local(ax, ay);
            let b1 = f.to_local(bx, by);
            let (dx, dy) = (b1.0 - b0.0, b1.1 - b0.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                continue;
            }
            let d = (dx / len, dy / len);
            if d.1.abs() < 0.26 {
                continue; // (nearly) parallel: no corner to close
            }
            let tol = ht + o.half_thick + BOUNDARY_TOLERANCE;
            let dist0 = ((b0.0 - end.0).powi(2) + (b0.1 - end.1).powi(2)).sqrt();
            let dist1 = ((b1.0 - end.0).powi(2) + (b1.1 - end.1).powi(2)).sqrt();
            // (is_corner, point on neighbour axis, direction into the neighbour, distance)
            let (corner, bk, dir, dist) = if dist0.min(dist1) <= tol {
                if dist0 <= dist1 {
                    (true, b0, d, dist0)
                } else {
                    (true, b1, (-d.0, -d.1), dist1)
                }
            } else {
                let dist = point_segment_distance(end, b0, b1);
                if dist > tol {
                    continue;
                }
                (false, b0, d, dist)
            };
            let nb = (-dir.1, dir.0);
            // x where our face y = s·ht meets the neighbour face offset `off`
            let x_at = |s: f64, off: f64| -> f64 {
                let t = (s * ht - bk.1 - off * nb.1) / dir.1;
                bk.0 + off * nb.0 + t * dir.0
            };
            // near = the cut closer to our body, far = beyond the neighbour
            let pick = |s: f64, inner: bool| -> f64 {
                let a = x_at(s, o.half_thick);
                let b = x_at(s, -o.half_thick);
                let (lo, hi) = (a.min(b), a.max(b));
                let (near, far) = if e > 0.0 { (lo, hi) } else { (hi, lo) };
                if inner {
                    near
                } else {
                    far
                }
            };
            let (xt, xb) = if corner {
                // The face on the side the neighbour leaves towards is the
                // inner corner; the other face runs out to the outer corner.
                let top_inner = dir.1 > 0.0;
                (pick(1.0, top_inner), pick(-1.0, !top_inner))
            } else {
                // Tee: butt both faces against the neighbour's near face.
                (pick(1.0, true), pick(-1.0, true))
            };
            if (xt - e * hl).abs() > MAX_EXTENSION || (xb - e * hl).abs() > MAX_EXTENSION {
                continue;
            }
            if best.is_none_or(|(bd, _, _)| dist < bd) {
                best = Some((dist, xt, xb));
            }
        }
        if let Some((_, xt, xb)) = best {
            x_top[ei] = xt;
            x_bot[ei] = xb;
        }
    }

    // Never let the ends cross.
    if x_top[0] <= x_top[1] + 1e-6 || x_bot[0] <= x_bot[1] + 1e-6 {
        return vec![(hl, ht), (-hl, ht), (-hl, -ht), (hl, -ht)];
    }
    vec![
        (x_top[0], ht),
        (x_top[1], ht),
        (x_bot[1], -ht),
        (x_bot[0], -ht),
    ]
}

/// Find the wall (index into `written`) that hosts door/window/opening
/// `i`: the element centre must lie within the wall's length and close to
/// its centreline. Nearest centreline wins when several qualify.
fn find_host_wall(written: &[Written], i: usize, bbox: &Rect) -> Option<usize> {
    let elem = &written[i].elem;
    let (cx, cy) = center_of(elem, bbox);
    let thin = elem.rect.width.min(elem.rect.height) / 2.0;
    let mut best: Option<(usize, f64)> = None;
    for (j, cand) in written.iter().enumerate() {
        let Some(f) = cand.wall else { continue };
        let (lx, ly) = f.to_local(cx, cy);
        let cross_tol = f.half_thick + thin.max(0.15) + HOST_TOLERANCE;
        if lx.abs() <= f.half_len + HOST_TOLERANCE && ly.abs() <= cross_tol {
            let d = ly.abs();
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((j, d));
            }
        }
    }
    best.map(|(j, _)| j)
}

/// Where an element sits along a wall: (centre offset along local X,
/// extent along local X) from its projected corners.
fn along_wall_extent(frame: &WallFrame, elem: &RoomElement, bbox: &Rect) -> (f64, f64) {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for (x, y) in corners_of(elem, bbox) {
        let (lx, _) = frame.to_local(x, y);
        min_x = min_x.min(lx);
        max_x = max_x.max(lx);
    }
    ((min_x + max_x) / 2.0, max_x - min_x)
}

/// `IFCOPENINGELEMENT` cutting `host` for `filler`, placed relative to the
/// wall and running through its full thickness (plus overcut).
fn write_opening(ctx: &mut Ctx, host: &Written, filler: &Written, along: f64, extent: f64) -> u32 {
    let owner = ctx.owner;
    let frame = host.wall.expect("host is a wall");
    let z = filler.elem.z_offset - host.elem.z_offset;
    let placement = ctx.placement(host.placement, along, 0.0, z, 0.0);
    // Exactly the host wall's thickness: the opening depth comes from the
    // wall rect, never from a separate constant.
    let thickness = frame.half_thick * 2.0;
    let profile = ctx.rect_profile(extent, thickness);
    let solid = ctx.extrude(profile, filler.height);
    let shape = ctx.product_shape(solid, None);
    let name = match filler.elem.label.as_deref() {
        Some(l) => format!("Opening for {l}"),
        None => "Opening".to_string(),
    };
    ctx.entity(&format!(
        "IFCOPENINGELEMENT('{}',#{owner},'{}',$,$,#{placement},#{shape},$)",
        derived_id(filler.elem.identifier.as_deref(), "opening"),
        escape(&name)
    ))
}

/// True if the element already carries a caller-supplied pset of this name.
fn has_pset(elem: &RoomElement, name: &str) -> bool {
    elem.psets.iter().any(|p| p.name == name)
}

/// Write the element's caller-supplied psets, the derived common pset and
/// base quantities, and return the type-object key it belongs to.
fn register_element(
    ctx: &mut Ctx,
    w: &Written,
    deduction: (f64, f64),
    bbox: &Rect,
) -> Option<TypeKey> {
    let elem = &w.elem;
    let id = w.id;

    for ps in &elem.psets {
        ctx.pset(&[id], &ps.name, &ps.properties);
    }

    let key = match &elem.kind {
        ElementKind::Wall => {
            let f = w.wall.expect("wall frame");
            let key = TypeKey::Wall {
                thick_mm: mm(f.half_thick * 2.0),
                material: material_of(elem).unwrap_or_default(),
            };
            let length = f.half_len * 2.0;
            let width = f.half_thick * 2.0;
            let side = length * w.height;
            let gross_volume = side * width;
            common_pset(
                ctx,
                id,
                elem,
                "Pset_WallCommon",
                &key.name(),
                f.is_external(bbox),
            );
            ctx.quantities(
                &[id],
                &[
                    IfcQuantity::Length("Length".into(), length),
                    IfcQuantity::Length("Width".into(), width),
                    IfcQuantity::Length("Height".into(), w.height),
                    IfcQuantity::Area("GrossFootprintArea".into(), length * width),
                    IfcQuantity::Area("GrossSideArea".into(), side),
                    IfcQuantity::Area("NetSideArea".into(), (side - deduction.0).max(0.0)),
                    IfcQuantity::Volume("GrossVolume".into(), gross_volume),
                    IfcQuantity::Volume("NetVolume".into(), (gross_volume - deduction.1).max(0.0)),
                ],
            );
            Some(key)
        }
        ElementKind::Door | ElementKind::Window => {
            let width = leaf_width(elem);
            let key = if matches!(elem.kind, ElementKind::Door) {
                TypeKey::Door {
                    w_mm: mm(width),
                    h_mm: mm(w.height),
                }
            } else {
                TypeKey::Window {
                    w_mm: mm(width),
                    h_mm: mm(w.height),
                }
            };
            let pset_name = if matches!(elem.kind, ElementKind::Door) {
                "Pset_DoorCommon"
            } else {
                "Pset_WindowCommon"
            };
            common_pset(ctx, id, elem, pset_name, &key.name(), w.is_external);
            ctx.quantities(
                &[id],
                &[
                    IfcQuantity::Length("Width".into(), width),
                    IfcQuantity::Length("Height".into(), w.height),
                    IfcQuantity::Area("Area".into(), width * w.height),
                ],
            );
            Some(key)
        }
        ElementKind::Furniture(category) => {
            Some(TypeKey::Furniture(map_furniture_type(category).to_string()))
        }
        _ => None,
    };
    key
}

/// `Pset_*Common` with `Reference` and `IsExternal`, unless the caller
/// supplied a set of that name.
fn common_pset(
    ctx: &mut Ctx,
    id: u32,
    elem: &RoomElement,
    pset_name: &str,
    type_name: &str,
    is_external: bool,
) {
    if has_pset(elem, pset_name) {
        return;
    }
    let reference = elem.label.clone().unwrap_or_else(|| type_name.to_string());
    ctx.pset(
        &[id],
        pset_name,
        &[
            IfcProperty::new("Reference", IfcValue::Identifier(reference)),
            IfcProperty::new("IsExternal", IfcValue::Boolean(is_external)),
        ],
    );
}

/// Emit one type object per key, link all occurrences with
/// `IFCRELDEFINESBYTYPE`, and hang wall materials off the wall types as
/// `IfcMaterialLayerSet` (type) + `IfcMaterialLayerSetUsage` (occurrences).
fn write_types(ctx: &mut Ctx, types: &BTreeMap<TypeKey, Vec<u32>>) {
    let owner = ctx.owner;
    for (key, occurrences) in types {
        let name = escape(&key.name());
        let type_id = match key {
            TypeKey::Wall { thick_mm, material } => {
                let t = ctx.entity(&format!(
                    "IFCWALLTYPE('{}',#{owner},'{name}',$,$,$,$,$,$,.STANDARD.)",
                    derived_global_id(&key.name(), "type")
                ));
                if !material.is_empty() {
                    let thickness = *thick_mm as f64 / 1000.0;
                    let mat = ctx.entity(&ctx_material(ctx.schema, material));
                    let layer = ctx.entity(&format!(
                        "IFCMATERIALLAYER(#{mat},{},$)",
                        r(thickness)
                    ));
                    let set = ctx.entity(&format!("IFCMATERIALLAYERSET((#{layer}),'{name}')"));
                    ctx.rel_associates_material(&[t], set);
                    // Layers stack across the wall (local Y); the axis
                    // runs through the centre, so offset by half the thickness.
                    let usage = ctx.entity(&format!(
                        "IFCMATERIALLAYERSETUSAGE(#{set},.AXIS2.,.POSITIVE.,{})",
                        r(-thickness / 2.0)
                    ));
                    ctx.rel_associates_material(occurrences, usage);
                }
                t
            }
            TypeKey::Door { .. } => ctx.entity(&format!(
                "IFCDOORSTYLE('{}',#{owner},'{name}',$,$,$,$,$,.NOTDEFINED.,.NOTDEFINED.,.F.,.F.)",
                derived_global_id(&key.name(), "type")
            )),
            TypeKey::Window { .. } => ctx.entity(&format!(
                "IFCWINDOWSTYLE('{}',#{owner},'{name}',$,$,$,$,$,.NOTDEFINED.,.NOTDEFINED.,.F.,.F.)",
                derived_global_id(&key.name(), "type")
            )),
            TypeKey::Furniture(_) => ctx.entity(&format!(
                "IFCFURNITURETYPE('{}',#{owner},'{name}',$,$,$,$,$,'{name}',.NOTDEFINED.)",
                derived_global_id(&key.name(), "type")
            )),
        };
        ctx.rel_defines_by_type(occurrences, type_id);
    }
}

/// Normalise a space outline: translate into the bounding-box frame and
/// drop a repeated closing point. `None` if fewer than 3 points remain.
fn space_polygon(sp: &SpaceData, bbox: &Rect) -> Option<Vec<(f64, f64)>> {
    let mut pts: Vec<(f64, f64)> = sp
        .polygon
        .iter()
        .map(|(x, y)| (x - bbox.x, y - bbox.y))
        .collect();
    if pts.len() >= 2 {
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        if (first.0 - last.0).abs() < 1e-9 && (first.1 - last.1).abs() < 1e-9 {
            pts.pop();
        }
    }
    (pts.len() >= 3).then_some(pts)
}

/// Shoelace area (absolute) of a simple polygon.
fn polygon_area(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len();
    let mut twice = 0.0;
    for i in 0..n {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % n];
        twice += x1 * y2 - x2 * y1;
    }
    twice.abs() / 2.0
}

fn polygon_perimeter(pts: &[(f64, f64)]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (x1, y1) = pts[i];
            let (x2, y2) = pts[(i + 1) % n];
            ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt()
        })
        .sum()
}

fn polygon_centroid(pts: &[(f64, f64)]) -> (f64, f64) {
    let n = pts.len() as f64;
    let (sx, sy) = pts
        .iter()
        .fold((0.0, 0.0), |(ax, ay), (x, y)| (ax + x, ay + y));
    (sx / n, sy / n)
}

/// Write an `IFCSPACE` with its common pset and base quantities. Returns
/// the space id and its normalised outline.
fn write_space(
    ctx: &mut Ctx,
    sp: &SpaceData,
    bbox: &Rect,
    z: f64,
    height: f64,
    storey_placement: u32,
) -> Option<(u32, Vec<(f64, f64)>)> {
    let owner = ctx.owner;
    let pts = space_polygon(sp, bbox)?;

    let placement = ctx.placement(storey_placement, 0.0, 0.0, z, 0.0);
    let profile = ctx.polygon_profile(&pts);
    let solid = ctx.extrude(profile, height);
    let shape = ctx.product_shape(solid, None);

    let long_name = match sp.long_name.as_deref() {
        Some(l) => format!("'{}'", escape(l)),
        None => "$".to_string(),
    };
    let space = ctx.entity(&format!(
        // Slot 10 has the same shape but a different meaning per schema:
        // 2x3 `InteriorOrExteriorSpace`, IFC4 `PredefinedType`. Writing
        // .INTERNAL. in IFC4 happens to parse (it is a valid
        // IfcSpaceTypeEnum value) but means something else, so pick the
        // right one per schema.
        "IFCSPACE('{}',#{owner},'{}',$,$,#{placement},#{shape},{long_name},.ELEMENT.,{},$)",
        space_global_id(sp),
        escape(&sp.name),
        // 2x3: IfcInternalOrExternalEnum. IFC4: IfcSpaceTypeEnum, where
        // .INTERNAL. also exists but means "an internal space" as a type
        // rather than a location — .SPACE. is the honest value for a
        // scanned room.
        if ctx.schema.is_ifc4() {
            ".SPACE."
        } else {
            ".INTERNAL."
        }
    ));

    for ps in &sp.psets {
        ctx.pset(&[space], &ps.name, &ps.properties);
    }
    if !sp.psets.iter().any(|p| p.name == "Pset_SpaceCommon") {
        let reference = sp.long_name.clone().unwrap_or_else(|| sp.name.clone());
        ctx.pset(
            &[space],
            "Pset_SpaceCommon",
            &[IfcProperty::new(
                "Reference",
                IfcValue::Identifier(reference),
            )],
        );
    }

    let area = polygon_area(&pts);
    ctx.quantities(
        &[space],
        &[
            IfcQuantity::Length("Height".into(), height),
            IfcQuantity::Length("GrossPerimeter".into(), polygon_perimeter(&pts)),
            IfcQuantity::Area("GrossFloorArea".into(), area),
            IfcQuantity::Area("NetFloorArea".into(), area),
            IfcQuantity::Volume("GrossVolume".into(), area * height),
            IfcQuantity::Volume("NetVolume".into(), area * height),
        ],
    );

    Some((space, pts))
}

fn space_global_id(sp: &SpaceData) -> String {
    match sp.identifier.as_deref().map(str::trim) {
        Some(id) if !id.is_empty() => global_id_from_identifier(id),
        _ => new_global_id(),
    }
}

/// `IfcSlab` under a space, from the same polygon, `thickness` deep with
/// its top face at the space floor. Returns the slab id.
fn write_space_slab(
    ctx: &mut Ctx,
    sp: &SpaceData,
    pts: &[(f64, f64)],
    z_floor: f64,
    thickness: f64,
    storey_placement: u32,
) -> u32 {
    let owner = ctx.owner;
    let placement = ctx.placement(storey_placement, 0.0, 0.0, z_floor - thickness, 0.0);
    let profile = ctx.polygon_profile(pts);
    let solid = ctx.extrude(profile, thickness);
    if let Some(color) = default_color(&ElementKind::Slab) {
        ctx.style(solid, color);
    }
    let shape = ctx.product_shape(solid, None);
    let name = format!("Floor {}", sp.name);
    let slab = ctx.entity(&format!(
        "IFCSLAB('{}',#{owner},'{}',$,$,#{placement},#{shape},$,.FLOOR.)",
        derived_id(sp.identifier.as_deref(), "slab"),
        escape(&name)
    ));
    let area = polygon_area(pts);
    ctx.quantities(
        &[slab],
        &[
            IfcQuantity::Length("Width".into(), thickness),
            IfcQuantity::Length("Perimeter".into(), polygon_perimeter(pts)),
            IfcQuantity::Area("GrossArea".into(), area),
            IfcQuantity::Volume("GrossVolume".into(), area * thickness),
        ],
    );
    slab
}

/// Distance from `p` to segment `a`–`b`.
fn point_segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let len2 = abx * abx + aby * aby;
    let t = if len2 < 1e-12 {
        0.0
    } else {
        (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / len2).clamp(0.0, 1.0)
    };
    let (qx, qy) = (a.0 + t * abx, a.1 + t * aby);
    ((p.0 - qx).powi(2) + (p.1 - qy).powi(2)).sqrt()
}

/// Where a space outline runs along a wall: the wall-local X interval
/// `(lo, hi)` covered by polygon edges that are parallel to the wall and
/// lie within its thickness (plus tolerance), and the side (`+1.0` for
/// the local +Y face, `-1.0` for -Y) the space is on. `None` if no edge
/// lines the wall.
fn wall_space_overlap(frame: &WallFrame, pts: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    let n = pts.len();
    let hl = frame.half_len;
    let tol = frame.half_thick + BOUNDARY_TOLERANCE;
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let mut side_sum = 0.0;
    for i in 0..n {
        let a = frame.to_local(pts[i].0, pts[i].1);
        let b = frame.to_local(pts[(i + 1) % n].0, pts[(i + 1) % n].1);
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        let len = (ex * ex + ey * ey).sqrt();
        if len < 1e-9 || (ey / len).abs() >= 0.3 {
            continue; // degenerate or not parallel
        }
        if a.1.abs() > tol || b.1.abs() > tol {
            continue; // not alongside this wall
        }
        let t0 = a.0.min(b.0).max(-hl);
        let t1 = a.0.max(b.0).min(hl);
        if t1 - t0 < 0.05 {
            continue;
        }
        lo = lo.min(t0);
        hi = hi.max(t1);
        side_sum += (a.1 + b.1) / 2.0 * (t1 - t0);
    }
    if lo >= hi {
        return None;
    }
    let side = if side_sum.abs() > 1e-6 {
        side_sum.signum()
    } else {
        // Outline runs through the wall axis: take the centroid's side.
        let (cx, cy) = polygon_centroid(pts);
        let (_, ly) = frame.to_local(cx, cy);
        if ly >= 0.0 {
            1.0
        } else {
            -1.0
        }
    };
    Some((lo, hi, side))
}

/// Boundary surface on a wall face: the rectangle from wall-local
/// `t_lo..t_hi` along the wall, `z0..z0+height` up, on the face at
/// `side * half_thick`, in the space's coordinate system.
fn wall_face_surface(
    ctx: &mut Ctx,
    frame: &WallFrame,
    side: f64,
    t_lo: f64,
    t_hi: f64,
    z0: f64,
    height: f64,
) -> u32 {
    let (dx, dy) = frame.dir();
    // Face normal pointing into the space.
    let n = (-dy * side, dx * side);
    // In-plane X chosen so that plane Y (= normal × X) points up.
    let ref_dir = (-n.1, n.0);
    let t_origin = if ref_dir.0 * dx + ref_dir.1 * dy > 0.0 {
        t_lo
    } else {
        t_hi
    };
    let (px, py) = frame.along(t_origin / frame.half_len);
    let origin = (px + n.0 * frame.half_thick, py + n.1 * frame.half_thick, z0);
    let len = t_hi - t_lo;
    ctx.boundary_surface(
        origin,
        (n.0, n.1, 0.0),
        (ref_dir.0, ref_dir.1, 0.0),
        &[(0.0, 0.0), (len, 0.0), (len, height), (0.0, height)],
    )
}

/// True if `p` (bbox-relative) lies inside the element's rotated rect.
fn rect_contains(elem: &RoomElement, bbox: &Rect, p: (f64, f64)) -> bool {
    let (cx, cy) = center_of(elem, bbox);
    let (s, c) = elem.rotation.sin_cos();
    let dx = p.0 - cx;
    let dy = p.1 - cy;
    let lx = dx * c + dy * s;
    let ly = -dx * s + dy * c;
    lx.abs() <= elem.rect.width / 2.0 && ly.abs() <= elem.rect.height / 2.0
}

/// First-level `IFCRELSPACEBOUNDARY` from `space` to each enclosing wall,
/// the doors/windows hosted in those walls, and slabs under its centroid.
/// Each boundary carries its connection geometry: a rectangle on the wall
/// face towards the space (clipped to the door/window span for fillers),
/// or the space outline on the slab's top face.
#[allow(clippy::too_many_arguments)]
fn write_space_boundaries(
    ctx: &mut Ctx,
    space: u32,
    polygon: &[(f64, f64)],
    written: &[Written],
    host_of: &[Option<usize>],
    spans: &[Option<(f64, f64)>],
    bbox: &Rect,
    z_space: f64,
) {
    let owner = ctx.owner;
    let overlaps: Vec<Option<(f64, f64, f64)>> = written
        .iter()
        .map(|w| w.wall.and_then(|f| wall_space_overlap(&f, polygon)))
        .collect();
    let centroid = polygon_centroid(polygon);

    for (i, w) in written.iter().enumerate() {
        let surface = match w.elem.kind {
            ElementKind::Wall => match overlaps[i] {
                Some((lo, hi, side)) => {
                    let f = w.wall.expect("wall frame");
                    let z0 = w.elem.z_offset - z_space;
                    wall_face_surface(ctx, &f, side, lo, hi, z0, w.height)
                }
                None => continue,
            },
            ElementKind::Door | ElementKind::Window => {
                let Some(h) = host_of[i] else { continue };
                let (Some((lo, hi, side)), Some((along, extent))) = (overlaps[h], spans[i]) else {
                    continue;
                };
                let t_lo = (along - extent / 2.0).max(lo);
                let t_hi = (along + extent / 2.0).min(hi);
                if t_hi - t_lo < 1e-3 {
                    continue;
                }
                let f = written[h].wall.expect("host is a wall");
                let z0 = w.elem.z_offset - z_space;
                wall_face_surface(ctx, &f, side, t_lo, t_hi, z0, w.height)
            }
            ElementKind::Slab => {
                if !rect_contains(&w.elem, bbox, centroid) {
                    continue;
                }
                let z_top = w.elem.z_offset + w.height - z_space;
                ctx.boundary_surface((0.0, 0.0, z_top), (0.0, 0.0, 1.0), (1.0, 0.0, 0.0), polygon)
            }
            _ => continue,
        };
        let side = if w.is_external {
            "EXTERNAL"
        } else {
            "INTERNAL"
        };
        let elem_id = w.id;
        ctx.entity(&format!(
            "IFCRELSPACEBOUNDARY('{}',#{owner},'1stLevel',$,#{space},#{elem_id},#{surface},.PHYSICAL.,.{side}.)",
            new_global_id()
        ));
    }
}

/// Write a light fixture as `IFCLIGHTFIXTURETYPE` + `IFCFLOWTERMINAL`
/// occurrence (IFC2X3 convention), with photometry property sets.
/// Returns the occurrence id for spatial containment.
fn write_light_fixture(
    ctx: &mut Ctx,
    fx: &LightFixtureData,
    bbox: &Rect,
    storey_placement: u32,
) -> u32 {
    // Simple housing box centred on the mount point, extruded downward.
    const HOUSING_W: f64 = 0.3;
    const HOUSING_D: f64 = 0.3;
    const HOUSING_H: f64 = 0.08;

    let owner = ctx.owner;
    let cx = fx.x - bbox.x;
    let cy = fx.y - bbox.y;
    let cz = fx.z;

    let local_placement = ctx.placement(storey_placement, cx, cy, cz, fx.rotation);

    let profile = ctx.rect_profile(HOUSING_W, HOUSING_D);

    // Extrude downward so the housing hangs below the mount point.
    let extrude_dir = ctx.entity("IFCDIRECTION((0.,0.,-1.))");
    let solid_origin = ctx.entity("IFCCARTESIANPOINT((0.,0.,0.))");
    let (dz, dx) = (ctx.dir_z, ctx.dir_x);
    let solid_placement = ctx.entity(&format!("IFCAXIS2PLACEMENT3D(#{solid_origin},#{dz},#{dx})"));
    let solid = ctx.entity(&format!(
        "IFCEXTRUDEDAREASOLID(#{profile},#{solid_placement},#{extrude_dir},{HOUSING_H})"
    ));

    let product_shape = ctx.product_shape(solid, None);

    let name = escape(&fx.name);
    let occurrence_entity = if ctx.schema.is_ifc4() {
        // IFC4 has a real IfcLightFixture occurrence class.
        format!(
            "IFCLIGHTFIXTURE('{}',#{owner},'{name}',$,'LightFixture',#{local_placement},#{product_shape},$,.NOTDEFINED.)",
            new_global_id()
        )
    } else {
        // IFC2x3 has none: an IfcFlowTerminal typed by the
        // IfcLightFixtureType written below.
        format!(
            "IFCFLOWTERMINAL('{}',#{owner},'{name}',$,'LightFixture',#{local_placement},#{product_shape},$)",
            new_global_id()
        )
    };
    let occurrence = ctx.entity(&occurrence_entity);

    // Type object + IFCRELDEFINESBYTYPE. In 2x3 this is what makes it a
    // luminaire at all; in IFC4 it still carries the product data.
    let fixture_type = ctx.entity(&format!(
        "IFCLIGHTFIXTURETYPE('{}',#{owner},'{name}',$,$,$,$,$,$,.POINTSOURCE.)",
        new_global_id(),
    ));
    ctx.rel_defines_by_type(&[occurrence], fixture_type);

    // Photometric key figures.
    let mut photometry_props = vec![IfcProperty::new(
        "RatedLuminousFlux",
        IfcValue::LuminousFluxMeasure(fx.flux_lm),
    )];
    if let Some(cct) = fx.cct_k {
        photometry_props.push(IfcProperty::new(
            "ColorTemperature",
            IfcValue::ThermodynamicTemperatureMeasure(cct),
        ));
    }
    ctx.pset(&[occurrence], "Pset_GLDF_Emitter", &photometry_props);

    // Lighting control. The standard IFC set carries what a BIM tool
    // looks for; the ATLA set keeps the detail that has no standard home
    // (exact protocol string, dimming range, control gear).
    if let Some(control) = &fx.control {
        ctx.pset(
            &[occurrence],
            "Pset_LightFixtureTypeCommon",
            &[IfcProperty::new(
                "IsDimmable",
                IfcValue::Boolean(control.dimmable),
            )],
        );

        let mut props = vec![IfcProperty::new(
            "Dimmable",
            IfcValue::Boolean(control.dimmable),
        )];
        if let Some(protocol) = &control.protocol {
            props.push(IfcProperty::new(
                "DimmingMethod",
                IfcValue::Label(protocol.clone()),
            ));
        }
        if let Some(interface) = &control.interface {
            props.push(IfcProperty::new(
                "ControlInterface",
                IfcValue::Label(interface.clone()),
            ));
        }
        if let Some((min, max)) = control.range_percent {
            props.push(IfcProperty::new("DimmingRangeMin", IfcValue::Real(min)));
            props.push(IfcProperty::new("DimmingRangeMax", IfcValue::Real(max)));
        }
        if control.color_controllable {
            props.push(IfcProperty::new(
                "ColorControllable",
                IfcValue::Boolean(true),
            ));
        }
        if control.cct_controllable {
            props.push(IfcProperty::new("CctControllable", IfcValue::Boolean(true)));
        }
        if let Some(gear) = &control.control_gear {
            props.push(IfcProperty::new(
                "ControlGear",
                IfcValue::Label(gear.clone()),
            ));
        }
        if let Some(included) = control.control_gear_included {
            props.push(IfcProperty::new(
                "ControlGearIncluded",
                IfcValue::Boolean(included),
            ));
        }
        ctx.pset(&[occurrence], "Pset_ATLA_Control", &props);
    }

    // Horticultural photometry. Lumens describe what the eye sees;
    // a plant responds to photon flux, so a grow light needs PPF and
    // efficacy in the file or the export is useless for growing.
    if let Some(hort) = &fx.horticulture {
        let mut props =
            vec![
                IfcProperty::new("PhotosyntheticPhotonFlux", IfcValue::Real(hort.ppf_umol_s))
                    .with_description("PPF in µmol/s, 400–700 nm"),
            ];
        if let Some(watts) = hort.input_watts {
            props.push(IfcProperty::new("InputPower", IfcValue::Real(watts)));
        }
        if let Some(efficacy) = hort.efficacy_umol_per_j {
            props.push(
                IfcProperty::new("PhotonEfficacy", IfcValue::Real(efficacy))
                    .with_description("µmol/J"),
            );
        }
        if let Some(far_red) = hort.far_red_ppf_umol_s {
            props.push(
                IfcProperty::new("FarRedPhotonFlux", IfcValue::Real(far_red))
                    .with_description("700–800 nm, µmol/s"),
            );
        }
        if let Some(spectrum) = &hort.spectrum {
            props.push(IfcProperty::new(
                "SpectrumDescription",
                IfcValue::Label(spectrum.clone()),
            ));
        }
        ctx.pset(&[occurrence], "Pset_ATLA_Horticultural", &props);
    }

    // Raw LDT photometry, base64 — gldf-rs `Pset_GLDF_LDTRawContent` layout.
    if let Some(ldt_b64) = &fx.ldt_base64 {
        let ldt_props = vec![
            IfcProperty::new(
                "LDT_0_Filename",
                IfcValue::Label(
                    fx.ldt_file_name
                        .clone()
                        .unwrap_or_else(|| "photometry.ldt".into()),
                ),
            ),
            // IFCLABEL, matching gldf-rs's writer and its importer
            // (`Pset_GLDF_LDTRawContent`): the two must agree or the
            // round trip breaks. Long base64 in a label is technically
            // over the schema's 255-character guidance, but it is the
            // established interchange format here.
            IfcProperty::new("LDT_0_Content", IfcValue::Label(ldt_b64.clone())),
        ];
        ctx.pset(&[occurrence], "Pset_GLDF_LDTRawContent", &ldt_props);
    }

    // Caller-supplied extra psets.
    for ps in &fx.psets {
        ctx.pset(&[occurrence], &ps.name, &ps.properties);
    }

    occurrence
}

/// `Pset_LightFixtureTypeCommon` PlacingType / MountingType for a scanned
/// fixture, from its base elevation above the storey floor: on the floor
/// → FLOOR / FREESTANDING, mid-height → WALL / SURFACE, near the ceiling
/// → CEILING / SURFACE (a pendant reads as SUSPENDED when it hangs well
/// below the ceiling line).
fn light_fixture_placing(elem: &RoomElement) -> (&'static str, &'static str) {
    let base = elem.z_offset;
    if base < 0.5 {
        ("FLOOR", "FREESTANDING")
    } else if base < 1.6 {
        ("WALL", "SURFACE")
    } else if base < 2.1 {
        ("CEILING", "SUSPENDED")
    } else {
        ("CEILING", "SURFACE")
    }
}

/// IfcSanitaryTerminalTypeEnum value for a category.
fn map_sanitary_type(category: &str) -> &str {
    match category.to_lowercase().as_str() {
        "toilet" => "TOILETPAN",
        "bathtub" => "BATH",
        "sink" => "WASHHANDBASIN",
        _ => "NOTDEFINED",
    }
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

/// `IfcCompoundPlaneAngleMeasure` for a WGS84 angle in decimal degrees:
/// `(degrees, minutes, seconds, millionths of a second)`, every component
/// carrying the sign of the angle, as IFC2x3 requires.
fn compound_plane_angle(degrees: f64) -> String {
    let sign = if degrees < 0.0 { -1i64 } else { 1i64 };
    let total_millionths = (degrees.abs() * 3600.0 * 1_000_000.0).round() as i64;
    let d = total_millionths / (3600 * 1_000_000);
    let rem = total_millionths % (3600 * 1_000_000);
    let m = rem / (60 * 1_000_000);
    let rem = rem % (60 * 1_000_000);
    let s = rem / 1_000_000;
    let micro = rem % 1_000_000;
    format!("({},{},{},{})", sign * d, sign * m, sign * s, sign * micro)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pset::{IfcProperty, IfcValue};

    fn elem(kind: ElementKind, x: f64, y: f64, w: f64, h: f64, height: f64) -> RoomElement {
        RoomElement {
            kind,
            rect: Rect {
                x,
                y,
                width: w,
                height: h,
            },
            rotation: 0.0,
            label: None,
            height,
            z_offset: 0.0,
            storey: String::new(),
            color: None,
            psets: Vec::new(),
            material: None,
            identifier: None,
            mesh: None,
        }
    }

    fn sample_room() -> RoomData {
        let mut north = elem(ElementKind::Wall, 0.0, 0.0, 5.0, 0.2, 2.8);
        north.label = Some("North Wall".into());
        let east = elem(ElementKind::Wall, 4.8, 0.0, 0.2, 4.0, 2.8);
        let mut door = elem(ElementKind::Door, 1.0, 0.0, 0.9, 0.2, 2.1);
        door.label = Some("Main Door".into());
        let window = elem(ElementKind::Window, 4.8, 1.5, 0.2, 1.2, 1.5);
        let mut bed = elem(
            ElementKind::Furniture("bed".into()),
            1.0,
            1.5,
            2.0,
            1.6,
            0.6,
        );
        bed.label = Some("Double Bed".into());
        let mut table = elem(
            ElementKind::Furniture("table".into()),
            3.0,
            2.0,
            1.0,
            0.6,
            0.75,
        );
        table.rotation = 0.785; // ~45 degrees
        table.label = Some("Desk".into());

        RoomData::new(
            vec![north, east, door, window, bed, table],
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

    /// Split a STEP entity's argument list at top-level commas.
    fn step_args(line: &str) -> Vec<String> {
        let line = line.trim_end_matches(';');
        let open = line.find('(').expect("entity has arguments");
        let inner = &line[open + 1..line.len() - 1];
        let mut args = Vec::new();
        let mut depth = 0;
        let mut quoted = false;
        let mut cur = String::new();
        for c in inner.chars() {
            match c {
                '\'' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => depth -= 1,
                ',' if !quoted && depth == 0 => {
                    args.push(std::mem::take(&mut cur));
                    continue;
                }
                _ => {}
            }
            cur.push(c);
        }
        args.push(cur);
        args
    }

    /// All `#n` references inside a STEP argument.
    fn step_refs(arg: &str) -> Vec<u32> {
        arg.split('#')
            .skip(1)
            .filter_map(|s| {
                s.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse()
                    .ok()
            })
            .collect()
    }

    /// The data line `#id=…` for an entity.
    fn step_line(ifc: &str, id: u32) -> &str {
        let prefix = format!("#{id}=");
        ifc.lines()
            .find(|l| l.starts_with(&prefix))
            .unwrap_or_else(|| panic!("entity #{id} present"))
    }

    /// Body representation items of the first entity of `type_name`.
    fn body_items(ifc: &str, type_name: &str) -> Vec<u32> {
        let line = ifc
            .lines()
            .find(|l| l.contains(&format!("={type_name}(")))
            .unwrap_or_else(|| panic!("{type_name} present"));
        let shape = step_refs(&step_args(line)[6])[0];
        let reps = step_refs(&step_args(step_line(ifc, shape))[2]);
        for rep in reps {
            let args = step_args(step_line(ifc, rep));
            if args[1] == "'Body'" {
                return step_refs(&args[3]);
            }
        }
        panic!("{type_name} has a Body representation");
    }

    /// (r, g, b, transparency) of the surface style attached to an item.
    fn item_style(ifc: &str, item: u32) -> ([f32; 3], f32) {
        let styled = ifc
            .lines()
            .find(|l| l.contains(&format!("=IFCSTYLEDITEM(#{item},")))
            .unwrap_or_else(|| panic!("item #{item} is styled"));
        let surface = step_refs(&step_args(styled)[1])[0];
        let rendering = step_refs(&step_args(step_line(ifc, surface))[2])[0];
        let rendering_args = step_args(step_line(ifc, rendering));
        let colour = step_refs(&rendering_args[0])[0];
        let transparency: f32 = rendering_args[1].parse().expect("transparency");
        let c = step_args(step_line(ifc, colour));
        let f = |s: &str| s.parse::<f32>().expect("colour component");
        ([f(&c[1]), f(&c[2]), f(&c[3])], transparency)
    }

    fn count(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    #[test]
    fn produces_valid_step_structure() {
        let ifc = write_ifc(&sample_room());

        assert!(ifc.starts_with("ISO-10303-21;"));
        assert!(ifc.ends_with("END-ISO-10303-21;\n"));
        assert!(ifc.contains("HEADER;"));
        assert!(ifc.contains("DATA;"));
        assert!(ifc.contains("FILE_SCHEMA(('IFC4'))"), "IFC4 is the default");
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
    fn light_fixture_override_writes_typed_flow_terminal() {
        let mut room = sample_room();
        // A "chair" that is really a floor lamp, and a ceiling light.
        room.elements[5].kind = ElementKind::LightFixture("chair".into());
        room.elements[5].z_offset = 0.0;
        room.elements[4].kind = ElementKind::LightFixture("storage".into());
        room.elements[4].z_offset = 2.4;
        let ifc = write_ifc(&room);
        assert_eq!(ifc.matches("IFCLIGHTFIXTURETYPE(").count(), 2);
        assert!(ifc.contains(",.NOTDEFINED.)"));
        assert_eq!(ifc.matches("'Pset_LightFixtureTypeCommon'").count(), 2);
        assert!(ifc.contains("IFCLABEL('FLOOR')") && ifc.contains("IFCLABEL('FREESTANDING')"));
        assert!(ifc.contains("IFCLABEL('CEILING')") && ifc.contains("IFCLABEL('SURFACE')"));
        assert_eq!(ifc.matches("'LightFixture',").count(), 2);
    }

    #[test]
    fn contains_building_elements() {
        let ifc = write_ifc(&sample_room());

        assert!(ifc.contains("IFCWALL('"), "IFC4 default writes IfcWall");
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
    fn door_body_is_frame_plus_leaf_with_own_styles() {
        let ifc = write_ifc(&sample_room());
        let items = body_items(&ifc, "IFCDOOR");
        assert_eq!(items.len(), 2, "frame + leaf");
        for item in &items {
            assert!(step_line(&ifc, *item).contains("=IFCEXTRUDEDAREASOLID("));
        }
        let (frame_rgb, frame_t) = item_style(&ifc, items[0]);
        let (leaf_rgb, leaf_t) = item_style(&ifc, items[1]);
        assert_eq!(frame_rgb, [0.42, 0.27, 0.15], "frame is darker wood");
        assert_eq!(
            leaf_rgb,
            [0.55, 0.36, 0.2],
            "leaf keeps the door palette colour"
        );
        assert_eq!(frame_t, 0.0);
        assert_eq!(leaf_t, 0.0);
        // Frame profile is the U shape: 8 corner points, jamb width 0.06.
        let frame_args = step_args(step_line(&ifc, items[0]));
        let profile = step_refs(&frame_args[0])[0];
        let polyline = step_refs(&step_args(step_line(&ifc, profile))[2])[0];
        let pts = step_refs(&step_args(step_line(&ifc, polyline))[0]);
        assert_eq!(pts.len(), 9, "8 corners + closing point");
        assert!(
            ifc.contains("IFCCARTESIANPOINT((-0.39,2.04))"),
            "inner jamb/head corner"
        );
        // Frame runs through the 0.2 wall thickness plus 8 mm proud of each
        // face (no coplanar faces with the wall), leaf is 0.04 thick.
        assert_eq!(frame_args[3], "0.216");
        assert_eq!(step_args(step_line(&ifc, items[1]))[3], "0.04");
    }

    #[test]
    fn explicit_colour_applies_to_both_door_solids() {
        let mut room = sample_room();
        room.elements[2].color = Some([0.1, 0.2, 0.3, 1.0]);
        let ifc = write_ifc(&room);
        for item in body_items(&ifc, "IFCDOOR") {
            assert_eq!(item_style(&ifc, item).0, [0.1, 0.2, 0.3]);
        }
    }

    #[test]
    fn window_body_is_frame_plus_transparent_pane() {
        let ifc = write_ifc(&sample_room());
        let items = body_items(&ifc, "IFCWINDOW");
        assert_eq!(items.len(), 2, "1.2 m window: frame + pane, no mullion");
        let frame_line = step_line(&ifc, items[0]);
        assert!(frame_line.contains("=IFCEXTRUDEDAREASOLID("));
        let profile = step_refs(&step_args(frame_line)[0])[0];
        assert!(
            step_line(&ifc, profile).contains("=IFCARBITRARYPROFILEDEFWITHVOIDS("),
            "frame is a ring profile"
        );
        let (frame_rgb, frame_t) = item_style(&ifc, items[0]);
        let (pane_rgb, pane_t) = item_style(&ifc, items[1]);
        assert_eq!(frame_rgb, [0.92, 0.92, 0.9]);
        assert_eq!(frame_t, 0.0, "frame is opaque");
        assert_eq!(pane_rgb, [0.6, 0.8, 0.95]);
        assert!((pane_t - 0.65).abs() < 1e-6, "pane keeps glass alpha 0.35");
        assert_eq!(
            step_args(step_line(&ifc, items[1]))[3],
            "0.01",
            "pane thickness"
        );
    }

    #[test]
    fn wide_window_gets_a_mullion() {
        let mut room = sample_room();
        room.elements[3].rect.height = 1.5;
        let ifc = write_ifc(&room);
        let items = body_items(&ifc, "IFCWINDOW");
        assert_eq!(items.len(), 3, "frame + pane + mullion");
        assert_eq!(item_style(&ifc, items[2]).0, [0.92, 0.92, 0.9]);
        // Quantities still come from the rect, not the sub-solids.
        assert!(ifc.contains("IFCQUANTITYLENGTH('Width',$,$,1.5,$)"));
    }

    #[test]
    fn opening_still_spans_the_full_door_rect() {
        let ifc = write_ifc(&sample_room());
        let opening = ifc
            .lines()
            .find(|l| l.contains("'Opening for Main Door'"))
            .expect("door opening");
        let shape = step_refs(&step_args(opening)[6])[0];
        let rep = step_refs(&step_args(step_line(&ifc, shape))[2])[0];
        let items = step_refs(&step_args(step_line(&ifc, rep))[3]);
        assert_eq!(items.len(), 1, "opening is one box");
        let solid = step_args(step_line(&ifc, items[0]));
        assert_eq!(solid[3], "2.1", "opening is door height");
        let profile = step_refs(&solid[0])[0];
        let polyline = step_refs(&step_args(step_line(&ifc, profile))[2])[0];
        let pts = step_refs(&step_args(step_line(&ifc, polyline))[0]);
        let coords: Vec<(f64, f64)> = pts
            .iter()
            .map(|p| {
                let a = step_args(step_line(&ifc, *p));
                let xy = step_args(&format!("P{}", a[0]));
                (xy[0].parse().unwrap(), xy[1].parse().unwrap())
            })
            .collect();
        let w = coords.iter().map(|p| p.0).fold(f64::MIN, f64::max)
            - coords.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        let d = coords.iter().map(|p| p.1).fold(f64::MIN, f64::max)
            - coords.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        assert!((w - 0.9).abs() < 1e-9, "full door width, got {w}");
        assert!((d - 0.2).abs() < 1e-9, "full wall thickness, got {d}");
    }

    #[test]
    fn walls_carry_axis_representation() {
        let ifc = write_ifc(&sample_room());
        assert_eq!(count(&ifc, "'Axis','Curve2D'"), 2);
        assert!(ifc.contains(".GRAPH_VIEW."));
    }

    #[test]
    fn attaches_element_property_sets() {
        let mut room = sample_room();
        // Attach a typed pset to the first element (the north wall).
        room.elements[0].psets.push(ElementPset {
            name: "Pset_WallCommon".into(),
            properties: vec![
                IfcProperty::new("LoadBearing", IfcValue::Boolean(true)),
                IfcProperty::new("FireRating", IfcValue::Label("F90".into())),
            ],
        });

        let ifc = write_ifc(&room);

        assert!(ifc.contains("IFCPROPERTYSINGLEVALUE('LoadBearing',$,IFCBOOLEAN(.T.),$)"));
        assert!(ifc.contains("IFCPROPERTYSINGLEVALUE('FireRating',$,IFCLABEL('F90'),$)"));
        assert!(ifc.contains("IFCRELDEFINESBYPROPERTIES("));
        // Caller-supplied Pset_WallCommon replaces the derived one on that
        // wall; the east wall still gets the derived set → 2 total.
        assert_eq!(count(&ifc, "'Pset_WallCommon'"), 2);
    }

    #[test]
    fn furniture_only_room_has_no_psets() {
        let room = RoomData::new(
            vec![elem(
                ElementKind::Furniture("bed".into()),
                1.0,
                1.5,
                2.0,
                1.6,
                0.6,
            )],
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
        );
        let ifc = write_ifc(&room);
        assert!(!ifc.contains("IFCPROPERTYSET"));
        assert!(!ifc.contains("IFCELEMENTQUANTITY"));
        assert!(!ifc.contains("IFCRELDEFINESBYPROPERTIES"));
        // …but it does get a furniture type and a material.
        assert!(ifc.contains("IFCFURNITURETYPE("));
        assert!(ifc.contains("IFCMATERIAL('Wood',$,$)"));
    }

    #[test]
    fn derives_common_psets_and_base_quantities() {
        let ifc = write_ifc(&sample_room());

        assert_eq!(count(&ifc, "'Pset_WallCommon'"), 2);
        assert_eq!(count(&ifc, "'Pset_DoorCommon'"), 1);
        assert_eq!(count(&ifc, "'Pset_WindowCommon'"), 1);
        // Both sample walls sit on the bounding box → external.
        assert_eq!(
            count(
                &ifc,
                "IFCPROPERTYSINGLEVALUE('IsExternal',$,IFCBOOLEAN(.T.),$)"
            ),
            4
        );
        assert!(ifc.contains("IFCPROPERTYSINGLEVALUE('Reference',$,IFCIDENTIFIER('North Wall'),$)"));

        // 2 walls + 1 door + 1 window → 4 quantity sets
        assert_eq!(count(&ifc, "'BaseQuantities'"), 4);
        assert!(ifc.contains("IFCQUANTITYLENGTH('Length',$,$,5.,$)"));
        assert!(ifc.contains("IFCQUANTITYLENGTH('Width',$,$,0.2,$)"));
        assert!(ifc.contains("IFCQUANTITYAREA('GrossSideArea',$,$,14.,$)"));
        // North wall: 5 × 2.8 = 14 minus the 0.9 × 2.1 door = 12.11
        assert!(ifc.contains("IFCQUANTITYAREA('NetSideArea',$,$,12.11"));
        // Door: 0.9 × 2.1
        assert!(ifc.contains("IFCQUANTITYLENGTH('Width',$,$,0.9,$)"));
        assert!(ifc.contains("IFCQUANTITYAREA('Area',$,$,1.89"));
    }

    #[test]
    fn splits_walls_at_doors_and_windows() {
        let ifc = write_ifc(&sample_room());

        assert_eq!(count(&ifc, "IFCOPENINGELEMENT("), 2);
        assert_eq!(count(&ifc, "IFCRELVOIDSELEMENT("), 2);
        assert_eq!(count(&ifc, "IFCRELFILLSELEMENT("), 2);
        assert!(ifc.contains("'Opening for Main Door'"));
        // The window opening is exactly as deep as the 0.2 wall it cuts:
        // profile depth 0.2 → half 0.1, width 1.2 → half 0.6.
        assert!(ifc.contains("IFCCARTESIANPOINT((0.6,0.1))"));
    }

    #[test]
    fn standalone_opening_voids_wall_and_is_not_contained() {
        let mut room = sample_room();
        room.elements
            .push(elem(ElementKind::Opening, 3.0, 0.0, 1.0, 0.2, 2.4));
        let ifc = write_ifc(&room);

        // door + window openings + the standalone one
        assert_eq!(count(&ifc, "IFCOPENINGELEMENT("), 3);
        assert_eq!(count(&ifc, "IFCRELVOIDSELEMENT("), 3);
        assert_eq!(count(&ifc, "IFCRELFILLSELEMENT("), 2);

        // Containment lists 6 elements, not 7: the opening is excluded.
        let line = ifc
            .lines()
            .find(|l| l.contains("IFCRELCONTAINEDINSPATIALSTRUCTURE("))
            .unwrap();
        let list = line.split("$,$,(").nth(1).unwrap();
        let list = list.split(')').next().unwrap();
        assert_eq!(list.split(',').count(), 6);
    }

    #[test]
    fn door_far_from_any_wall_is_left_alone() {
        let room = RoomData::new(
            vec![
                elem(ElementKind::Wall, 0.0, 0.0, 5.0, 0.2, 2.8),
                elem(ElementKind::Door, 2.0, 2.0, 0.9, 0.2, 2.1),
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
        );
        let ifc = write_ifc(&room);
        assert!(!ifc.contains("IFCOPENINGELEMENT("));
        assert!(!ifc.contains("IFCRELVOIDSELEMENT("));
    }

    #[test]
    fn emits_type_objects_with_rel_defines_by_type() {
        let ifc = write_ifc(&sample_room());

        // Both walls are 200 mm masonry → one shared type.
        assert_eq!(count(&ifc, "IFCWALLTYPE("), 1);
        assert!(ifc.contains("'Wall 200mm Masonry'"));
        assert_eq!(count(&ifc, "IFCDOORSTYLE("), 1);
        assert!(ifc.contains("'Door 900x2100'"));
        assert_eq!(count(&ifc, "IFCWINDOWSTYLE("), 1);
        assert!(ifc.contains("'Window 1200x1500'"));
        // bed + table → two furniture types
        assert_eq!(count(&ifc, "IFCFURNITURETYPE("), 2);
        assert!(ifc.contains("IFCFURNITURETYPE(") && ifc.contains("'Bed'"));
        // 1 wall + 1 door + 1 window + 2 furniture = 5 rel-defines-by-type
        assert_eq!(count(&ifc, "IFCRELDEFINESBYTYPE("), 5);
    }

    #[test]
    fn walls_get_layer_set_usage_and_others_plain_materials() {
        let mut room = sample_room();
        room.elements[0].material = Some("Brick".into());
        let ifc = write_ifc(&room);

        // North wall (Brick) and east wall (default Masonry) → 2 wall types
        assert_eq!(count(&ifc, "IFCWALLTYPE("), 2);
        assert!(ifc.contains("IFCMATERIAL('Brick',$,$)"));
        assert!(ifc.contains("IFCMATERIAL('Masonry',$,$)"));
        assert_eq!(count(&ifc, "IFCMATERIALLAYERSET("), 2);
        assert!(ifc.contains("IFCMATERIALLAYERSETUSAGE("));
        assert!(ifc.contains(".AXIS2.,.POSITIVE.,-0.1)"));
        // door → Wood, window → Glass, furniture → Wood (shared)
        assert_eq!(count(&ifc, "IFCMATERIAL('Wood',$,$)"), 1);
        assert_eq!(count(&ifc, "IFCMATERIAL('Glass',$,$)"), 1);
        assert!(ifc.contains("IFCRELASSOCIATESMATERIAL("));
    }

    fn room_with_space() -> RoomData {
        let mut room = sample_room();
        room.spaces.push(SpaceData {
            name: "101".into(),
            long_name: Some("Bedroom".into()),
            polygon: vec![(0.2, 0.2), (4.8, 0.2), (4.8, 3.8), (0.2, 3.8), (0.2, 0.2)],
            height: 0.0,
            z_offset: 0.0,
            storey: String::new(),
            psets: Vec::new(),
            identifier: None,
            slab_thickness: None,
            slab_only: false,
        });
        room
    }

    #[test]
    fn emits_spaces_from_polygons() {
        let ifc = write_ifc(&room_with_space());

        assert_eq!(count(&ifc, "IFCSPACE("), 1);
        assert!(ifc.contains("'101',$,$,"));
        assert!(ifc.contains("'Bedroom',.ELEMENT.,.SPACE.,$)"));
        assert!(ifc.contains("'Pset_SpaceCommon'"));
        // 4.6 × 3.6 = 16.56 m²
        assert!(ifc.contains("IFCQUANTITYAREA('GrossFloorArea',$,$,16.56"));
        assert!(ifc.contains("IFCQUANTITYLENGTH('GrossPerimeter',$,$,16.4"));
        // Falls back to the room height.
        assert!(ifc.contains("IFCQUANTITYLENGTH('Height',$,$,2.8,$)"));
        // Space is aggregated under the storey, not contained.
        let agg = ifc
            .lines()
            .filter(|l| l.contains("IFCRELAGGREGATES("))
            .count();
        assert_eq!(agg, 4, "project, site, building, storey→space");
    }

    #[test]
    fn degenerate_space_polygon_is_skipped() {
        let mut room = sample_room();
        room.spaces.push(SpaceData {
            name: "bad".into(),
            long_name: None,
            polygon: vec![(0.0, 0.0), (1.0, 1.0)],
            height: 2.8,
            z_offset: 0.0,
            storey: String::new(),
            psets: Vec::new(),
            identifier: None,
            slab_thickness: None,
            slab_only: false,
        });
        let ifc = write_ifc(&room);
        assert!(!ifc.contains("IFCSPACE("));
    }

    #[test]
    fn emits_space_boundaries() {
        let mut room = room_with_space();
        // A slab under the room and a slab elsewhere.
        room.elements
            .push(elem(ElementKind::Slab, 0.0, 0.0, 5.0, 4.0, 0.3));
        room.elements
            .push(elem(ElementKind::Slab, 10.0, 10.0, 2.0, 2.0, 0.3));
        let ifc = write_ifc(&room);

        // north wall + east wall + door + window + one slab = 5
        assert_eq!(count(&ifc, "IFCRELSPACEBOUNDARY("), 5);
        assert!(ifc.contains(".PHYSICAL.,.EXTERNAL.)"));
    }

    #[test]
    fn space_slab_is_generated_on_request() {
        let mut room = room_with_space();
        room.spaces[0].slab_thickness = Some(0.25);
        let ifc = write_ifc(&room);
        assert_eq!(count(&ifc, "IFCSLAB("), 1);
        assert!(ifc.contains("'Floor 101'"));
        // Slab top at the floor: placed 0.25 below and extruded 0.25 up.
        assert!(ifc.contains("IFCCARTESIANPOINT((0.,0.,-0.25))"));
        assert!(ifc.contains("IFCQUANTITYAREA('GrossArea',$,$,16.56"));
        // walls, door, window + the slab
        assert_eq!(count(&ifc, "IFCRELSPACEBOUNDARY("), 5);
        assert!(ifc.contains("IFCMATERIAL('Concrete',$,$)"));
    }

    #[test]
    fn stable_identifiers_give_stable_guids() {
        let mut room = sample_room();
        room.elements[0].identifier = Some("6BA7B810-9DAD-11D1-80B4-00C04FD430C8".into());
        room.elements[2].identifier = Some("door-1".into());
        room.spaces.push(SpaceData {
            name: "101".into(),
            long_name: None,
            polygon: vec![(0.2, 0.2), (4.8, 0.2), (4.8, 3.8), (0.2, 3.8)],
            height: 0.0,
            z_offset: 0.0,
            storey: String::new(),
            psets: Vec::new(),
            identifier: Some("room-1".into()),
            slab_thickness: Some(0.2),
            slab_only: false,
        });

        let guid_of = |ifc: &str, entity: &str, needle: &str| -> String {
            ifc.lines()
                .find(|l| l.contains(entity) && l.contains(needle))
                .unwrap_or_else(|| panic!("{entity} {needle} missing"))
                .split('\'')
                .nth(1)
                .unwrap()
                .to_string()
        };

        let a = write_ifc(&room);
        let b = write_ifc(&room);
        for (entity, needle) in [
            ("IFCWALL('", "'North Wall'"),
            ("IFCDOOR(", "'Main Door'"),
            ("IFCOPENINGELEMENT(", "'Opening for Main Door'"),
            ("IFCSPACE(", "'101'"),
            ("IFCSLAB(", "'Floor 101'"),
            ("IFCWALLTYPE(", "'Wall 200mm Masonry'"),
            ("IFCDOORSTYLE(", "'Door 900x2100'"),
        ] {
            assert_eq!(
                guid_of(&a, entity, needle),
                guid_of(&b, entity, needle),
                "{entity}"
            );
        }
        // Elements without an identifier still get fresh ids.
        assert_ne!(
            guid_of(&a, "IFCWINDOW(", "IFCWINDOW("),
            guid_of(&b, "IFCWINDOW(", "IFCWINDOW(")
        );
    }

    #[test]
    fn default_palette_and_explicit_colour() {
        let ifc = write_ifc(&sample_room());
        // walls grey, door leaf wood, window glass with alpha 0.35 → transparency 0.65
        assert!(ifc.contains("IFCCOLOURRGB($,0.85,0.85,0.85)"));
        assert!(ifc.contains("IFCCOLOURRGB($,0.55,0.36,0.2)"));
        let glass_transparency = format!(",{},.BOTH.", 1.0f32 - 0.35f32);
        assert!(ifc.contains("IFCSURFACESTYLERENDERING(") && ifc.contains(&glass_transparency));

        let mut room = sample_room();
        room.elements[0].color = Some([1.0, 0.0, 0.0, 1.0]);
        let ifc = write_ifc(&room);
        assert!(ifc.contains("IFCCOLOURRGB($,1,0,0)"));
    }

    #[test]
    fn application_metadata_lands_in_header_and_ifcapplication() {
        let mut room = sample_room();
        room.project.application_name = "RoomScan".into();
        room.project.application_version = "2.4.1".into();
        let ifc = write_ifc(&room);
        assert!(ifc.contains("'2.4.1','RoomScan','RoomScan')"));
        assert!(ifc.contains("'RoomScan 2.4.1'"));
    }

    #[test]
    fn wall_corners_are_mitered() {
        let ifc = write_ifc(&sample_room());
        // North wall (axis y=0.1, local x -2.5..2.5) meets the east wall at
        // its +x end: inner face cut back to x=2.3 (world 4.8), outer face
        // out to 2.5 (world 5.0). The -x end has no neighbour: stays -2.5.
        assert!(ifc.contains("IFCCARTESIANPOINT((2.3,0.1))"));
        assert!(ifc.contains("IFCCARTESIANPOINT((2.5,-0.1))"));
        assert!(ifc.contains("IFCCARTESIANPOINT((-2.5,0.1))"));
        // East wall (local x along +y, -2..2) at its -x end: inner face
        // (world x=4.8) stops at y=0.2 → local -1.8, outer at y=0 → -2.
        assert!(ifc.contains("IFCCARTESIANPOINT((-1.8,0.1))"));
        assert!(ifc.contains("IFCCARTESIANPOINT((-2.,-0.1))"));
    }

    #[test]
    fn lone_wall_keeps_rectangular_profile() {
        let room = RoomData::new(
            vec![elem(ElementKind::Wall, 0.0, 0.0, 3.0, 0.2, 2.8)],
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
        let ifc = write_ifc(&room);
        assert!(ifc.contains("IFCCARTESIANPOINT((1.5,0.1))"));
        assert!(ifc.contains("IFCCARTESIANPOINT((-1.5,-0.1))"));
    }

    #[test]
    fn tee_junction_butts_against_the_through_wall() {
        // Long wall along x, a partition coming in from below at x=2.
        let room = RoomData::new(
            vec![
                elem(ElementKind::Wall, 0.0, 2.0, 5.0, 0.2, 2.8),
                elem(ElementKind::Wall, 1.9, 0.0, 0.2, 2.1, 2.8),
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
        );
        let ifc = write_ifc(&room);
        // Partition runs local x from -1.05 (y=0) to 1.05 (y=2.1), which
        // overlaps the through wall (y 2.0..2.2). Both faces trimmed to
        // the near face y=2.0 → local x = 0.95.
        assert!(ifc.contains("IFCCARTESIANPOINT((0.95,0.1))"));
        assert!(ifc.contains("IFCCARTESIANPOINT((0.95,-0.1))"));
        // Through wall stays a full rectangle (no end meets the partition).
        assert!(ifc.contains("IFCCARTESIANPOINT((2.5,0.1))"));
    }

    /// Schema-dependent attribute shapes. Each of these is a silent
    /// corruption if written with the wrong schema's layout: a missing
    /// or extra attribute breaks the STEP line for every consumer.
    #[test]
    fn ifc4_attribute_shapes_differ_from_2x3() {
        let mut room = sample_room();
        room.elements[0].material = Some("Concrete".into());
        room.spaces = vec![SpaceData {
            name: "Room".into(),
            long_name: None,
            polygon: vec![(0.2, 0.2), (4.8, 0.2), (4.8, 3.8), (0.2, 3.8)],
            height: 2.6,
            z_offset: 0.0,
            storey: String::new(),
            psets: Vec::new(),
            identifier: None,
            slab_thickness: None,
            slab_only: false,
        }];

        room.project.schema = IfcSchema::Ifc2x3;
        let v2 = write_ifc(&room);
        // IfcMaterial has a single attribute in 2x3.
        assert!(v2.contains("IFCMATERIAL('Concrete')"), "2x3 material");
        assert!(!v2.contains("IFCMATERIAL('Concrete',$,$)"));
        // Quantities have 4 attributes: no Formula slot.
        for line in v2.lines().filter(|l| l.contains("IFCQUANTITY")) {
            assert!(
                !line.trim_end_matches(';').ends_with(",$)"),
                "2x3 quantity must not carry a Formula: {line}"
            );
        }
        // Slot 10 is InteriorOrExteriorSpace.
        assert!(v2.contains(".ELEMENT.,.INTERNAL.,$)"), "2x3 space");

        room.project.schema = IfcSchema::Ifc4;
        let v4 = write_ifc(&room);
        // IfcMaterial gained Description + Category.
        assert!(v4.contains("IFCMATERIAL('Concrete',$,$)"), "IFC4 material");
        // The bare 2x3 form must not appear.
        assert!(!v4.contains("IFCMATERIAL('Concrete');"));
        // Every IfcQuantity* gained a trailing optional Formula.
        for q in ["IFCQUANTITYLENGTH", "IFCQUANTITYAREA", "IFCQUANTITYVOLUME"] {
            for line in v4.lines().filter(|l| l.contains(q)) {
                assert!(
                    line.trim_end_matches(';').ends_with(",$)"),
                    "IFC4 quantity needs the Formula slot: {line}"
                );
            }
        }
        // Slot 10 is PredefinedType, a different enumeration.
        assert!(
            v4.contains(".ELEMENT.,.SPACE.,$)"),
            "IFC4 space predefined type"
        );
        // The 2x3 InteriorOrExteriorSpace value must not appear.
        assert!(!v4.contains(".ELEMENT.,.INTERNAL.,$)"));
    }

    /// IFC4 mode: the classes that differ from 2x3 all switch together,
    /// and the header declares the schema it actually wrote.
    #[test]
    fn ifc4_uses_the_modern_entity_classes() {
        let mut room = sample_room();
        room.elements.push(RoomElement {
            kind: ElementKind::SanitaryTerminal("toilet".into()),
            rect: Rect {
                x: 0.5,
                y: 0.5,
                width: 0.4,
                height: 0.7,
            },
            rotation: 0.0,
            label: Some("WC".into()),
            height: 0.8,
            z_offset: 0.0,
            storey: String::new(),
            color: None,
            psets: Vec::new(),
            material: None,
            identifier: None,
            mesh: None,
        });
        room.light_fixtures = vec![LightFixtureData {
            name: "Panel".into(),
            x: 2.0,
            y: 2.0,
            z: 2.5,
            rotation: 0.0,
            flux_lm: 3000.0,
            cct_k: None,
            ldt_base64: None,
            ldt_file_name: None,
            psets: Vec::new(),
            storey: String::new(),
            control: None,
            horticulture: None,
        }];

        // IFC2x3 on request — the compatibility option.
        room.project.schema = IfcSchema::Ifc2x3;
        let ifc2x3 = write_ifc(&room);
        assert!(ifc2x3.contains("FILE_SCHEMA(('IFC2X3'))"));
        assert!(ifc2x3.contains("IFCWALLSTANDARDCASE("));
        assert!(
            !ifc2x3.contains("IFCLIGHTFIXTURE('"),
            "no occurrence class in 2x3"
        );
        assert!(!ifc2x3.contains("IFCSANITARYTERMINAL('"));
        assert!(ifc2x3.contains("IFCFLOWTERMINAL("));

        room.project.schema = IfcSchema::Ifc4;
        let ifc4 = write_ifc(&room);
        assert!(ifc4.contains("FILE_SCHEMA(('IFC4'))"));
        assert!(ifc4.contains("ViewDefinition [ReferenceView_V1.2]"));
        assert!(
            ifc4.contains("IFCWALL('"),
            "IfcWallStandardCase is deprecated"
        );
        assert!(!ifc4.contains("IFCWALLSTANDARDCASE("));
        assert!(ifc4.contains("IFCLIGHTFIXTURE('"), "real luminaire class");
        assert!(
            ifc4.contains("IFCSANITARYTERMINAL('"),
            "real sanitary class"
        );
        // The type objects stay in both: they carry the product data.
        assert!(ifc4.contains("IFCLIGHTFIXTURETYPE("));
        assert!(ifc4.contains("IFCSANITARYTERMINALTYPE("));
        // Everything else is schema-independent.
        for shared in [
            "IFCDOOR(",
            "IFCWINDOW(",
            "IFCBUILDINGSTOREY(",
            "IFCFURNISHINGELEMENT(",
        ] {
            assert!(ifc4.contains(shared), "{shared} in IFC4");
            assert!(ifc2x3.contains(shared), "{shared} in IFC2x3");
        }
    }

    /// Photometry travels as base64 in `IFCLABEL`, the layout gldf-rs
    /// writes and reads. Keep both ends identical or the round trip
    /// through IFC loses the distribution.
    #[test]
    fn raw_photometry_matches_the_gldf_layout() {
        let mut room = sample_room();
        let long = "A".repeat(4000);
        room.light_fixtures = vec![LightFixtureData {
            name: "Panel".into(),
            x: 2.0,
            y: 2.0,
            z: 2.5,
            rotation: 0.0,
            flux_lm: 3000.0,
            cct_k: Some(4000.0),
            ldt_base64: Some(long.clone()),
            ldt_file_name: Some("panel.ldt".into()),
            psets: Vec::new(),
            storey: String::new(),
            control: None,
            horticulture: None,
        }];
        let ifc = write_ifc(&room);
        assert!(ifc.contains("Pset_GLDF_LDTRawContent"));
        // gldf-rs layout: LDT_0_Filename + LDT_0_Content, both IFCLABEL.
        assert!(
            ifc.contains(&format!("IFCLABEL('{long}')")),
            "content as label"
        );
        assert!(ifc.contains("'LDT_0_Content'"));
        assert!(ifc.contains("IFCLABEL('panel.ldt')"));
        assert!(ifc.contains("'LDT_0_Filename'"));
    }

    /// A fixture naming a storey is written into that storey, not the
    /// project default. Without this every luminaire in a building was
    /// filed under the default floor (the cellar in our demo) and drawn
    /// a storey height too low.
    #[test]
    fn light_fixtures_follow_their_storey() {
        let mut room = sample_room();
        room.storey_elevations = vec![("Cellar".into(), -2.6), ("Ground Floor".into(), 0.0)];
        room.project.storey_name = "Cellar".into();
        room.elements[0].storey = "Cellar".into();
        room.elements[1].storey = "Ground Floor".into();
        room.light_fixtures = vec![LightFixtureData {
            name: "Panel".into(),
            x: 2.0,
            y: 2.0,
            z: 2.5,
            rotation: 0.0,
            flux_lm: 3000.0,
            cct_k: None,
            ldt_base64: None,
            ldt_file_name: None,
            psets: Vec::new(),
            storey: "Ground Floor".into(),
            control: None,
            horticulture: None,
        }];
        let ifc = write_ifc(&room);

        // The fixture must be contained by the Ground Floor storey.
        let ground = ifc
            .lines()
            .find(|l| l.contains("IFCBUILDINGSTOREY(") && l.contains("'Ground Floor'"))
            .expect("ground floor storey");
        let ground_id = ground.split('=').next().unwrap().trim_start_matches('#');
        let fixture = ifc
            .lines()
            .find(|l| l.contains("IFCLIGHTFIXTURE('"))
            .expect("fixture");
        let fixture_id = fixture.split('=').next().unwrap().trim_start_matches('#');
        let containment = ifc
            .lines()
            .filter(|l| l.contains("IFCRELCONTAINEDINSPATIALSTRUCTURE("))
            .find(|l| {
                l.contains(&format!("#{fixture_id},")) || l.contains(&format!("#{fixture_id})"))
            })
            .expect("fixture is contained somewhere");
        assert!(
            containment
                .trim_end_matches(");")
                .ends_with(&format!("#{ground_id}")),
            "fixture must sit in the Ground Floor storey: {containment}"
        );
    }

    #[test]
    fn mesh_elements_become_faceted_breps() {
        let mut room = sample_room();
        // A tetrahedron for the bed.
        room.elements[4].mesh = Some(ElementMesh {
            vertices: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            indices: vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3],
        });
        let ifc = write_ifc(&room);
        assert_eq!(count(&ifc, "IFCFACETEDBREP("), 1);
        assert_eq!(count(&ifc, "IFCCLOSEDSHELL("), 1);
        assert_eq!(count(&ifc, "IFCFACE("), 4);
        assert_eq!(count(&ifc, "'Body','Brep'"), 1);
        // Style still applies (mid-grey furniture).
        assert!(ifc.contains("IFCCOLOURRGB($,0.6,0.6,0.6)"));

        // Malformed mesh → box fallback.
        room.elements[4].mesh = Some(ElementMesh {
            vertices: vec![[0.0; 3]],
            indices: vec![0, 1, 2],
        });
        let ifc = write_ifc(&room);
        assert!(!ifc.contains("IFCFACETEDBREP("));
    }

    #[test]
    fn space_boundaries_carry_face_geometry() {
        let mut room = room_with_space();
        room.elements
            .push(elem(ElementKind::Slab, 0.0, 0.0, 5.0, 4.0, 0.3));
        let ifc = write_ifc(&room);

        assert_eq!(count(&ifc, "IFCRELSPACEBOUNDARY("), 5);
        assert_eq!(count(&ifc, "IFCCONNECTIONSURFACEGEOMETRY("), 5);
        assert_eq!(count(&ifc, "IFCCURVEBOUNDEDPLANE("), 5);
        assert!(
            !ifc.contains(",$,.PHYSICAL."),
            "every boundary has geometry"
        );

        // North wall face towards the room (y=0.2), outline 0.2..4.8 →
        // 4.6 long, full wall height. Plane X runs -x so origin at x=4.8.
        assert!(ifc.contains("IFCCARTESIANPOINT((4.8,0.2,0.))"));
        assert!(ifc.contains("IFCDIRECTION((0.,1.,0.))"));
        assert!(ifc.contains("IFCCARTESIANPOINT((4.6,2.8))"));
        // Door in that wall: 0.9 wide, 2.1 high.
        assert!(ifc.contains("IFCCARTESIANPOINT((0.9,2.1))"));
        // Slab top at z=0.3 with the room outline.
        assert!(ifc.contains("IFCCARTESIANPOINT((0.,0.,0.3))"));
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
            vec![elem(ElementKind::Wall, 0.0, 0.0, 3.0, 0.2, 0.0)],
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

    #[test]
    fn wall_frame_normalises_run_direction() {
        let bbox = Rect {
            x: 0.0,
            y: 0.0,
            width: 5.0,
            height: 4.0,
        };
        let east = elem(ElementKind::Wall, 4.8, 0.0, 0.2, 4.0, 2.8);
        let f = WallFrame::new(&east, &bbox);
        assert!((f.rot - FRAC_PI_2).abs() < 1e-9);
        assert!((f.half_len - 2.0).abs() < 1e-9);
        assert!((f.half_thick - 0.1).abs() < 1e-9);
        // A point 1 m up the wall is 1 m along local X.
        let (lx, ly) = f.to_local(4.9, 3.0);
        assert!((lx - 1.0).abs() < 1e-9);
        assert!(ly.abs() < 1e-9);
        assert!(f.is_external(&bbox));
    }
}
