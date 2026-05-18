//! UniFFI bindings for bimifc — IFC parser, model, geometry, and spatial access.
//!
//! Generates native Swift (and Kotlin) bindings automatically.
//! On Swift side you get: `IfcModel`, `MeshData`, `SpatialNode`, `PropertySet`, etc.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bimifc_geometry::GeometryRouter;
use bimifc_model::{AttributeValue, EntityId, EntityResolver};

uniffi::include_scaffolding!("bimifc_ffi");

// ═══════════════════════════════════════════════════════════════════════════
// Error type
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum IfcError {
    #[error("Parse error: {message}")]
    ParseError { message: String },
    #[error("IO error: {message}")]
    IoError { message: String },
    #[error("Entity not found: #{id}")]
    EntityNotFound { id: u32 },
}

// ═══════════════════════════════════════════════════════════════════════════
// Record types (Swift structs)
// ═══════════════════════════════════════════════════════════════════════════

#[derive(uniffi::Record)]
pub struct ModelMetadata {
    pub schema_version: String,
    pub originating_system: Option<String>,
    pub preprocessor_version: Option<String>,
    pub file_name: Option<String>,
    pub file_description: Option<String>,
    pub author: Option<String>,
    pub organization: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(uniffi::Record)]
pub struct MeshData {
    /// Flattened vertex positions [x, y, z, x, y, z, ...]
    pub positions: Vec<f32>,
    /// Flattened vertex normals [nx, ny, nz, nx, ny, nz, ...]
    pub normals: Vec<f32>,
    /// Triangle indices
    pub indices: Vec<u32>,
    pub vertex_count: u32,
    pub triangle_count: u32,
}

#[derive(uniffi::Record)]
pub struct EntityInfo {
    pub id: u32,
    pub ifc_type: String,
    pub has_geometry: bool,
    pub is_spatial: bool,
}

#[derive(uniffi::Record)]
pub struct Property {
    pub name: String,
    pub value: String,
    pub unit: Option<String>,
}

#[derive(uniffi::Record)]
pub struct PropertySet {
    pub name: String,
    pub properties: Vec<Property>,
}

#[derive(uniffi::Record)]
pub struct Quantity {
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub quantity_type: String,
}

#[derive(uniffi::Record)]
pub struct SpatialNode {
    pub id: u32,
    pub node_type: String,
    pub name: String,
    pub entity_type: String,
    pub elevation: Option<f32>,
    pub has_geometry: bool,
    pub children: Vec<SpatialNode>,
}

#[derive(uniffi::Record)]
pub struct StoreyInfo {
    pub id: u32,
    pub name: String,
    pub elevation: f32,
    pub element_count: u32,
}

#[derive(uniffi::Record)]
pub struct EntityMesh {
    pub entity_id: u32,
    pub ifc_type: String,
    pub mesh: MeshData,
}

#[derive(uniffi::Record)]
pub struct LightFixtureInfo {
    pub entity_id: u32,
    pub name: String,
    pub position: Vec<f32>,
    pub color_temperature: Option<f64>,
    pub luminous_flux: Option<f64>,
}

/// RGBA color [r, g, b, a] in 0.0–1.0 range.
#[derive(uniffi::Record)]
pub struct ColorRgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Entity mesh with its IFC-authored or default color.
#[derive(uniffi::Record)]
pub struct ColoredEntityMesh {
    pub entity_id: u32,
    pub ifc_type: String,
    pub name: String,
    pub mesh: MeshData,
    pub color: ColorRgba,
    /// 4x4 column-major transform matrix (16 floats), if available.
    pub transform: Option<Vec<f32>>,
}

/// Goniometric photometric data for a light source.
#[derive(uniffi::Record)]
pub struct GoniometricInfo {
    pub name: String,
    pub colour_temperature: f64,
    pub luminous_flux: f64,
    pub emitter_type: String,
    pub distribution_type: String,
}

/// Type statistics: count of entities per IFC type.
#[derive(uniffi::Record)]
pub struct TypeCount {
    pub ifc_type: String,
    pub count: u32,
}

// ═══════════════════════════════════════════════════════════════════════════
// Writer types (for IFC export from RoomPlan data)
// ═══════════════════════════════════════════════════════════════════════════

/// 2D rectangle matching CGRect semantics.
#[derive(uniffi::Record)]
pub struct FfiRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Room dimensions in meters.
#[derive(uniffi::Record)]
pub struct FfiRoomDimensions {
    pub width: f64,
    pub height: f64,
    pub depth: f64,
}

/// Element type for IFC export.
#[derive(uniffi::Enum)]
pub enum FfiElementKind {
    Wall,
    Door,
    Window,
    Opening,
    Furniture { category: String },
}

/// A single room element for IFC export.
#[derive(uniffi::Record)]
pub struct FfiRoomElement {
    pub kind: FfiElementKind,
    pub rect: FfiRect,
    /// Rotation in radians.
    pub rotation: f64,
    pub label: Option<String>,
    /// Extrusion height in meters (0 = use default).
    pub height: f64,
}

/// Project metadata for the IFC file header.
#[derive(uniffi::Record)]
pub struct FfiProjectInfo {
    pub project_name: String,
    pub site_name: String,
    pub building_name: String,
    pub storey_name: String,
    pub author: String,
    pub organization: String,
}

// ═══════════════════════════════════════════════════════════════════════════
// IfcWriter object
// ═══════════════════════════════════════════════════════════════════════════

/// IFC file writer — converts RoomPlan geometry to IFC 2x3 STEP format.
#[derive(uniffi::Object)]
pub struct IfcWriter;

#[uniffi::export]
impl IfcWriter {
    /// Create a new IFC writer.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    /// Generate IFC 2x3 content from room elements.
    ///
    /// Returns the complete IFC file as a string.
    pub fn write_ifc(
        &self,
        elements: Vec<FfiRoomElement>,
        bounding_box: FfiRect,
        dimensions: FfiRoomDimensions,
        project: FfiProjectInfo,
    ) -> String {
        let room = bimifc_writer::RoomData {
            elements: elements.into_iter().map(convert_ffi_element).collect(),
            bounding_box: bimifc_writer::Rect {
                x: bounding_box.x,
                y: bounding_box.y,
                width: bounding_box.width,
                height: bounding_box.height,
            },
            dimensions: bimifc_writer::RoomDimensions {
                width: dimensions.width,
                height: dimensions.height,
                depth: dimensions.depth,
            },
            project: bimifc_writer::ProjectInfo {
                project_name: project.project_name,
                site_name: project.site_name,
                building_name: project.building_name,
                storey_name: project.storey_name,
                author: project.author,
                organization: project.organization,
            },
            storey_elevations: Vec::new(),
        };
        bimifc_writer::write_ifc(&room)
    }

    /// Generate IFC and write directly to a file path.
    pub fn write_ifc_to_file(
        &self,
        elements: Vec<FfiRoomElement>,
        bounding_box: FfiRect,
        dimensions: FfiRoomDimensions,
        project: FfiProjectInfo,
        path: String,
    ) -> Result<(), IfcError> {
        let content = self.write_ifc(elements, bounding_box, dimensions, project);
        std::fs::write(&path, content).map_err(|e| IfcError::IoError {
            message: e.to_string(),
        })
    }
}

fn convert_ffi_element(e: FfiRoomElement) -> bimifc_writer::RoomElement {
    bimifc_writer::RoomElement {
        kind: match e.kind {
            FfiElementKind::Wall => bimifc_writer::ElementKind::Wall,
            FfiElementKind::Door => bimifc_writer::ElementKind::Door,
            FfiElementKind::Window => bimifc_writer::ElementKind::Window,
            FfiElementKind::Opening => bimifc_writer::ElementKind::Opening,
            FfiElementKind::Furniture { category } => {
                bimifc_writer::ElementKind::Furniture(category)
            }
        },
        rect: bimifc_writer::Rect {
            x: e.rect.x,
            y: e.rect.y,
            width: e.rect.width,
            height: e.rect.height,
        },
        rotation: e.rotation,
        label: e.label,
        height: e.height,
        z_offset: 0.0,
        storey: String::new(),
        color: None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Main object: IfcModel
// ═══════════════════════════════════════════════════════════════════════════

#[derive(uniffi::Object)]
pub struct IfcModel {
    inner: Arc<dyn bimifc_model::IfcModel>,
    geometry: Mutex<Option<GeometryRouter>>,
}

#[uniffi::export]
impl IfcModel {
    // ── Construction ─────────────────────────────────────────────────

    /// Parse IFC content from a string (auto-detects STEP vs IFCX).
    #[uniffi::constructor]
    pub fn parse(content: &str) -> Result<Arc<Self>, IfcError> {
        let model = bimifc_parser::parse_auto(content).map_err(|e| IfcError::ParseError {
            message: e.to_string(),
        })?;
        Ok(Arc::new(Self {
            inner: model,
            geometry: Mutex::new(None),
        }))
    }

    /// Parse an IFC file from disk.
    #[uniffi::constructor]
    pub fn from_file(path: &str) -> Result<Arc<Self>, IfcError> {
        let content = std::fs::read_to_string(path).map_err(|e| IfcError::IoError {
            message: e.to_string(),
        })?;
        let model = bimifc_parser::parse_auto(&content).map_err(|e| IfcError::ParseError {
            message: e.to_string(),
        })?;
        Ok(Arc::new(Self {
            inner: model,
            geometry: Mutex::new(None),
        }))
    }

    // ── Metadata ─────────────────────────────────────────────────────

    pub fn metadata(&self) -> ModelMetadata {
        let m = self.inner.metadata();
        ModelMetadata {
            schema_version: m.schema_version.clone(),
            originating_system: m.originating_system.clone(),
            preprocessor_version: m.preprocessor_version.clone(),
            file_name: m.file_name.clone(),
            file_description: m.file_description.clone(),
            author: m.author.clone(),
            organization: m.organization.clone(),
            timestamp: m.timestamp.clone(),
        }
    }

    pub fn schema_version(&self) -> String {
        self.inner.metadata().schema_version.clone()
    }

    pub fn unit_scale(&self) -> f64 {
        self.inner.unit_scale()
    }

    // ── Entity access ────────────────────────────────────────────────

    pub fn entity_count(&self) -> u32 {
        self.inner.resolver().entity_count() as u32
    }

    pub fn all_entity_ids(&self) -> Vec<u32> {
        self.inner
            .resolver()
            .all_ids()
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    pub fn get_entity(&self, id: u32) -> Option<EntityInfo> {
        let entity = self.inner.resolver().get(EntityId(id))?;
        Some(EntityInfo {
            id: entity.id.0,
            ifc_type: entity.ifc_type.name().to_string(),
            has_geometry: entity.ifc_type.has_geometry(),
            is_spatial: entity.ifc_type.is_spatial(),
        })
    }

    pub fn entities_by_type(&self, type_name: &str) -> Vec<u32> {
        self.inner
            .resolver()
            .find_by_type_name(type_name)
            .into_iter()
            .map(|e| e.id.0)
            .collect()
    }

    pub fn count_by_type(&self, type_name: &str) -> u32 {
        let ifc_type = bimifc_model::IfcType::parse(type_name);
        self.inner.resolver().count_by_type(&ifc_type) as u32
    }

    pub fn entity_name(&self, id: u32) -> Option<String> {
        self.inner.properties().name(EntityId(id))
    }

    pub fn entity_global_id(&self, id: u32) -> Option<String> {
        self.inner.properties().global_id(EntityId(id))
    }

    pub fn entity_description(&self, id: u32) -> Option<String> {
        self.inner.properties().description(EntityId(id))
    }

    // ── Properties ───────────────────────────────────────────────────

    pub fn property_sets(&self, id: u32) -> Vec<PropertySet> {
        self.inner
            .properties()
            .property_sets(EntityId(id))
            .into_iter()
            .map(|ps| PropertySet {
                name: ps.name,
                properties: ps
                    .properties
                    .into_iter()
                    .map(|p| Property {
                        name: p.name,
                        value: p.value,
                        unit: p.unit,
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn quantities(&self, id: u32) -> Vec<Quantity> {
        self.inner
            .properties()
            .quantities(EntityId(id))
            .into_iter()
            .map(|q| {
                let qt = match q.quantity_type {
                    bimifc_model::QuantityType::Length => "Length",
                    bimifc_model::QuantityType::Area => "Area",
                    bimifc_model::QuantityType::Volume => "Volume",
                    bimifc_model::QuantityType::Count => "Count",
                    bimifc_model::QuantityType::Weight => "Weight",
                    bimifc_model::QuantityType::Time => "Time",
                };
                Quantity {
                    name: q.name,
                    value: q.value,
                    unit: q.unit,
                    quantity_type: qt.to_string(),
                }
            })
            .collect()
    }

    // ── Spatial structure ────────────────────────────────────────────

    pub fn spatial_tree(&self) -> Option<SpatialNode> {
        self.inner
            .spatial()
            .spatial_tree()
            .map(convert_spatial_node)
    }

    pub fn storeys(&self) -> Vec<StoreyInfo> {
        self.inner
            .spatial()
            .storeys()
            .into_iter()
            .map(|s| StoreyInfo {
                id: s.id.0,
                name: s.name,
                elevation: s.elevation,
                element_count: s.element_count as u32,
            })
            .collect()
    }

    pub fn elements_in_storey(&self, storey_id: u32) -> Vec<u32> {
        self.inner
            .spatial()
            .elements_in_storey(EntityId(storey_id))
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    pub fn search(&self, query: &str) -> Vec<u32> {
        self.inner
            .spatial()
            .search(query)
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    pub fn all_elements(&self) -> Vec<u32> {
        self.inner
            .spatial()
            .all_elements()
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    /// Get all entity IDs that have geometry — scans all entities regardless of spatial tree.
    /// This is the reliable way to get renderable entities (same approach as the web viewer).
    pub fn all_geometry_ids(&self) -> Vec<u32> {
        self.inner
            .resolver()
            .all_ids()
            .into_iter()
            .filter(|id| {
                self.inner
                    .resolver()
                    .get(*id)
                    .map(|e| e.ifc_type.has_geometry())
                    .unwrap_or(false)
            })
            .map(|id| id.0)
            .collect()
    }

    // ── Per-entity lookups ─────────────────────────────────────────

    /// Get the ObjectType for an entity (more specific than IFC type).
    pub fn entity_object_type(&self, id: u32) -> Option<String> {
        self.inner.properties().object_type(EntityId(id))
    }

    /// Get the Tag for an entity.
    pub fn entity_tag(&self, id: u32) -> Option<String> {
        self.inner.properties().tag(EntityId(id))
    }

    /// Get a single property by name across all property sets of an entity.
    pub fn get_property(&self, id: u32, name: &str) -> Option<Property> {
        self.inner
            .properties()
            .get_property(EntityId(id), name)
            .map(|p| Property {
                name: p.name,
                value: p.value,
                unit: p.unit,
            })
    }

    /// Get a single quantity by name for an entity.
    pub fn get_quantity(&self, id: u32, name: &str) -> Option<Quantity> {
        self.inner
            .properties()
            .get_quantity(EntityId(id), name)
            .map(|q| {
                let qt = match q.quantity_type {
                    bimifc_model::QuantityType::Length => "Length",
                    bimifc_model::QuantityType::Area => "Area",
                    bimifc_model::QuantityType::Volume => "Volume",
                    bimifc_model::QuantityType::Count => "Count",
                    bimifc_model::QuantityType::Weight => "Weight",
                    bimifc_model::QuantityType::Time => "Time",
                };
                Quantity {
                    name: q.name,
                    value: q.value,
                    unit: q.unit,
                    quantity_type: qt.to_string(),
                }
            })
    }

    /// Find which storey contains a given element.
    pub fn containing_storey(&self, element_id: u32) -> Option<u32> {
        self.inner
            .spatial()
            .containing_storey(EntityId(element_id))
            .map(|id| id.0)
    }

    /// Get storey name and elevation for an element (convenience).
    pub fn element_storey_info(&self, element_id: u32) -> Option<StoreyInfo> {
        let storey_id = self
            .inner
            .spatial()
            .containing_storey(EntityId(element_id))?;
        let entity = self.inner.resolver().get(storey_id)?;
        let name = self.inner.properties().name(storey_id).unwrap_or_default();
        let elevation = entity.get_float(9).unwrap_or(0.0) as f32;
        let elements = self.inner.spatial().elements_in_storey(storey_id);
        Some(StoreyInfo {
            id: storey_id.0,
            name,
            elevation,
            element_count: elements.len() as u32,
        })
    }

    /// Get the 4x4 world transform (column-major, 16 floats) for an entity.
    pub fn entity_transform(&self, id: u32) -> Option<Vec<f32>> {
        let entity = self.inner.resolver().get(EntityId(id))?;
        let placement_id = entity.get_ref(5)?; // ObjectPlacement
        let transform =
            bimifc_geometry::transform::resolve_placement(placement_id, self.inner.resolver())?;
        let scale = self.inner.unit_scale();
        // Return 16 floats column-major; apply unit_scale to translation
        let mut m = Vec::with_capacity(16);
        for col in 0..4 {
            for row in 0..4 {
                let mut v = transform[(row, col)];
                // Scale translation column
                if col == 3 && row < 3 {
                    v *= scale;
                }
                m.push(v as f32);
            }
        }
        Some(m)
    }

    /// Get world position [x, y, z] for an entity (unit-scaled).
    pub fn entity_position(&self, id: u32) -> Option<Vec<f32>> {
        let entity = self.inner.resolver().get(EntityId(id))?;
        let placement_id = entity.get_ref(5)?;
        let transform =
            bimifc_geometry::transform::resolve_placement(placement_id, self.inner.resolver())?;
        let scale = self.inner.unit_scale();
        Some(vec![
            (transform[(0, 3)] * scale) as f32,
            (transform[(1, 3)] * scale) as f32,
            (transform[(2, 3)] * scale) as f32,
        ])
    }

    // ── Colors ──────────────────────────────────────────────────────

    /// Get the IFC-authored surface color for an entity (from IfcStyledItem chain).
    pub fn entity_surface_color(&self, id: u32) -> Option<ColorRgba> {
        let resolver = self.inner.resolver();
        let entity = resolver.get(EntityId(id))?;
        let styled_colors = build_styled_item_colors(resolver);
        let rgba = get_entity_surface_color(&entity, resolver, &styled_colors)?;
        Some(ColorRgba {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            a: rgba[3],
        })
    }

    /// Get default color for an IFC type name (e.g. "IfcWall" → cream).
    pub fn default_color_for_type(&self, ifc_type: &str) -> ColorRgba {
        let c = get_default_color(ifc_type);
        ColorRgba {
            r: c[0],
            g: c[1],
            b: c[2],
            a: c[3],
        }
    }

    // ── Geometry ─────────────────────────────────────────────────────

    pub fn get_geometry(&self, id: u32) -> Option<MeshData> {
        let entity = self.inner.resolver().get(EntityId(id))?;
        let mut guard = self.geometry.lock().unwrap();
        if guard.is_none() {
            *guard = Some(GeometryRouter::with_default_processors_and_unit_scale(
                self.inner.unit_scale(),
            ));
        }
        let router = guard.as_ref().unwrap();
        let mesh = router
            .process_element(&entity, self.inner.resolver())
            .ok()?;
        let md = mesh.to_mesh_data();
        if md.is_empty() {
            return None;
        }
        Some(to_ffi_mesh(md))
    }

    pub fn batch_geometry(&self, ids: Vec<u32>) -> Vec<EntityMesh> {
        let resolver = self.inner.resolver();
        let mut guard = self.geometry.lock().unwrap();
        if guard.is_none() {
            *guard = Some(GeometryRouter::with_default_processors_and_unit_scale(
                self.inner.unit_scale(),
            ));
        }
        let router = guard.as_ref().unwrap();

        ids.into_iter()
            .filter_map(|id| {
                let entity = resolver.get(EntityId(id))?;
                let ifc_type = entity.ifc_type.name().to_string();
                let mesh = router.process_element(&entity, resolver).ok()?;
                let md = mesh.to_mesh_data();
                if md.is_empty() {
                    return None;
                }
                Some(EntityMesh {
                    entity_id: id,
                    ifc_type,
                    mesh: to_ffi_mesh(md),
                })
            })
            .collect()
    }

    /// Like `batch_geometry` but includes color (IFC-authored or type default)
    /// and entity transform. This is the full-featured method matching the web viewer.
    pub fn batch_colored_geometry(&self, ids: Vec<u32>) -> Vec<ColoredEntityMesh> {
        let resolver = self.inner.resolver();
        let scale = self.inner.unit_scale();
        let styled_colors = build_styled_item_colors(resolver);

        let mut guard = self.geometry.lock().unwrap();
        if guard.is_none() {
            *guard = Some(GeometryRouter::with_default_processors_and_unit_scale(
                scale,
            ));
        }
        let router = guard.as_ref().unwrap();

        ids.into_iter()
            .filter_map(|id| {
                let entity = resolver.get(EntityId(id))?;
                let ifc_type = entity.ifc_type.name().to_string();
                let name = self
                    .inner
                    .properties()
                    .name(EntityId(id))
                    .unwrap_or_default();
                let mesh = router.process_element(&entity, resolver).ok()?;
                let md = mesh.to_mesh_data();
                if md.is_empty() {
                    return None;
                }

                // Color: prefer IFC-authored, fall back to type default
                let color =
                    if let Some(c) = get_entity_surface_color(&entity, resolver, &styled_colors) {
                        ColorRgba {
                            r: c[0],
                            g: c[1],
                            b: c[2],
                            a: c[3],
                        }
                    } else {
                        let c = get_default_color(&ifc_type);
                        ColorRgba {
                            r: c[0],
                            g: c[1],
                            b: c[2],
                            a: c[3],
                        }
                    };

                // Transform
                let transform = entity.get_ref(5).and_then(|pid| {
                    let t = bimifc_geometry::transform::resolve_placement(pid, resolver)?;
                    let mut m = Vec::with_capacity(16);
                    for col in 0..4 {
                        for row in 0..4 {
                            let mut v = t[(row, col)];
                            if col == 3 && row < 3 {
                                v *= scale;
                            }
                            m.push(v as f32);
                        }
                    }
                    Some(m)
                });

                Some(ColoredEntityMesh {
                    entity_id: id,
                    ifc_type,
                    name,
                    mesh: to_ffi_mesh(md),
                    color,
                    transform,
                })
            })
            .collect()
    }

    // ── Statistics ───────────────────────────────────────────────────

    /// Get entity count per IFC type (useful for type breakdowns).
    pub fn type_statistics(&self) -> Vec<TypeCount> {
        let resolver = self.inner.resolver();
        let mut counts: HashMap<String, u32> = HashMap::new();
        for id in resolver.all_ids() {
            if let Some(entity) = resolver.get(id) {
                *counts
                    .entry(entity.ifc_type.name().to_string())
                    .or_default() += 1;
            }
        }
        let mut result: Vec<TypeCount> = counts
            .into_iter()
            .map(|(ifc_type, count)| TypeCount { ifc_type, count })
            .collect();
        result.sort_by(|a, b| b.count.cmp(&a.count));
        result
    }

    // ── Photometry ──────────────────────────────────────────────────

    /// Get goniometric (photometric) source data for an entity.
    pub fn goniometric_sources(&self, id: u32) -> Vec<GoniometricInfo> {
        self.inner
            .properties()
            .goniometric_sources(EntityId(id))
            .into_iter()
            .map(|g| GoniometricInfo {
                name: g.name,
                colour_temperature: g.colour_temperature,
                luminous_flux: g.luminous_flux,
                emitter_type: g.emitter_type,
                distribution_type: g.distribution_type,
            })
            .collect()
    }

    /// Convert goniometric data for an entity to EULUMDAT/LDT string.
    pub fn goniometric_to_ldt(&self, id: u32) -> Option<String> {
        let sources = self.inner.properties().goniometric_sources(EntityId(id));
        let src = sources.first()?;
        Some(bimifc_parser::goniometric_to_ldt(src))
    }

    // ── Lighting ─────────────────────────────────────────────────────

    /// Extract lighting data as a JSON string.
    pub fn lighting_json(&self) -> String {
        let export = bimifc_parser::extract_lighting_data(self.inner.resolver());
        bimifc_parser::export_to_json(&export)
    }

    /// Get light fixture info with positions and photometric data.
    pub fn light_fixtures(&self) -> Vec<LightFixtureInfo> {
        let export = bimifc_parser::extract_lighting_data(self.inner.resolver());
        export
            .light_fixtures
            .into_iter()
            .map(|f| {
                let (ct, flux) = if let Some(src) = f.light_sources.first() {
                    (src.color_temperature, src.luminous_flux)
                } else {
                    (None, None)
                };
                LightFixtureInfo {
                    entity_id: f.id as u32,
                    name: f.name.unwrap_or_default(),
                    position: vec![
                        f.position.0 as f32,
                        f.position.1 as f32,
                        f.position.2 as f32,
                    ],
                    color_temperature: ct,
                    luminous_flux: flux,
                }
            })
            .collect()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// IFC surface color extraction (same approach as bimifc-bevy loader)
// ═══════════════════════════════════════════════════════════════════════════

fn build_styled_item_colors(resolver: &dyn EntityResolver) -> HashMap<u32, [f32; 4]> {
    let mut color_map = HashMap::new();

    for styled_item in resolver.entities_by_type(&bimifc_model::IfcType::IfcStyledItem) {
        // [0] Item — the geometry entity this style applies to
        let item_id = match styled_item.get_ref(0) {
            Some(id) => id,
            None => continue,
        };

        // [1] Styles — list of style assignments
        let styles = match styled_item.get(1) {
            Some(AttributeValue::List(list)) => list,
            _ => continue,
        };

        // Follow first style ref → IfcSurfaceStyle
        let surface_style_id = match styles.first().and_then(|v| v.as_entity_ref()) {
            Some(id) => id,
            None => continue,
        };
        let surface_style = match resolver.get(surface_style_id) {
            Some(e) => e,
            None => continue,
        };

        // IfcSurfaceStyle[2] = Styles (list of rendering styles)
        let sub_styles = match surface_style.get(2) {
            Some(AttributeValue::List(list)) => list,
            _ => continue,
        };

        let rendering_id = match sub_styles.first().and_then(|v| v.as_entity_ref()) {
            Some(id) => id,
            None => continue,
        };
        let rendering = match resolver.get(rendering_id) {
            Some(e) => e,
            None => continue,
        };

        // IfcSurfaceStyleRendering[0] = SurfaceColour → IfcColourRgb
        let colour_id = match rendering.get_ref(0) {
            Some(id) => id,
            None => continue,
        };
        let colour = match resolver.get(colour_id) {
            Some(e) => e,
            None => continue,
        };

        let r = colour.get_float(1).unwrap_or(0.7) as f32;
        let g = colour.get_float(2).unwrap_or(0.7) as f32;
        let b = colour.get_float(3).unwrap_or(0.7) as f32;

        color_map.insert(item_id.0, [r, g, b, 1.0]);
    }

    color_map
}

fn get_entity_surface_color(
    entity: &bimifc_model::DecodedEntity,
    resolver: &dyn EntityResolver,
    styled_colors: &HashMap<u32, [f32; 4]>,
) -> Option<[f32; 4]> {
    let rep_id = entity.get_ref(6)?;
    let representation = resolver.get(rep_id)?;

    let reps = match representation.get(2) {
        Some(AttributeValue::List(list)) => list,
        _ => return None,
    };

    for rep_ref in reps {
        let shape_rep_id = rep_ref.as_entity_ref()?;
        let shape_rep = resolver.get(shape_rep_id)?;

        let items = match shape_rep.get(3) {
            Some(AttributeValue::List(list)) => list,
            _ => continue,
        };

        for item_ref in items {
            if let Some(item_id) = item_ref.as_entity_ref() {
                if let Some(color) = styled_colors.get(&item_id.0) {
                    return Some(*color);
                }
            }
        }
    }

    None
}

// ═══════════════════════════════════════════════════════════════════════════
// Default color palette (matches bimifc-bevy vibrant palette)
// ═══════════════════════════════════════════════════════════════════════════

fn get_default_color(entity_type: &str) -> [f32; 4] {
    let upper = entity_type.to_uppercase();

    if upper.contains("WALL") {
        [0.95, 0.90, 0.80, 1.0]
    } else if upper.contains("SLAB") {
        [0.85, 0.82, 0.78, 1.0]
    } else if upper.contains("ROOF") {
        [0.85, 0.45, 0.35, 1.0]
    } else if upper.contains("BEAM") || upper.contains("COLUMN") || upper.contains("MEMBER") {
        [0.45, 0.55, 0.75, 1.0]
    } else if upper.contains("DOOR") {
        [0.65, 0.40, 0.25, 1.0]
    } else if upper.contains("WINDOW") || upper.contains("CURTAINWALL") {
        [0.4, 0.7, 0.9, 0.4]
    } else if upper.contains("STAIR") || upper.contains("RAMP") {
        [0.75, 0.70, 0.65, 1.0]
    } else if upper.contains("RAILING") {
        [0.30, 0.30, 0.35, 1.0]
    } else if upper.contains("FURNITURE") || upper.contains("FURNISHING") {
        [0.70, 0.50, 0.30, 1.0]
    } else if upper.contains("SPACE") {
        [0.7, 0.85, 0.95, 0.15]
    } else if upper.contains("PLATE") {
        [0.70, 0.72, 0.78, 1.0]
    } else if upper.contains("COVERING") {
        [0.88, 0.85, 0.80, 1.0]
    } else if upper.contains("FOOTING") || upper.contains("PILE") {
        [0.60, 0.58, 0.55, 1.0]
    } else if upper.contains("PROXY") {
        [0.75, 0.60, 0.80, 1.0]
    } else if upper.contains("LIGHTFIXTURE") {
        [1.0, 0.9, 0.3, 1.0]
    } else if upper.contains("FLOW") || upper.contains("DUCT") || upper.contains("PIPE") {
        [0.40, 0.75, 0.50, 1.0]
    } else if upper.contains("ELECTRIC") || upper.contains("ENERGY") {
        [0.90, 0.80, 0.30, 1.0]
    } else if upper.contains("SANITARY") || upper.contains("FIRE") {
        [0.95, 0.95, 0.98, 1.0]
    } else if upper.contains("SHADING") {
        [0.40, 0.45, 0.55, 0.85]
    } else if upper.contains("TRANSPORT") {
        [0.45, 0.45, 0.50, 1.0]
    } else if upper.contains("GEOGRAPHIC") || upper.contains("VIRTUAL") {
        [0.65, 0.85, 0.65, 0.3]
    } else {
        [0.80, 0.78, 0.75, 1.0]
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Mesh conversion helper
// ═══════════════════════════════════════════════════════════════════════════

fn to_ffi_mesh(md: bimifc_model::MeshData) -> MeshData {
    let vc = md.vertex_count() as u32;
    let tc = md.triangle_count() as u32;
    MeshData {
        positions: md.positions,
        normals: md.normals,
        indices: md.indices,
        vertex_count: vc,
        triangle_count: tc,
    }
}

fn convert_spatial_node(node: &bimifc_model::SpatialNode) -> SpatialNode {
    SpatialNode {
        id: node.id.0,
        node_type: node.node_type.display_name().to_string(),
        name: node.name.clone(),
        entity_type: node.entity_type.clone(),
        elevation: node.elevation,
        has_geometry: node.has_geometry,
        children: node.children.iter().map(convert_spatial_node).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_bayarena_full_api() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../crates/bimifc-viewer/ifc/bayarena_lighting.ifc"
        );
        let model = IfcModel::from_file(path).unwrap();

        // Basic counts
        let total = model.entity_count();
        println!("Total entities: {total}");
        assert!(total > 0);

        println!("Schema: {}", model.schema_version());
        println!("Unit scale: {}", model.unit_scale());

        // Spatial
        let tree = model.spatial_tree();
        assert!(tree.is_some(), "Should have spatial tree");

        let storeys = model.storeys();
        println!("Storeys: {}", storeys.len());

        // Type statistics
        let stats = model.type_statistics();
        println!("Type statistics ({} types):", stats.len());
        for s in stats.iter().take(5) {
            println!("  {} = {}", s.ifc_type, s.count);
        }

        // Geometry IDs
        let geom_ids = model.all_geometry_ids();
        println!("Geometry IDs: {}", geom_ids.len());
        assert!(!geom_ids.is_empty());

        // Colored batch geometry (the full-featured method)
        let colored = model.batch_colored_geometry(geom_ids.clone());
        println!("Colored meshes: {}", colored.len());
        for m in colored.iter().take(3) {
            println!(
                "  #{} {} '{}' verts={} color=({:.2},{:.2},{:.2},{:.2}) transform={}",
                m.entity_id,
                m.ifc_type,
                m.name,
                m.mesh.vertex_count,
                m.color.r,
                m.color.g,
                m.color.b,
                m.color.a,
                if m.transform.is_some() { "yes" } else { "no" }
            );
        }

        // Light fixtures
        let lights = model.light_fixtures();
        println!("Light fixtures: {}", lights.len());

        // Per-entity: position, storey, properties
        if let Some(first_geom) = geom_ids.first() {
            let pos = model.entity_position(*first_geom);
            println!("Entity #{} position: {:?}", first_geom, pos);

            let storey = model.containing_storey(*first_geom);
            println!("Entity #{} storey: {:?}", first_geom, storey);

            let psets = model.property_sets(*first_geom);
            println!("Entity #{} property sets: {}", first_geom, psets.len());

            let color = model.entity_surface_color(*first_geom);
            println!(
                "Entity #{} surface color: {:?}",
                first_geom,
                color.map(|c| (c.r, c.g, c.b))
            );
        }

        // Default color
        let wall_color = model.default_color_for_type("IfcWall");
        assert!((wall_color.r - 0.95).abs() < 0.01);

        // Goniometric
        if let Some(light) = lights.first() {
            let gonio = model.goniometric_sources(light.entity_id);
            println!(
                "Light #{} goniometric sources: {}",
                light.entity_id,
                gonio.len()
            );

            let ldt = model.goniometric_to_ldt(light.entity_id);
            println!(
                "Light #{} LDT: {}",
                light.entity_id,
                if ldt.is_some() { "yes" } else { "no" }
            );
        }
    }
}
