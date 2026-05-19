//! Count triangles per MEP discipline in a given IFC file.
//!
//! Run:
//!   cargo run --release -p bimifc-viewer-tui --example discipline_counts -- <file.ifc>
//!
//! Used to verify the discipline filter classifies legacy IFC2x3 files
//! correctly via the Name-attribute fallback.

use bimifc_parser::ParsedModel;
use bimifc_viewer_tui::scene::{Discipline, Scene};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("need IFC path");
    let content = std::fs::read_to_string(&path).expect("read");
    let t0 = Instant::now();
    let model = Arc::new(ParsedModel::parse(&content, true, true).expect("parse"));
    println!("Parsed in {:?}", t0.elapsed());

    let t1 = Instant::now();
    let scene = Scene::from_content(&content, &model);
    println!("Scene built in {:?}", t1.elapsed());
    println!("Total triangles: {}", scene.triangles.len());

    let mut counts = [0usize; 5];
    for t in &scene.triangles {
        counts[t.discipline as usize] += 1;
    }
    println!();
    println!("Triangles per discipline:");
    for d in [
        Discipline::Other,
        Discipline::Electrical,
        Discipline::Plumbing,
        Discipline::Hvac,
        Discipline::Lighting,
    ] {
        let n = counts[d as usize];
        let pct = if scene.triangles.is_empty() {
            0.0
        } else {
            n as f32 / scene.triangles.len() as f32 * 100.0
        };
        println!("  {:?}: {} ({:.1}%)", d, n, pct);
    }
}
