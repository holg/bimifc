// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! SweptDiskSolid processor - sweeps a circular profile along a curve.

use crate::{Error, Mesh, Result, Vector3};
use bimifc_model::{DecodedEntity, EntityResolver, IfcType};
use nalgebra::Point3;

use crate::router::GeometryProcessor;

/// SweptDiskSolid processor
///
/// Handles IfcSweptDiskSolid - sweeps a circular profile along a curve.
pub struct SweptDiskSolidProcessor;

impl SweptDiskSolidProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Extract points from a curve
    fn get_curve_points(
        &self,
        curve: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Result<Vec<Point3<f64>>> {
        match curve.ifc_type {
            IfcType::IfcPolyline => {
                // IfcPolyline: Points at index 0
                let points_list = curve
                    .get(0)
                    .and_then(|v| v.as_list())
                    .ok_or_else(|| Error::invalid_attribute(0, "Missing Points"))?;

                let mut points = Vec::with_capacity(points_list.len());
                for point_ref in points_list {
                    if let Some(point_id) = point_ref.as_entity_ref() {
                        if let Some(point_entity) = resolver.get(point_id) {
                            if let Some(coords) = point_entity.get(0).and_then(|v| v.as_list()) {
                                let x = coords.first().and_then(|v| v.as_float()).unwrap_or(0.0);
                                let y = coords.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
                                let z = coords.get(2).and_then(|v| v.as_float()).unwrap_or(0.0);
                                points.push(Point3::new(x, y, z));
                            }
                        }
                    }
                }
                Ok(points)
            }
            IfcType::IfcIndexedPolyCurve => {
                // IfcIndexedPolyCurve: Points (IfcCartesianPointList3D) at index 0
                let points_ref_id = curve
                    .get_ref(0)
                    .ok_or_else(|| Error::invalid_attribute(0, "Missing Points"))?;

                let points_entity = resolver
                    .get(points_ref_id)
                    .ok_or_else(|| Error::entity_not_found(points_ref_id.0))?;

                // IfcCartesianPointList3D: CoordList at index 0
                let coord_list = points_entity
                    .get(0)
                    .and_then(|v| v.as_list())
                    .ok_or_else(|| Error::invalid_attribute(0, "Missing CoordList"))?;

                let mut points = Vec::with_capacity(coord_list.len());
                for coord in coord_list {
                    if let Some(point) = coord.as_list() {
                        let x = point.first().and_then(|v| v.as_float()).unwrap_or(0.0);
                        let y = point.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
                        let z = point.get(2).and_then(|v| v.as_float()).unwrap_or(0.0);
                        points.push(Point3::new(x, y, z));
                    }
                }
                Ok(points)
            }
            _ => Err(Error::unsupported_type(format!(
                "Curve type {:?}",
                curve.ifc_type
            ))),
        }
    }
}

impl Default for SweptDiskSolidProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl GeometryProcessor for SweptDiskSolidProcessor {
    fn process(
        &self,
        entity: &DecodedEntity,
        resolver: &dyn EntityResolver,
        _unit_scale: f64,
    ) -> Result<Mesh> {
        // IfcSweptDiskSolid attributes:
        // 0: Directrix (IfcCurve)
        // 1: Radius
        // 2: InnerRadius (optional)
        // 3: StartParam (optional)
        // 4: EndParam (optional)

        let directrix_id = entity
            .get_ref(0)
            .ok_or_else(|| Error::invalid_attribute(0, "Missing Directrix"))?;

        let directrix = resolver
            .get(directrix_id)
            .ok_or_else(|| Error::entity_not_found(directrix_id.0))?;

        let radius = entity
            .get_float(1)
            .ok_or_else(|| Error::invalid_attribute(1, "Missing Radius"))?;

        let curve_points = self.get_curve_points(&directrix, resolver)?;

        if curve_points.len() < 2 {
            return Ok(Mesh::new());
        }

        // Generate tube mesh
        let segments = 12;
        let mut positions = Vec::new();
        let mut indices = Vec::new();

        for i in 0..curve_points.len() {
            let p = curve_points[i];

            // Calculate tangent
            let tangent = if i == 0 {
                (curve_points[1] - curve_points[0]).normalize()
            } else if i == curve_points.len() - 1 {
                (curve_points[i] - curve_points[i - 1]).normalize()
            } else {
                ((curve_points[i + 1] - curve_points[i - 1]) / 2.0).normalize()
            };

            // Create perpendicular vectors
            let up = if tangent.x.abs() < 0.9 {
                Vector3::new(1.0, 0.0, 0.0)
            } else {
                Vector3::new(0.0, 1.0, 0.0)
            };

            let perp1 = tangent.cross(&up).normalize();
            let perp2 = tangent.cross(&perp1).normalize();

            // Create ring of vertices
            for j in 0..segments {
                let angle = 2.0 * std::f64::consts::PI * j as f64 / segments as f64;
                let offset = perp1 * (radius * angle.cos()) + perp2 * (radius * angle.sin());
                let vertex = p + offset;

                positions.push(vertex.x as f32);
                positions.push(vertex.y as f32);
                positions.push(vertex.z as f32);
            }

            // Create triangles connecting this ring to the next
            if i < curve_points.len() - 1 {
                let base = (i * segments) as u32;
                let next_base = ((i + 1) * segments) as u32;

                for j in 0..segments {
                    let j_next = (j + 1) % segments;

                    indices.push(base + j as u32);
                    indices.push(next_base + j as u32);
                    indices.push(next_base + j_next as u32);

                    indices.push(base + j as u32);
                    indices.push(next_base + j_next as u32);
                    indices.push(base + j_next as u32);
                }
            }
        }

        // Add end caps
        let center_idx = (positions.len() / 3) as u32;
        let start = curve_points[0];
        positions.push(start.x as f32);
        positions.push(start.y as f32);
        positions.push(start.z as f32);

        for j in 0..segments {
            let j_next = (j + 1) % segments;
            indices.push(center_idx);
            indices.push(j_next as u32);
            indices.push(j as u32);
        }

        let end_center_idx = (positions.len() / 3) as u32;
        let end_base = ((curve_points.len() - 1) * segments) as u32;
        let end = curve_points[curve_points.len() - 1];
        positions.push(end.x as f32);
        positions.push(end.y as f32);
        positions.push(end.z as f32);

        for j in 0..segments {
            let j_next = (j + 1) % segments;
            indices.push(end_center_idx);
            indices.push(end_base + j as u32);
            indices.push(end_base + j_next as u32);
        }

        Ok(Mesh {
            positions,
            normals: Vec::new(),
            indices,
        })
    }

    fn supported_types(&self) -> Vec<IfcType> {
        vec![IfcType::IfcSweptDiskSolid]
    }
}
