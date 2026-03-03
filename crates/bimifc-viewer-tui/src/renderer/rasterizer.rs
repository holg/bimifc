// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Triangle rasterization with Z-buffer

use super::framebuffer::Framebuffer;
use glam::Vec3;

/// Triangle rasterizer
pub struct Rasterizer {
    width: usize,
    height: usize,
}

impl Rasterizer {
    /// Create new rasterizer for given dimensions
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }

    /// Rasterize a triangle to the framebuffer
    #[allow(clippy::too_many_arguments)]
    pub fn rasterize_triangle(
        &mut self,
        fb: &mut Framebuffer,
        v0: Vec3,
        v1: Vec3,
        v2: Vec3,
        normal: Vec3,
        color: [f32; 4],
        entity_id: u64,
    ) {
        // Compute bounding box
        let min_x = v0.x.min(v1.x).min(v2.x).max(0.0) as usize;
        let max_x = v0.x.max(v1.x).max(v2.x).min(self.width as f32 - 1.0) as usize;
        let min_y = v0.y.min(v1.y).min(v2.y).max(0.0) as usize;
        let max_y = v0.y.max(v1.y).max(v2.y).min(self.height as f32 - 1.0) as usize;

        // Skip degenerate triangles
        if min_x >= max_x || min_y >= max_y {
            return;
        }

        // Precompute edge functions for barycentric coordinates
        let denom = edge_function(v0.truncate(), v1.truncate(), v2.truncate());
        if denom.abs() < 1e-8 {
            return; // Degenerate triangle
        }
        let inv_denom = 1.0 / denom;

        // Iterate over bounding box
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let p = glam::Vec2::new(x as f32 + 0.5, y as f32 + 0.5);

                // Compute barycentric coordinates
                let w0 = edge_function(v1.truncate(), v2.truncate(), p) * inv_denom;
                let w1 = edge_function(v2.truncate(), v0.truncate(), p) * inv_denom;
                let w2 = edge_function(v0.truncate(), v1.truncate(), p) * inv_denom;

                // Check if point is inside triangle
                if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                    // Interpolate depth
                    let depth = w0 * v0.z + w1 * v1.z + w2 * v2.z;

                    // Depth test and write
                    fb.set_pixel(x, y, depth, color, normal, entity_id);
                }
            }
        }
    }
}

/// Edge function for barycentric coordinates
/// Returns positive if p is on the left side of the edge v0->v1
#[inline]
fn edge_function(v0: glam::Vec2, v1: glam::Vec2, p: glam::Vec2) -> f32 {
    (p.x - v0.x) * (v1.y - v0.y) - (p.y - v0.y) * (v1.x - v0.x)
}
