// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! PropertyReader trait implementation

use bimifc_model::{
    AttributeValue, DecodedEntity, EntityId, EntityResolver, GoniometricData, IfcType,
    LightDistributionPlane, Property, PropertyReader, PropertySet, Quantity, QuantityType,
};
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Property reader implementation
pub struct PropertyReaderImpl {
    /// Reference to resolver for entity lookups
    resolver: Arc<dyn EntityResolver>,
    /// Cache: entity ID -> property set IDs
    pset_cache: FxHashMap<u32, Vec<EntityId>>,
    /// Cache: entity ID -> quantity set IDs
    qset_cache: FxHashMap<u32, Vec<EntityId>>,
    /// Cache: instance entity ID -> type entity ID (from IfcRelDefinesByType)
    type_cache: FxHashMap<u32, EntityId>,
}

impl PropertyReaderImpl {
    /// Create a new property reader
    pub fn new(resolver: Arc<dyn EntityResolver>) -> Self {
        // Build property relationship cache
        let mut pset_cache: FxHashMap<u32, Vec<EntityId>> = FxHashMap::default();
        let mut qset_cache: FxHashMap<u32, Vec<EntityId>> = FxHashMap::default();
        let mut type_cache: FxHashMap<u32, EntityId> = FxHashMap::default();

        // Find all IFCRELDEFINESBYPROPERTIES relationships
        for rel in resolver.entities_by_type(&IfcType::IfcRelDefinesByProperties) {
            // RelatedObjects at index 4, RelatingPropertyDefinition at index 5
            let related_ids = match rel.get(4) {
                Some(AttributeValue::List(list)) => list
                    .iter()
                    .filter_map(|v| v.as_entity_ref())
                    .collect::<Vec<_>>(),
                _ => continue,
            };

            let pset_id = match rel.get_ref(5) {
                Some(id) => id,
                None => continue,
            };

            // Check if it's a property set or element quantity
            if let Some(pset) = resolver.get(pset_id) {
                let cache = match pset.ifc_type {
                    IfcType::IfcPropertySet => &mut pset_cache,
                    IfcType::IfcElementQuantity => &mut qset_cache,
                    _ => continue,
                };

                for related_id in related_ids {
                    cache.entry(related_id.0).or_default().push(pset_id);
                }
            }
        }

        // Build IfcRelDefinesByType cache: instance -> type
        for rel in resolver.entities_by_type(&IfcType::IfcRelDefinesByType) {
            // RelatedObjects at index 4, RelatingType at index 5
            let related_ids = match rel.get(4) {
                Some(AttributeValue::List(list)) => list
                    .iter()
                    .filter_map(|v| v.as_entity_ref())
                    .collect::<Vec<_>>(),
                _ => continue,
            };
            let type_id = match rel.get_ref(5) {
                Some(id) => id,
                None => continue,
            };
            for related_id in related_ids {
                type_cache.insert(related_id.0, type_id);
            }
        }

        Self {
            resolver,
            pset_cache,
            qset_cache,
            type_cache,
        }
    }

    /// Extract properties from a property set entity
    fn extract_properties(&self, pset: &DecodedEntity) -> Vec<Property> {
        let mut properties = Vec::new();

        // HasProperties at index 4
        let prop_refs = match pset.get(4) {
            Some(AttributeValue::List(list)) => list,
            _ => return properties,
        };

        for prop_ref in prop_refs {
            if let AttributeValue::EntityRef(prop_id) = prop_ref {
                if let Some(prop_entity) = self.resolver.get(*prop_id) {
                    if let Some(prop) = self.extract_single_property(&prop_entity) {
                        properties.push(prop);
                    }
                }
            }
        }

        properties
    }

    /// Extract a single property from an IfcProperty entity
    fn extract_single_property(&self, prop: &DecodedEntity) -> Option<Property> {
        // Name at index 0
        let name = prop.get_string(0)?.to_string();

        match prop.ifc_type {
            IfcType::IfcPropertySingleValue => {
                // NominalValue at index 2, Unit at index 3
                let value = self.format_value(prop.get(2)?);
                let unit = prop.get(3).and_then(|v| self.extract_unit(v));
                Some(Property { name, value, unit })
            }
            IfcType::IfcPropertyEnumeratedValue => {
                // EnumerationValues at index 2
                let values = match prop.get(2) {
                    Some(AttributeValue::List(list)) => list
                        .iter()
                        .map(|v| self.format_value(v))
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => return None,
                };
                Some(Property {
                    name,
                    value: values,
                    unit: None,
                })
            }
            IfcType::IfcPropertyBoundedValue => {
                // UpperBoundValue, LowerBoundValue at indices 2, 3
                let upper = prop.get(2).map(|v| self.format_value(v));
                let lower = prop.get(3).map(|v| self.format_value(v));
                let value = match (lower, upper) {
                    (Some(l), Some(u)) => format!("{} - {}", l, u),
                    (Some(l), None) => format!(">= {}", l),
                    (None, Some(u)) => format!("<= {}", u),
                    _ => return None,
                };
                Some(Property {
                    name,
                    value,
                    unit: None,
                })
            }
            IfcType::IfcPropertyListValue => {
                // ListValues at index 2
                let values = match prop.get(2) {
                    Some(AttributeValue::List(list)) => list
                        .iter()
                        .map(|v| self.format_value(v))
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => return None,
                };
                Some(Property {
                    name,
                    value: values,
                    unit: None,
                })
            }
            _ => None,
        }
    }

    /// Format an attribute value as a string
    fn format_value(&self, attr: &AttributeValue) -> String {
        match attr {
            AttributeValue::String(s) => s.clone(),
            AttributeValue::Integer(i) => i.to_string(),
            AttributeValue::Float(f) => format!("{:.6}", f)
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string(),
            AttributeValue::Bool(b) => b.to_string(),
            AttributeValue::Enum(e) => e.clone(),
            AttributeValue::TypedValue(_, args) if !args.is_empty() => self.format_value(&args[0]),
            AttributeValue::Null => "".to_string(),
            _ => format!("{:?}", attr),
        }
    }

    /// Extract unit from a unit reference
    fn extract_unit(&self, attr: &AttributeValue) -> Option<String> {
        let unit_id = attr.as_entity_ref()?;
        let unit = self.resolver.get(unit_id)?;

        // Try to get a readable unit name
        match unit.ifc_type {
            IfcType::IfcSIUnit => {
                let prefix = unit.get_enum(2).unwrap_or("");
                let name = unit.get_enum(3)?;
                let prefix_str = match prefix {
                    "MILLI" => "m",
                    "CENTI" => "c",
                    "KILO" => "k",
                    _ => "",
                };
                let unit_str = match name {
                    "METRE" => "m",
                    "SQUARE_METRE" => "m²",
                    "CUBIC_METRE" => "m³",
                    "GRAM" => "g",
                    "SECOND" => "s",
                    "KELVIN" => "K",
                    "AMPERE" => "A",
                    _ => name,
                };
                Some(format!("{}{}", prefix_str, unit_str))
            }
            IfcType::IfcConversionBasedUnit => {
                // Name at index 2
                unit.get_string(2).map(|s| s.to_string())
            }
            _ => None,
        }
    }

    /// Extract quantities from an element quantity entity
    fn extract_quantities(&self, qset: &DecodedEntity) -> Vec<Quantity> {
        let mut quantities = Vec::new();

        // Quantities at index 5
        let qty_refs = match qset.get(5) {
            Some(AttributeValue::List(list)) => list,
            _ => return quantities,
        };

        for qty_ref in qty_refs {
            if let AttributeValue::EntityRef(qty_id) = qty_ref {
                if let Some(qty_entity) = self.resolver.get(*qty_id) {
                    if let Some(qty) = self.extract_single_quantity(&qty_entity) {
                        quantities.push(qty);
                    }
                }
            }
        }

        quantities
    }

    /// Extract a single quantity from an IfcQuantity entity
    fn extract_single_quantity(&self, qty: &DecodedEntity) -> Option<Quantity> {
        // Name at index 0
        let name = qty.get_string(0)?.to_string();

        let (value, quantity_type) = match qty.ifc_type {
            IfcType::IfcQuantityLength => (qty.get_float(3)?, QuantityType::Length),
            IfcType::IfcQuantityArea => (qty.get_float(3)?, QuantityType::Area),
            IfcType::IfcQuantityVolume => (qty.get_float(3)?, QuantityType::Volume),
            IfcType::IfcQuantityCount => (qty.get_float(3)?, QuantityType::Count),
            IfcType::IfcQuantityWeight => (qty.get_float(3)?, QuantityType::Weight),
            IfcType::IfcQuantityTime => (qty.get_float(3)?, QuantityType::Time),
            _ => return None,
        };

        Some(Quantity::new(name, value, quantity_type))
    }
}

impl PropertyReader for PropertyReaderImpl {
    fn property_sets(&self, id: EntityId) -> Vec<PropertySet> {
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Collect pset IDs from instance and its type
        let empty = Vec::new();
        let instance_ids = self.pset_cache.get(&id.0).unwrap_or(&empty);
        let type_ids = self
            .type_cache
            .get(&id.0)
            .and_then(|tid| self.pset_cache.get(&tid.0));

        for pset_id in instance_ids.iter().chain(type_ids.into_iter().flatten()) {
            if !seen.insert(pset_id.0) {
                continue;
            }
            if let Some(pset) = self.resolver.get(*pset_id) {
                let name = pset.get_string(2).unwrap_or("Unknown").to_string();
                let properties = self.extract_properties(&pset);
                if !properties.is_empty() {
                    result.push(PropertySet { name, properties });
                }
            }
        }

        result
    }

    fn quantities(&self, id: EntityId) -> Vec<Quantity> {
        let mut result = Vec::new();

        let empty = Vec::new();
        let instance_ids = self.qset_cache.get(&id.0).unwrap_or(&empty);
        let type_ids = self
            .type_cache
            .get(&id.0)
            .and_then(|tid| self.qset_cache.get(&tid.0));

        for qset_id in instance_ids.iter().chain(type_ids.into_iter().flatten()) {
            if let Some(qset) = self.resolver.get(*qset_id) {
                result.extend(self.extract_quantities(&qset));
            }
        }

        result
    }

    fn global_id(&self, id: EntityId) -> Option<String> {
        let entity = self.resolver.get(id)?;
        // GlobalId is typically at index 0 for most entities
        entity.get_string(0).map(|s| s.to_string())
    }

    fn name(&self, id: EntityId) -> Option<String> {
        let entity = self.resolver.get(id)?;
        // Name is typically at index 2 for most building elements
        entity.get_string(2).map(|s| s.to_string())
    }

    fn description(&self, id: EntityId) -> Option<String> {
        let entity = self.resolver.get(id)?;
        // Description is typically at index 3
        entity.get_string(3).map(|s| s.to_string())
    }

    fn object_type(&self, id: EntityId) -> Option<String> {
        let entity = self.resolver.get(id)?;
        // ObjectType is typically at index 4
        entity.get_string(4).map(|s| s.to_string())
    }

    fn tag(&self, id: EntityId) -> Option<String> {
        let entity = self.resolver.get(id)?;
        // Tag varies by entity type, usually at index 7 for building elements
        entity.get_string(7).map(|s| s.to_string())
    }

    fn goniometric_sources(&self, id: EntityId) -> Vec<GoniometricData> {
        let entity = match self.resolver.get(id) {
            Some(e) => e,
            None => return Vec::new(),
        };

        let mut results = Vec::new();

        // For fixture instances: walk ProductDefinitionShape → ShapeRepresentations
        // IfcLightFixture inherits IfcProduct: attr 5 = ObjectPlacement, attr 6 = Representation
        if let Some(pds_id) = entity.get_ref(6) {
            self.find_goniometric_in_pds(pds_id, &mut results);
        }

        // Also try the type's RepresentationMaps (attr 6 on IfcTypeProduct)
        if results.is_empty() {
            if let Some(type_id) = self.type_cache.get(&id.0) {
                if let Some(type_entity) = self.resolver.get(*type_id) {
                    // IfcTypeProduct.RepresentationMaps is at index 6
                    if let Some(map_refs) = type_entity.get_refs(6) {
                        for map_id in map_refs {
                            self.find_goniometric_in_rep_map(map_id, &mut results);
                        }
                    }
                }
            }
        }

        results
    }
}

impl PropertyReaderImpl {
    /// Find IfcLightSourceGoniometric in a ProductDefinitionShape
    fn find_goniometric_in_pds(&self, pds_id: EntityId, results: &mut Vec<GoniometricData>) {
        let pds = match self.resolver.get(pds_id) {
            Some(e) if e.ifc_type == IfcType::IfcProductDefinitionShape => e,
            _ => return,
        };

        // Representations at index 2
        let rep_refs = match pds.get_refs(2) {
            Some(refs) => refs,
            None => return,
        };

        for rep_id in rep_refs {
            if let Some(rep) = self.resolver.get(rep_id) {
                if rep.ifc_type == IfcType::IfcShapeRepresentation {
                    self.find_goniometric_in_shape_rep(&rep, results);
                }
            }
        }
    }

    /// Find IfcLightSourceGoniometric in a ShapeRepresentation (direct or via MappedItem)
    fn find_goniometric_in_shape_rep(
        &self,
        rep: &DecodedEntity,
        results: &mut Vec<GoniometricData>,
    ) {
        // Items at index 3
        let item_refs = match rep.get_refs(3) {
            Some(refs) => refs,
            None => return,
        };

        for item_id in item_refs {
            if let Some(item) = self.resolver.get(item_id) {
                match item.ifc_type {
                    IfcType::IfcLightSourceGoniometric => {
                        if let Some(data) = self.extract_goniometric(&item) {
                            results.push(data);
                        }
                    }
                    IfcType::IfcMappedItem => {
                        // IfcMappedItem: attr 0 = MappingSource (IfcRepresentationMap)
                        if let Some(map_id) = item.get_ref(0) {
                            self.find_goniometric_in_rep_map(map_id, results);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Find IfcLightSourceGoniometric in a RepresentationMap
    fn find_goniometric_in_rep_map(&self, map_id: EntityId, results: &mut Vec<GoniometricData>) {
        let map = match self.resolver.get(map_id) {
            Some(e) if e.ifc_type == IfcType::IfcRepresentationMap => e,
            _ => return,
        };

        // IfcRepresentationMap: attr 0 = MappingOrigin, attr 1 = MappedRepresentation
        if let Some(mapped_rep_id) = map.get_ref(1) {
            if let Some(mapped_rep) = self.resolver.get(mapped_rep_id) {
                if mapped_rep.ifc_type == IfcType::IfcShapeRepresentation {
                    self.find_goniometric_in_shape_rep(&mapped_rep, results);
                }
            }
        }
    }

    /// Extract GoniometricData from an IfcLightSourceGoniometric entity
    fn extract_goniometric(&self, entity: &DecodedEntity) -> Option<GoniometricData> {
        // IfcLightSourceGoniometric attributes:
        // 0: Name
        // 1: LightColour (IfcColourRgb)
        // 2: AmbientIntensity
        // 3: Intensity
        // 4: Position (IfcAxis2Placement3D)
        // 5: ColourAppearance
        // 6: ColourTemperature
        // 7: LuminousFlux
        // 8: LightEmitterType (.LIGHTEMITTINGDIODE. etc)
        // 9: LightDistributionDataSource (IfcLightIntensityDistribution)
        let name = entity.get_string(0).unwrap_or("").to_string();
        let colour_temperature = entity.get_float(6).unwrap_or(0.0);
        let luminous_flux = entity.get_float(7).unwrap_or(0.0);
        let emitter_type = entity.get_enum(8).unwrap_or("UNKNOWN").to_string();

        let distribution_id = entity.get_ref(9)?;
        let distribution = self.resolver.get(distribution_id)?;

        // IfcLightIntensityDistribution:
        // 0: LightDistributionCurve (.TYPE_C. etc)
        // 1: DistributionData (list of IfcLightDistributionData refs)
        let distribution_type = distribution.get_enum(0).unwrap_or("TYPE_C").to_string();

        let data_refs = distribution.get_refs(1).unwrap_or_default();
        let mut planes = Vec::with_capacity(data_refs.len());

        for data_id in data_refs {
            if let Some(data_entity) = self.resolver.get(data_id) {
                if data_entity.ifc_type == IfcType::IfcLightDistributionData {
                    if let Some(plane) = self.extract_distribution_plane(&data_entity) {
                        planes.push(plane);
                    }
                }
            }
        }

        // Sort by C-plane angle
        planes.sort_by(|a, b| {
            a.c_angle
                .partial_cmp(&b.c_angle)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Some(GoniometricData {
            name,
            colour_temperature,
            luminous_flux,
            emitter_type,
            distribution_type,
            planes,
        })
    }

    /// Extract a single distribution plane from IfcLightDistributionData
    fn extract_distribution_plane(&self, entity: &DecodedEntity) -> Option<LightDistributionPlane> {
        // IfcLightDistributionData:
        // 0: MainPlaneAngle (C-plane angle)
        // 1: SecondaryPlaneAngle (list of gamma angles)
        // 2: LuminousIntensity (list of cd values)
        let c_angle = entity.get_float(0)?;

        let gamma_angles = extract_float_list(entity.get(1)?);
        let intensities = extract_float_list(entity.get(2)?);

        if gamma_angles.is_empty() || intensities.is_empty() {
            return None;
        }

        Some(LightDistributionPlane {
            c_angle,
            gamma_angles,
            intensities,
        })
    }
}

/// Extract a list of floats from an AttributeValue
fn extract_float_list(attr: &AttributeValue) -> Vec<f64> {
    match attr {
        AttributeValue::List(list) => list.iter().filter_map(|v| v.as_float()).collect(),
        _ => Vec::new(),
    }
}
