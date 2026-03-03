use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::PyErr;

/// Convert bimifc_model::ParseError to PyErr
pub fn parse_to_py_err(err: bimifc_model::ParseError) -> PyErr {
    PyValueError::new_err(err.to_string())
}

/// Convert bimifc_geometry::Error to PyErr
pub fn geometry_to_py_err(err: bimifc_geometry::Error) -> PyErr {
    PyRuntimeError::new_err(err.to_string())
}

/// Convert std::io::Error to PyErr
pub fn io_to_py_err(err: std::io::Error) -> PyErr {
    PyIOError::new_err(err.to_string())
}

/// Helper trait for converting Results to Python-compatible Results
pub trait ToPyResult<T> {
    fn to_py(self) -> pyo3::PyResult<T>;
}

impl<T> ToPyResult<T> for Result<T, bimifc_model::ParseError> {
    fn to_py(self) -> pyo3::PyResult<T> {
        self.map_err(parse_to_py_err)
    }
}

impl<T> ToPyResult<T> for Result<T, bimifc_geometry::Error> {
    fn to_py(self) -> pyo3::PyResult<T> {
        self.map_err(geometry_to_py_err)
    }
}

impl<T> ToPyResult<T> for Result<T, std::io::Error> {
    fn to_py(self) -> pyo3::PyResult<T> {
        self.map_err(io_to_py_err)
    }
}
