// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! RevolvedAreaSolid processor - rotates a 2D profile around an axis.

use crate::{Error, Mesh, Result, Vector3};
use bimifc_model::{DecodedEntity, EntityResolver, IfcType};
use nalgebra::{Point2, Point3};

use crate::router::GeometryProcessor;

/// RevolvedAreaSolid processor
///
/// Handles IfcRevolvedAreaSolid - rotates a 2D profile around an axis.
pub struct RevolvedAreaSolidProcessor;

impl RevolvedAreaSolidProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Extract 2D profile points
    fn extract_profile(
        &self,
        profile: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Result<Vec<Point2<f64>>> {
        match profile.ifc_type {
            IfcType::IfcRectangleProfileDef => {
                let x_dim = profile.get_float(3).unwrap_or(1.0);
                let y_dim = profile.get_float(4).unwrap_or(1.0);
                let hx = x_dim / 2.0;
                let hy = y_dim / 2.0;
                Ok(vec![
                    Point2::new(-hx, -hy),
                    Point2::new(hx, -hy),
                    Point2::new(hx, hy),
                    Point2::new(-hx, hy),
                ])
            }
            IfcType::IfcCircleProfileDef => {
                let radius = profile.get_float(3).unwrap_or(1.0);
                let segments = crate::profile::calculate_circle_segments(radius);
                let mut points = Vec::with_capacity(segments);
                for i in 0..segments {
                    let angle = 2.0 * std::f64::consts::PI * (i as f64) / (segments as f64);
                    points.push(Point2::new(radius * angle.cos(), radius * angle.sin()));
                }
                Ok(points)
            }
            IfcType::IfcArbitraryClosedProfileDef => {
                let curve_id = profile
                    .get_ref(2)
                    .ok_or_else(|| Error::invalid_attribute(2, "Missing OuterCurve"))?;

                let curve = resolver
                    .get(curve_id)
                    .ok_or_else(|| Error::entity_not_found(curve_id.0))?;

                self.extract_curve_2d_points(&curve, resolver)
            }
            _ => Err(Error::unsupported_type(format!("{:?}", profile.ifc_type))),
        }
    }

    fn extract_curve_2d_points(
        &self,
        curve: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Result<Vec<Point2<f64>>> {
        if curve.ifc_type == IfcType::IfcPolyline {
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
                            points.push(Point2::new(x, y));
                        }
                    }
                }
            }
            Ok(points)
        } else {
            Err(Error::unsupported_type(format!("{:?}", curve.ifc_type)))
        }
    }

    fn parse_axis_location(
        &self,
        axis: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Result<Point3<f64>> {
        let loc_id = axis
            .get_ref(0)
            .ok_or_else(|| Error::invalid_attribute(0, "Missing Location"))?;

        let loc = resolver
            .get(loc_id)
            .ok_or_else(|| Error::entity_not_found(loc_id.0))?;

        let coords = loc
            .get(0)
            .and_then(|v| v.as_list())
            .ok_or_else(|| Error::invalid_attribute(0, "Missing coordinates"))?;

        Ok(Point3::new(
            coords.first().and_then(|v| v.as_float()).unwrap_or(0.0),
            coords.get(1).and_then(|v| v.as_float()).unwrap_or(0.0),
            coords.get(2).and_then(|v| v.as_float()).unwrap_or(0.0),
        ))
    }

    fn parse_axis_direction(
        &self,
        axis: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Vector3<f64> {
        if let Some(dir_id) = axis.get_ref(1) {
            if let Some(dir) = resolver.get(dir_id) {
                if let Some(coords) = dir.get(0).and_then(|v| v.as_list()) {
                    return Vector3::new(
                        coords.first().and_then(|v| v.as_float()).unwrap_or(0.0),
                        coords.get(1).and_then(|v| v.as_float()).unwrap_or(1.0),
                        coords.get(2).and_then(|v| v.as_float()).unwrap_or(0.0),
                    )
                    .normalize();
                }
            }
        }
        Vector3::new(0.0, 1.0, 0.0) // Default Y axis
    }
}

impl Default for RevolvedAreaSolidProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl GeometryProcessor for RevolvedAreaSolidProcessor {
    fn process(
        &self,
        entity: &DecodedEntity,
        resolver: &dyn EntityResolver,
        _unit_scale: f64,
    ) -> Result<Mesh> {
        // IfcRevolvedAreaSolid attributes:
        // 0: SweptArea (IfcProfileDef)
        // 1: Position (IfcAxis2Placement3D)
        // 2: Axis (IfcAxis1Placement)
        // 3: Angle

        let profile_id = entity
            .get_ref(0)
            .ok_or_else(|| Error::invalid_attribute(0, "Missing SweptArea"))?;

        let profile = resolver
            .get(profile_id)
            .ok_or_else(|| Error::entity_not_found(profile_id.0))?;

        let axis_id = entity
            .get_ref(2)
            .ok_or_else(|| Error::invalid_attribute(2, "Missing Axis"))?;

        let axis = resolver
            .get(axis_id)
            .ok_or_else(|| Error::entity_not_found(axis_id.0))?;

        let angle = entity
            .get_float(3)
            .ok_or_else(|| Error::invalid_attribute(3, "Missing Angle"))?;

        let profile_points = self.extract_profile(&profile, resolver)?;
        if profile_points.is_empty() {
            return Ok(Mesh::new());
        }

        let axis_location = self.parse_axis_location(&axis, resolver)?;
        let axis_direction = self.parse_axis_direction(&axis, resolver);

        // Generate revolved mesh
        let full_circle = angle.abs() >= std::f64::consts::PI * 1.99;
        let segments = if full_circle {
            24
        } else {
            ((angle.abs() / std::f64::consts::PI * 12.0).ceil() as usize).max(4)
        };

        let num_profile_points = profile_points.len();
        let mut positions = Vec::new();
        let mut indices = Vec::new();

        let (ax, ay, az) = (axis_direction.x, axis_direction.y, axis_direction.z);

        for i in 0..=segments {
            let t = if full_circle && i == segments {
                0.0
            } else {
                angle * i as f64 / segments as f64
            };

            let cos_t = t.cos();
            let sin_t = t.sin();

            // Rodrigues' rotation formula helper
            let k_matrix = |v: Vector3<f64>| -> Vector3<f64> {
                Vector3::new(
                    ay * v.z - az * v.y,
                    az * v.x - ax * v.z,
                    ax * v.y - ay * v.x,
                )
            };

            for (j, p2d) in profile_points.iter().enumerate() {
                let radius = p2d.x;
                let height = p2d.y;

                let v = Vector3::new(radius, 0.0, 0.0);

                let k_cross_v = k_matrix(v);
                let k_dot_v = ax * v.x + ay * v.y + az * v.z;

                let v_rot =
                    v * cos_t + k_cross_v * sin_t + axis_direction * k_dot_v * (1.0 - cos_t);

                let pos = axis_location + axis_direction * height + v_rot;

                positions.push(pos.x as f32);
                positions.push(pos.y as f32);
                positions.push(pos.z as f32);

                if i < segments && j < num_profile_points - 1 {
                    let current = (i * num_profile_points + j) as u32;
                    let next_seg = ((i + 1) * num_profile_points + j) as u32;
                    let current_next = current + 1;
                    let next_seg_next = next_seg + 1;

                    indices.push(current);
                    indices.push(next_seg);
                    indices.push(next_seg_next);

                    indices.push(current);
                    indices.push(next_seg_next);
                    indices.push(current_next);
                }
            }
        }

        Ok(Mesh {
            positions,
            normals: Vec::new(),
            indices,
        })
    }

    fn supported_types(&self) -> Vec<IfcType> {
        vec![IfcType::IfcRevolvedAreaSolid]
    }
}
