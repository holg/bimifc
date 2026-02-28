// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! FacetedBrep processor - explicit mesh with faces, supports holes.

use crate::{Error, Mesh, Result};
use bimifc_model::{DecodedEntity, EntityId, EntityResolver, IfcType};
use nalgebra::{Point2, Point3};

use crate::router::GeometryProcessor;

/// FacetedBrep processor
///
/// Handles IfcFacetedBrep - explicit mesh with faces.
/// Supports faces with inner bounds (holes).
pub struct FacetedBrepProcessor;

impl FacetedBrepProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Extract polygon points from a loop entity
    fn extract_loop_points(
        &self,
        loop_id: EntityId,
        resolver: &dyn EntityResolver,
    ) -> Option<Vec<Point3<f64>>> {
        let loop_entity = resolver.get(loop_id)?;

        // IfcPolyLoop has Polygon attribute at index 0
        let polygon_attr = loop_entity.get(0)?;
        let point_refs = polygon_attr.as_list()?;

        let mut points = Vec::with_capacity(point_refs.len());

        for point_ref in point_refs {
            let point_id = point_ref.as_entity_ref()?;
            let point_entity = resolver.get(point_id)?;

            // IfcCartesianPoint has Coordinates at index 0
            let coords = point_entity.get(0)?.as_list()?;

            let x = coords.first().and_then(|v| v.as_float()).unwrap_or(0.0);
            let y = coords.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
            let z = coords.get(2).and_then(|v| v.as_float()).unwrap_or(0.0);

            points.push(Point3::new(x, y, z));
        }

        if points.len() >= 3 {
            Some(points)
        } else {
            None
        }
    }

    /// Triangulate a face (supports holes)
    pub fn triangulate_face(
        &self,
        outer_points: &[Point3<f64>],
        hole_points: &[Vec<Point3<f64>>],
    ) -> (Vec<f32>, Vec<u32>) {
        let n = outer_points.len();

        // Fast path: triangle without holes
        if n == 3 && hole_points.is_empty() {
            let mut positions = Vec::with_capacity(9);
            for point in outer_points {
                positions.push(point.x as f32);
                positions.push(point.y as f32);
                positions.push(point.z as f32);
            }
            return (positions, vec![0, 1, 2]);
        }

        // Fast path: quad without holes
        if n == 4 && hole_points.is_empty() {
            let mut positions = Vec::with_capacity(12);
            for point in outer_points {
                positions.push(point.x as f32);
                positions.push(point.y as f32);
                positions.push(point.z as f32);
            }
            return (positions, vec![0, 1, 2, 0, 2, 3]);
        }

        // Complex polygon or has holes - use triangulation
        use crate::triangulation::{
            calculate_polygon_normal, project_to_2d, project_to_2d_with_basis,
            triangulate_polygon_with_holes,
        };

        let normal = calculate_polygon_normal(outer_points);
        let (outer_2d, u_axis, v_axis, origin) = project_to_2d(outer_points, &normal);

        let holes_2d: Vec<Vec<Point2<f64>>> = hole_points
            .iter()
            .map(|hole| project_to_2d_with_basis(hole, &u_axis, &v_axis, &origin))
            .collect();

        let tri_indices = match triangulate_polygon_with_holes(&outer_2d, &holes_2d) {
            Ok(idx) => idx,
            Err(_) => {
                // Fallback to simple fan triangulation
                let mut positions = Vec::with_capacity(n * 3);
                for point in outer_points {
                    positions.push(point.x as f32);
                    positions.push(point.y as f32);
                    positions.push(point.z as f32);
                }
                let mut indices = Vec::with_capacity((n - 2) * 3);
                for i in 1..n - 1 {
                    indices.push(0);
                    indices.push(i as u32);
                    indices.push(i as u32 + 1);
                }
                return (positions, indices);
            }
        };

        // Combine all 3D points (outer + holes)
        let mut all_points: Vec<&Point3<f64>> = outer_points.iter().collect();
        for hole in hole_points {
            all_points.extend(hole.iter());
        }

        let mut positions = Vec::with_capacity(all_points.len() * 3);
        for point in &all_points {
            positions.push(point.x as f32);
            positions.push(point.y as f32);
            positions.push(point.z as f32);
        }

        let indices: Vec<u32> = tri_indices.iter().map(|&i| i as u32).collect();

        (positions, indices)
    }
}

impl Default for FacetedBrepProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl GeometryProcessor for FacetedBrepProcessor {
    fn process(
        &self,
        entity: &DecodedEntity,
        resolver: &dyn EntityResolver,
        _unit_scale: f64,
    ) -> Result<Mesh> {
        // IfcFacetedBrep attributes:
        // 0: Outer (IfcClosedShell)

        let shell_id = entity
            .get_ref(0)
            .ok_or_else(|| Error::invalid_attribute(0, "Missing Outer shell"))?;

        let shell_entity = resolver
            .get(shell_id)
            .ok_or_else(|| Error::entity_not_found(shell_id.0))?;

        // IfcClosedShell has CfsFaces at index 0
        let faces = shell_entity
            .get(0)
            .and_then(|v| v.as_list())
            .ok_or_else(|| Error::invalid_attribute(0, "Missing CfsFaces"))?;

        let mut all_positions = Vec::new();
        let mut all_indices = Vec::new();

        for face_ref in faces {
            let face_id = match face_ref.as_entity_ref() {
                Some(id) => id,
                None => continue,
            };

            let face_entity = match resolver.get(face_id) {
                Some(e) => e,
                None => continue,
            };

            // IfcFace has Bounds at index 0
            let bounds = match face_entity.get(0).and_then(|v| v.as_list()) {
                Some(b) => b,
                None => continue,
            };

            let mut outer_points: Option<Vec<Point3<f64>>> = None;
            let mut hole_points_list: Vec<Vec<Point3<f64>>> = Vec::new();

            for bound_ref in bounds {
                let bound_id = match bound_ref.as_entity_ref() {
                    Some(id) => id,
                    None => continue,
                };

                let bound_entity = match resolver.get(bound_id) {
                    Some(e) => e,
                    None => continue,
                };

                // Get loop reference (index 0)
                let loop_id = match bound_entity.get_ref(0) {
                    Some(id) => id,
                    None => continue,
                };

                // Get orientation (index 1)
                let orientation = bound_entity
                    .get(1)
                    .map(|v| match v {
                        bimifc_model::AttributeValue::Enum(e) => e != "F" && e != ".F.",
                        bimifc_model::AttributeValue::Bool(b) => *b,
                        _ => true,
                    })
                    .unwrap_or(true);

                let mut points = match self.extract_loop_points(loop_id, resolver) {
                    Some(p) => p,
                    None => continue,
                };

                if !orientation {
                    points.reverse();
                }

                let is_outer = bound_entity.ifc_type == IfcType::IfcFaceOuterBound;

                if is_outer || outer_points.is_none() {
                    if outer_points.is_some() && is_outer {
                        if let Some(prev_outer) = outer_points.take() {
                            hole_points_list.push(prev_outer);
                        }
                    }
                    outer_points = Some(points);
                } else {
                    hole_points_list.push(points);
                }
            }

            if let Some(outer) = outer_points {
                let base_idx = (all_positions.len() / 3) as u32;
                let (positions, indices) = self.triangulate_face(&outer, &hole_points_list);

                all_positions.extend(positions);
                for idx in indices {
                    all_indices.push(base_idx + idx);
                }
            }
        }

        Ok(Mesh {
            positions: all_positions,
            normals: Vec::new(),
            indices: all_indices,
        })
    }

    fn supported_types(&self) -> Vec<IfcType> {
        vec![IfcType::IfcFacetedBrep]
    }
}
