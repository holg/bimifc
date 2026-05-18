//! Input data model for IFC export.
//!
//! These types describe the room geometry that gets converted to IFC entities.
//! They are intentionally simple and decoupled from Apple's RoomPlan types —
//! the FFI layer maps RoomPlan data into these structures.

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
    /// Alpha < 1.0 makes the element transparent (glass).
    pub color: Option<[f32; 4]>,
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
}

impl RoomData {
    pub fn new(elements: Vec<RoomElement>, bounding_box: Rect, dimensions: RoomDimensions) -> Self {
        Self {
            elements,
            bounding_box,
            dimensions,
            project: ProjectInfo::default(),
            storey_elevations: Vec::new(),
        }
    }
}
