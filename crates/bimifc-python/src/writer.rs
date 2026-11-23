// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! IFC export bindings — `IfcWriter` plus the typed value / property /
//! room-element classes used to assemble export input.
//!
//! Mirrors the FFI (`bimifc-ffi`) writer surface so Python and Swift stay
//! symmetric: build `RoomElement`s (optionally carrying property sets with
//! typed `IfcValue`s) and call `IfcWriter.write_ifc(...)`, or use the
//! low-level `IfcWriter.write_property_set(...)` for a standalone fragment.

use crate::error::io_to_py_err;
use bimifc_writer as bw;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// A typed IFC property value (label, measure, boolean, …).
///
/// Construct via the classmethods, which name the IFC defined type the value
/// serialises to (so units show up correctly in Revit/Solibri/etc.):
///
/// ```python
/// IfcValue.power_measure(18.5)      # IFCPOWERMEASURE
/// IfcValue.label("F90")             # IFCLABEL
/// IfcValue.boolean(True)            # IFCBOOLEAN
/// ```
#[pyclass(name = "IfcValue", from_py_object)]
#[derive(Clone)]
pub struct PyIfcValue {
    inner: bw::IfcValue,
}

impl PyIfcValue {
    fn wrap(inner: bw::IfcValue) -> Self {
        Self { inner }
    }
    pub fn into_inner(self) -> bw::IfcValue {
        self.inner
    }
}

#[pymethods]
impl PyIfcValue {
    /// IFCLABEL — short string (≤255 chars).
    #[staticmethod]
    fn label(value: String) -> Self {
        Self::wrap(bw::IfcValue::Label(value))
    }
    /// IFCTEXT — longer prose.
    #[staticmethod]
    fn text(value: String) -> Self {
        Self::wrap(bw::IfcValue::Text(value))
    }
    /// IFCIDENTIFIER — machine-oriented id string.
    #[staticmethod]
    fn identifier(value: String) -> Self {
        Self::wrap(bw::IfcValue::Identifier(value))
    }
    /// IFCINTEGER
    #[staticmethod]
    fn integer(value: i64) -> Self {
        Self::wrap(bw::IfcValue::Integer(value))
    }
    /// IFCREAL
    #[staticmethod]
    fn real(value: f64) -> Self {
        Self::wrap(bw::IfcValue::Real(value))
    }
    /// IFCBOOLEAN
    #[staticmethod]
    fn boolean(value: bool) -> Self {
        Self::wrap(bw::IfcValue::Boolean(value))
    }
    /// IFCPOWERMEASURE — watts.
    #[staticmethod]
    fn power_measure(value: f64) -> Self {
        Self::wrap(bw::IfcValue::PowerMeasure(value))
    }
    /// IFCELECTRICVOLTAGEMEASURE — volts.
    #[staticmethod]
    fn electric_voltage_measure(value: f64) -> Self {
        Self::wrap(bw::IfcValue::ElectricVoltageMeasure(value))
    }
    /// IFCELECTRICCURRENTMEASURE — amperes.
    #[staticmethod]
    fn electric_current_measure(value: f64) -> Self {
        Self::wrap(bw::IfcValue::ElectricCurrentMeasure(value))
    }
    /// IFCFREQUENCYMEASURE — hertz.
    #[staticmethod]
    fn frequency_measure(value: f64) -> Self {
        Self::wrap(bw::IfcValue::FrequencyMeasure(value))
    }
    /// IFCMASSMEASURE — kilograms.
    #[staticmethod]
    fn mass_measure(value: f64) -> Self {
        Self::wrap(bw::IfcValue::MassMeasure(value))
    }
    /// IFCPOSITIVELENGTHMEASURE — meters.
    #[staticmethod]
    fn positive_length_measure(value: f64) -> Self {
        Self::wrap(bw::IfcValue::PositiveLengthMeasure(value))
    }
    /// IFCTHERMODYNAMICTEMPERATUREMEASURE — kelvin.
    #[staticmethod]
    fn thermodynamic_temperature_measure(value: f64) -> Self {
        Self::wrap(bw::IfcValue::ThermodynamicTemperatureMeasure(value))
    }
    /// IFCLUMINOUSFLUXMEASURE — lumens.
    #[staticmethod]
    fn luminous_flux_measure(value: f64) -> Self {
        Self::wrap(bw::IfcValue::LuminousFluxMeasure(value))
    }
    /// IFCPLANEANGLEMEASURE — radians.
    #[staticmethod]
    fn plane_angle_measure(value: f64) -> Self {
        Self::wrap(bw::IfcValue::PlaneAngleMeasure(value))
    }

    fn __repr__(&self) -> String {
        format!("IfcValue({:?})", self.inner)
    }
}

/// One named property inside a property set.
#[pyclass(name = "IfcProperty", from_py_object)]
#[derive(Clone)]
pub struct PyIfcProperty {
    inner: bw::IfcProperty,
}

impl PyIfcProperty {
    pub fn into_inner(self) -> bw::IfcProperty {
        self.inner
    }
}

#[pymethods]
impl PyIfcProperty {
    /// `IfcProperty(name, value, description=None)`. The optional description
    /// conventionally carries a bSDD GlobalId as `"bSDD:<id>"`.
    #[new]
    #[pyo3(signature = (name, value, description=None))]
    fn new(name: String, value: PyIfcValue, description: Option<String>) -> Self {
        Self {
            inner: bw::IfcProperty {
                name,
                description,
                value: value.into_inner(),
            },
        }
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    fn __repr__(&self) -> String {
        format!("IfcProperty('{}')", self.inner.name)
    }
}

/// A property set attached to an element on export.
#[pyclass(name = "ElementPset", from_py_object)]
#[derive(Clone)]
pub struct PyElementPset {
    inner: bw::ElementPset,
}

impl PyElementPset {
    fn into_inner(self) -> bw::ElementPset {
        self.inner
    }
}

#[pymethods]
impl PyElementPset {
    /// `ElementPset(name, properties)`.
    #[new]
    fn new(name: String, properties: Vec<PyIfcProperty>) -> Self {
        Self {
            inner: bw::ElementPset {
                name,
                properties: properties.into_iter().map(|p| p.into_inner()).collect(),
            },
        }
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    fn __repr__(&self) -> String {
        format!(
            "ElementPset('{}', {} properties)",
            self.inner.name,
            self.inner.properties.len()
        )
    }
}

/// 2D rectangle (position + size), meters.
#[pyclass(name = "Rect", from_py_object)]
#[derive(Clone)]
pub struct PyRect {
    inner: bw::Rect,
}

#[pymethods]
impl PyRect {
    #[new]
    fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            inner: bw::Rect {
                x,
                y,
                width,
                height,
            },
        }
    }
}

/// Room dimensions in meters.
#[pyclass(name = "RoomDimensions", from_py_object)]
#[derive(Clone)]
pub struct PyRoomDimensions {
    inner: bw::RoomDimensions,
}

#[pymethods]
impl PyRoomDimensions {
    #[new]
    fn new(width: f64, height: f64, depth: f64) -> Self {
        Self {
            inner: bw::RoomDimensions {
                width,
                height,
                depth,
            },
        }
    }
}

/// Project metadata for the IFC header.
#[pyclass(name = "ProjectInfo", from_py_object)]
#[derive(Clone)]
pub struct PyProjectInfo {
    inner: bw::ProjectInfo,
}

#[pymethods]
impl PyProjectInfo {
    #[new]
    #[pyo3(signature = (
        project_name="RoomPlan Export".to_string(),
        site_name="Default Site".to_string(),
        building_name="Building".to_string(),
        storey_name="Ground Floor".to_string(),
        author="bimifc".to_string(),
        organization="bimifc".to_string(),
        application_name="bimifc-python".to_string(),
        application_version=env!("CARGO_PKG_VERSION").to_string(),
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        project_name: String,
        site_name: String,
        building_name: String,
        storey_name: String,
        author: String,
        organization: String,
        application_name: String,
        application_version: String,
    ) -> Self {
        Self {
            inner: bw::ProjectInfo {
                project_name,
                site_name,
                building_name,
                storey_name,
                author,
                organization,
                application_name,
                application_version,
                ..Default::default()
            },
        }
    }
}

/// A single room element (wall, door, furniture, …) for export.
#[pyclass(name = "RoomElement", from_py_object)]
#[derive(Clone)]
pub struct PyRoomElement {
    inner: bw::RoomElement,
}

impl PyRoomElement {
    fn into_inner(self) -> bw::RoomElement {
        self.inner
    }
}

/// Map a kind string to the writer's `ElementKind`. `furniture` requires a
/// category; everything else ignores it.
fn parse_kind(kind: &str, category: Option<String>) -> PyResult<bw::ElementKind> {
    Ok(match kind.to_ascii_lowercase().as_str() {
        "wall" => bw::ElementKind::Wall,
        "door" => bw::ElementKind::Door,
        "window" => bw::ElementKind::Window,
        "opening" => bw::ElementKind::Opening,
        "slab" => bw::ElementKind::Slab,
        "roofgable" | "roof_gable" => bw::ElementKind::RoofGable,
        "roofhip" | "roof_hip" => bw::ElementKind::RoofHip,
        "roofbarrel" | "roof_barrel" => bw::ElementKind::RoofBarrel,
        "column" => bw::ElementKind::Column,
        "beam" => bw::ElementKind::Beam,
        "stair" => bw::ElementKind::Stair,
        "railing" => bw::ElementKind::Railing,
        "furniture" => {
            bw::ElementKind::Furniture(category.unwrap_or_else(|| "furniture".to_string()))
        }
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown element kind '{other}'"
            )))
        }
    })
}

#[pymethods]
impl PyRoomElement {
    /// `RoomElement(kind, rect, rotation=0.0, label=None, height=0.0,
    /// z_offset=0.0, storey="", category=None, psets=[], material=None,
    /// identifier=None, color=None, mesh=None)`.
    ///
    /// `kind` is one of: wall, door, window, opening, slab, roof_gable,
    /// roof_hip, roof_barrel, column, beam, stair, railing, furniture.
    /// `category` names the furniture type when `kind == "furniture"`.
    /// `material` names the IfcMaterial; `None` picks a per-kind default.
    /// `identifier` is a stable source id (UUID or any string) that fixes
    /// the IFC GlobalId across re-exports. `color` is an `(r, g, b, a)`
    /// tuple 0–1; `None` uses the per-kind palette. `mesh` is a
    /// `(vertices, indices)` pair — vertices as `(x, y, z)` tuples in
    /// element-local meters, indices a flat triangle list — written as
    /// `IfcFacetedBrep` instead of the extruded box.
    /// The writer derives `Pset_*Common` and `BaseQuantities`; a pset in
    /// `psets` with the same name wins.
    #[new]
    #[pyo3(signature = (
        kind,
        rect,
        rotation=0.0,
        label=None,
        height=0.0,
        z_offset=0.0,
        storey=String::new(),
        category=None,
        psets=Vec::new(),
        material=None,
        identifier=None,
        color=None,
        mesh=None,
    ))]
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn new(
        kind: &str,
        rect: PyRect,
        rotation: f64,
        label: Option<String>,
        height: f64,
        z_offset: f64,
        storey: String,
        category: Option<String>,
        psets: Vec<PyElementPset>,
        material: Option<String>,
        identifier: Option<String>,
        color: Option<(f32, f32, f32, f32)>,
        mesh: Option<(Vec<(f64, f64, f64)>, Vec<u32>)>,
    ) -> PyResult<Self> {
        let mesh = mesh.map(|(v, indices)| bw::ElementMesh {
            vertices: v.into_iter().map(|(x, y, z)| [x, y, z]).collect(),
            indices,
        });
        if let Some(m) = &mesh {
            if !m.is_valid() {
                return Err(PyValueError::new_err(
                    "mesh indices must be a non-empty triangle list within the vertex range",
                ));
            }
        }
        Ok(Self {
            inner: bw::RoomElement {
                kind: parse_kind(kind, category)?,
                rect: rect.inner,
                rotation,
                label,
                height,
                z_offset,
                storey,
                color: color.map(|(r, g, b, a)| [r, g, b, a]),
                psets: psets.into_iter().map(|p| p.into_inner()).collect(),
                material,
                identifier,
                mesh,
            },
        })
    }
}

/// A room outline exported as `IFCSPACE`.
///
/// The writer derives `Pset_SpaceCommon`, `BaseQuantities` (floor area,
/// perimeter, volume) and first-level `IfcRelSpaceBoundary` links to the
/// walls, doors and windows enclosing the polygon.
#[pyclass(name = "Space", from_py_object)]
#[derive(Clone)]
pub struct PySpace {
    inner: bw::SpaceData,
}

impl PySpace {
    fn into_inner(self) -> bw::SpaceData {
        self.inner
    }
}

#[pymethods]
impl PySpace {
    /// `Space(name, polygon, height=0.0, long_name=None, z_offset=0.0,
    /// storey="", psets=[], identifier=None, slab_thickness=None)` —
    /// `polygon` is a list of `(x, y)` tuples in meters (at least 3
    /// points, closing point optional). `slab_thickness` generates an
    /// `IfcSlab` under the space from the same polygon.
    #[new]
    #[pyo3(signature = (
        name,
        polygon,
        height=0.0,
        long_name=None,
        z_offset=0.0,
        storey=String::new(),
        psets=Vec::new(),
        identifier=None,
        slab_thickness=None,
        slab_only=false,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        name: String,
        polygon: Vec<(f64, f64)>,
        height: f64,
        long_name: Option<String>,
        z_offset: f64,
        storey: String,
        psets: Vec<PyElementPset>,
        identifier: Option<String>,
        slab_thickness: Option<f64>,
        slab_only: bool,
    ) -> PyResult<Self> {
        if polygon.len() < 3 {
            return Err(PyValueError::new_err(
                "Space polygon needs at least 3 points",
            ));
        }
        Ok(Self {
            inner: bw::SpaceData {
                name,
                long_name,
                polygon,
                height,
                z_offset,
                storey,
                psets: psets.into_iter().map(|p| p.into_inner()).collect(),
                identifier,
                slab_thickness,
                slab_only,
            },
        })
    }

    #[getter]
    fn name(&self) -> &str {
        &self.inner.name
    }

    fn __repr__(&self) -> String {
        format!(
            "Space(name={:?}, points={})",
            self.inner.name,
            self.inner.polygon.len()
        )
    }
}

/// IFC 2x3 STEP file writer — full parity with the Swift/Kotlin FFI writer.
#[pyclass(name = "IfcWriter")]
pub struct PyIfcWriter;

#[pymethods]
impl PyIfcWriter {
    #[new]
    fn new() -> Self {
        Self
    }

    /// Generate IFC 2x3 content from room elements (and optional `Space`
    /// outlines). Returns the file as a string.
    #[pyo3(signature = (elements, bounding_box, dimensions, project=None, spaces=Vec::new()))]
    fn write_ifc(
        &self,
        elements: Vec<PyRoomElement>,
        bounding_box: PyRect,
        dimensions: PyRoomDimensions,
        project: Option<PyProjectInfo>,
        spaces: Vec<PySpace>,
    ) -> String {
        let room = bw::RoomData {
            elements: elements.into_iter().map(|e| e.into_inner()).collect(),
            bounding_box: bounding_box.inner,
            dimensions: dimensions.inner,
            project: project.map(|p| p.inner).unwrap_or_default(),
            storey_elevations: Vec::new(),
            light_fixtures: Vec::new(),
            spaces: spaces.into_iter().map(|s| s.into_inner()).collect(),
        };
        bw::write_ifc(&room)
    }

    /// Generate IFC and write it directly to `path`.
    #[pyo3(signature = (elements, bounding_box, dimensions, path, project=None, spaces=Vec::new()))]
    fn write_ifc_to_file(
        &self,
        elements: Vec<PyRoomElement>,
        bounding_box: PyRect,
        dimensions: PyRoomDimensions,
        path: &str,
        project: Option<PyProjectInfo>,
        spaces: Vec<PySpace>,
    ) -> PyResult<()> {
        let content = self.write_ifc(elements, bounding_box, dimensions, project, spaces);
        std::fs::write(path, content).map_err(io_to_py_err)
    }

    /// Low-level: emit a standalone property set as a STEP DATA fragment
    /// (owner history #1 + property entities + IFCPROPERTYSET, no rel).
    /// Returns an empty string if `properties` is empty.
    fn write_property_set(&self, pset_name: &str, properties: Vec<PyIfcProperty>) -> String {
        if properties.is_empty() {
            return String::new();
        }
        let props: Vec<bw::IfcProperty> = properties.into_iter().map(|p| p.into_inner()).collect();

        let mut ids = bw::IdAlloc::new();
        let mut b = bw::StepBuilder::new();
        let owner = ids.next();
        b.entity(owner, "IFCOWNERHISTORY($,$,$,.ADDED.,$,$,$,0)");
        bw::write_property_set(&mut ids, &mut b, owner, &[], pset_name, &props);
        b.into_data_section()
    }
}
