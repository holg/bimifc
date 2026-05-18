// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! IFC drawing functions — project 3D meshes to 2D plan view wireframes.

use std::collections::HashSet;

use acadlisp::{DrawEntity, Expr, ForeignContext, Interpreter};
use bimifc_model::{EntityId, IfcModel};

use crate::ifc_functions::{expr_to_entity_id, get_model};
use crate::IfcState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Canonical edge key for deduplication: sort the two endpoints.
fn edge_key(x1: f32, y1: f32, x2: f32, y2: f32) -> (i64, i64, i64, i64) {
    let (a, b, c, d) = (
        (x1 * 10000.0) as i64,
        (y1 * 10000.0) as i64,
        (x2 * 10000.0) as i64,
        (y2 * 10000.0) as i64,
    );
    if (a, b) <= (c, d) {
        (a, b, c, d)
    } else {
        (c, d, a, b)
    }
}

/// Project triangle mesh edges to XY (drop Z), deduplicate shared edges.
/// Returns a list of (x1, y1, x2, y2) line segments.
fn project_mesh_edges(positions: &[f32], indices: &[u32]) -> Vec<(f64, f64, f64, f64)> {
    let mut seen = HashSet::new();
    let mut edges = Vec::new();

    for tri in indices.chunks(3) {
        if tri.len() < 3 {
            continue;
        }
        let verts: [(f32, f32); 3] = [
            (
                positions[tri[0] as usize * 3],
                positions[tri[0] as usize * 3 + 1],
            ),
            (
                positions[tri[1] as usize * 3],
                positions[tri[1] as usize * 3 + 1],
            ),
            (
                positions[tri[2] as usize * 3],
                positions[tri[2] as usize * 3 + 1],
            ),
        ];

        for &(a, b) in &[(0, 1), (1, 2), (2, 0)] {
            let key = edge_key(verts[a].0, verts[a].1, verts[b].0, verts[b].1);
            if seen.insert(key) {
                edges.push((
                    verts[a].0 as f64,
                    verts[a].1 as f64,
                    verts[b].0 as f64,
                    verts[b].1 as f64,
                ));
            }
        }
    }
    edges
}

/// Draw a single entity's mesh into the drawing, returning line count.
fn draw_entity_mesh(ctx: &mut ForeignContext, entity_id: EntityId) -> Result<usize, String> {
    let state = ctx
        .user_data
        .downcast_mut::<IfcState>()
        .ok_or("no IfcState")?;
    let model = state.model.clone().ok_or("no model loaded")?;
    let router = state.router.take().ok_or("no geometry router")?;

    let resolver = model.resolver();
    let entity = resolver
        .get(entity_id)
        .ok_or_else(|| format!("entity #{} not found", entity_id.0))?;

    let mesh_result = router.process_element(&entity, resolver);

    // Put the router back before returning
    let state = ctx.user_data.downcast_mut::<IfcState>().unwrap();
    state.router = Some(router);

    let mesh = mesh_result.map_err(|e| format!("geometry error: {}", e))?;
    if mesh.positions.is_empty() {
        return Ok(0);
    }

    let edges = project_mesh_edges(&mesh.positions, &mesh.indices);
    let layer = format!("{:?}", entity.ifc_type);

    for (x1, y1, x2, y2) in &edges {
        ctx.drawing.entities.push(DrawEntity::Line {
            x1: *x1,
            y1: *y1,
            x2: *x2,
            y2: *y2,
            layer: layer.clone(),
        });
    }

    Ok(edges.len())
}

// ---------------------------------------------------------------------------
// Lisp functions
// ---------------------------------------------------------------------------

/// (ifc-draw entity-id) → integer (line count) or Nil
fn ifc_draw(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let id = match args.first().and_then(expr_to_entity_id) {
        Some(id) => id,
        None => {
            ctx.output
                .push("Error: IFC-DRAW requires an entity ID\n".to_string());
            return Expr::Nil;
        }
    };
    match draw_entity_mesh(ctx, id) {
        Ok(count) => Expr::Integer(count as i32),
        Err(e) => {
            ctx.output.push(format!("Error: {}\n", e));
            Expr::Nil
        }
    }
}

/// (ifc-draw-all) → integer (total line count)
fn ifc_draw_all(ctx: &mut ForeignContext, _args: &[Expr]) -> Expr {
    let model: std::sync::Arc<dyn IfcModel> = match get_model(ctx) {
        Some(m) => m,
        None => return Expr::Nil,
    };
    let element_ids = model.spatial().all_elements();
    let mut total = 0;
    for id in element_ids {
        if let Ok(count) = draw_entity_mesh(ctx, id) {
            total += count;
        }
    }
    Expr::Integer(total as i32)
}

/// (ifc-draw-storey storey-id) → integer (line count)
fn ifc_draw_storey(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let storey_id = match args.first().and_then(expr_to_entity_id) {
        Some(id) => id,
        None => {
            ctx.output
                .push("Error: IFC-DRAW-STOREY requires a storey ID\n".to_string());
            return Expr::Nil;
        }
    };
    let model: std::sync::Arc<dyn IfcModel> = match get_model(ctx) {
        Some(m) => m,
        None => return Expr::Nil,
    };
    let element_ids = model.spatial().elements_in_storey(storey_id);
    let mut total = 0;
    for id in element_ids {
        if let Ok(count) = draw_entity_mesh(ctx, id) {
            total += count;
        }
    }
    Expr::Integer(total as i32)
}

/// (ifc-draw-svg "output.svg") → T or Nil
fn ifc_draw_svg(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let path = match args.first() {
        Some(Expr::String(s)) => s.clone(),
        _ => {
            ctx.output
                .push("Error: IFC-DRAW-SVG requires a file path string\n".to_string());
            return Expr::Nil;
        }
    };

    let entities = &ctx.drawing.entities;
    if entities.is_empty() {
        ctx.output
            .push("Warning: no entities to export\n".to_string());
        return Expr::Nil;
    }

    let svg = entities_to_svg(entities);
    match std::fs::write(&path, svg) {
        Ok(_) => {
            ctx.output.push(format!("SVG written to {}\n", path));
            Expr::T
        }
        Err(e) => {
            ctx.output.push(format!("Error writing SVG: {}\n", e));
            Expr::Nil
        }
    }
}

/// Generate SVG from drawing entities (lines only for plan views).
fn entities_to_svg(entities: &[DrawEntity]) -> String {
    // Calculate bounding box
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for entity in entities {
        if let DrawEntity::Line { x1, y1, x2, y2, .. } = entity {
            min_x = min_x.min(*x1).min(*x2);
            min_y = min_y.min(*y1).min(*y2);
            max_x = max_x.max(*x1).max(*x2);
            max_y = max_y.max(*y1).max(*y2);
        }
    }

    if min_x >= max_x || min_y >= max_y {
        return "<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_string();
    }

    let padding = 20.0;
    let w = max_x - min_x;
    let h = max_y - min_y;
    let scale = if w > h { 800.0 / w } else { 800.0 / h };
    let svg_w = w * scale + 2.0 * padding;
    let svg_h = h * scale + 2.0 * padding;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {:.0} {:.0}\">\n",
        svg_w, svg_h, svg_w, svg_h
    );
    svg.push_str("<style>\n");
    svg.push_str("  svg { background: #1a1a2e; }\n");
    svg.push_str("  line { stroke: #00ff88; stroke-width: 0.5; }\n");
    svg.push_str("  line.wall { stroke: #ffffff; stroke-width: 1.0; }\n");
    svg.push_str("  line.slab { stroke: #555555; stroke-width: 0.3; }\n");
    svg.push_str("  line.door { stroke: #ff8800; stroke-width: 0.8; }\n");
    svg.push_str("  line.window { stroke: #00aaff; stroke-width: 0.8; }\n");
    svg.push_str("</style>\n");

    for entity in entities {
        if let DrawEntity::Line {
            x1,
            y1,
            x2,
            y2,
            layer,
        } = entity
        {
            let sx1 = (*x1 - min_x) * scale + padding;
            let sy1 = svg_h - ((*y1 - min_y) * scale + padding);
            let sx2 = (*x2 - min_x) * scale + padding;
            let sy2 = svg_h - ((*y2 - min_y) * scale + padding);
            let class = layer_to_css_class(layer);
            svg.push_str(&format!(
                "  <line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" class=\"{}\"/>\n",
                sx1, sy1, sx2, sy2, class
            ));
        }
    }

    svg.push_str("</svg>\n");
    svg
}

fn layer_to_css_class(layer: &str) -> &str {
    let lower = layer.to_ascii_lowercase();
    if lower.contains("wall") {
        "wall"
    } else if lower.contains("slab") || lower.contains("floor") {
        "slab"
    } else if lower.contains("door") {
        "door"
    } else if lower.contains("window") {
        "window"
    } else {
        ""
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register_all(interp: &mut Interpreter) {
    interp.register_fn("IFC-DRAW", ifc_draw);
    interp.register_fn("IFC-DRAW-ALL", ifc_draw_all);
    interp.register_fn("IFC-DRAW-STOREY", ifc_draw_storey);
    interp.register_fn("IFC-DRAW-SVG", ifc_draw_svg);
}
