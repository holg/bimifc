// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Terminal-based IFC viewer
//!
//! A 3D IFC model viewer that runs in the terminal using block characters.

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use bimifc_parser::ParsedModel;
use bimifc_model::IfcModel;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, stdout};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use bimifc_viewer_tui::App;

/// Terminal-based IFC 3D viewer
#[derive(Parser, Debug)]
#[command(name = "bimifc-tui")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// IFC file to view
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// Show test cube (no file needed)
    #[arg(short, long)]
    test: bool,

    /// Debug mode - print projection info and exit
    #[arg(long)]
    debug: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Debug mode - print projection info without terminal
    if args.debug {
        return run_debug(&args);
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app
    let result = run_app(&mut terminal, args);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Print any errors
    if let Err(ref e) = result {
        eprintln!("Error: {e:?}");
    }

    result
}

fn run_debug(args: &Args) -> Result<()> {
    use bimifc_viewer_tui::{camera::OrbitCamera, renderer::culling, scene::Scene};

    let (scene, mut camera) = if args.file.is_some() {
        let file_path = args.file.as_ref().unwrap();
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        let model = Arc::new(
            ParsedModel::parse(&content, true, true)
                .with_context(|| format!("Failed to parse IFC file: {}", file_path.display()))?,
        );

        let scene = Scene::from_content(&content, &model);
        let mut camera = OrbitCamera::new();
        if scene.diagonal() > 0.0 {
            camera.fit_bounds(scene.bounds_min, scene.bounds_max);
        }
        (scene, camera)
    } else {
        let scene = Scene::test_cube();
        let mut camera = OrbitCamera::new();
        camera.fit_bounds(scene.bounds_min, scene.bounds_max);
        (scene, camera)
    };

    camera.set_terminal_aspect(80, 24);

    println!("=== Debug Info ===");
    if let Some(file_path) = &args.file {
        if let Ok(content) = std::fs::read_to_string(file_path) {
            if let Ok(model) = ParsedModel::parse(&content, true, true) {
                println!("Model unit_scale: {}", model.unit_scale());
            }
        }
    }
    println!("Scene: {} triangles", scene.triangles.len());
    println!("Bounds: ({:.1}, {:.1}, {:.1}) to ({:.1}, {:.1}, {:.1})",
        scene.bounds_min.x, scene.bounds_min.y, scene.bounds_min.z,
        scene.bounds_max.x, scene.bounds_max.y, scene.bounds_max.z);
    println!("Diagonal: {:.1}", scene.diagonal());
    println!();
    println!("Camera:");
    println!("  Target: ({:.1}, {:.1}, {:.1})", camera.target.x, camera.target.y, camera.target.z);
    println!("  Distance: {:.1}", camera.distance);
    println!("  Position: ({:.1}, {:.1}, {:.1})", camera.position().x, camera.position().y, camera.position().z);
    println!("  Near: {:.2}, Far: {:.2}", camera.near, camera.far);
    println!("  FOV: {:.1}°, Aspect: {:.2}", camera.fov, camera.aspect);

    // Check some triangle world sizes
    let mut min_area = f32::MAX;
    let mut max_area = 0.0f32;
    let mut total_area = 0.0f64;
    for tri in &scene.triangles {
        let edge1 = tri.v1 - tri.v0;
        let edge2 = tri.v2 - tri.v0;
        let area = edge1.cross(edge2).length() * 0.5;
        min_area = min_area.min(area);
        max_area = max_area.max(area);
        total_area += area as f64;
    }
    println!();
    println!("Triangle world sizes:");
    println!("  Min area: {:.4} sq units", min_area);
    println!("  Max area: {:.4} sq units", max_area);
    println!("  Avg area: {:.4} sq units", total_area / scene.triangles.len() as f64);
    println!("  Total area: {:.1} sq units", total_area);
    println!();

    let vp = camera.view_projection_matrix();
    println!("View-Projection Matrix:");
    println!("  [{:>10.4} {:>10.4} {:>10.4} {:>10.4}]", vp.col(0).x, vp.col(1).x, vp.col(2).x, vp.col(3).x);
    println!("  [{:>10.4} {:>10.4} {:>10.4} {:>10.4}]", vp.col(0).y, vp.col(1).y, vp.col(2).y, vp.col(3).y);
    println!("  [{:>10.4} {:>10.4} {:>10.4} {:>10.4}]", vp.col(0).z, vp.col(1).z, vp.col(2).z, vp.col(3).z);
    println!("  [{:>10.4} {:>10.4} {:>10.4} {:>10.4}]", vp.col(0).w, vp.col(1).w, vp.col(2).w, vp.col(3).w);
    println!();

    // Test first 5 triangles
    println!("First 5 triangles:");
    for (i, tri) in scene.triangles.iter().take(5).enumerate() {
        println!("  Triangle {}:", i);
        println!("    World: v0=({:.1},{:.1},{:.1})", tri.v0.x, tri.v0.y, tri.v0.z);

        let v0_clip = vp * tri.v0.extend(1.0);
        let v1_clip = vp * tri.v1.extend(1.0);
        let v2_clip = vp * tri.v2.extend(1.0);

        println!("    Clip: v0=({:.3},{:.3},{:.3},{:.3})", v0_clip.x, v0_clip.y, v0_clip.z, v0_clip.w);

        // Check frustum
        let outside = culling::is_triangle_outside_frustum(v0_clip, v1_clip, v2_clip);
        println!("    Frustum culled: {}", outside);

        // Detailed checks
        let all_left = v0_clip.x < -v0_clip.w && v1_clip.x < -v1_clip.w && v2_clip.x < -v2_clip.w;
        let all_right = v0_clip.x > v0_clip.w && v1_clip.x > v1_clip.w && v2_clip.x > v2_clip.w;
        let all_bottom = v0_clip.y < -v0_clip.w && v1_clip.y < -v1_clip.w && v2_clip.y < -v2_clip.w;
        let all_top = v0_clip.y > v0_clip.w && v1_clip.y > v1_clip.w && v2_clip.y > v2_clip.w;
        let all_near = v0_clip.z < -v0_clip.w && v1_clip.z < -v1_clip.w && v2_clip.z < -v2_clip.w;
        let all_far = v0_clip.z > v0_clip.w && v1_clip.z > v1_clip.w && v2_clip.z > v2_clip.w;
        let all_behind = v0_clip.w <= 0.0 && v1_clip.w <= 0.0 && v2_clip.w <= 0.0;

        if outside {
            if all_left { println!("      -> All LEFT of frustum"); }
            if all_right { println!("      -> All RIGHT of frustum"); }
            if all_bottom { println!("      -> All BOTTOM of frustum"); }
            if all_top { println!("      -> All TOP of frustum"); }
            if all_near { println!("      -> All NEAR (in front of near plane)"); }
            if all_far { println!("      -> All FAR (behind far plane)"); }
            if all_behind { println!("      -> All BEHIND camera (w <= 0)"); }
        }
    }

    // Test NDC and screen conversion for first triangle
    if let Some(tri) = scene.triangles.first() {
        let v0_clip = vp * tri.v0.extend(1.0);
        let v1_clip = vp * tri.v1.extend(1.0);
        let v2_clip = vp * tri.v2.extend(1.0);

        let v0_ndc = v0_clip.truncate() / v0_clip.w;
        let v1_ndc = v1_clip.truncate() / v1_clip.w;
        let v2_ndc = v2_clip.truncate() / v2_clip.w;

        let fw = 78.0f32;  // Typical framebuffer width
        let fh = 22.0f32;  // Typical framebuffer height

        let v0_screen = glam::Vec3::new(
            (v0_ndc.x + 1.0) * 0.5 * fw,
            (1.0 - v0_ndc.y) * 0.5 * fh,
            (v0_ndc.z + 1.0) * 0.5,
        );
        let v1_screen = glam::Vec3::new(
            (v1_ndc.x + 1.0) * 0.5 * fw,
            (1.0 - v1_ndc.y) * 0.5 * fh,
            (v1_ndc.z + 1.0) * 0.5,
        );
        let v2_screen = glam::Vec3::new(
            (v2_ndc.x + 1.0) * 0.5 * fw,
            (1.0 - v2_ndc.y) * 0.5 * fh,
            (v2_ndc.z + 1.0) * 0.5,
        );

        println!();
        println!("Screen coordinate test (assuming {}x{} framebuffer):", fw as i32, fh as i32);
        println!("  v0_screen: ({:.2}, {:.2}, {:.4})", v0_screen.x, v0_screen.y, v0_screen.z);
        println!("  v1_screen: ({:.2}, {:.2}, {:.4})", v1_screen.x, v1_screen.y, v1_screen.z);
        println!("  v2_screen: ({:.2}, {:.2}, {:.4})", v2_screen.x, v2_screen.y, v2_screen.z);

        // Calculate 2D area
        let a = glam::Vec2::new(v1_screen.x - v0_screen.x, v1_screen.y - v0_screen.y);
        let b = glam::Vec2::new(v2_screen.x - v0_screen.x, v2_screen.y - v0_screen.y);
        let area = (a.x * b.y - a.y * b.x).abs() * 0.5;
        println!("  Triangle 2D area: {:.4} pixels", area);
        println!("  Subpixel culled (area < 0.5): {}", area < 0.5);

        // World space triangle size
        let edge1 = tri.v1 - tri.v0;
        let edge2 = tri.v2 - tri.v0;
        let world_area = edge1.cross(edge2).length() * 0.5;
        println!("  World space area: {:.2} sq units", world_area);
        println!("  Edge lengths: {:.2}, {:.2}, {:.2}",
            edge1.length(),
            edge2.length(),
            (tri.v2 - tri.v1).length());
    }

    // Count subpixel triangles
    let mut subpixel_count = 0usize;
    let mut total_screen_area = 0.0f64;
    for tri in &scene.triangles {
        let v0_clip = vp * tri.v0.extend(1.0);
        let v1_clip = vp * tri.v1.extend(1.0);
        let v2_clip = vp * tri.v2.extend(1.0);

        if v0_clip.w <= 0.0 || v1_clip.w <= 0.0 || v2_clip.w <= 0.0 {
            continue;
        }

        let v0_ndc = v0_clip.truncate() / v0_clip.w;
        let v1_ndc = v1_clip.truncate() / v1_clip.w;
        let v2_ndc = v2_clip.truncate() / v2_clip.w;

        let fw = 78.0f32;
        let fh = 22.0f32;

        let v0_s = glam::Vec2::new((v0_ndc.x + 1.0) * 0.5 * fw, (1.0 - v0_ndc.y) * 0.5 * fh);
        let v1_s = glam::Vec2::new((v1_ndc.x + 1.0) * 0.5 * fw, (1.0 - v1_ndc.y) * 0.5 * fh);
        let v2_s = glam::Vec2::new((v2_ndc.x + 1.0) * 0.5 * fw, (1.0 - v2_ndc.y) * 0.5 * fh);

        let a = v1_s - v0_s;
        let b = v2_s - v0_s;
        let area = (a.x * b.y - a.y * b.x).abs() * 0.5;
        total_screen_area += area as f64;

        if area < 0.5 {
            subpixel_count += 1;
        }
    }

    println!();
    println!("Subpixel analysis:");
    println!("  Subpixel triangles (area < 0.5): {} / {}", subpixel_count, scene.triangles.len());
    println!("  Total screen area: {:.2} pixels", total_screen_area);
    println!("  Average triangle area: {:.4} pixels", total_screen_area / scene.triangles.len() as f64);

    // Count culling reasons
    let mut left = 0usize;
    let mut right = 0usize;
    let mut bottom = 0usize;
    let mut top = 0usize;
    let mut near = 0usize;
    let mut far = 0usize;
    let mut behind = 0usize;
    let mut passed = 0usize;

    for tri in &scene.triangles {
        let v0_clip = vp * tri.v0.extend(1.0);
        let v1_clip = vp * tri.v1.extend(1.0);
        let v2_clip = vp * tri.v2.extend(1.0);

        if v0_clip.x < -v0_clip.w && v1_clip.x < -v1_clip.w && v2_clip.x < -v2_clip.w {
            left += 1;
        } else if v0_clip.x > v0_clip.w && v1_clip.x > v1_clip.w && v2_clip.x > v2_clip.w {
            right += 1;
        } else if v0_clip.y < -v0_clip.w && v1_clip.y < -v1_clip.w && v2_clip.y < -v2_clip.w {
            bottom += 1;
        } else if v0_clip.y > v0_clip.w && v1_clip.y > v1_clip.w && v2_clip.y > v2_clip.w {
            top += 1;
        } else if v0_clip.z < -v0_clip.w && v1_clip.z < -v1_clip.w && v2_clip.z < -v2_clip.w {
            near += 1;
        } else if v0_clip.z > v0_clip.w && v1_clip.z > v1_clip.w && v2_clip.z > v2_clip.w {
            far += 1;
        } else if v0_clip.w <= 0.0 && v1_clip.w <= 0.0 && v2_clip.w <= 0.0 {
            behind += 1;
        } else {
            passed += 1;
        }
    }

    println!();
    println!("Culling breakdown:");
    println!("  Left:   {}", left);
    println!("  Right:  {}", right);
    println!("  Bottom: {}", bottom);
    println!("  Top:    {}", top);
    println!("  Near:   {}", near);
    println!("  Far:    {}", far);
    println!("  Behind: {}", behind);
    println!("  PASSED: {}", passed);

    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, args: Args) -> Result<()> {
    let app = if args.test || args.file.is_none() {
        // Test mode with cube
        App::new()
    } else {
        // Load IFC file
        let file_path = args.file.unwrap();

        // Show loading message
        terminal.draw(|frame| {
            let area = frame.area();
            let msg = format!("Loading {}...", file_path.display());
            let paragraph = ratatui::widgets::Paragraph::new(msg)
                .alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(paragraph, area);
        })?;

        let start = Instant::now();

        // Read file
        let content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        // Parse IFC using ParsedModel directly (same as Bevy viewer)
        let model = Arc::new(
            ParsedModel::parse(&content, true, true)
                .with_context(|| format!("Failed to parse IFC file: {}", file_path.display()))?,
        );

        let elapsed = start.elapsed();
        eprintln!("Parsed in {:.2}s", elapsed.as_secs_f32());

        App::with_content(&content, model)
    };

    // Run main loop
    let mut app = app;
    app.run(terminal)
}
