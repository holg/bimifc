// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Frustum and backface culling

use glam::Vec4;

/// Check if a triangle is completely outside the view frustum
/// Uses homogeneous clip coordinates
pub fn is_triangle_outside_frustum(v0: Vec4, v1: Vec4, v2: Vec4) -> bool {
    // Check each frustum plane
    // If all three vertices are outside the same plane, the triangle is culled

    // Left plane: x > -w
    if v0.x < -v0.w && v1.x < -v1.w && v2.x < -v2.w {
        return true;
    }
    // Right plane: x < w
    if v0.x > v0.w && v1.x > v1.w && v2.x > v2.w {
        return true;
    }
    // Bottom plane: y > -w
    if v0.y < -v0.w && v1.y < -v1.w && v2.y < -v2.w {
        return true;
    }
    // Top plane: y < w
    if v0.y > v0.w && v1.y > v1.w && v2.y > v2.w {
        return true;
    }
    // Near plane: z > -w (for RH, or z > 0 depending on convention)
    if v0.z < -v0.w && v1.z < -v1.w && v2.z < -v2.w {
        return true;
    }
    // Far plane: z < w
    if v0.z > v0.w && v1.z > v1.w && v2.z > v2.w {
        return true;
    }
    // Behind camera (w <= 0 for all)
    if v0.w <= 0.0 && v1.w <= 0.0 && v2.w <= 0.0 {
        return true;
    }

    false
}
