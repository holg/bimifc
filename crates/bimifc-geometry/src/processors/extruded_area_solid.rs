// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! ExtrudedAreaSolid processor - the most common IFC geometry type.
//! Extrudes 2D profiles along a direction vector.

use crate::{
    extrusion::{apply_transform, extrude_profile},
    profile::Profile2D,
    Error, Mesh, Result, Vector3,
};
use bimifc_model::{DecodedEntity, EntityId, EntityResolver, IfcType};
use nalgebra::{Matrix4, Point2};

use crate::router::GeometryProcessor;

/// ExtrudedAreaSolid processor
///
/// Handles IfcExtrudedAreaSolid - the most common IFC geometry type.
/// Extrudes 2D profiles along a direction vector.
pub struct ExtrudedAreaSolidProcessor;

impl ExtrudedAreaSolidProcessor {
    /// Create new processor
    pub fn new() -> Self {
        Self
    }

    /// Extract a 2D profile from an IFC profile definition
    fn extract_profile(
        &self,
        profile_entity: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Result<Profile2D> {
        match profile_entity.ifc_type {
            IfcType::IfcRectangleProfileDef => self.extract_rectangle_profile(profile_entity),
            IfcType::IfcCircleProfileDef => self.extract_circle_profile(profile_entity),
            IfcType::IfcCircleHollowProfileDef => {
                self.extract_circle_hollow_profile(profile_entity)
            }
            IfcType::IfcArbitraryClosedProfileDef => {
                self.extract_arbitrary_profile(profile_entity, resolver)
            }
            IfcType::IfcArbitraryProfileDefWithVoids => {
                self.extract_arbitrary_profile_with_voids(profile_entity, resolver)
            }
            IfcType::IfcIShapeProfileDef => self.extract_i_shape_profile(profile_entity),
            IfcType::IfcLShapeProfileDef => self.extract_l_shape_profile(profile_entity),
            IfcType::IfcTShapeProfileDef => self.extract_t_shape_profile(profile_entity),
            _ => Err(Error::unsupported_type(format!(
                "Profile type {:?}",
                profile_entity.ifc_type
            ))),
        }
    }

    /// Extract rectangle profile
    fn extract_rectangle_profile(&self, entity: &DecodedEntity) -> Result<Profile2D> {
        // XDim at index 3, YDim at index 4
        let x_dim = entity
            .get_float(3)
            .ok_or_else(|| Error::invalid_attribute(3, "Missing XDim"))?;
        let y_dim = entity
            .get_float(4)
            .ok_or_else(|| Error::invalid_attribute(4, "Missing YDim"))?;

        Ok(Profile2D::rectangle(x_dim, y_dim))
    }

    /// Extract circle profile
    fn extract_circle_profile(&self, entity: &DecodedEntity) -> Result<Profile2D> {
        // Radius at index 3
        let radius = entity
            .get_float(3)
            .ok_or_else(|| Error::invalid_attribute(3, "Missing Radius"))?;

        Ok(Profile2D::circle(radius, None))
    }

    /// Extract hollow circle profile
    fn extract_circle_hollow_profile(&self, entity: &DecodedEntity) -> Result<Profile2D> {
        // Radius at index 3, WallThickness at index 4
        let radius = entity
            .get_float(3)
            .ok_or_else(|| Error::invalid_attribute(3, "Missing Radius"))?;
        let wall_thickness = entity
            .get_float(4)
            .ok_or_else(|| Error::invalid_attribute(4, "Missing WallThickness"))?;

        let inner_radius = radius - wall_thickness;
        if inner_radius <= 0.0 {
            return Err(Error::geometry("Invalid hollow circle: inner radius <= 0"));
        }

        // Create outer circle
        let mut profile = Profile2D::circle(radius, None);

        // Add inner circle as hole
        let segments = crate::profile::calculate_circle_segments(inner_radius);
        let mut hole = Vec::with_capacity(segments);
        for i in 0..segments {
            let angle = 2.0 * std::f64::consts::PI * (i as f64) / (segments as f64);
            hole.push(Point2::new(
                inner_radius * angle.cos(),
                inner_radius * angle.sin(),
            ));
        }
        hole.reverse(); // Clockwise for hole
        profile.add_hole(hole);

        Ok(profile)
    }

    /// Extract arbitrary closed profile
    fn extract_arbitrary_profile(
        &self,
        entity: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Result<Profile2D> {
        // OuterCurve at index 2
        let curve_id = entity
            .get_ref(2)
            .ok_or_else(|| Error::invalid_attribute(2, "Missing OuterCurve"))?;

        let points = self.extract_polyline_points(curve_id, resolver)?;
        if points.len() < 3 {
            return Err(Error::profile("Profile must have at least 3 points"));
        }

        Ok(Profile2D::new(points))
    }

    /// Extract arbitrary profile with voids (holes)
    fn extract_arbitrary_profile_with_voids(
        &self,
        entity: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Result<Profile2D> {
        // OuterCurve at index 2
        let outer_curve_id = entity
            .get_ref(2)
            .ok_or_else(|| Error::invalid_attribute(2, "Missing OuterCurve"))?;

        let outer_points = self.extract_polyline_points(outer_curve_id, resolver)?;
        if outer_points.len() < 3 {
            return Err(Error::profile("Outer profile must have at least 3 points"));
        }

        let mut profile = Profile2D::new(outer_points);

        // InnerCurves at index 3
        if let Some(inner_curves) = entity.get(3) {
            if let Some(list) = inner_curves.as_list() {
                for curve_ref in list {
                    if let Some(curve_id) = curve_ref.as_entity_ref() {
                        if let Ok(hole_points) = self.extract_polyline_points(curve_id, resolver) {
                            if hole_points.len() >= 3 {
                                profile.add_hole(hole_points);
                            }
                        }
                    }
                }
            }
        }

        Ok(profile)
    }

    /// Extract I-shape profile
    fn extract_i_shape_profile(&self, entity: &DecodedEntity) -> Result<Profile2D> {
        // Attributes: OverallWidth(3), OverallDepth(4), WebThickness(5), FlangeThickness(6)
        let width = entity
            .get_float(3)
            .ok_or_else(|| Error::invalid_attribute(3, "Missing OverallWidth"))?;
        let depth = entity
            .get_float(4)
            .ok_or_else(|| Error::invalid_attribute(4, "Missing OverallDepth"))?;
        let web_thickness = entity
            .get_float(5)
            .ok_or_else(|| Error::invalid_attribute(5, "Missing WebThickness"))?;
        let flange_thickness = entity
            .get_float(6)
            .ok_or_else(|| Error::invalid_attribute(6, "Missing FlangeThickness"))?;

        // Create I-shape profile
        let hw = width / 2.0;
        let hd = depth / 2.0;
        let hwt = web_thickness / 2.0;
        let ft = flange_thickness;

        let points = vec![
            Point2::new(-hw, -hd),
            Point2::new(hw, -hd),
            Point2::new(hw, -hd + ft),
            Point2::new(hwt, -hd + ft),
            Point2::new(hwt, hd - ft),
            Point2::new(hw, hd - ft),
            Point2::new(hw, hd),
            Point2::new(-hw, hd),
            Point2::new(-hw, hd - ft),
            Point2::new(-hwt, hd - ft),
            Point2::new(-hwt, -hd + ft),
            Point2::new(-hw, -hd + ft),
        ];

        Ok(Profile2D::new(points))
    }

    /// Extract L-shape profile
    fn extract_l_shape_profile(&self, entity: &DecodedEntity) -> Result<Profile2D> {
        // Attributes: Depth(3), Width(4), Thickness(5)
        let depth = entity
            .get_float(3)
            .ok_or_else(|| Error::invalid_attribute(3, "Missing Depth"))?;
        let width = entity
            .get_float(4)
            .ok_or_else(|| Error::invalid_attribute(4, "Missing Width"))?;
        let thickness = entity
            .get_float(5)
            .ok_or_else(|| Error::invalid_attribute(5, "Missing Thickness"))?;

        let points = vec![
            Point2::new(0.0, 0.0),
            Point2::new(width, 0.0),
            Point2::new(width, thickness),
            Point2::new(thickness, thickness),
            Point2::new(thickness, depth),
            Point2::new(0.0, depth),
        ];

        Ok(Profile2D::new(points))
    }

    /// Extract T-shape profile
    fn extract_t_shape_profile(&self, entity: &DecodedEntity) -> Result<Profile2D> {
        // Attributes: Depth(3), FlangeWidth(4), WebThickness(5), FlangeThickness(6)
        let depth = entity
            .get_float(3)
            .ok_or_else(|| Error::invalid_attribute(3, "Missing Depth"))?;
        let flange_width = entity
            .get_float(4)
            .ok_or_else(|| Error::invalid_attribute(4, "Missing FlangeWidth"))?;
        let web_thickness = entity
            .get_float(5)
            .ok_or_else(|| Error::invalid_attribute(5, "Missing WebThickness"))?;
        let flange_thickness = entity
            .get_float(6)
            .ok_or_else(|| Error::invalid_attribute(6, "Missing FlangeThickness"))?;

        let hfw = flange_width / 2.0;
        let hwt = web_thickness / 2.0;

        let points = vec![
            Point2::new(-hfw, 0.0),
            Point2::new(hfw, 0.0),
            Point2::new(hfw, flange_thickness),
            Point2::new(hwt, flange_thickness),
            Point2::new(hwt, depth),
            Point2::new(-hwt, depth),
            Point2::new(-hwt, flange_thickness),
            Point2::new(-hfw, flange_thickness),
        ];

        Ok(Profile2D::new(points))
    }

    /// Extract points from a polyline
    fn extract_polyline_points(
        &self,
        curve_id: EntityId,
        resolver: &dyn EntityResolver,
    ) -> Result<Vec<Point2<f64>>> {
        let curve = resolver
            .get(curve_id)
            .ok_or_else(|| Error::entity_not_found(curve_id.0))?;

        match curve.ifc_type {
            IfcType::IfcPolyline => {
                // Points at index 0
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

                // Remove duplicate last point if closed
                if points.len() > 1 {
                    let first = points.first().unwrap();
                    let last = points.last().unwrap();
                    if (first.x - last.x).abs() < 1e-10 && (first.y - last.y).abs() < 1e-10 {
                        points.pop();
                    }
                }

                Ok(points)
            }
            IfcType::IfcIndexedPolyCurve => {
                // Handle indexed poly curve
                self.extract_indexed_poly_curve_points(&curve, resolver)
            }
            _ => Err(Error::unsupported_type(format!(
                "Curve type {:?}",
                curve.ifc_type
            ))),
        }
    }

    /// Extract points from an indexed poly curve
    fn extract_indexed_poly_curve_points(
        &self,
        curve: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Result<Vec<Point2<f64>>> {
        // Points at index 0 (IfcCartesianPointList2D)
        let points_ref_id = curve
            .get_ref(0)
            .ok_or_else(|| Error::invalid_attribute(0, "Missing Points"))?;

        let points_list = resolver
            .get(points_ref_id)
            .ok_or_else(|| Error::entity_not_found(points_ref_id.0))?;

        // CoordList at index 0
        let coords = points_list
            .get(0)
            .and_then(|v| v.as_list())
            .ok_or_else(|| Error::invalid_attribute(0, "Missing CoordList"))?;

        let mut points = Vec::with_capacity(coords.len());
        for coord in coords {
            if let Some(coord_list) = coord.as_list() {
                let x = coord_list.first().and_then(|v| v.as_float()).unwrap_or(0.0);
                let y = coord_list.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
                points.push(Point2::new(x, y));
            }
        }

        Ok(points)
    }

    /// Extract IfcAxis2Placement3D transform
    fn extract_position_transform(
        &self,
        position_id: EntityId,
        resolver: &dyn EntityResolver,
    ) -> Option<Matrix4<f64>> {
        let position = resolver.get(position_id)?;

        if position.ifc_type != IfcType::IfcAxis2Placement3D {
            return None;
        }

        // Location (index 0)
        let location_id = position.get_ref(0)?;
        let location = resolver.get(location_id)?;
        let coords = location.get(0)?.as_list()?;

        let x = coords.first().and_then(|v| v.as_float()).unwrap_or(0.0);
        let y = coords.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
        let z = coords.get(2).and_then(|v| v.as_float()).unwrap_or(0.0);

        // Axis (index 1) - Z direction
        let axis = position
            .get_ref(1)
            .and_then(|id| self.extract_direction(id, resolver))
            .unwrap_or_else(|| Vector3::new(0.0, 0.0, 1.0));

        // RefDirection (index 2) - X direction
        let ref_dir = position
            .get_ref(2)
            .and_then(|id| self.extract_direction(id, resolver))
            .unwrap_or_else(|| Vector3::new(1.0, 0.0, 0.0));

        // Build orthonormal basis
        let z_axis = axis.normalize();
        let x_axis = ref_dir.normalize();
        let y_axis = z_axis.cross(&x_axis).normalize();
        let x_axis = y_axis.cross(&z_axis).normalize();

        Some(Matrix4::new(
            x_axis.x, y_axis.x, z_axis.x, x, x_axis.y, y_axis.y, z_axis.y, y, x_axis.z, y_axis.z,
            z_axis.z, z, 0.0, 0.0, 0.0, 1.0,
        ))
    }

    /// Extract direction vector
    fn extract_direction(
        &self,
        dir_id: EntityId,
        resolver: &dyn EntityResolver,
    ) -> Option<Vector3<f64>> {
        let direction = resolver.get(dir_id)?;
        let ratios = direction.get(0)?.as_list()?;

        let x = ratios.first().and_then(|v| v.as_float()).unwrap_or(0.0);
        let y = ratios.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
        let z = ratios.get(2).and_then(|v| v.as_float()).unwrap_or(1.0);

        Some(Vector3::new(x, y, z))
    }
}

impl Default for ExtrudedAreaSolidProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl GeometryProcessor for ExtrudedAreaSolidProcessor {
    fn process(
        &self,
        entity: &DecodedEntity,
        resolver: &dyn EntityResolver,
        _unit_scale: f64,
    ) -> Result<Mesh> {
        // IfcExtrudedAreaSolid attributes:
        // 0: SweptArea (IfcProfileDef)
        // 1: Position (IfcAxis2Placement3D)
        // 2: ExtrudedDirection (IfcDirection)
        // 3: Depth (IfcPositiveLengthMeasure)

        // Get profile
        let profile_id = entity
            .get_ref(0)
            .ok_or_else(|| Error::invalid_attribute(0, "Missing SweptArea"))?;

        let profile_entity = resolver
            .get(profile_id)
            .ok_or_else(|| Error::entity_not_found(profile_id.0))?;

        let profile = self.extract_profile(&profile_entity, resolver)?;

        if profile.outer.is_empty() {
            return Ok(Mesh::new());
        }

        // Get extrusion direction
        let direction_id = entity
            .get_ref(2)
            .ok_or_else(|| Error::invalid_attribute(2, "Missing ExtrudedDirection"))?;

        let direction_entity = resolver
            .get(direction_id)
            .ok_or_else(|| Error::entity_not_found(direction_id.0))?;

        let ratios = direction_entity
            .get(0)
            .and_then(|v| v.as_list())
            .ok_or_else(|| Error::invalid_attribute(0, "Missing direction ratios"))?;

        let dir_x = ratios.first().and_then(|v| v.as_float()).unwrap_or(0.0);
        let dir_y = ratios.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
        let dir_z = ratios.get(2).and_then(|v| v.as_float()).unwrap_or(1.0);

        let direction = Vector3::new(dir_x, dir_y, dir_z).normalize();

        // Get depth
        let depth = entity
            .get_float(3)
            .ok_or_else(|| Error::invalid_attribute(3, "Missing Depth"))?;

        // Determine transform based on extrusion direction
        let extrusion_transform = if direction.x.abs() < 0.001 && direction.y.abs() < 0.001 {
            // Extrusion along Z axis (common case)
            if direction.z < 0.0 {
                // Negative Z - shift down
                Some(Matrix4::new_translation(&Vector3::new(0.0, 0.0, -depth)))
            } else {
                None
            }
        } else {
            // Non-Z-aligned extrusion - compute rotation
            let z_axis = Vector3::new(0.0, 0.0, 1.0);
            let rotation_axis = z_axis.cross(&direction);
            let rotation_angle = z_axis.dot(&direction).acos();

            if rotation_axis.norm() > 1e-10 {
                Some(Matrix4::new_rotation(
                    rotation_axis.normalize() * rotation_angle,
                ))
            } else {
                None
            }
        };

        // Extrude profile
        let mut mesh = extrude_profile(&profile, depth, extrusion_transform)?;

        // Apply position transform
        if let Some(position_id) = entity.get_ref(1) {
            if let Some(transform) = self.extract_position_transform(position_id, resolver) {
                apply_transform(&mut mesh, &transform);
            }
        }

        Ok(mesh)
    }

    fn supported_types(&self) -> Vec<IfcType> {
        vec![IfcType::IfcExtrudedAreaSolid]
    }
}
