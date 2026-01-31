// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Projection utilities for coordinate transforms

use glam::{Mat4, Vec3, Vec4};

/// Transform a point from world space through the MVP matrix to screen coordinates
#[inline]
pub fn world_to_screen(
    point: Vec3,
    mvp: &Mat4,
    screen_width: f32,
    screen_height: f32,
) -> Option<Vec3> {
    // Transform to clip space
    let clip = *mvp * point.extend(1.0);

    // Perspective divide (skip if behind camera)
    if clip.w <= 0.0 {
        return None;
    }

    let ndc = clip.truncate() / clip.w;

    // Check if in NDC range
    if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 || ndc.z < -1.0 || ndc.z > 1.0 {
        return None;
    }

    // NDC to screen
    let screen_x = (ndc.x + 1.0) * 0.5 * screen_width;
    let screen_y = (1.0 - ndc.y) * 0.5 * screen_height; // Flip Y
    let screen_z = ndc.z; // Depth for Z-buffer

    Some(Vec3::new(screen_x, screen_y, screen_z))
}

/// Transform a normal from world space to view space
#[inline]
pub fn transform_normal(normal: Vec3, view_matrix: &Mat4) -> Vec3 {
    // For normals, we use the inverse transpose of the upper-left 3x3
    // For orthonormal matrices (rotation only), this is the same as the matrix itself
    let n = *view_matrix * Vec4::new(normal.x, normal.y, normal.z, 0.0);
    Vec3::new(n.x, n.y, n.z).normalize()
}
