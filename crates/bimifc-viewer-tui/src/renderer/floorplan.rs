// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! 2D Floor plan renderer for terminal display
//!
//! Shows a top-down orthographic view of the model, suitable for terminal resolution.

use super::framebuffer::Framebuffer;
use crate::scene::Scene;
use glam::{Vec2, Vec3};

/// Floor plan render statistics
#[derive(Default, Clone, Debug)]
pub struct FloorPlanStats {
    pub total_edges: usize,
    pub visible_edges: usize,
    pub pixels_drawn: usize,
    pub current_level: i32,
    pub level_count: i32,
}

/// Floor plan view settings
pub struct FloorPlanView {
    /// Current Y level (height) to slice at
    pub slice_y: f32,
    /// Thickness of the slice (edges within this range are shown)
    pub slice_thickness: f32,
    /// Zoom level (1.0 = fit to screen)
    pub zoom: f32,
    /// Pan offset in world units
    pub pan: Vec2,
}

impl Default for FloorPlanView {
    fn default() -> Self {
        Self {
            slice_y: 0.0,
            slice_thickness: 3.0, // 3 meters thick slice
            zoom: 1.0,
            pan: Vec2::ZERO,
        }
    }
}

impl FloorPlanView {
    /// Move to next floor level
    pub fn next_level(&mut self, scene: &Scene) {
        self.slice_y += 3.0; // Assume ~3m floor height
        self.slice_y = self.slice_y.min(scene.bounds_max.y - 0.5);
    }

    /// Move to previous floor level
    pub fn prev_level(&mut self, scene: &Scene) {
        self.slice_y -= 3.0;
        self.slice_y = self.slice_y.max(scene.bounds_min.y + 0.5);
    }

    /// Fit view to scene bounds
    pub fn fit_to_scene(&mut self, scene: &Scene) {
        self.slice_y = (scene.bounds_min.y + scene.bounds_max.y) * 0.5;
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
    }

    /// Zoom in
    pub fn zoom_in(&mut self) {
        self.zoom *= 1.2;
    }

    /// Zoom out
    pub fn zoom_out(&mut self) {
        self.zoom = (self.zoom / 1.2).max(0.1);
    }

    /// Pan the view
    pub fn pan_by(&mut self, dx: f32, dy: f32, scene: &Scene) {
        let range_x = scene.bounds_max.x - scene.bounds_min.x;
        let range_z = scene.bounds_max.z - scene.bounds_min.z;
        self.pan.x += dx * range_x * 0.05 / self.zoom;
        self.pan.y += dy * range_z * 0.05 / self.zoom;
    }
}

/// Render the scene as a 2D floor plan
pub fn render_floorplan(
    framebuffer: &mut Framebuffer,
    scene: &Scene,
    view: &FloorPlanView,
) -> FloorPlanStats {
    let mut stats = FloorPlanStats {
        total_edges: scene.edges.len(),
        ..Default::default()
    };

    framebuffer.clear();

    if scene.edges.is_empty() {
        return stats;
    }

    let w = framebuffer.width as f32;
    let h = framebuffer.height as f32;

    // Calculate bounds for the current slice level
    let min_y = view.slice_y - view.slice_thickness * 0.5;
    let max_y = view.slice_y + view.slice_thickness * 0.5;

    // Calculate level info
    let total_height = scene.bounds_max.y - scene.bounds_min.y;
    stats.level_count = (total_height / 3.0).ceil() as i32;
    stats.current_level = ((view.slice_y - scene.bounds_min.y) / 3.0) as i32;

    // Calculate view transform (world XZ -> screen XY)
    // Center on scene center with pan offset
    let center_x = (scene.bounds_min.x + scene.bounds_max.x) * 0.5 + view.pan.x;
    let center_z = (scene.bounds_min.z + scene.bounds_max.z) * 0.5 + view.pan.y;

    // Scale to fit screen (with some margin)
    let range_x = scene.bounds_max.x - scene.bounds_min.x;
    let range_z = scene.bounds_max.z - scene.bounds_min.z;
    let margin = 0.9; // Use 90% of screen

    // Account for character aspect ratio (chars are ~2x tall)
    let aspect_correction = 2.0;
    let scale_x = (w * margin) / range_x * view.zoom;
    let scale_z = (h * margin * aspect_correction) / range_z * view.zoom;
    let scale = scale_x.min(scale_z);

    // Transform world XZ to screen XY
    let world_to_screen = |world: Vec3| -> Vec2 {
        let sx = (world.x - center_x) * scale + w * 0.5;
        let sy = (center_z - world.z) * scale / aspect_correction + h * 0.5; // Flip Z for screen Y
        Vec2::new(sx, sy)
    };

    // Draw edges that intersect the current slice
    for edge in &scene.edges {
        // Check if edge is within the Y slice
        let edge_min_y = edge.v0.y.min(edge.v1.y);
        let edge_max_y = edge.v0.y.max(edge.v1.y);

        // Skip edges completely above or below the slice
        if edge_max_y < min_y || edge_min_y > max_y {
            continue;
        }

        // Project to 2D (ignore Y)
        let p0 = world_to_screen(edge.v0);
        let p1 = world_to_screen(edge.v1);

        // Skip if completely off screen
        if (p0.x < -10.0 && p1.x < -10.0)
            || (p0.x > w + 10.0 && p1.x > w + 10.0)
            || (p0.y < -10.0 && p1.y < -10.0)
            || (p0.y > h + 10.0 && p1.y > h + 10.0)
        {
            continue;
        }

        // Calculate intensity based on how much of the edge is in the slice
        let in_slice_ratio = if edge_max_y - edge_min_y < 0.01 {
            1.0 // Horizontal edge
        } else {
            let overlap_min = edge_min_y.max(min_y);
            let overlap_max = edge_max_y.min(max_y);
            ((overlap_max - overlap_min) / (edge_max_y - edge_min_y)).clamp(0.0, 1.0)
        };

        // Choose character based on intensity
        let ch = if in_slice_ratio > 0.7 {
            '█'
        } else if in_slice_ratio > 0.3 {
            '▓'
        } else {
            '░'
        };

        let intensity = (in_slice_ratio * 200.0 + 55.0) as u8;
        let color = [intensity, intensity, intensity];

        stats.pixels_drawn += draw_line_2d(framebuffer, p0, p1, color, ch);
        stats.visible_edges += 1;
    }

    // Draw bounding box outline
    let bb_color = [60u8, 60, 80];
    let corners = [
        world_to_screen(Vec3::new(scene.bounds_min.x, 0.0, scene.bounds_min.z)),
        world_to_screen(Vec3::new(scene.bounds_max.x, 0.0, scene.bounds_min.z)),
        world_to_screen(Vec3::new(scene.bounds_max.x, 0.0, scene.bounds_max.z)),
        world_to_screen(Vec3::new(scene.bounds_min.x, 0.0, scene.bounds_max.z)),
    ];
    draw_line_2d(framebuffer, corners[0], corners[1], bb_color, '·');
    draw_line_2d(framebuffer, corners[1], corners[2], bb_color, '·');
    draw_line_2d(framebuffer, corners[2], corners[3], bb_color, '·');
    draw_line_2d(framebuffer, corners[3], corners[0], bb_color, '·');

    stats
}

/// Draw a 2D line (no depth testing needed)
fn draw_line_2d(fb: &mut Framebuffer, p0: Vec2, p1: Vec2, color: [u8; 3], ch: char) -> usize {
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

    loop {
        if x >= 0 && x < w && y >= 0 && y < h {
            let idx = (y as usize) * fb.width + (x as usize);
            // Only draw if cell is empty (background) or we're brighter
            if fb.chars[idx] == '·'
                || fb.chars[idx] == ' '
                || (fb.char_colors[idx][0] < color[0] && ch != '·')
            {
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
            if x == x1 {
                break;
            }
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            if y == y1 {
                break;
            }
            err += dx;
            y += sy;
        }
    }

    pixels
}
