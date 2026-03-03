use bimifc_model::{SpatialNode, StoreyInfo};
use pyo3::prelude::*;

/// A node in the IFC spatial hierarchy tree
#[pyclass(name = "SpatialNode", skip_from_py_object)]
#[derive(Clone)]
pub struct PySpatialNode {
    inner: SpatialNode,
}

impl PySpatialNode {
    pub fn new(inner: SpatialNode) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PySpatialNode {
    /// Entity ID
    #[getter]
    fn id(&self) -> u32 {
        self.inner.id.0
    }

    /// Node type (Project, Site, Building, Storey, Space, Element, Facility, FacilityPart)
    #[getter]
    fn node_type(&self) -> &str {
        self.inner.node_type.display_name()
    }

    /// Display name
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// IFC entity type name (e.g. "IfcWall")
    #[getter]
    fn entity_type(&self) -> &str {
        &self.inner.entity_type
    }

    /// Elevation in meters (for storeys)
    #[getter]
    fn elevation(&self) -> Option<f32> {
        self.inner.elevation
    }

    /// Whether this entity has geometry
    #[getter]
    fn has_geometry(&self) -> bool {
        self.inner.has_geometry
    }

    /// Child nodes
    #[getter]
    fn children(&self) -> Vec<PySpatialNode> {
        self.inner
            .children
            .iter()
            .map(|c| PySpatialNode::new(c.clone()))
            .collect()
    }

    /// Total element count (recursive)
    fn element_count(&self) -> usize {
        self.inner.element_count()
    }

    /// All element IDs in this subtree
    fn element_ids(&self) -> Vec<u32> {
        self.inner.element_ids().into_iter().map(|id| id.0).collect()
    }

    /// Find a node by entity ID (recursive)
    fn find(&self, id: u32) -> Option<PySpatialNode> {
        self.inner
            .find(bimifc_model::EntityId(id))
            .map(|n| PySpatialNode::new(n.clone()))
    }

    fn __repr__(&self) -> String {
        format!(
            "SpatialNode(#{}, type='{}', name='{}', children={})",
            self.inner.id.0,
            self.inner.node_type.display_name(),
            self.inner.name,
            self.inner.children.len()
        )
    }

    fn __str__(&self) -> String {
        format!(
            "{} '{}' ({})",
            self.inner.node_type.display_name(),
            self.inner.name,
            self.inner.entity_type
        )
    }
}

/// Building storey information
#[pyclass(name = "StoreyInfo", skip_from_py_object)]
#[derive(Clone)]
pub struct PyStoreyInfo {
    inner: StoreyInfo,
}

impl PyStoreyInfo {
    pub fn new(inner: StoreyInfo) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyStoreyInfo {
    /// Entity ID
    #[getter]
    fn id(&self) -> u32 {
        self.inner.id.0
    }

    /// Storey name
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// Elevation in meters
    #[getter]
    fn elevation(&self) -> f32 {
        self.inner.elevation
    }

    /// Number of elements in this storey
    #[getter]
    fn element_count(&self) -> usize {
        self.inner.element_count
    }

    fn __repr__(&self) -> String {
        format!(
            "StoreyInfo(#{}, name='{}', elevation={:.2}, elements={})",
            self.inner.id.0, self.inner.name, self.inner.elevation, self.inner.element_count
        )
    }

    fn __str__(&self) -> String {
        format!(
            "'{}' at {:.2}m ({} elements)",
            self.inner.name, self.inner.elevation, self.inner.element_count
        )
    }
}
