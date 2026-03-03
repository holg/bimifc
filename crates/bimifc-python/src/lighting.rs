use bimifc_parser::LightingExport;
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Extracted lighting data from an IFC model
#[pyclass(name = "LightingExport", skip_from_py_object)]
#[derive(Clone)]
pub struct PyLightingExport {
    inner: LightingExport,
}

impl PyLightingExport {
    pub fn new(inner: LightingExport) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyLightingExport {
    /// IFC schema version
    #[getter]
    fn schema(&self) -> &str {
        &self.inner.schema
    }

    /// Project name
    #[getter]
    fn project_name(&self) -> Option<&str> {
        self.inner.project_name.as_deref()
    }

    /// Total number of light fixtures
    #[getter]
    fn total_fixtures(&self) -> usize {
        self.inner.summary.total_fixtures
    }

    /// Total number of light sources
    #[getter]
    fn total_light_sources(&self) -> usize {
        self.inner.summary.total_light_sources
    }

    /// Total luminous flux (lumens), if available
    #[getter]
    fn total_luminous_flux(&self) -> Option<f64> {
        self.inner.summary.total_luminous_flux
    }

    /// Fixture type names used
    #[getter]
    fn fixture_types_used(&self) -> Vec<String> {
        self.inner.summary.fixture_types_used.clone()
    }

    /// Export as JSON string
    fn to_json(&self) -> String {
        bimifc_parser::export_to_json(&self.inner)
    }

    /// Light fixtures as list of dicts
    fn fixtures<'py>(&self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
        let mut result = Vec::new();
        for fixture in &self.inner.light_fixtures {
            let dict = PyDict::new(py);
            dict.set_item("id", fixture.id)?;
            dict.set_item("global_id", fixture.global_id.as_deref())?;
            dict.set_item("name", fixture.name.as_deref())?;
            dict.set_item("description", fixture.description.as_deref())?;
            dict.set_item("object_type", fixture.object_type.as_deref())?;
            dict.set_item(
                "position",
                (fixture.position.0, fixture.position.1, fixture.position.2),
            )?;
            dict.set_item("storey", fixture.storey.as_deref())?;
            dict.set_item("storey_elevation", fixture.storey_elevation)?;
            dict.set_item("light_source_count", fixture.light_sources.len())?;

            // Fixture type info
            if let Some(ref ft) = fixture.fixture_type {
                let ft_dict = PyDict::new(py);
                ft_dict.set_item("id", ft.id)?;
                ft_dict.set_item("name", ft.name.as_deref())?;
                ft_dict.set_item("description", ft.description.as_deref())?;
                ft_dict.set_item("predefined_type", ft.predefined_type.as_deref())?;
                dict.set_item("fixture_type", ft_dict)?;
            } else {
                dict.set_item("fixture_type", py.None())?;
            }

            // Light sources
            let mut sources = Vec::new();
            for src in &fixture.light_sources {
                let src_dict = PyDict::new(py);
                src_dict.set_item("id", src.id)?;
                src_dict.set_item("source_type", &src.source_type)?;
                src_dict.set_item("color_temperature", src.color_temperature)?;
                src_dict.set_item("luminous_flux", src.luminous_flux)?;
                src_dict.set_item("emission_source", src.emission_source.as_deref())?;
                src_dict.set_item("intensity", src.intensity)?;
                if let Some((r, g, b)) = src.color_rgb {
                    src_dict.set_item("color_rgb", (r, g, b))?;
                } else {
                    src_dict.set_item("color_rgb", py.None())?;
                }
                sources.push(src_dict);
            }
            dict.set_item("light_sources", sources)?;

            result.push(dict);
        }
        Ok(result)
    }

    fn __repr__(&self) -> String {
        format!(
            "LightingExport(fixtures={}, sources={}, flux={:?})",
            self.inner.summary.total_fixtures,
            self.inner.summary.total_light_sources,
            self.inner.summary.total_luminous_flux
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}
