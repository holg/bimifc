use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyKeyError;
use pyo3::prelude::*;

use bimifc_model::{EntityId, IfcModel, IfcType};
use bimifc_parser::{extract_lighting_data, StepParser};

use crate::error::ToPyResult;
use crate::geometry::GeometryContext;
use crate::lighting::PyLightingExport;
use crate::properties::{PyProperty, PyPropertySet, PyQuantity};
use crate::spatial::{PySpatialNode, PyStoreyInfo};
use crate::types::{Entity, PyMeshData, PyModelMetadata};

/// Central IFC model class — wraps a parsed IFC file
///
/// Provides access to entities, properties, spatial structure, geometry,
/// and lighting data through a single Pythonic interface.
#[pyclass(name = "IfcModel")]
pub struct PyIfcModel {
    model: Arc<dyn IfcModel>,
    geometry: Mutex<Option<GeometryContext>>,
}

impl PyIfcModel {
    fn with_geometry<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&GeometryContext) -> R,
    {
        let mut guard = self.geometry.lock().unwrap();
        if guard.is_none() {
            *guard = Some(GeometryContext::new(self.model.unit_scale()));
        }
        f(guard.as_ref().unwrap())
    }
}

#[pymethods]
impl PyIfcModel {
    // ── Construction ─────────────────────────────────────────────────

    /// Parse IFC content from a string
    #[staticmethod]
    pub fn parse(content: &str) -> PyResult<Self> {
        let model = bimifc_parser::parse(content).to_py()?;
        Ok(Self {
            model,
            geometry: Mutex::new(None),
        })
    }

    /// Auto-detect format (STEP or IFCX) and parse
    #[staticmethod]
    pub fn parse_auto(content: &str) -> PyResult<Self> {
        let model = bimifc_parser::parse_auto(content).to_py()?;
        Ok(Self {
            model,
            geometry: Mutex::new(None),
        })
    }

    /// Parse an IFC file from disk
    #[staticmethod]
    pub fn from_file(path: &str) -> PyResult<Self> {
        let content = std::fs::read_to_string(path).map_err(crate::error::io_to_py_err)?;
        let model = bimifc_parser::parse_auto(&content).to_py()?;
        Ok(Self {
            model,
            geometry: Mutex::new(None),
        })
    }

    /// Parse with explicit options
    #[staticmethod]
    #[pyo3(signature = (content, spatial_tree=true, properties=true))]
    pub fn parse_with_options(
        content: &str,
        spatial_tree: bool,
        properties: bool,
    ) -> PyResult<Self> {
        let parser = StepParser::new()
            .with_spatial_tree(spatial_tree)
            .with_properties(properties);
        let model = bimifc_model::IfcParser::parse(&parser, content).to_py()?;
        Ok(Self {
            model,
            geometry: Mutex::new(None),
        })
    }

    /// Parse with a progress callback fn(phase: str, progress: float)
    #[staticmethod]
    pub fn parse_with_progress(
        py: Python<'_>,
        content: &str,
        callback: Py<PyAny>,
    ) -> PyResult<Self> {
        let model = py
            .detach(|| {
                bimifc_parser::parse_with_progress(content, move |phase, progress| {
                    Python::attach(|py| {
                        let _ = callback.call1(py, (phase, progress));
                    });
                })
            })
            .to_py()?;
        Ok(Self {
            model,
            geometry: Mutex::new(None),
        })
    }

    // ── Metadata ─────────────────────────────────────────────────────

    /// Model metadata (schema, author, system, etc.)
    #[getter]
    fn metadata(&self) -> PyModelMetadata {
        PyModelMetadata::new(self.model.metadata().clone())
    }

    /// Unit scale factor (file units to meters)
    #[getter]
    fn unit_scale(&self) -> f64 {
        self.model.unit_scale()
    }

    /// IFC schema version string
    #[getter]
    fn schema_version(&self) -> String {
        self.model.metadata().schema_version.clone()
    }

    // ── Entities ─────────────────────────────────────────────────────

    /// Get entity by ID
    fn get(&self, id: u32) -> Option<Entity> {
        self.model.resolver().get(EntityId(id)).map(Entity::new)
    }

    /// Get all entities of a given type name (e.g. "IfcWall")
    fn entities_by_type(&self, name: &str) -> Vec<Entity> {
        self.model
            .resolver()
            .find_by_type_name(name)
            .into_iter()
            .map(Entity::new)
            .collect()
    }

    /// All entity IDs in the model
    fn all_ids(&self) -> Vec<u32> {
        self.model
            .resolver()
            .all_ids()
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    /// Total entity count
    #[getter]
    fn entity_count(&self) -> usize {
        self.model.resolver().entity_count()
    }

    /// Count entities of a given type name
    fn count_by_type(&self, name: &str) -> usize {
        let ifc_type = IfcType::parse(name);
        self.model.resolver().count_by_type(&ifc_type)
    }

    // ── Properties ───────────────────────────────────────────────────

    /// Get all property sets for an entity
    fn property_sets(&self, id: u32) -> Vec<PyPropertySet> {
        self.model
            .properties()
            .property_sets(EntityId(id))
            .into_iter()
            .map(PyPropertySet::new)
            .collect()
    }

    /// Get all quantities for an entity
    fn quantities(&self, id: u32) -> Vec<PyQuantity> {
        self.model
            .properties()
            .quantities(EntityId(id))
            .into_iter()
            .map(PyQuantity::new)
            .collect()
    }

    /// Get a specific property by name across all property sets
    fn get_property(&self, id: u32, name: &str) -> Option<PyProperty> {
        self.model
            .properties()
            .get_property(EntityId(id), name)
            .map(PyProperty::new)
    }

    /// Get entity GlobalId (IFC GUID)
    fn global_id(&self, id: u32) -> Option<String> {
        self.model.properties().global_id(EntityId(id))
    }

    /// Get entity name
    fn name(&self, id: u32) -> Option<String> {
        self.model.properties().name(EntityId(id))
    }

    /// Get entity description
    fn description(&self, id: u32) -> Option<String> {
        self.model.properties().description(EntityId(id))
    }

    // ── Spatial ──────────────────────────────────────────────────────

    /// Get the spatial hierarchy tree (root is typically IfcProject)
    fn spatial_tree(&self) -> Option<PySpatialNode> {
        self.model
            .spatial()
            .spatial_tree()
            .map(|n| PySpatialNode::new(n.clone()))
    }

    /// Get all building storeys
    fn storeys(&self) -> Vec<PyStoreyInfo> {
        self.model
            .spatial()
            .storeys()
            .into_iter()
            .map(PyStoreyInfo::new)
            .collect()
    }

    /// Get element IDs contained in a storey
    fn elements_in_storey(&self, storey_id: u32) -> Vec<u32> {
        self.model
            .spatial()
            .elements_in_storey(EntityId(storey_id))
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    /// Search entities by name or type (case-insensitive)
    fn search(&self, query: &str) -> Vec<u32> {
        self.model
            .spatial()
            .search(query)
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    /// Get all building element IDs
    fn all_elements(&self) -> Vec<u32> {
        self.model
            .spatial()
            .all_elements()
            .into_iter()
            .map(|id| id.0)
            .collect()
    }

    // ── Geometry ─────────────────────────────────────────────────────

    /// Get mesh geometry for a single entity (returns None if no geometry)
    fn get_geometry(&self, id: u32) -> Option<PyMeshData> {
        let entity = self.model.resolver().get(EntityId(id))?;
        self.with_geometry(|ctx| {
            ctx.process_element(&entity, self.model.resolver())
                .map(PyMeshData::new)
        })
    }

    /// Batch process geometry for multiple entities
    ///
    /// Returns list of (id, MeshData) tuples for entities that have geometry.
    fn batch_geometry(&self, ids: Vec<u32>) -> Vec<(u32, PyMeshData)> {
        let resolver = self.model.resolver();
        self.with_geometry(|ctx| {
            ids.into_iter()
                .filter_map(|id| {
                    let entity = resolver.get(EntityId(id))?;
                    let mesh = ctx.process_element(&entity, resolver)?;
                    Some((id, PyMeshData::new(mesh)))
                })
                .collect()
        })
    }

    // ── Lighting ─────────────────────────────────────────────────────

    /// Extract lighting data (fixtures, sources, photometry)
    fn extract_lighting(&self) -> PyLightingExport {
        let export = extract_lighting_data(self.model.resolver());
        PyLightingExport::new(export)
    }

    /// Extract lighting data as JSON string
    fn lighting_json(&self) -> String {
        let export = extract_lighting_data(self.model.resolver());
        bimifc_parser::export_to_json(&export)
    }

    // ── Dunder methods ───────────────────────────────────────────────

    fn __repr__(&self) -> String {
        let meta = self.model.metadata();
        format!(
            "IfcModel(schema='{}', entities={}, unit_scale={})",
            meta.schema_version,
            self.model.resolver().entity_count(),
            self.model.unit_scale()
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    fn __len__(&self) -> usize {
        self.model.resolver().entity_count()
    }

    fn __getitem__(&self, id: u32) -> PyResult<Entity> {
        self.model
            .resolver()
            .get(EntityId(id))
            .map(Entity::new)
            .ok_or_else(|| PyKeyError::new_err(format!("Entity #{} not found", id)))
    }

    fn __contains__(&self, id: u32) -> bool {
        self.model.resolver().get(EntityId(id)).is_some()
    }
}
