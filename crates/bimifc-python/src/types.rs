use bimifc_model::{AttributeValue, DecodedEntity, MeshData, ModelMetadata};
use pyo3::prelude::*;
use pyo3::types::PyList;
use std::sync::Arc;

/// Convert an AttributeValue to a native Python object
pub fn attribute_to_python<'py>(py: Python<'py>, attr: &AttributeValue) -> Bound<'py, PyAny> {
    match attr {
        AttributeValue::Null | AttributeValue::Derived => py.None().into_bound(py),
        AttributeValue::EntityRef(id) => id.0.into_pyobject(py).unwrap().into_any(),
        AttributeValue::Bool(b) => (*b).into_pyobject(py).unwrap().to_owned().into_any(),
        AttributeValue::Integer(i) => i.into_pyobject(py).unwrap().into_any(),
        AttributeValue::Float(f) => f.into_pyobject(py).unwrap().into_any(),
        AttributeValue::String(s) => s.as_str().into_pyobject(py).unwrap().into_any(),
        AttributeValue::Enum(s) => s.as_str().into_pyobject(py).unwrap().into_any(),
        AttributeValue::List(items) => {
            let py_items: Vec<Bound<'py, PyAny>> =
                items.iter().map(|v| attribute_to_python(py, v)).collect();
            PyList::new(py, &py_items).unwrap().into_any()
        }
        AttributeValue::TypedValue(type_name, args) => {
            let py_args: Vec<Bound<'py, PyAny>> =
                args.iter().map(|v| attribute_to_python(py, v)).collect();
            let list = PyList::new(py, &py_args).unwrap();
            (type_name.as_str(), list)
                .into_pyobject(py)
                .unwrap()
                .into_any()
        }
    }
}

/// Wrapper around a decoded IFC entity
#[pyclass(name = "Entity")]
pub struct Entity {
    inner: Arc<DecodedEntity>,
}

impl Entity {
    pub fn new(inner: Arc<DecodedEntity>) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl Entity {
    /// Entity ID number
    #[getter]
    fn id(&self) -> u32 {
        self.inner.id.0
    }

    /// IFC type name (e.g. "IFCWALL")
    #[getter]
    fn ifc_type(&self) -> &str {
        self.inner.ifc_type.name()
    }

    /// Whether this entity type typically has geometry
    #[getter]
    fn has_geometry(&self) -> bool {
        self.inner.ifc_type.has_geometry()
    }

    /// Whether this entity is a spatial structure element
    #[getter]
    fn is_spatial(&self) -> bool {
        self.inner.ifc_type.is_spatial()
    }

    /// Number of attributes
    fn __len__(&self) -> usize {
        self.inner.attributes.len()
    }

    /// Get attribute at index as a native Python value
    fn get<'py>(&self, py: Python<'py>, index: usize) -> Option<Bound<'py, PyAny>> {
        self.inner
            .get(index)
            .map(|attr| attribute_to_python(py, attr))
    }

    /// Get entity reference at index (returns int or None)
    fn get_ref(&self, index: usize) -> Option<u32> {
        self.inner.get_ref(index).map(|id| id.0)
    }

    /// Get string value at index
    fn get_string(&self, index: usize) -> Option<String> {
        self.inner.get_string(index).map(|s| s.to_string())
    }

    /// Get float value at index
    fn get_float(&self, index: usize) -> Option<f64> {
        self.inner.get_float(index)
    }

    /// Get integer value at index
    fn get_integer(&self, index: usize) -> Option<i64> {
        self.inner.get_integer(index)
    }

    /// Get boolean value at index
    fn get_bool(&self, index: usize) -> Option<bool> {
        self.inner.get_bool(index)
    }

    /// Get enum string at index
    fn get_enum(&self, index: usize) -> Option<String> {
        self.inner.get_enum(index).map(|s| s.to_string())
    }

    /// Get all attributes as a list of native Python values
    fn attributes<'py>(&self, py: Python<'py>) -> Vec<Bound<'py, PyAny>> {
        self.inner
            .attributes
            .iter()
            .map(|attr| attribute_to_python(py, attr))
            .collect()
    }

    fn __repr__(&self) -> String {
        format!(
            "Entity(#{}, type='{}')",
            self.inner.id.0,
            self.inner.ifc_type.name()
        )
    }

    fn __str__(&self) -> String {
        format!("#{} {}", self.inner.id.0, self.inner.ifc_type.name())
    }
}

/// GPU-ready mesh data (positions, normals, indices)
#[pyclass(name = "MeshData", skip_from_py_object)]
#[derive(Clone)]
pub struct PyMeshData {
    inner: MeshData,
}

impl PyMeshData {
    pub fn new(inner: MeshData) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyMeshData {
    /// Flattened vertex positions [x, y, z, x, y, z, ...]
    #[getter]
    fn positions(&self) -> Vec<f32> {
        self.inner.positions.clone()
    }

    /// Flattened vertex normals [nx, ny, nz, nx, ny, nz, ...]
    #[getter]
    fn normals(&self) -> Vec<f32> {
        self.inner.normals.clone()
    }

    /// Triangle indices
    #[getter]
    fn indices(&self) -> Vec<u32> {
        self.inner.indices.clone()
    }

    /// Number of vertices
    #[getter]
    fn vertex_count(&self) -> usize {
        self.inner.vertex_count()
    }

    /// Number of triangles
    #[getter]
    fn triangle_count(&self) -> usize {
        self.inner.triangle_count()
    }

    /// Whether the mesh is empty
    #[getter]
    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn __repr__(&self) -> String {
        format!(
            "MeshData(vertices={}, triangles={})",
            self.inner.vertex_count(),
            self.inner.triangle_count()
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

/// Model metadata from the IFC file header
#[pyclass(name = "ModelMetadata", skip_from_py_object)]
#[derive(Clone)]
pub struct PyModelMetadata {
    inner: ModelMetadata,
}

impl PyModelMetadata {
    pub fn new(inner: ModelMetadata) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyModelMetadata {
    /// IFC schema version (e.g. "IFC2X3", "IFC4", "IFC4X3")
    #[getter]
    fn schema_version(&self) -> &str {
        &self.inner.schema_version
    }

    /// Originating CAD application
    #[getter]
    fn originating_system(&self) -> Option<&str> {
        self.inner.originating_system.as_deref()
    }

    /// Preprocessor version
    #[getter]
    fn preprocessor_version(&self) -> Option<&str> {
        self.inner.preprocessor_version.as_deref()
    }

    /// File name from header
    #[getter]
    fn file_name(&self) -> Option<&str> {
        self.inner.file_name.as_deref()
    }

    /// File description
    #[getter]
    fn file_description(&self) -> Option<&str> {
        self.inner.file_description.as_deref()
    }

    /// Author
    #[getter]
    fn author(&self) -> Option<&str> {
        self.inner.author.as_deref()
    }

    /// Organization
    #[getter]
    fn organization(&self) -> Option<&str> {
        self.inner.organization.as_deref()
    }

    /// Timestamp
    #[getter]
    fn timestamp(&self) -> Option<&str> {
        self.inner.timestamp.as_deref()
    }

    fn __repr__(&self) -> String {
        format!(
            "ModelMetadata(schema='{}', system={:?})",
            self.inner.schema_version,
            self.inner.originating_system.as_deref().unwrap_or("?")
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}
