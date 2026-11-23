//! Input data model for IFC export.
//!
//! These types describe the room geometry that gets converted to IFC entities.
//! They are intentionally simple and decoupled from Apple's RoomPlan types —
//! the FFI layer maps RoomPlan data into these structures.

use crate::pset::IfcProperty;

/// A 2D rectangle (position + size) — matches CGRect semantics.
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Room dimensions in meters.
#[derive(Debug, Clone, Copy)]
pub struct RoomDimensions {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
}

/// Type of building element.
#[derive(Debug, Clone)]
pub enum ElementKind {
    Wall,
    Door,
    Window,
    Opening,
    Slab,
    /// Gable roof (Satteldach) — triangular profile extruded along depth.
    RoofGable,
    /// Hip/pyramid roof (Walmdach) — 4 triangular faces meeting at a point/ridge.
    RoofHip,
    /// Barrel/arch roof (Tonnendach) — semicircular profile extruded along depth.
    RoofBarrel,
    Column,
    Beam,
    Stair,
    Railing,
    /// Furniture / fixture with a category name (e.g. "bed", "toilet").
    Furniture(String),
    /// Sanitary fixture (sink, toilet, bathtub) with a category name.
    /// Written IFC2X3-style as an IFCFLOWTERMINAL occurrence typed via
    /// IFCSANITARYTERMINALTYPE + IFCRELDEFINESBYTYPE.
    SanitaryTerminal(String),
    /// Luminaire the scanner mis-detected as furniture (floor lamp,
    /// pendant, ceiling light), with the original category name. IFC2X3
    /// has no `IfcLightFixture` occurrence, so it is written as an
    /// `IFCFLOWTERMINAL` typed by `IFCLIGHTFIXTURETYPE` (PredefinedType
    /// NOTDEFINED — photometry unknown) with `Pset_LightFixtureTypeCommon`
    /// placing/mounting derived from where the scan found it.
    LightFixture(String),
    /// Generic element (IFCBUILDINGELEMENTPROXY) with a category name —
    /// the honest choice when nothing more specific fits.
    Proxy(String),
}

/// A property set attached to a single element on export.
///
/// Each set becomes an `IFCPROPERTYSET` linked to the element's product
/// occurrence via `IFCRELDEFINESBYPROPERTIES`. The `properties` carry typed
/// IFC values (measures, labels, booleans …) — see [`IfcProperty`] / `IfcValue`.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementPset {
    /// Property-set name (e.g. `Pset_WallCommon` or a custom set).
    pub name: String,
    /// The typed properties contained in this set.
    pub properties: Vec<IfcProperty>,
}

/// A single room element with 2D footprint and extrusion height.
#[derive(Debug, Clone)]
pub struct RoomElement {
    pub kind: ElementKind,
    pub rect: Rect,
    /// Rotation in radians around the element center.
    pub rotation: f64,
    /// Optional human-readable label.
    pub label: Option<String>,
    /// Extrusion height in meters (defaults applied if zero).
    pub height: f64,
    /// Z offset in meters (elevation above ground, default 0.0).
    pub z_offset: f64,
    /// Storey name (elements with the same name are grouped into one storey).
    /// If empty, uses the project's default storey name.
    pub storey: String,
    /// Optional surface color as [R, G, B, A] with values 0.0–1.0.
    /// Alpha < 1.0 makes the element transparent (glass). `None` uses a
    /// per-kind palette (light-grey walls, wooden doors, glass windows,
    /// mid-grey furniture).
    pub color: Option<[f32; 4]>,
    /// Property sets attached to this element on export. Empty = none.
    ///
    /// The writer adds `Pset_WallCommon` / `Pset_DoorCommon` /
    /// `Pset_WindowCommon` and `BaseQuantities` automatically; a
    /// caller-supplied set with the same name takes precedence.
    pub psets: Vec<ElementPset>,
    /// Material name written as `IfcMaterial` (walls: one-layer
    /// `IfcMaterialLayerSet` + `IfcMaterialLayerSetUsage`). `None` picks a
    /// per-kind default (masonry walls, wooden doors, glass windows, …).
    pub material: Option<String>,
    /// Stable source identifier (e.g. the RoomPlan surface/object UUID).
    /// When set, the IFC GlobalId is derived from it so re-exports keep
    /// their ids; `None` generates a fresh random GlobalId.
    pub identifier: Option<String>,
    /// Optional triangle mesh written as `IfcFacetedBrep` instead of the
    /// extruded box (e.g. RoomPlan `.model` furniture from USDZ). Ignored
    /// when empty or malformed.
    pub mesh: Option<ElementMesh>,
}

/// A closed triangle mesh in element-local coordinates: origin at the
/// rect centre on the element base, X along `rect.width`, Y along
/// `rect.height`, Z up, meters. Rotation and placement come from the
/// owning [`RoomElement`].
#[derive(Debug, Clone, PartialEq)]
pub struct ElementMesh {
    pub vertices: Vec<[f64; 3]>,
    /// Triangle list, three vertex indices per face, counter-clockwise
    /// seen from outside.
    pub indices: Vec<u32>,
}

impl ElementMesh {
    /// True if the mesh has at least one triangle and every index is in
    /// range.
    pub fn is_valid(&self) -> bool {
        !self.indices.is_empty()
            && self.indices.len().is_multiple_of(3)
            && self
                .indices
                .iter()
                .all(|&i| (i as usize) < self.vertices.len())
    }
}

/// A room / space outline for `IFCSPACE` export.
///
/// The polygon is the floor-plan outline in the same plan frame as the
/// element rects (absolute, before bounding-box translation). It is
/// extruded by `height` and aggregated under its storey. The writer also
/// derives `Pset_SpaceCommon`, `BaseQuantities` and first-level
/// `IfcRelSpaceBoundary` links to the walls/doors/windows enclosing it.
#[derive(Debug, Clone)]
pub struct SpaceData {
    /// Short name / number (IfcSpace.Name).
    pub name: String,
    /// Long descriptive name (IfcSpace.LongName), e.g. "Living Room".
    pub long_name: Option<String>,
    /// Closed outline (x, y) in meters. At least 3 points; the closing
    /// point may be repeated or omitted.
    pub polygon: Vec<(f64, f64)>,
    /// Space height in meters (0 = room height from [`RoomDimensions`]).
    pub height: f64,
    /// Floor elevation in meters (absolute, like `RoomElement::z_offset`).
    pub z_offset: f64,
    /// Storey name; empty = project default storey.
    pub storey: String,
    /// Extra property sets attached to the space.
    pub psets: Vec<ElementPset>,
    /// Stable source identifier (see [`RoomElement::identifier`]).
    pub identifier: Option<String>,
    /// When set, an `IfcSlab` of this thickness (meters) is generated
    /// under the space from the same polygon and linked as a boundary.
    pub slab_thickness: Option<f64>,
    /// Preview mode: write only the slab (when `slab_thickness` is set)
    /// and no `IfcSpace`. Viewers draw spaces as translucent volumes that
    /// hide the walls behind them; a preview still wants the floor.
    pub slab_only: bool,
}

/// A placed light fixture for IFC export (e.g. a GLDF-sourced luminaire).
///
/// Written as an `IFCLIGHTFIXTURETYPE` + `IFCFLOWTERMINAL` occurrence pair
/// (the IFC2X3 convention — `IfcLightFixture` only exists from IFC4). The
/// raw LDT photometry travels in a `Pset_GLDF_LDTRawContent` property set
/// using gldf-rs's `LDT_{i}_Filename` / `LDT_{i}_Content` (base64) layout.
#[derive(Debug, Clone)]
pub struct LightFixtureData {
    pub name: String,
    /// Plan position in meters, same frame as element rects (x/y),
    /// z = mounting height above the floor.
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Rotation about the vertical axis in radians.
    pub rotation: f64,
    /// Total luminous flux in lumens.
    pub flux_lm: f64,
    /// Correlated colour temperature in kelvin.
    pub cct_k: Option<f64>,
    /// Raw LDT photometry, base64-encoded.
    pub ldt_base64: Option<String>,
    /// Original LDT file name (for the raw-content pset).
    pub ldt_file_name: Option<String>,
    /// Extra property sets attached to the occurrence.
    pub psets: Vec<ElementPset>,
    /// Storey this fixture belongs to (matching a name in
    /// `RoomData::storey_elevations`). Empty = the project's default
    /// storey, which is only right for single-storey scans: in a
    /// building it would file every luminaire under one floor, and `z`
    /// is relative to that floor, so they end up at the wrong height.
    pub storey: String,
    /// Lighting control, from the luminaire's ATLA/GLDF data. Written as
    /// `Pset_LightFixtureTypeCommon` properties plus a `Pset_ATLA_Control`
    /// set, so a receiving tool sees what can be dimmed and how.
    pub control: Option<LightControlData>,
    /// Horticultural photometry (grow lights). Illuminance says nothing
    /// useful about plant growth: photosynthetic photon flux does.
    pub horticulture: Option<HorticulturalData>,
}

/// Lighting control capability (DALI, 0-10 V, DMX, …).
#[derive(Debug, Clone, Default)]
pub struct LightControlData {
    /// Can the output be dimmed at all.
    pub dimmable: bool,
    /// Protocol as the manufacturer states it: "DALI-2", "0-10V", …
    pub protocol: Option<String>,
    /// Control interface when it differs from the dimming protocol.
    pub interface: Option<String>,
    /// Dimming range in percent, e.g. (1.0, 100.0).
    pub range_percent: Option<(f64, f64)>,
    /// Tunable white / colour control.
    pub color_controllable: bool,
    pub cct_controllable: bool,
    /// Driver description, and whether it ships with the luminaire.
    pub control_gear: Option<String>,
    pub control_gear_included: Option<bool>,
}

/// Horticultural photometry for a grow light.
#[derive(Debug, Clone, Default)]
pub struct HorticulturalData {
    /// Photosynthetic photon flux, µmol/s (400–700 nm).
    pub ppf_umol_s: f64,
    /// Input power in watts, for the efficacy figure.
    pub input_watts: Option<f64>,
    /// Photon efficacy, µmol/J.
    pub efficacy_umol_per_j: Option<f64>,
    /// Photon flux outside PAR, when the product reports it.
    pub far_red_ppf_umol_s: Option<f64>,
    /// Spectrum label from the product data ("full spectrum", "R/B 4:1").
    pub spectrum: Option<String>,
}

/// Which IFC schema to write.
///
/// The entity model differs in ways that matter to consumers: IFC2x3 has
/// no occurrence class for a luminaire or a sanitary fixture (both are
/// `IfcFlowTerminal` typed by a `…Type`), and uses `IfcWallStandardCase`.
/// IFC4 has real classes for all three. Older BIM tools still import 2x3
/// most reliably, so both are offered rather than one being "right".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IfcSchema {
    /// IFC2x3 TC1 — for older installations and contracts that still
    /// specify it by name.
    Ifc2x3,
    /// IFC4 ADD2 TC1 — the default. Real occurrence classes
    /// (`IfcLightFixture`, `IfcSanitaryTerminal`, `IfcWall`) mean a
    /// consumer does not have to infer a luminaire from a flow terminal
    /// plus its type, which is exactly the inference that goes wrong.
    #[default]
    Ifc4,
}

impl IfcSchema {
    /// The `FILE_SCHEMA` identifier.
    pub fn file_schema(self) -> &'static str {
        match self {
            IfcSchema::Ifc2x3 => "IFC2X3",
            IfcSchema::Ifc4 => "IFC4",
        }
    }

    /// The MVD written into `FILE_DESCRIPTION`.
    pub fn view_definition(self) -> &'static str {
        match self {
            IfcSchema::Ifc2x3 => "CoordinationView_V2.0",
            IfcSchema::Ifc4 => "ReferenceView_V1.2",
        }
    }

    pub fn is_ifc4(self) -> bool {
        matches!(self, IfcSchema::Ifc4)
    }
}

/// Project-level metadata for the IFC header.
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub project_name: String,
    pub site_name: String,
    pub building_name: String,
    pub storey_name: String,
    pub author: String,
    pub organization: String,
    /// Name written to `IfcApplication.ApplicationFullName` and the STEP
    /// header's originating system (e.g. the exporting app).
    pub application_name: String,
    /// Version written to `IfcApplication.Version` (e.g. the app bundle
    /// version).
    pub application_version: String,
    /// Site georeference (WGS84 degrees) → `IfcSite.RefLatitude` /
    /// `RefLongitude`. `None` leaves the site unreferenced.
    pub site_latitude: Option<f64>,
    pub site_longitude: Option<f64>,
    /// Site elevation above sea level in meters → `IfcSite.RefElevation`.
    pub site_elevation: Option<f64>,
    /// Direction of true north in the plan frame (x, y), unit length not
    /// required → `IfcGeometricRepresentationContext.TrueNorth`.
    pub true_north: Option<(f64, f64)>,
    /// Schema to write; IFC4 by default.
    pub schema: IfcSchema,
}

impl Default for ProjectInfo {
    fn default() -> Self {
        Self {
            project_name: "RoomPlan Export".into(),
            site_name: "Default Site".into(),
            building_name: "Building".into(),
            storey_name: "Ground Floor".into(),
            author: "RoomPlan User".into(),
            organization: "RoomPlan App".into(),
            application_name: "bimifc-writer".into(),
            application_version: env!("CARGO_PKG_VERSION").into(),
            site_latitude: None,
            site_longitude: None,
            site_elevation: None,
            true_north: None,
            schema: IfcSchema::default(),
        }
    }
}

/// Complete room data ready for IFC export.
#[derive(Debug, Clone)]
pub struct RoomData {
    pub elements: Vec<RoomElement>,
    pub bounding_box: Rect,
    pub dimensions: RoomDimensions,
    pub project: ProjectInfo,
    /// Explicit storey elevations: (name, elevation_meters).
    /// If empty, elevations are derived from element z_offsets.
    pub storey_elevations: Vec<(String, f64)>,
    /// Placed light fixtures (written into the default storey).
    pub light_fixtures: Vec<LightFixtureData>,
    /// Room outlines written as `IFCSPACE` (one per entry).
    pub spaces: Vec<SpaceData>,
}

impl RoomData {
    pub fn new(elements: Vec<RoomElement>, bounding_box: Rect, dimensions: RoomDimensions) -> Self {
        Self {
            elements,
            bounding_box,
            dimensions,
            project: ProjectInfo::default(),
            storey_elevations: Vec::new(),
            light_fixtures: Vec::new(),
            spaces: Vec::new(),
        }
    }
}
