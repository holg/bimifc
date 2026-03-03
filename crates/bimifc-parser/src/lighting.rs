// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Lighting data extraction from IFC files
//!
//! This module extracts light fixtures, light sources, and photometric data
//! from IFC files for use with lighting analysis and GLDF viewers.

use bimifc_model::{AttributeValue, DecodedEntity, EntityResolver, IfcType};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

/// Extracted light fixture data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightFixtureData {
    /// Entity ID
    pub id: u64,
    /// GlobalId (GUID)
    pub global_id: Option<String>,
    /// Name
    pub name: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Object type (predefined or user-defined)
    pub object_type: Option<String>,
    /// Position (X, Y, Z) in meters
    pub position: (f64, f64, f64),
    /// Associated storey name
    pub storey: Option<String>,
    /// Storey elevation
    pub storey_elevation: Option<f64>,
    /// Light fixture type reference
    pub fixture_type: Option<LightFixtureTypeData>,
    /// Light sources
    pub light_sources: Vec<LightSourceData>,
    /// Properties from property sets
    pub properties: FxHashMap<String, PropertySetData>,
}

/// Light fixture type definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightFixtureTypeData {
    pub id: u64,
    pub name: Option<String>,
    pub description: Option<String>,
    pub predefined_type: Option<String>,
}

/// Light source data (goniometric, positional, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightSourceData {
    pub id: u64,
    pub source_type: String,
    /// Color temperature in Kelvin
    pub color_temperature: Option<f64>,
    /// Luminous flux in lumens
    pub luminous_flux: Option<f64>,
    /// Light emission source (LED, FLUORESCENT, etc.)
    pub emission_source: Option<String>,
    /// Intensity (cd)
    pub intensity: Option<f64>,
    /// Color RGB
    pub color_rgb: Option<(f64, f64, f64)>,
    /// Light distribution data
    pub distribution: Option<LightDistributionData>,
}

/// Light distribution/photometry data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightDistributionData {
    /// Distribution type (TYPE_A, TYPE_B, TYPE_C)
    pub distribution_type: String,
    /// Distribution planes (C-planes for Type C)
    pub planes: Vec<DistributionPlane>,
}

/// A single distribution plane
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionPlane {
    /// Main plane angle (C-angle for Type C)
    pub main_angle: f64,
    /// Intensity values at secondary angles (gamma angles)
    pub intensities: Vec<(f64, f64)>, // (angle, intensity in cd)
}

/// Property set with values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySetData {
    pub name: String,
    pub properties: FxHashMap<String, String>,
}

/// Complete lighting data from an IFC file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingExport {
    /// Schema version
    pub schema: String,
    /// Project name
    pub project_name: Option<String>,
    /// Building name
    pub building_name: Option<String>,
    /// Building storeys
    pub storeys: Vec<StoreyData>,
    /// Light fixtures
    pub light_fixtures: Vec<LightFixtureData>,
    /// Light fixture types
    pub light_fixture_types: Vec<LightFixtureTypeData>,
    /// Summary statistics
    pub summary: LightingSummary,
}

/// Building storey data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreyData {
    pub id: u64,
    pub name: String,
    pub elevation: f64,
}

/// Summary of lighting data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingSummary {
    pub total_fixtures: usize,
    pub total_light_sources: usize,
    pub fixtures_per_storey: FxHashMap<String, usize>,
    pub fixture_types_used: Vec<String>,
    /// Total luminous flux if available
    pub total_luminous_flux: Option<f64>,
}

/// Extract lighting data from an IFC model
pub fn extract_lighting_data(resolver: &dyn EntityResolver) -> LightingExport {
    let mut export = LightingExport {
        schema: String::new(),
        project_name: None,
        building_name: None,
        storeys: Vec::new(),
        light_fixtures: Vec::new(),
        light_fixture_types: Vec::new(),
        summary: LightingSummary {
            total_fixtures: 0,
            total_light_sources: 0,
            fixtures_per_storey: FxHashMap::default(),
            fixture_types_used: Vec::new(),
            total_luminous_flux: None,
        },
    };

    // Extract project info
    let projects = resolver.entities_by_type(&IfcType::IfcProject);
    if let Some(project) = projects.first() {
        export.project_name = project.get_string(2).map(|s| s.to_string());
    }

    // Extract building info
    let buildings = resolver.entities_by_type(&IfcType::IfcBuilding);
    if let Some(building) = buildings.first() {
        export.building_name = building.get_string(2).map(|s| s.to_string());
    }

    // Extract storeys
    let storeys = resolver.entities_by_type(&IfcType::IfcBuildingStorey);
    for storey in storeys {
        let name = storey
            .get_string(2)
            .map(|s| s.to_string())
            .unwrap_or_default();
        let elevation = storey.get_float(9).unwrap_or(0.0);
        export.storeys.push(StoreyData {
            id: storey.id.0 as u64,
            name,
            elevation,
        });
    }

    // Extract light fixture types
    let fixture_types = resolver.entities_by_type(&IfcType::IfcLightFixtureType);
    for fixture_type in fixture_types {
        let type_data = extract_fixture_type(&fixture_type);
        export.light_fixture_types.push(type_data);
    }

    // Extract light fixtures
    let fixtures = resolver.entities_by_type(&IfcType::IfcLightFixture);
    let mut total_flux: f64 = 0.0;
    let mut has_flux = false;

    for fixture in fixtures {
        let fixture_data = extract_fixture(&fixture, resolver);

        // Update summary
        if let Some(ref storey) = fixture_data.storey {
            *export
                .summary
                .fixtures_per_storey
                .entry(storey.clone())
                .or_insert(0) += 1;
        }

        for source in &fixture_data.light_sources {
            if let Some(flux) = source.luminous_flux {
                total_flux += flux;
                has_flux = true;
            }
        }

        export.summary.total_light_sources += fixture_data.light_sources.len();
        export.light_fixtures.push(fixture_data);
    }

    export.summary.total_fixtures = export.light_fixtures.len();
    if has_flux {
        export.summary.total_luminous_flux = Some(total_flux);
    }

    // Collect unique fixture types used
    for fixture in &export.light_fixtures {
        if let Some(ref ft) = fixture.fixture_type {
            if let Some(ref name) = ft.name {
                if !export.summary.fixture_types_used.contains(name) {
                    export.summary.fixture_types_used.push(name.clone());
                }
            }
        }
    }

    export
}

/// Extract data from a light fixture type entity
fn extract_fixture_type(entity: &DecodedEntity) -> LightFixtureTypeData {
    LightFixtureTypeData {
        id: entity.id.0 as u64,
        name: entity.get_string(2).map(|s| s.to_string()),
        description: entity.get_string(3).map(|s| s.to_string()),
        predefined_type: entity.get_enum(9).map(|s| s.to_string()),
    }
}

/// Extract data from a light fixture entity
fn extract_fixture(entity: &DecodedEntity, resolver: &dyn EntityResolver) -> LightFixtureData {
    let global_id = entity.get_string(0).map(|s| s.to_string());
    let name = entity.get_string(2).map(|s| s.to_string());
    let description = entity.get_string(3).map(|s| s.to_string());
    let object_type = entity.get_string(4).map(|s| s.to_string());

    // Get position from placement
    let position = extract_position(entity, resolver);

    // Get fixture type
    let fixture_type = entity.get_ref(5).and_then(|type_ref| {
        resolver
            .get(type_ref)
            .map(|type_entity| extract_fixture_type(&type_entity))
    });

    // Extract light sources associated with this fixture
    // In IFC, light sources are typically referenced through the representation
    let light_sources = extract_light_sources(entity, resolver);

    LightFixtureData {
        id: entity.id.0 as u64,
        global_id,
        name,
        description,
        object_type,
        position,
        storey: None, // Could be populated from spatial containment
        storey_elevation: None,
        fixture_type,
        light_sources,
        properties: FxHashMap::default(),
    }
}

/// Extract position from entity placement
fn extract_position(entity: &DecodedEntity, resolver: &dyn EntityResolver) -> (f64, f64, f64) {
    // ObjectPlacement is typically at index 5 for IfcProduct
    let placement_ref = match entity.get_ref(5) {
        Some(id) => id,
        None => return (0.0, 0.0, 0.0),
    };

    let placement = match resolver.get(placement_ref) {
        Some(p) => p,
        None => return (0.0, 0.0, 0.0),
    };

    // IfcLocalPlacement has RelativePlacement at index 1
    if placement.ifc_type == IfcType::IfcLocalPlacement {
        if let Some(rel_placement_ref) = placement.get_ref(1) {
            if let Some(axis_placement) = resolver.get(rel_placement_ref) {
                return extract_cartesian_point(&axis_placement, resolver);
            }
        }
    }

    (0.0, 0.0, 0.0)
}

/// Extract coordinates from axis placement
fn extract_cartesian_point(
    axis_placement: &DecodedEntity,
    resolver: &dyn EntityResolver,
) -> (f64, f64, f64) {
    // IfcAxis2Placement3D has Location at index 0
    let point_ref = match axis_placement.get_ref(0) {
        Some(id) => id,
        None => return (0.0, 0.0, 0.0),
    };

    let point = match resolver.get(point_ref) {
        Some(p) => p,
        None => return (0.0, 0.0, 0.0),
    };

    if point.ifc_type == IfcType::IfcCartesianPoint {
        // Coordinates are in a list at index 0
        if let Some(coords) = point.get_list(0) {
            let x = coords.first().and_then(|v| v.as_float()).unwrap_or(0.0);
            let y = coords.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
            let z = coords.get(2).and_then(|v| v.as_float()).unwrap_or(0.0);
            return (x, y, z);
        }
    }

    (0.0, 0.0, 0.0)
}

/// Extract light sources from a fixture
fn extract_light_sources(
    _fixture: &DecodedEntity,
    resolver: &dyn EntityResolver,
) -> Vec<LightSourceData> {
    let mut sources = Vec::new();

    // Light sources can be found in various ways:
    // 1. Through IFCRELASSOCIATESMATERIAL relationships
    // 2. Through representation items
    // 3. Direct references

    // For now, search all goniometric light sources and match by containment
    let goniometric_sources = resolver.entities_by_type(&IfcType::IfcLightSourceGoniometric);

    for source in goniometric_sources {
        sources.push(extract_goniometric_source(&source, resolver));
    }

    sources
}

/// Extract data from a goniometric light source
fn extract_goniometric_source(
    entity: &DecodedEntity,
    resolver: &dyn EntityResolver,
) -> LightSourceData {
    // IfcLightSourceGoniometric attributes:
    // 0: Name
    // 1: LightColour (IfcColourRgb)
    // 2: AmbientIntensity
    // 3: Intensity
    // 4: Position (IfcAxis2Placement3D)
    // 5: ColourAppearance (IfcColourRgb)
    // 6: ColourTemperature
    // 7: LuminousFlux
    // 8: LightEmissionSource
    // 9: LightDistributionDataSource

    let color_temperature = entity.get_float(6);
    let luminous_flux = entity.get_float(7);
    let emission_source = entity.get_enum(8).map(|s| s.to_string());
    let intensity = entity.get_float(3);

    // Extract color RGB
    let color_rgb = entity.get_ref(1).and_then(|color_ref| {
        resolver.get(color_ref).and_then(|color| {
            let r = color.get_float(1)?;
            let g = color.get_float(2)?;
            let b = color.get_float(3)?;
            Some((r, g, b))
        })
    });

    // Extract distribution data
    let distribution = entity.get_ref(9).and_then(|dist_ref| {
        resolver
            .get(dist_ref)
            .map(|dist| extract_distribution(&dist, resolver))
    });

    LightSourceData {
        id: entity.id.0 as u64,
        source_type: "GONIOMETRIC".to_string(),
        color_temperature,
        luminous_flux,
        emission_source,
        intensity,
        color_rgb,
        distribution,
    }
}

/// Extract light intensity distribution data
fn extract_distribution(
    entity: &DecodedEntity,
    resolver: &dyn EntityResolver,
) -> LightDistributionData {
    // IfcLightIntensityDistribution attributes:
    // 0: LightDistributionCurve (enum: TYPE_A, TYPE_B, TYPE_C)
    // 1: DistributionData (list of IfcLightDistributionData)

    let distribution_type = entity
        .get_enum(0)
        .map(|s| s.to_string())
        .unwrap_or_else(|| "TYPE_C".to_string());

    let mut planes = Vec::new();

    if let Some(data_list) = entity.get_list(1) {
        for data_item in data_list {
            if let AttributeValue::EntityRef(data_ref) = data_item {
                if let Some(data_entity) = resolver.get(*data_ref) {
                    // IfcLightDistributionData:
                    // 0: MainPlaneAngle
                    // 1: SecondaryPlaneAngle (list)
                    // 2: LuminousIntensity (list)

                    let main_angle = data_entity.get_float(0).unwrap_or(0.0);
                    let mut intensities = Vec::new();

                    if let (Some(angles), Some(values)) =
                        (data_entity.get_list(1), data_entity.get_list(2))
                    {
                        for (angle, value) in angles.iter().zip(values.iter()) {
                            let a = angle.as_float().unwrap_or(0.0);
                            let v = value.as_float().unwrap_or(0.0);
                            intensities.push((a, v));
                        }
                    }

                    planes.push(DistributionPlane {
                        main_angle,
                        intensities,
                    });
                }
            }
        }
    }

    LightDistributionData {
        distribution_type,
        planes,
    }
}

/// Export lighting data to JSON format compatible with gldf-ifc-viewer
pub fn export_to_json(export: &LightingExport) -> String {
    serde_json::to_string_pretty(export).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_light_fixture_data_serialization() {
        let fixture = LightFixtureData {
            id: 123,
            global_id: Some("abc-def".to_string()),
            name: Some("Test Fixture".to_string()),
            description: None,
            object_type: None,
            position: (1.0, 2.0, 3.0),
            storey: Some("Ground Floor".to_string()),
            storey_elevation: Some(0.0),
            fixture_type: None,
            light_sources: vec![],
            properties: FxHashMap::default(),
        };

        let json = serde_json::to_string(&fixture).unwrap();
        assert!(json.contains("Test Fixture"));
        assert!(json.contains("123"));
    }
}
