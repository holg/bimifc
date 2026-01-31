// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Wireframe/silhouette renderer for terminal display
//!
//! Uses pre-computed edges from Scene for fast rendering.

use super::framebuffer::Framebuffer;
use crate::camera::OrbitCamera;
use crate::scene::Scene;
use glam::Vec2;

/// Render statistics
#[derive(Default, Clone, Debug)]
pub struct WireframeStats {
    pub total_triangles: usize,
    pub total_edges: usize,
    pub silhouette_edges: usize,
    pub visible_edges: usize,
    pub pixels_drawn: usize,
}

/// Render the scene as wireframe/silhouette to the framebuffer
pub fn render_wireframe(
    framebuffer: &mut Framebuffer,
    scene: &Scene,
    camera: &OrbitCamera,
    _selected_entity: Option<u64>,
) -> WireframeStats {
    let mut stats = WireframeStats::default();
    stats.total_triangles = scene.triangles.len();
    stats.total_edges = scene.edges.len();

    framebuffer.clear();

    let vp = camera.view_projection_matrix();
    let camera_pos = camera.position();
    let w = framebuffer.width as f32;
    let h = framebuffer.height as f32;

    // Helper to project and draw a line
    let mut draw_edge = |v0: glam::Vec3, v1: glam::Vec3, depth_offset: f32| -> bool {
        let clip0 = vp * v0.extend(1.0);
        let clip1 = vp * v1.extend(1.0);

        if clip0.w <= 0.001 || clip1.w <= 0.001 {
            return false;
        }

        let ndc0 = clip0.truncate() / clip0.w;
        let ndc1 = clip1.truncate() / clip1.w;

        if (ndc0.x < -1.5 && ndc1.x < -1.5) || (ndc0.x > 1.5 && ndc1.x > 1.5) ||
           (ndc0.y < -1.5 && ndc1.y < -1.5) || (ndc0.y > 1.5 && ndc1.y > 1.5) {
            return false;
        }

        let p0 = Vec2::new((ndc0.x + 1.0) * 0.5 * w, (1.0 - ndc0.y) * 0.5 * h);
        let p1 = Vec2::new((ndc1.x + 1.0) * 0.5 * w, (1.0 - ndc1.y) * 0.5 * h);
        let depth = ((ndc0.z + ndc1.z) * 0.5 + 1.0) * 0.5 - depth_offset;

        draw_line(framebuffer, p0, p1, depth);
        true
    };

    // First draw bounding box to verify rendering works
    let min = scene.bounds_min;
    let max = scene.bounds_max;
    let corners = [
        glam::Vec3::new(min.x, min.y, min.z),
        glam::Vec3::new(max.x, min.y, min.z),
        glam::Vec3::new(max.x, max.y, min.z),
        glam::Vec3::new(min.x, max.y, min.z),
        glam::Vec3::new(min.x, min.y, max.z),
        glam::Vec3::new(max.x, min.y, max.z),
        glam::Vec3::new(max.x, max.y, max.z),
        glam::Vec3::new(min.x, max.y, max.z),
    ];
    // Draw bounding box edges (12 edges)
    let bb_edges = [
        (0,1), (1,2), (2,3), (3,0),  // bottom
        (4,5), (5,6), (6,7), (7,4),  // top
        (0,4), (1,5), (2,6), (3,7),  // verticals
    ];
    for (i, j) in bb_edges {
        draw_edge(corners[i], corners[j], 0.001);
    }

    // Now draw model edges
    if scene.edges.is_empty() {
        return stats;
    }

    for edge in &scene.edges {
        // Check if edge is roughly front-facing
        let dominated_by_backface = if !edge.face_normals.is_empty() {
            let edge_center = (edge.v0 + edge.v1) * 0.5;
            let view_dir = (camera_pos - edge_center).normalize_or_zero();
            edge.face_normals.iter().all(|n| n.dot(view_dir) < -0.1)
        } else {
            false
        };

        if dominated_by_backface {
            continue;
        }

        stats.silhouette_edges += 1;

        if draw_edge(edge.v0, edge.v1, 0.0) {
            stats.visible_edges += 1;
        }
    }

    stats
}

/// Draw a line using Bresenham's algorithm (optimized)
#[inline]
fn draw_line(fb: &mut Framebuffer, p0: Vec2, p1: Vec2, depth: f32) -> usize {
    let mut pixels = 0;

    let x0 = p0.x as i32;
    let y0 = p0.y as i32;
    let x1 = p1.x as i32;
    let y1 = p1.y as i32;

    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    let mut x = x0;
    let mut y = y0;

    let w = fb.width as i32;
    let h = fb.height as i32;

    let color = [220u8, 220, 220];
    let ch = '█';

    loop {
        if x >= 0 && x < w && y >= 0 && y < h {
            let idx = (y as usize) * fb.width + (x as usize);
            if depth < fb.depth[idx] {
                fb.depth[idx] = depth;
                fb.chars[idx] = ch;
                fb.char_colors[idx] = color;
                pixels += 1;
            }
        }

        if x == x1 && y == y1 {
            break;
        }

        let e2 = 2 * err;
        if e2 >= dy {
            if x == x1 { break; }
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            if y == y1 { break; }
            err += dx;
            y += sy;
        }
    }

    pixels
}
