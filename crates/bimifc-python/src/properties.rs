use bimifc_model::{Property, PropertySet, Quantity, QuantityType};
use pyo3::prelude::*;

/// A single IFC property (name/value/unit)
#[pyclass(name = "Property", skip_from_py_object)]
#[derive(Clone)]
pub struct PyProperty {
    inner: Property,
}

impl PyProperty {
    pub fn new(inner: Property) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyProperty {
    /// Property name
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// Property value as string
    #[getter]
    fn value(&self) -> &str {
        &self.inner.value
    }

    /// Unit of measurement (if applicable)
    #[getter]
    fn unit(&self) -> Option<&str> {
        self.inner.unit.as_deref()
    }

    fn __repr__(&self) -> String {
        match &self.inner.unit {
            Some(u) => format!(
                "Property('{}', '{}', unit='{}')",
                self.inner.name, self.inner.value, u
            ),
            None => format!("Property('{}', '{}')", self.inner.name, self.inner.value),
        }
    }

    fn __str__(&self) -> String {
        match &self.inner.unit {
            Some(u) => format!("{}: {} {}", self.inner.name, self.inner.value, u),
            None => format!("{}: {}", self.inner.name, self.inner.value),
        }
    }
}

/// A property set containing multiple properties
#[pyclass(name = "PropertySet", skip_from_py_object)]
#[derive(Clone)]
pub struct PyPropertySet {
    inner: PropertySet,
}

impl PyPropertySet {
    pub fn new(inner: PropertySet) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyPropertySet {
    /// Property set name (e.g. "Pset_WallCommon")
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// All properties in this set
    #[getter]
    fn properties(&self) -> Vec<PyProperty> {
        self.inner
            .properties
            .iter()
            .map(|p| PyProperty::new(p.clone()))
            .collect()
    }

    /// Get a property by name
    fn get(&self, name: &str) -> Option<PyProperty> {
        self.inner.get(name).map(|p| PyProperty::new(p.clone()))
    }

    /// Number of properties in this set
    fn __len__(&self) -> usize {
        self.inner.properties.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "PropertySet('{}', {} properties)",
            self.inner.name,
            self.inner.properties.len()
        )
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }
}

/// A quantity value with type and unit
#[pyclass(name = "Quantity", skip_from_py_object)]
#[derive(Clone)]
pub struct PyQuantity {
    inner: Quantity,
}

impl PyQuantity {
    pub fn new(inner: Quantity) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyQuantity {
    /// Quantity name
    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    /// Numeric value
    #[getter]
    fn value(&self) -> f64 {
        self.inner.value
    }

    /// Unit of measurement
    #[getter]
    fn unit(&self) -> &str {
        &self.inner.unit
    }

    /// Quantity type (Length, Area, Volume, Count, Weight, Time)
    #[getter]
    fn quantity_type(&self) -> &str {
        match self.inner.quantity_type {
            QuantityType::Length => "Length",
            QuantityType::Area => "Area",
            QuantityType::Volume => "Volume",
            QuantityType::Count => "Count",
            QuantityType::Weight => "Weight",
            QuantityType::Time => "Time",
        }
    }

    /// Formatted value with unit (e.g. "12.5 m")
    fn formatted(&self) -> String {
        self.inner.formatted()
    }

    fn __repr__(&self) -> String {
        format!(
            "Quantity('{}', {}, unit='{}', type='{}')",
            self.inner.name,
            self.inner.value,
            self.inner.unit,
            self.quantity_type()
        )
    }

    fn __str__(&self) -> String {
        format!("{}: {}", self.inner.name, self.inner.formatted())
    }
}
