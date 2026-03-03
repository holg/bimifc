// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Transform resolution for IFC placement chains
//!
//! Resolves IFC placement entities (IfcLocalPlacement, IfcAxis2Placement3D,
//! IfcCartesianTransformationOperator3D) into 4x4 transformation matrices.

use bimifc_model::{EntityId, EntityResolver, IfcType};
use nalgebra::Matrix4;

/// Resolve a placement to a transformation matrix.
///
/// For IfcLocalPlacement, recursively resolves the parent placement chain:
/// `PlacementRelTo → parent transform * RelativePlacement → local transform`
///
/// Note: Translation components are in file units. The caller should apply
/// unit_scale to the translation after retrieving the transform.
pub fn resolve_placement(
    placement_id: EntityId,
    resolver: &dyn EntityResolver,
) -> Option<Matrix4<f64>> {
    let placement = resolver.get(placement_id)?;

    match placement.ifc_type {
        IfcType::IfcLocalPlacement => {
            // Recursively resolve parent placement (attribute 0: PlacementRelTo)
            let parent_transform = placement
                .get_ref(0)
                .and_then(|parent_id| resolve_placement(parent_id, resolver))
                .unwrap_or_else(Matrix4::identity);

            // Resolve local transform (attribute 1: RelativePlacement)
            let local_transform = placement
                .get_ref(1)
                .and_then(|rel_id| resolve_axis_placement(rel_id, resolver))
                .unwrap_or_else(Matrix4::identity);

            Some(parent_transform * local_transform)
        }
        IfcType::IfcAxis2Placement3D => resolve_axis_placement(placement_id, resolver),
        IfcType::IfcCartesianTransformationOperator3D
        | IfcType::IfcCartesianTransformationOperator3DnonUniform => {
            resolve_transformation_operator(placement_id, resolver)
        }
        _ => None,
    }
}

/// Resolve an IfcAxis2Placement3D to a transformation matrix.
pub fn resolve_axis_placement(
    placement_id: EntityId,
    resolver: &dyn EntityResolver,
) -> Option<Matrix4<f64>> {
    let placement = resolver.get(placement_id)?;

    if placement.ifc_type != IfcType::IfcAxis2Placement3D {
        return None;
    }

    // Location (index 0)
    let location = resolve_cartesian_point(placement.get_ref(0)?, resolver)?;

    // Axis (index 1) - Z direction, optional
    let axis = placement
        .get_ref(1)
        .and_then(|id| resolve_direction(id, resolver))
        .unwrap_or_else(|| nalgebra::Vector3::new(0.0, 0.0, 1.0));

    // RefDirection (index 2) - X direction, optional
    let ref_dir = placement
        .get_ref(2)
        .and_then(|id| resolve_direction(id, resolver))
        .unwrap_or_else(|| nalgebra::Vector3::new(1.0, 0.0, 0.0));

    // Build orthonormal basis
    let z = axis.normalize();
    let x = ref_dir.normalize();
    let y = z.cross(&x).normalize();
    let x = y.cross(&z).normalize();

    Some(Matrix4::new(
        x.x, y.x, z.x, location.x, x.y, y.y, z.y, location.y, x.z, y.z, z.z, location.z, 0.0, 0.0,
        0.0, 1.0,
    ))
}

/// Resolve a CartesianTransformationOperator3D to a transformation matrix.
pub fn resolve_transformation_operator(
    op_id: EntityId,
    resolver: &dyn EntityResolver,
) -> Option<Matrix4<f64>> {
    let op = resolver.get(op_id)?;

    // LocalOrigin (index 3)
    let origin = resolve_cartesian_point(op.get_ref(3)?, resolver)?;

    // Scale (index 6), default 1.0
    let scale = op.get_float(6).unwrap_or(1.0);

    // Build matrix with translation and uniform scale
    let mut matrix = Matrix4::identity();
    matrix[(0, 0)] = scale;
    matrix[(1, 1)] = scale;
    matrix[(2, 2)] = scale;
    matrix[(0, 3)] = origin.x;
    matrix[(1, 3)] = origin.y;
    matrix[(2, 3)] = origin.z;

    Some(matrix)
}

/// Resolve an IfcCartesianPoint to a Point3.
pub fn resolve_cartesian_point(
    point_id: EntityId,
    resolver: &dyn EntityResolver,
) -> Option<nalgebra::Point3<f64>> {
    let point = resolver.get(point_id)?;

    if point.ifc_type != IfcType::IfcCartesianPoint {
        return None;
    }

    // Coordinates at index 0
    let coords = point.get(0)?.as_list()?;

    let x = coords.first().and_then(|v| v.as_float()).unwrap_or(0.0);
    let y = coords.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
    let z = coords.get(2).and_then(|v| v.as_float()).unwrap_or(0.0);

    Some(nalgebra::Point3::new(x, y, z))
}

/// Resolve an IfcDirection to a Vector3.
pub fn resolve_direction(
    dir_id: EntityId,
    resolver: &dyn EntityResolver,
) -> Option<nalgebra::Vector3<f64>> {
    let direction = resolver.get(dir_id)?;

    if direction.ifc_type != IfcType::IfcDirection {
        return None;
    }

    // DirectionRatios at index 0
    let ratios = direction.get(0)?.as_list()?;

    let x = ratios.first().and_then(|v| v.as_float()).unwrap_or(0.0);
    let y = ratios.get(1).and_then(|v| v.as_float()).unwrap_or(0.0);
    let z = ratios.get(2).and_then(|v| v.as_float()).unwrap_or(1.0);

    Some(nalgebra::Vector3::new(x, y, z))
}
