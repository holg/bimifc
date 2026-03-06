// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! 3D viewport widget using ratatui Canvas with braille markers

use crate::renderer::floorplan::FloorPlanView;
use crate::renderer::Framebuffer;
use crate::scene::Scene;
use glam::Vec3;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    symbols::Marker,
    widgets::{
        canvas::{Canvas, Line as CanvasLine},
        Block, Borders, Widget,
    },
};

/// View mode for the viewport
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// 3D isometric wireframe (braille canvas)
    #[default]
    Iso3D,
    /// 2D floor plan slice (braille canvas)
    FloorPlan,
    /// Polar photometric diagram (braille canvas)
    Polar,
    /// Legacy block-char floor plan
    BlockChar,
}

impl ViewMode {
    pub fn next(self) -> Self {
        match self {
            ViewMode::Iso3D => ViewMode::FloorPlan,
            ViewMode::FloorPlan => ViewMode::Polar,
            ViewMode::Polar => ViewMode::BlockChar,
            ViewMode::BlockChar => ViewMode::Iso3D,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Iso3D => "3D Iso",
            ViewMode::FloorPlan => "Floor Plan",
            ViewMode::Polar => "Polar",
            ViewMode::BlockChar => "Block",
        }
    }
}

/// Orbit camera angles for 3D view
#[derive(Clone, Copy, Debug)]
pub struct OrbitAngles {
    /// Azimuth angle in radians (rotation around Y axis)
    pub azimuth: f64,
    /// Elevation angle in radians (tilt from horizontal)
    pub elevation: f64,
}

impl Default for OrbitAngles {
    fn default() -> Self {
        Self {
            azimuth: std::f64::consts::FRAC_PI_6,    // 30° — classic isometric
            elevation: std::f64::consts::FRAC_PI_6,   // 30° elevation
        }
    }
}

impl OrbitAngles {
    const STEP: f64 = 0.15; // ~8.5° per keypress

    pub fn rotate_left(&mut self) {
        self.azimuth -= Self::STEP;
    }
    pub fn rotate_right(&mut self) {
        self.azimuth += Self::STEP;
    }
    pub fn rotate_up(&mut self) {
        self.elevation = (self.elevation + Self::STEP).min(std::f64::consts::FRAC_PI_2 - 0.05);
    }
    pub fn rotate_down(&mut self) {
        self.elevation = (self.elevation - Self::STEP).max(-std::f64::consts::FRAC_PI_2 + 0.05);
    }
}

/// Project a 3D point to 2D using orbit camera angles
fn orbit_project(v: Vec3, angles: &OrbitAngles) -> (f64, f64) {
    let (x, y, z) = (v.x as f64, v.y as f64, v.z as f64);
    let (sa, ca) = angles.azimuth.sin_cos();
    let (se, ce) = angles.elevation.sin_cos();

    // Rotate around Y axis (azimuth), then tilt (elevation)
    let rx = x * ca + z * sa;
    let rz = -x * sa + z * ca;
    let sx = rx;
    let sy = y * ce - rz * se;
    (sx, sy)
}

/// Viewport widget that renders scene using ratatui Canvas with braille
pub struct Viewport<'a> {
    scene: &'a Scene,
    view_mode: ViewMode,
    floorplan_view: &'a FloorPlanView,
    framebuffer: &'a Framebuffer,
    orbit: &'a OrbitAngles,
    /// Raw LDT content for polar diagram (from selected entity's photometry)
    ldt_content: Option<&'a str>,
    focused: bool,
}

impl<'a> Viewport<'a> {
    pub fn new(
        scene: &'a Scene,
        view_mode: ViewMode,
        floorplan_view: &'a FloorPlanView,
        framebuffer: &'a Framebuffer,
        orbit: &'a OrbitAngles,
    ) -> Self {
        Self {
            scene,
            view_mode,
            floorplan_view,
            framebuffer,
            orbit,
            ldt_content: None,
            focused: false,
        }
    }

    pub fn ldt_content(mut self, content: Option<&'a str>) -> Self {
        self.ldt_content = content;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for Viewport<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.view_mode {
            ViewMode::Iso3D => render_iso3d(self.scene, self.orbit, self.focused, area, buf),
            ViewMode::FloorPlan => {
                render_floorplan_braille(self.scene, self.floorplan_view, self.focused, area, buf);
            }
            ViewMode::Polar => {
                render_polar_diagram(self.ldt_content, self.focused, area, buf);
            }
            ViewMode::BlockChar => render_block_char(self.framebuffer, self.focused, area, buf),
        }
    }
}

/// Render 3D orbit wireframe using braille Canvas
fn render_iso3d(scene: &Scene, orbit: &OrbitAngles, focused: bool, area: Rect, buf: &mut Buffer) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    if scene.edges.is_empty() && scene.triangles.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" 3D (empty) ");
        block.render(area, buf);
        return;
    }

    // Center geometry at origin for rotation
    let center = Vec3::new(
        (scene.bounds_min.x + scene.bounds_max.x) * 0.5,
        (scene.bounds_min.y + scene.bounds_max.y) * 0.5,
        (scene.bounds_min.z + scene.bounds_max.z) * 0.5,
    );

    let project = |v: Vec3| orbit_project(v - center, orbit);

    // Compute projected bounding box from scene bounds
    let corners = [
        Vec3::new(scene.bounds_min.x, scene.bounds_min.y, scene.bounds_min.z),
        Vec3::new(scene.bounds_max.x, scene.bounds_min.y, scene.bounds_min.z),
        Vec3::new(scene.bounds_min.x, scene.bounds_max.y, scene.bounds_min.z),
        Vec3::new(scene.bounds_max.x, scene.bounds_max.y, scene.bounds_min.z),
        Vec3::new(scene.bounds_min.x, scene.bounds_min.y, scene.bounds_max.z),
        Vec3::new(scene.bounds_max.x, scene.bounds_min.y, scene.bounds_max.z),
        Vec3::new(scene.bounds_min.x, scene.bounds_max.y, scene.bounds_max.z),
        Vec3::new(scene.bounds_max.x, scene.bounds_max.y, scene.bounds_max.z),
    ];

    let projected: Vec<(f64, f64)> = corners.iter().map(|c| project(*c)).collect();
    let mut x_min = f64::MAX;
    let mut x_max = f64::MIN;
    let mut y_min = f64::MAX;
    let mut y_max = f64::MIN;
    for &(px, py) in &projected {
        x_min = x_min.min(px);
        x_max = x_max.max(px);
        y_min = y_min.min(py);
        y_max = y_max.max(py);
    }

    // Add margin
    let dx = (x_max - x_min).max(1.0) * 0.05;
    let dy = (y_max - y_min).max(1.0) * 0.05;
    x_min -= dx;
    x_max += dx;
    y_min -= dy;
    y_max += dy;

    // Collect edges as projected lines
    let edges: Vec<_> = scene
        .edges
        .iter()
        .map(|e| {
            let (x0, y0) = project(e.v0);
            let (x1, y1) = project(e.v1);
            (x0, y0, x1, y1)
        })
        .collect();

    // If no pre-built edges, extract from triangles directly
    let tri_edges: Vec<_> = if edges.is_empty() {
        scene
            .triangles
            .iter()
            .flat_map(|tri| {
                let (x0, y0) = project(tri.v0);
                let (x1, y1) = project(tri.v1);
                let (x2, y2) = project(tri.v2);
                vec![(x0, y0, x1, y1), (x1, y1, x2, y2), (x2, y2, x0, y0)]
            })
            .collect()
    } else {
        Vec::new()
    };

    let all_edges = if edges.is_empty() {
        &tri_edges
    } else {
        &edges
    };

    let edge_count = all_edges.len();
    let az_deg = orbit.azimuth.to_degrees();
    let el_deg = orbit.elevation.to_degrees();

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(format!(
                    " 3D ({} edges, az:{:.0}\u{00b0} el:{:.0}\u{00b0}) ",
                    edge_count, az_deg, el_deg
                )),
        )
        .marker(Marker::Braille)
        .x_bounds([x_min, x_max])
        .y_bounds([y_min, y_max])
        .paint(|ctx| {
            for &(x0, y0, x1, y1) in all_edges {
                ctx.draw(&CanvasLine {
                    x1: x0,
                    y1: y0,
                    x2: x1,
                    y2: y1,
                    color: Color::Rgb(180, 180, 200),
                });
            }
        });

    canvas.render(area, buf);
}

/// Render 2D floor plan using braille Canvas
fn render_floorplan_braille(
    scene: &Scene,
    view: &FloorPlanView,
    focused: bool,
    area: Rect,
    buf: &mut Buffer,
) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    if scene.edges.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Floor Plan (empty) ");
        block.render(area, buf);
        return;
    }

    // Slice parameters
    let min_y = view.slice_y - view.slice_thickness * 0.5;
    let max_y = view.slice_y + view.slice_thickness * 0.5;

    // World XZ bounds (centered + pan + zoom)
    let cx = (scene.bounds_min.x + scene.bounds_max.x) * 0.5 + view.pan.x;
    let cz = (scene.bounds_min.z + scene.bounds_max.z) * 0.5 + view.pan.y;
    let range_x = (scene.bounds_max.x - scene.bounds_min.x).max(1.0);
    let range_z = (scene.bounds_max.z - scene.bounds_min.z).max(1.0);
    let half_range = (range_x.max(range_z) * 0.55) / view.zoom;

    let x_min = (cx - half_range) as f64;
    let x_max = (cx + half_range) as f64;
    let z_min = (cz - half_range) as f64;
    let z_max = (cz + half_range) as f64;

    // Collect visible edges
    let visible_edges: Vec<_> = scene
        .edges
        .iter()
        .filter(|e| {
            let edge_min_y = e.v0.y.min(e.v1.y);
            let edge_max_y = e.v0.y.max(e.v1.y);
            edge_max_y >= min_y && edge_min_y <= max_y
        })
        .collect();

    let level_info = format!(
        "Y={:.1}m, {} edges",
        view.slice_y,
        visible_edges.len()
    );

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(format!(" Floor Plan ({}) ", level_info)),
        )
        .marker(Marker::Braille)
        .x_bounds([x_min, x_max])
        .y_bounds([z_min, z_max])
        .paint(|ctx| {
            for edge in &visible_edges {
                // Calculate intensity based on Y overlap
                let edge_min_y = edge.v0.y.min(edge.v1.y);
                let edge_max_y = edge.v0.y.max(edge.v1.y);
                let in_slice = if edge_max_y - edge_min_y < 0.01 {
                    1.0f32
                } else {
                    let overlap_min = edge_min_y.max(min_y);
                    let overlap_max = edge_max_y.min(max_y);
                    ((overlap_max - overlap_min) / (edge_max_y - edge_min_y)).clamp(0.0, 1.0)
                };

                let intensity = (in_slice * 180.0 + 60.0) as u8;

                // Project XZ → canvas (ignore Y)
                ctx.draw(&CanvasLine {
                    x1: edge.v0.x as f64,
                    y1: edge.v0.z as f64,
                    x2: edge.v1.x as f64,
                    y2: edge.v1.z as f64,
                    color: Color::Rgb(intensity, intensity, intensity),
                });
            }
        });

    canvas.render(area, buf);
}

/// Render polar photometric diagram using braille Canvas
fn render_polar_diagram(
    ldt_content: Option<&str>,
    focused: bool,
    area: Rect,
    buf: &mut Buffer,
) {
    use eulumdat::diagram::PolarDiagram;
    use eulumdat::Eulumdat;

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let ldt_content = match ldt_content {
        Some(c) if !c.is_empty() => c,
        _ => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(" Polar (select a light fixture) ");
            block.render(area, buf);
            return;
        }
    };

    let ldt = match Eulumdat::parse(ldt_content) {
        Ok(l) => l,
        Err(_) => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(" Polar (parse error) ");
            block.render(area, buf);
            return;
        }
    };

    let polar = PolarDiagram::from_eulumdat(&ldt);
    let scale_max = polar.scale.scale_max;

    // Bounds: polar curves are centered at origin, range [-scale_max, scale_max]
    let bound = scale_max * 1.15; // margin

    let title = format!(
        " Polar - {} ({:.0} cd max) ",
        truncate_str(&ldt.luminaire_name, 30),
        polar.scale.max_intensity,
    );

    let c0_points: Vec<(f64, f64)> = polar
        .c0_c180_curve
        .points
        .iter()
        .map(|p| (p.x, -p.y)) // flip Y for canvas (canvas Y goes up)
        .collect();

    let c90_points: Vec<(f64, f64)> = polar
        .c90_c270_curve
        .points
        .iter()
        .map(|p| (p.x, -p.y))
        .collect();

    let show_c90 = polar.show_c90_c270();
    let grid_values = polar.scale.grid_values.clone();

    let canvas = Canvas::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        )
        .marker(Marker::Braille)
        .x_bounds([-bound, bound])
        .y_bounds([-bound, bound])
        .paint(move |ctx| {
            // Draw grid circles
            for &grid_val in &grid_values {
                let steps = 72;
                for i in 0..steps {
                    let a0 = i as f64 * std::f64::consts::TAU / steps as f64;
                    let a1 = (i + 1) as f64 * std::f64::consts::TAU / steps as f64;
                    ctx.draw(&CanvasLine {
                        x1: grid_val * a0.cos(),
                        y1: grid_val * a0.sin(),
                        x2: grid_val * a1.cos(),
                        y2: grid_val * a1.sin(),
                        color: Color::Rgb(40, 40, 55),
                    });
                }
            }

            // Draw axes
            ctx.draw(&CanvasLine {
                x1: -scale_max,
                y1: 0.0,
                x2: scale_max,
                y2: 0.0,
                color: Color::Rgb(60, 60, 80),
            });
            ctx.draw(&CanvasLine {
                x1: 0.0,
                y1: -scale_max,
                x2: 0.0,
                y2: scale_max,
                color: Color::Rgb(60, 60, 80),
            });

            // Draw C0-C180 curve (blue)
            for w in c0_points.windows(2) {
                ctx.draw(&CanvasLine {
                    x1: w[0].0,
                    y1: w[0].1,
                    x2: w[1].0,
                    y2: w[1].1,
                    color: Color::Rgb(80, 140, 255),
                });
            }

            // Draw C90-C270 curve (red)
            if show_c90 {
                for w in c90_points.windows(2) {
                    ctx.draw(&CanvasLine {
                        x1: w[0].0,
                        y1: w[0].1,
                        x2: w[1].0,
                        y2: w[1].1,
                        color: Color::Rgb(255, 100, 100),
                    });
                }
            }

            // Labels
            ctx.print(
                scale_max * 0.05,
                bound * 0.9,
                ratatui::text::Line::styled("0\u{00b0}", Style::default().fg(Color::DarkGray)),
            );
            ctx.print(
                scale_max * 0.05,
                -bound * 0.9,
                ratatui::text::Line::styled("180\u{00b0}", Style::default().fg(Color::DarkGray)),
            );
            ctx.print(
                bound * 0.8,
                0.0,
                ratatui::text::Line::styled("90\u{00b0}", Style::default().fg(Color::DarkGray)),
            );

            // Legend
            ctx.print(
                -bound * 0.95,
                -bound * 0.85,
                ratatui::text::Line::styled(
                    "C0-C180",
                    Style::default().fg(Color::Rgb(80, 140, 255)),
                ),
            );
            if show_c90 {
                ctx.print(
                    -bound * 0.95,
                    -bound * 0.95,
                    ratatui::text::Line::styled(
                        "C90-C270",
                        Style::default().fg(Color::Rgb(255, 100, 100)),
                    ),
                );
            }
        });

    canvas.render(area, buf);
}

/// Truncate a string (helper for titles)
fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max > 3 {
        format!("{}...", &s[..max - 3])
    } else {
        s[..max].to_string()
    }
}

/// Legacy block-char viewport (renders from pre-computed framebuffer)
fn render_block_char(framebuffer: &Framebuffer, focused: bool, area: Rect, buf: &mut Buffer) {
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Block Char ");

    let inner = block.inner(area);
    block.render(area, buf);

    let fb_width = framebuffer.width;
    let fb_height = framebuffer.height;

    for y in 0..inner.height as usize {
        for x in 0..inner.width as usize {
            if x >= fb_width || y >= fb_height {
                continue;
            }

            let idx = y * fb_width + x;
            let ch = framebuffer.chars[idx];
            let [r, g, b] = framebuffer.char_colors[idx];

            let cell_x = inner.x + x as u16;
            let cell_y = inner.y + y as u16;

            if cell_x < area.right() && cell_y < area.bottom() {
                buf[(cell_x, cell_y)]
                    .set_char(ch)
                    .set_fg(Color::Rgb(r, g, b));
            }
        }
    }
}
