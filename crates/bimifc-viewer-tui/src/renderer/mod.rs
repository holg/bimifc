// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Software renderer for terminal display

pub mod culling;
pub mod floorplan;
pub mod framebuffer;
pub mod projection;
pub mod rasterizer;
pub mod shader;
pub mod wireframe;

pub use floorplan::{render_floorplan, FloorPlanStats, FloorPlanView};
pub use framebuffer::Framebuffer;
pub use rasterizer::Rasterizer;
pub use wireframe::{render_wireframe, WireframeStats};

use crate::camera::OrbitCamera;
use crate::scene::Scene;

/// Render the scene to the framebuffer
pub fn render(
    framebuffer: &mut Framebuffer,
    scene: &Scene,
    camera: &OrbitCamera,
    selected_entity: Option<u64>,
) -> RenderStats {
    let mut stats = RenderStats::default();
    stats.total_triangles = scene.triangles.len();

    // Clear framebuffer
    framebuffer.clear();

    // Get view-projection matrix
    let vp = camera.view_projection_matrix();
    let camera_pos = camera.position();

    // Create rasterizer
    let mut rasterizer = Rasterizer::new(framebuffer.width, framebuffer.height);

    // Debug file for first frame
    let mut debug_file = std::fs::File::create("/tmp/ifc-render-debug.log").ok();
    let mut debug_count = 0;

    // Write camera debug info
    if let Some(ref mut f) = debug_file {
        use std::io::Write;
        writeln!(f, "Camera pos: ({:.1}, {:.1}, {:.1})", camera_pos.x, camera_pos.y, camera_pos.z).ok();
        writeln!(f, "VP matrix:\n{:?}", vp).ok();
        writeln!(f, "Framebuffer: {}x{}", framebuffer.width, framebuffer.height).ok();
        writeln!(f, "Total triangles: {}", scene.triangles.len()).ok();
        if let Some(first_tri) = scene.triangles.first() {
            writeln!(f, "First tri world: v0=({:.1},{:.1},{:.1}), v1=({:.1},{:.1},{:.1}), v2=({:.1},{:.1},{:.1})",
                first_tri.v0.x, first_tri.v0.y, first_tri.v0.z,
                first_tri.v1.x, first_tri.v1.y, first_tri.v1.z,
                first_tri.v2.x, first_tri.v2.y, first_tri.v2.z).ok();
            let v0c = vp * first_tri.v0.extend(1.0);
            let v1c = vp * first_tri.v1.extend(1.0);
            let v2c = vp * first_tri.v2.extend(1.0);
            writeln!(f, "First tri clip: v0=({:.3},{:.3},{:.3},{:.3})", v0c.x, v0c.y, v0c.z, v0c.w).ok();
            writeln!(f, "First tri clip: v1=({:.3},{:.3},{:.3},{:.3})", v1c.x, v1c.y, v1c.z, v1c.w).ok();
            writeln!(f, "First tri clip: v2=({:.3},{:.3},{:.3},{:.3})", v2c.x, v2c.y, v2c.z, v2c.w).ok();

            // Check why frustum culled
            let outside = culling::is_triangle_outside_frustum(v0c, v1c, v2c);
            writeln!(f, "First tri frustum culled: {}", outside).ok();

            // Check each plane
            writeln!(f, "  Left (x < -w): v0={}, v1={}, v2={}", v0c.x < -v0c.w, v1c.x < -v1c.w, v2c.x < -v2c.w).ok();
            writeln!(f, "  Right (x > w): v0={}, v1={}, v2={}", v0c.x > v0c.w, v1c.x > v1c.w, v2c.x > v2c.w).ok();
            writeln!(f, "  Bottom (y < -w): v0={}, v1={}, v2={}", v0c.y < -v0c.w, v1c.y < -v1c.w, v2c.y < -v2c.w).ok();
            writeln!(f, "  Top (y > w): v0={}, v1={}, v2={}", v0c.y > v0c.w, v1c.y > v1c.w, v2c.y > v2c.w).ok();
            writeln!(f, "  Near (z < -w): v0={}, v1={}, v2={}", v0c.z < -v0c.w, v1c.z < -v1c.w, v2c.z < -v2c.w).ok();
            writeln!(f, "  Far (z > w): v0={}, v1={}, v2={}", v0c.z > v0c.w, v1c.z > v1c.w, v2c.z > v2c.w).ok();
            writeln!(f, "  Behind (w <= 0): v0={}, v1={}, v2={}", v0c.w <= 0.0, v1c.w <= 0.0, v2c.w <= 0.0).ok();
        }
        writeln!(f, "---").ok();
    }

    // Process each triangle
    for tri in &scene.triangles {

        // Backface culling - disabled for debugging
        // TODO: re-enable once rendering works
        // let view_dir = (camera_pos - tri.center()).normalize();
        // let facing = tri.normal.dot(view_dir);
        // if facing < -0.01 {
        //     stats.backface_culled += 1;
        //     continue;
        // }

        // Transform vertices to clip space
        let v0_clip = vp * tri.v0.extend(1.0);
        let v1_clip = vp * tri.v1.extend(1.0);
        let v2_clip = vp * tri.v2.extend(1.0);

        // Frustum culling (simple: check if all vertices are outside same plane)
        if culling::is_triangle_outside_frustum(v0_clip, v1_clip, v2_clip) {
            if debug_count < 5 {
                if let Some(ref mut f) = debug_file {
                    use std::io::Write;
                    writeln!(f, "Frustum culled tri: v0_clip=({},{},{},{})", v0_clip.x, v0_clip.y, v0_clip.z, v0_clip.w).ok();
                }
            }
            debug_count += 1;
            stats.frustum_culled += 1;
            continue;
        }

        // Skip if any vertex is behind camera
        if v0_clip.w <= 0.0 || v1_clip.w <= 0.0 || v2_clip.w <= 0.0 {
            if debug_count < 5 {
                if let Some(ref mut f) = debug_file {
                    use std::io::Write;
                    writeln!(f, "Behind camera: v0_clip.w={}, v1={}, v2={}", v0_clip.w, v1_clip.w, v2_clip.w).ok();
                }
            }
            debug_count += 1;
            stats.frustum_culled += 1;
            continue;
        }

        // Perspective divide to NDC [-1, 1]
        let v0_ndc = v0_clip.truncate() / v0_clip.w;
        let v1_ndc = v1_clip.truncate() / v1_clip.w;
        let v2_ndc = v2_clip.truncate() / v2_clip.w;

        // NDC to screen coordinates
        let w = framebuffer.width as f32;
        let h = framebuffer.height as f32;

        // Convert depth from NDC [-1, 1] to [0, 1] for depth buffer
        let v0_screen = glam::Vec3::new(
            (v0_ndc.x + 1.0) * 0.5 * w,
            (1.0 - v0_ndc.y) * 0.5 * h, // Flip Y for screen coords
            (v0_ndc.z + 1.0) * 0.5,     // NDC z to [0, 1]
        );
        let v1_screen = glam::Vec3::new(
            (v1_ndc.x + 1.0) * 0.5 * w,
            (1.0 - v1_ndc.y) * 0.5 * h,
            (v1_ndc.z + 1.0) * 0.5,
        );
        let v2_screen = glam::Vec3::new(
            (v2_ndc.x + 1.0) * 0.5 * w,
            (1.0 - v2_ndc.y) * 0.5 * h,
            (v2_ndc.z + 1.0) * 0.5,
        );

        // Skip sub-pixel triangles (very low threshold for terminal rendering)
        // For terminal, we want to render triangles that cover any significant portion of a cell
        let area = triangle_area_2d(v0_screen.truncate(), v1_screen.truncate(), v2_screen.truncate());
        if area < 0.001 {
            stats.subpixel_culled += 1;
            continue;
        }

        // Determine color (highlight selected)
        let color = if Some(tri.entity_id) == selected_entity {
            [0.3, 0.7, 1.0, 1.0] // Light blue highlight
        } else {
            tri.color
        };

        // Rasterize
        rasterizer.rasterize_triangle(
            framebuffer,
            v0_screen,
            v1_screen,
            v2_screen,
            tri.normal,
            color,
            tri.entity_id,
        );

        stats.rendered += 1;
    }

    // Count pixels written (depth < 1.0)
    stats.pixels_written = framebuffer.depth.iter().filter(|&&d| d < 1.0).count();

    // Convert depth/color to characters
    shader::shade_framebuffer(framebuffer, camera_pos);

    stats
}

/// Calculate 2D triangle area (for culling small triangles)
fn triangle_area_2d(v0: glam::Vec2, v1: glam::Vec2, v2: glam::Vec2) -> f32 {
    let a = v1 - v0;
    let b = v2 - v0;
    (a.x * b.y - a.y * b.x).abs() * 0.5
}

/// Render statistics
#[derive(Default, Clone, Debug)]
pub struct RenderStats {
    pub total_triangles: usize,
    pub rendered: usize,
    pub backface_culled: usize,
    pub frustum_culled: usize,
    pub subpixel_culled: usize,
    pub pixels_written: usize,
}

impl RenderStats {
    pub fn culled(&self) -> usize {
        self.backface_culled + self.frustum_culled + self.subpixel_culled
    }
}
