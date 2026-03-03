pub mod error;
pub mod geometry;
pub mod lighting;
pub mod model;
pub mod properties;
pub mod spatial;
pub mod types;

use pyo3::prelude::*;

use lighting::PyLightingExport;
use model::PyIfcModel;
use properties::{PyProperty, PyPropertySet, PyQuantity};
use spatial::{PySpatialNode, PyStoreyInfo};
use types::{Entity, PyMeshData, PyModelMetadata};

/// Convenience: parse an IFC file from disk
#[pyfunction]
fn from_file(path: &str) -> PyResult<PyIfcModel> {
    PyIfcModel::from_file(path)
}

/// Convenience: parse IFC content from a string
#[pyfunction]
fn parse(content: &str) -> PyResult<PyIfcModel> {
    PyIfcModel::parse(content)
}

/// Convenience: auto-detect format and parse
#[pyfunction]
fn parse_auto(content: &str) -> PyResult<PyIfcModel> {
    PyIfcModel::parse_auto(content)
}

/// Python bindings for the bimifc IFC parser.
#[pymodule]
fn bimifc(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Central model class
    m.add_class::<PyIfcModel>()?;

    // Entity and data types
    m.add_class::<Entity>()?;
    m.add_class::<PyMeshData>()?;
    m.add_class::<PyModelMetadata>()?;

    // Spatial types
    m.add_class::<PySpatialNode>()?;
    m.add_class::<PyStoreyInfo>()?;

    // Property types
    m.add_class::<PyProperty>()?;
    m.add_class::<PyPropertySet>()?;
    m.add_class::<PyQuantity>()?;

    // Lighting
    m.add_class::<PyLightingExport>()?;

    // Convenience functions
    m.add_function(wrap_pyfunction!(from_file, m)?)?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(parse_auto, m)?)?;

    Ok(())
}
