// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! TriangulatedFaceSet processor - explicit triangle meshes (IFC4+)
//!
//! Includes a fast-path that parses coordinates and indices directly from
//! raw STEP text, bypassing the generic AttributeValue pipeline for large arrays.

use crate::{Error, Mesh, Result};
use bimifc_model::{DecodedEntity, EntityId, EntityResolver, IfcType};

use crate::router::GeometryProcessor;

/// TriangulatedFaceSet processor
///
/// Handles IfcTriangulatedFaceSet - explicit triangle meshes (IFC4+)
pub struct TriangulatedFaceSetProcessor;

impl TriangulatedFaceSetProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Fast-path: parse coordinates directly from raw STEP bytes.
    /// Returns flattened f32 positions [x0,y0,z0, x1,y1,z1, ...]
    fn parse_coords_fast(raw: &[u8]) -> Option<Vec<f32>> {
        let text = std::str::from_utf8(raw).ok()?;
        // Find nested list: ((x,y,z),(x,y,z),...)
        let start = text.find("((")?;
        let end = text.rfind("))")?;
        let list_content = &text[start + 1..end + 1];

        let mut coords = Vec::new();
        let mut current = list_content;

        while let Some(paren_start) = current.find('(') {
            let paren_end = current[paren_start..].find(')')? + paren_start;
            let point_str = &current[paren_start + 1..paren_end];

            for num_str in point_str.split(',') {
                let num_str = num_str.trim();
                if !num_str.is_empty() {
                    let val: f64 = num_str.parse().ok()?;
                    coords.push(val as f32);
                }
            }

            current = &current[paren_end + 1..];
        }

        if coords.is_empty() { None } else { Some(coords) }
    }

    /// Fast-path: parse index list directly from raw STEP bytes.
    /// Converts from 1-based IFC indices to 0-based.
    fn parse_indices_fast(raw: &[u8]) -> Option<Vec<u32>> {
        let text = std::str::from_utf8(raw).ok()?;

        // CoordIndex is attribute 3 in IfcTriangulatedFaceSet:
        // IFCTRIANGULATEDFACESET(#coords, normals, closed, ((i,j,k),(i,j,k),...))
        // We need to find the 4th attribute's nested list.
        // Strategy: skip 3 commas at top level (outside parens), then parse the list.
        let attr_start = find_nth_top_level_comma(text, 3)?;
        let remaining = &text[attr_start + 1..];

        let start = remaining.find("((")?;
        let end = remaining.rfind("))")?;
        let list_content = &remaining[start + 1..end + 1];

        let mut indices = Vec::new();
        let mut current = list_content;

        while let Some(paren_start) = current.find('(') {
            let paren_end = current[paren_start..].find(')')? + paren_start;
            let index_str = &current[paren_start + 1..paren_end];

            for num_str in index_str.split(',') {
                let num_str = num_str.trim();
                if !num_str.is_empty() {
                    let val: u32 = num_str.parse().ok()?;
                    indices.push(val.saturating_sub(1));
                }
            }

            current = &current[paren_end + 1..];
        }

        if indices.is_empty() { None } else { Some(indices) }
    }

    /// Generic path: parse coordinates from decoded AttributeValues
    fn parse_coords_generic(
        coord_id: EntityId,
        resolver: &dyn EntityResolver,
    ) -> Result<Vec<f32>> {
        let coord_entity = resolver
            .get(coord_id)
            .ok_or_else(|| Error::entity_not_found(coord_id.0))?;

        let coord_list = coord_entity
            .get(0)
            .and_then(|v| v.as_list())
            .ok_or_else(|| Error::invalid_attribute(0, "Missing CoordList"))?;

        let mut positions = Vec::with_capacity(coord_list.len() * 3);
        for coord in coord_list {
            if let Some(point) = coord.as_list() {
                let x = point.first().and_then(|v| v.as_float()).unwrap_or(0.0);
                let y = point.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
                let z = point.get(2).and_then(|v| v.as_float()).unwrap_or(0.0);
                positions.push(x as f32);
                positions.push(y as f32);
                positions.push(z as f32);
            }
        }
        Ok(positions)
    }

    /// Generic path: parse indices from decoded AttributeValues
    fn parse_indices_generic(entity: &DecodedEntity) -> Result<Vec<u32>> {
        let indices_attr = entity
            .get(3)
            .ok_or_else(|| Error::invalid_attribute(3, "Missing CoordIndex"))?;

        let face_list = indices_attr
            .as_list()
            .ok_or_else(|| Error::invalid_attribute(3, "Expected list for CoordIndex"))?;

        let mut indices = Vec::with_capacity(face_list.len() * 3);
        for face in face_list {
            if let Some(triangle) = face.as_list() {
                if triangle.len() >= 3 {
                    let i0 = triangle.first().and_then(|v| v.as_integer()).unwrap_or(1) as u32 - 1;
                    let i1 = triangle.get(1).and_then(|v| v.as_integer()).unwrap_or(1) as u32 - 1;
                    let i2 = triangle.get(2).and_then(|v| v.as_integer()).unwrap_or(1) as u32 - 1;
                    indices.push(i0);
                    indices.push(i1);
                    indices.push(i2);
                }
            }
        }
        Ok(indices)
    }
}

/// Find the position of the Nth top-level comma (outside nested parens).
fn find_nth_top_level_comma(text: &str, n: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut count = 0;
    // Start after the opening paren of the entity's attribute list
    let start = text.find('(')?;
    for (i, ch) in text[start + 1..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    return None; // End of attribute list
                }
                depth -= 1;
            }
            ',' if depth == 0 => {
                count += 1;
                if count == n {
                    return Some(start + 1 + i);
                }
            }
            _ => {}
        }
    }
    None
}

impl Default for TriangulatedFaceSetProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl GeometryProcessor for TriangulatedFaceSetProcessor {
    fn process(
        &self,
        entity: &DecodedEntity,
        resolver: &dyn EntityResolver,
        _unit_scale: f64,
    ) -> Result<Mesh> {
        // IfcTriangulatedFaceSet attributes:
        // 0: Coordinates (IfcCartesianPointList3D)
        // 1: Normals (optional)
        // 2: Closed (optional)
        // 3: CoordIndex (list of list of IfcPositiveInteger)

        let coord_id = entity
            .get_ref(0)
            .ok_or_else(|| Error::invalid_attribute(0, "Missing Coordinates"))?;

        // Try fast-path for coordinates: parse raw STEP bytes directly
        let positions = if let Some(raw) = resolver.raw_bytes(coord_id) {
            Self::parse_coords_fast(raw)
                .unwrap_or_else(|| Self::parse_coords_generic(coord_id, resolver).unwrap_or_default())
        } else {
            Self::parse_coords_generic(coord_id, resolver)?
        };

        // Try fast-path for indices: parse raw STEP bytes directly
        let indices = if let Some(raw) = resolver.raw_bytes(entity.id) {
            Self::parse_indices_fast(raw)
                .unwrap_or_else(|| Self::parse_indices_generic(entity).unwrap_or_default())
        } else {
            Self::parse_indices_generic(entity)?
        };

        Ok(Mesh {
            positions,
            normals: Vec::new(),
            indices,
        })
    }

    fn supported_types(&self) -> Vec<IfcType> {
        vec![IfcType::IfcTriangulatedFaceSet]
    }
}
