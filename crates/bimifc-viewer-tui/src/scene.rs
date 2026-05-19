// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Scene representation for rendering

use bimifc_geometry::{GeometryRouter, Mesh};
use bimifc_model::{get_default_color, EntityId, IfcModel, IfcType};
use bimifc_parser::{EntityScanner, ParsedModel};
use glam::Vec3;
use std::collections::HashMap;
use std::sync::Arc;

/// MEP discipline categorisation for a renderable element.
///
/// Used by the discipline-view filter in the viewport. The mapping is
/// derived from the IFC inheritance graph (see [`classify_discipline`]):
/// concrete subtypes inherit eligibility through `IfcType::is_subtype_of`,
/// so e.g. `IfcCableCarrierSegment` resolves as `Electrical` because it
/// transitively inherits from `IfcFlowSegment` and we treat that whole
/// subtree as electrical-or-plumbing-or-HVAC based on the concrete leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Discipline {
    /// Architectural / structural / generic — shown in every view.
    #[default]
    Other = 0,
    Electrical = 1,
    Plumbing = 2,
    Hvac = 3,
    Lighting = 4,
}

/// Map an IFC type name to a MEP discipline.
///
/// Handles both modern concrete types (IfcCableSegment, IfcPipeSegment, …)
/// and the IFC2x3 generic distribution types (IfcFlowSegment etc.) that
/// real-world files like the NBU medical clinic IFC use. For the generic
/// types we return `Other` — pair this with `classify_by_name` on the
/// entity's `Name` attribute for fuller classification on legacy files.
pub fn classify_discipline_name(type_name: &str) -> Discipline {
    let upper = type_name.to_ascii_uppercase();
    match upper.as_str() {
        // Concrete electrical
        "IFCCABLESEGMENT" | "IFCCABLECARRIERSEGMENT" | "IFCCABLECARRIERFITTING"
        | "IFCELECTRICAPPLIANCE" | "IFCELECTRICDISTRIBUTIONBOARD" | "IFCJUNCTIONBOX"
        | "IFCOUTLET" | "IFCSWITCHINGDEVICE" => Discipline::Electrical,
        // Concrete plumbing
        "IFCPIPESEGMENT" | "IFCPIPEFITTING" | "IFCSANITARYTERMINAL" | "IFCVALVE"
        | "IFCWASTETERMINAL" => Discipline::Plumbing,
        // Concrete HVAC
        "IFCAIRTERMINAL" | "IFCAIRTERMINALBOX" | "IFCSPACEHEATER" | "IFCDUCTSEGMENT"
        | "IFCDUCTFITTING" | "IFCFAN" => Discipline::Hvac,
        // Lighting
        "IFCLIGHTFIXTURE" => Discipline::Lighting,
        _ => Discipline::Other,
    }
}

/// Fallback: infer discipline from the entity's `Name` attribute when the
/// IFC type alone is too generic to tell (IFC2x3 files use `IfcFlowSegment`
/// for ducts, pipes, AND conduits indiscriminately). Keyword list is
/// derived from observation of real-world Revit-exported MEP models:
/// "rectangular duct", "pipe types: waste", "supply diffuser", "troffer
/// light", etc.
///
/// Returns `Other` if no keyword matches — caller decides whether to
/// override the type-based classification.
pub fn classify_by_name(name: &str) -> Discipline {
    let lower = name.to_ascii_lowercase();
    // Order matters: more specific keywords first.
    if lower.contains("troffer") || lower.contains("fixture") || lower.contains(" lamp") {
        return Discipline::Lighting;
    }
    if lower.contains("diffuser") || lower.contains("duct") || lower.contains("hvac")
        || lower.contains("supply air") || lower.contains("return air")
        || lower.contains("vav") || lower.contains("fan ") || lower.contains("coil")
        || lower.contains("damper")
    {
        return Discipline::Hvac;
    }
    if lower.contains("pipe") || lower.contains("plumb") || lower.contains("sanitary")
        || lower.contains("waste") || lower.contains("water")
        || lower.contains("hydronic") || lower.contains("sprinkler")
    {
        return Discipline::Plumbing;
    }
    if lower.contains("cable") || lower.contains("conduit") || lower.contains("electric")
        || lower.contains("circuit") || lower.contains("panel")
        || lower.contains("junction") || lower.contains("outlet")
    {
        return Discipline::Electrical;
    }
    Discipline::Other
}

/// Classify by `IfcType`. Falls back to the generic flow types from IFC2x3
/// when concrete subtypes aren't present — those land in `Other` because
/// the entity-type alone doesn't tell us the discipline.
pub fn classify_discipline(t: &IfcType) -> Discipline {
    use IfcType::*;
    match t {
        IfcCableSegment | IfcCableCarrierSegment | IfcCableCarrierFitting => Discipline::Electrical,
        IfcPipeSegment | IfcPipeFitting => Discipline::Plumbing,
        IfcAirTerminal | IfcSpaceHeater => Discipline::Hvac,
        IfcLightFixture => Discipline::Lighting,
        // IFC2x3 fallback: when files use the generic IfcFlowSegment/Fitting/
        // Terminal directly (like NBU_MedicalClinic, 2011-era), we can't
        // determine the discipline from the type alone.
        _ => Discipline::Other,
    }
}

/// A triangle ready for rendering
#[derive(Clone, Debug)]
pub struct RenderTriangle {
    pub v0: Vec3,
    pub v1: Vec3,
    pub v2: Vec3,
    pub normal: Vec3,
    pub color: [f32; 4],
    pub entity_id: u64,
    /// MEP discipline tag used by the viewport's discipline-view filter.
    /// `Other` for everything that isn't electrical/plumbing/HVAC/lighting
    /// — those triangles are always shown.
    pub discipline: Discipline,
}

impl RenderTriangle {
    /// Get triangle center
    pub fn center(&self) -> Vec3 {
        (self.v0 + self.v1 + self.v2) / 3.0
    }
}

/// Pre-computed edge for wireframe rendering
#[derive(Clone, Debug)]
pub struct SceneEdge {
    pub v0: Vec3,
    pub v1: Vec3,
    /// Normals of adjacent faces (1 for boundary, 2 for internal edges)
    pub face_normals: Vec<Vec3>,
}

/// Scene containing all triangles and pre-computed edges for rendering
pub struct Scene {
    pub triangles: Vec<RenderTriangle>,
    pub edges: Vec<SceneEdge>,
    pub bounds_min: Vec3,
    pub bounds_max: Vec3,
}

impl Scene {
    /// Create a new empty scene
    pub fn new() -> Self {
        Self {
            triangles: Vec::new(),
            edges: Vec::new(),
            bounds_min: Vec3::splat(f32::INFINITY),
            bounds_max: Vec3::splat(f32::NEG_INFINITY),
        }
    }

    /// Build edges from triangles (call after all triangles are added)
    pub fn build_edges(&mut self) {
        // Edge key for deduplication
        #[derive(Hash, Eq, PartialEq)]
        struct EdgeKey([i32; 3], [i32; 3]);

        fn quantize(v: Vec3) -> [i32; 3] {
            [
                (v.x * 1000.0) as i32,
                (v.y * 1000.0) as i32,
                (v.z * 1000.0) as i32,
            ]
        }

        fn make_key(a: [i32; 3], b: [i32; 3]) -> EdgeKey {
            if a < b {
                EdgeKey(a, b)
            } else {
                EdgeKey(b, a)
            }
        }

        // Collect edges with their face normals
        let mut edge_map: HashMap<EdgeKey, (Vec3, Vec3, Vec<Vec3>)> = HashMap::new();

        for tri in &self.triangles {
            let q0 = quantize(tri.v0);
            let q1 = quantize(tri.v1);
            let q2 = quantize(tri.v2);

            for (qa, qb, va, vb) in [
                (q0, q1, tri.v0, tri.v1),
                (q1, q2, tri.v1, tri.v2),
                (q2, q0, tri.v2, tri.v0),
            ] {
                let key = make_key(qa, qb);
                edge_map
                    .entry(key)
                    .or_insert_with(|| (va, vb, Vec::new()))
                    .2
                    .push(tri.normal);
            }
        }

        // Convert to SceneEdge
        self.edges = edge_map
            .into_values()
            .map(|(v0, v1, normals)| SceneEdge {
                v0,
                v1,
                face_normals: normals,
            })
            .collect();
    }

    /// Build scene from IFC model using the geometry router
    pub fn from_model(model: &Arc<dyn IfcModel>) -> Self {
        let mut scene = Scene::new();

        // Create geometry router with model's unit scale
        let router = GeometryRouter::with_default_processors_and_unit_scale(model.unit_scale());
        let resolver = model.resolver();

        // Get all entity IDs and process those with geometry types
        for id in resolver.all_ids() {
            let Some(entity) = resolver.get(id) else {
                continue;
            };

            // Skip non-geometric types
            if !entity.ifc_type.has_geometry() {
                continue;
            }

            // Get color for this type
            let color = get_default_color(&entity.ifc_type);
            // See `from_content`: fall back to Name-based heuristic when
            // the IFC type is a generic IFC2x3 flow class.
            let mut discipline = classify_discipline(&entity.ifc_type);
            if discipline == Discipline::Other {
                if let Some(name) = entity.get_string(2) {
                    discipline = classify_by_name(name);
                }
            }

            // Process geometry
            match router.process_element(&entity, resolver) {
                Ok(mesh) if !mesh.is_empty() => {
                    scene.add_mesh(&mesh, color, id.0 as u64, discipline);
                }
                _ => {}
            }
        }

        scene.build_edges();
        scene
    }

    /// Build scene from IFC content string (more efficient - uses scanner)
    pub fn from_content(content: &str, model: &Arc<ParsedModel>) -> Self {
        let mut scene = Scene::new();

        // Create geometry router with model's unit scale
        let router = GeometryRouter::with_default_processors_and_unit_scale(model.unit_scale());
        let resolver = model.resolver();

        // Use scanner for fast initial pass to find building elements
        let mut scanner = EntityScanner::new(content);
        let mut element_ids: Vec<(u32, String)> = Vec::new();

        while let Some((id, type_name, _, _)) = scanner.next_entity() {
            if has_geometry_type_name(type_name) {
                element_ids.push((id, type_name.to_string()));
            }
        }

        // Process each element
        for (id, type_name) in &element_ids {
            let Some(entity) = resolver.get(EntityId(*id)) else {
                continue;
            };

            // Get color for this type
            let color = get_color_for_type(type_name);
            // Try concrete-type classification first; for IFC2x3 generic
            // flow types (IfcFlowSegment etc.) fall back to the entity's
            // Name attribute (attribute index 2 for IfcRoot subclasses).
            // Real-world Revit exports name them helpfully:
            //   'Rectangular Duct:Mitered Elbows / Tees' → HVAC
            //   'Pipe Types: Waste:...' → Plumbing
            let mut discipline = classify_discipline_name(type_name);
            if discipline == Discipline::Other {
                if let Some(name) = entity.get_string(2) {
                    discipline = classify_by_name(name);
                }
            }

            // Process geometry
            if let Ok(mesh) = router.process_element(&entity, resolver) {
                if !mesh.is_empty() {
                    scene.add_mesh(&mesh, color, *id as u64, discipline);
                }
            }
        }

        scene.build_edges();
        scene
    }

    /// Add a mesh to the scene with a discipline tag
    fn add_mesh(&mut self, mesh: &Mesh, color: [f32; 4], entity_id: u64, discipline: Discipline) {
        let vertex_count = mesh.vertex_count();
        if vertex_count == 0 {
            return;
        }

        // Convert positions to Vec3 (IFC Z-up to Y-up)
        let positions: Vec<Vec3> = (0..vertex_count)
            .map(|i| {
                let idx = i * 3;
                // Z-up to Y-up conversion
                Vec3::new(
                    mesh.positions[idx],
                    mesh.positions[idx + 2],  // Z -> Y
                    -mesh.positions[idx + 1], // -Y -> Z
                )
            })
            .collect();

        // Convert normals
        let normals: Vec<Vec3> = if mesh.normals.len() == mesh.positions.len() {
            (0..vertex_count)
                .map(|i| {
                    let idx = i * 3;
                    Vec3::new(
                        mesh.normals[idx],
                        mesh.normals[idx + 2],
                        -mesh.normals[idx + 1],
                    )
                    .normalize()
                })
                .collect()
        } else {
            vec![Vec3::Y; vertex_count]
        };

        // Create triangles
        for tri in mesh.indices.chunks_exact(3) {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;

            if i0 >= vertex_count || i1 >= vertex_count || i2 >= vertex_count {
                continue;
            }

            let v0 = positions[i0];
            let v1 = positions[i1];
            let v2 = positions[i2];

            // Update bounds
            self.bounds_min = self.bounds_min.min(v0).min(v1).min(v2);
            self.bounds_max = self.bounds_max.max(v0).max(v1).max(v2);

            // Calculate face normal
            let edge1 = v1 - v0;
            let edge2 = v2 - v0;
            let face_normal = edge1.cross(edge2).normalize();

            // Use face normal if valid, otherwise average vertex normals
            let normal = if face_normal.is_finite() && face_normal.length() > 0.5 {
                face_normal
            } else {
                (normals[i0] + normals[i1] + normals[i2]).normalize()
            };

            self.triangles.push(RenderTriangle {
                v0,
                v1,
                v2,
                normal,
                color,
                entity_id,
                discipline,
            });
        }
    }

    /// Get scene center
    pub fn center(&self) -> Vec3 {
        (self.bounds_min + self.bounds_max) * 0.5
    }

    /// Get scene diagonal size
    pub fn diagonal(&self) -> f32 {
        (self.bounds_max - self.bounds_min).length()
    }

    /// Create a test cube scene for debugging
    pub fn test_cube() -> Self {
        let mut scene = Scene::new();

        // Cube vertices (unit cube centered at origin)
        // Using standard cube vertex ordering:
        //     7----6
        //    /|   /|
        //   3----2 |
        //   | 4--|-5
        //   |/   |/
        //   0----1
        let vertices = [
            Vec3::new(-1.0, -1.0, -1.0), // 0: front-bottom-left
            Vec3::new(1.0, -1.0, -1.0),  // 1: front-bottom-right
            Vec3::new(1.0, 1.0, -1.0),   // 2: front-top-right
            Vec3::new(-1.0, 1.0, -1.0),  // 3: front-top-left
            Vec3::new(-1.0, -1.0, 1.0),  // 4: back-bottom-left
            Vec3::new(1.0, -1.0, 1.0),   // 5: back-bottom-right
            Vec3::new(1.0, 1.0, 1.0),    // 6: back-top-right
            Vec3::new(-1.0, 1.0, 1.0),   // 7: back-top-left
        ];

        // Scale up
        let vertices: Vec<Vec3> = vertices.iter().map(|v| *v * 5.0).collect();

        // Define faces with CCW winding (outward-facing normals)
        // Each face has 2 triangles, normal pointing outward
        let faces = [
            // Front face (z = -1) - normal points toward -Z
            ([0, 2, 1], [0, 3, 2], Vec3::NEG_Z, [0.8, 0.2, 0.2, 1.0]),
            // Back face (z = +1) - normal points toward +Z
            ([4, 5, 6], [4, 6, 7], Vec3::Z, [0.2, 0.8, 0.2, 1.0]),
            // Left face (x = -1) - normal points toward -X
            ([0, 4, 7], [0, 7, 3], Vec3::NEG_X, [0.2, 0.2, 0.8, 1.0]),
            // Right face (x = +1) - normal points toward +X
            ([1, 2, 6], [1, 6, 5], Vec3::X, [0.8, 0.8, 0.2, 1.0]),
            // Top face (y = +1) - normal points toward +Y
            ([3, 7, 6], [3, 6, 2], Vec3::Y, [0.8, 0.2, 0.8, 1.0]),
            // Bottom face (y = -1) - normal points toward -Y
            ([0, 1, 5], [0, 5, 4], Vec3::NEG_Y, [0.2, 0.8, 0.8, 1.0]),
        ];

        for (tri1, tri2, normal, color) in faces {
            scene.triangles.push(RenderTriangle {
                v0: vertices[tri1[0]],
                v1: vertices[tri1[1]],
                v2: vertices[tri1[2]],
                normal,
                color,
                entity_id: 1,
                discipline: Discipline::Other,
            });
            scene.triangles.push(RenderTriangle {
                v0: vertices[tri2[0]],
                v1: vertices[tri2[1]],
                v2: vertices[tri2[2]],
                normal,
                color,
                entity_id: 1,
                discipline: Discipline::Other,
            });
        }

        scene.bounds_min = Vec3::splat(-5.0);
        scene.bounds_max = Vec3::splat(5.0);

        scene.build_edges();
        scene
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if an IFC type name (string) can have geometry representation
fn has_geometry_type_name(type_name: &str) -> bool {
    matches!(
        type_name.to_uppercase().as_str(),
        "IFCWALL"
            | "IFCWALLSTANDARDCASE"
            | "IFCCURTAINWALL"
            | "IFCSLAB"
            | "IFCROOF"
            | "IFCBEAM"
            | "IFCCOLUMN"
            | "IFCMEMBER"
            | "IFCPLATE"
            | "IFCDOOR"
            | "IFCWINDOW"
            | "IFCSTAIR"
            | "IFCSTAIRFLIGHT"
            | "IFCRAMP"
            | "IFCRAMPFLIGHT"
            | "IFCRAILING"
            | "IFCCOVERING"
            | "IFCFURNISHINGELEMENT"
            | "IFCFOOTING"
            | "IFCPILE"
            | "IFCBUILDINGELEMENTPROXY"
            | "IFCFLOWTERMINAL"
            | "IFCFLOWSEGMENT"
            | "IFCFLOWFITTING"
            | "IFCFLOWCONTROLLER"
            | "IFCSPACE"
            | "IFCLIGHTFIXTURE"
    )
}

/// Get color for an IFC type name
fn get_color_for_type(type_name: &str) -> [f32; 4] {
    match type_name.to_uppercase().as_str() {
        "IFCWALL" | "IFCWALLSTANDARDCASE" => [0.9, 0.9, 0.85, 1.0],
        "IFCCURTAINWALL" => [0.6, 0.8, 0.9, 0.5],
        "IFCSLAB" => [0.7, 0.7, 0.7, 1.0],
        "IFCROOF" => [0.6, 0.3, 0.2, 1.0],
        "IFCBEAM" | "IFCCOLUMN" | "IFCMEMBER" => [0.5, 0.5, 0.6, 1.0],
        "IFCPLATE" => [0.6, 0.6, 0.7, 1.0],
        "IFCDOOR" => [0.5, 0.35, 0.2, 1.0],
        "IFCWINDOW" => [0.7, 0.85, 0.95, 0.4],
        "IFCSTAIR" | "IFCSTAIRFLIGHT" | "IFCRAMP" | "IFCRAMPFLIGHT" => [0.6, 0.6, 0.55, 1.0],
        "IFCRAILING" => [0.4, 0.4, 0.45, 1.0],
        "IFCSPACE" => [0.3, 0.5, 0.7, 0.2],
        "IFCLIGHTFIXTURE" => [1.0, 0.9, 0.3, 1.0],
        _ => [0.7, 0.7, 0.7, 1.0],
    }
}
