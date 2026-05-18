// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! IFC creation and writing functions — create building elements and save IFC files.

use acadlisp::{DrawEntity, Expr, ForeignContext, Interpreter};
use bimifc_writer::{
    write_ifc, ElementKind, ProjectInfo, Rect, RoomData, RoomDimensions, RoomElement,
};

use crate::ifc_functions::expr_to_string;
use crate::IfcState;
use crate::WriterElement;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expr_to_f64(e: &Expr) -> Option<f64> {
    match e {
        Expr::Real(r) => Some(*r),
        Expr::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

/// Draw a 2D rectangle outline into the drawing for visual feedback.
fn draw_rect_outline(ctx: &mut ForeignContext, x: f64, y: f64, w: f64, d: f64, layer: &str) {
    let lines = [
        (x, y, x + w, y),
        (x + w, y, x + w, y + d),
        (x + w, y + d, x, y + d),
        (x, y + d, x, y),
    ];
    for (x1, y1, x2, y2) in &lines {
        ctx.drawing.entities.push(DrawEntity::Line {
            x1: *x1,
            y1: *y1,
            x2: *x2,
            y2: *y2,
            layer: layer.to_string(),
        });
    }
}

/// Parse common element args: (x y width depth height [z] ["name"]) → values
/// z defaults to 0.0 if a string is found in position 5, or if omitted.
#[allow(clippy::type_complexity)]
fn parse_element_args(args: &[Expr]) -> Option<(f64, f64, f64, f64, f64, f64, Option<String>)> {
    if args.len() < 5 {
        return None;
    }
    let x = expr_to_f64(&args[0])?;
    let y = expr_to_f64(&args[1])?;
    let w = expr_to_f64(&args[2])?;
    let d = expr_to_f64(&args[3])?;
    let h = expr_to_f64(&args[4])?;
    // If arg 5 is a number, it's z_offset; if string, it's name (z defaults to 0)
    let (z, name) = match args.get(5) {
        Some(Expr::String(_)) => (0.0, args.get(5).and_then(expr_to_string)),
        Some(e) => {
            let z = expr_to_f64(e).unwrap_or(0.0);
            let name = args.get(6).and_then(expr_to_string);
            (z, name)
        }
        None => (0.0, None),
    };
    Some((x, y, w, d, h, z, name))
}

#[allow(clippy::too_many_arguments)]
fn add_element(
    ctx: &mut ForeignContext,
    kind: ElementKind,
    x: f64,
    y: f64,
    w: f64,
    d: f64,
    h: f64,
    z: f64,
    name: Option<String>,
) -> Expr {
    let layer = match &kind {
        ElementKind::Wall => "IfcWall",
        ElementKind::Door => "IfcDoor",
        ElementKind::Window => "IfcWindow",
        ElementKind::Opening => "IfcOpening",
        ElementKind::Slab => "IfcSlab",
        ElementKind::RoofGable | ElementKind::RoofHip | ElementKind::RoofBarrel => "IfcRoof",
        ElementKind::Column => "IfcColumn",
        ElementKind::Beam => "IfcBeam",
        ElementKind::Stair => "IfcStair",
        ElementKind::Railing => "IfcRailing",
        ElementKind::Furniture(_) => "IfcFurnishingElement",
    };

    // Draw 2D footprint rectangle
    draw_rect_outline(ctx, x, y, w, d, layer);

    let state = match ctx.user_data.downcast_mut::<IfcState>() {
        Some(s) => s,
        None => return Expr::Nil,
    };

    let id = state.next_writer_id;
    state.next_writer_id += 1;

    state.writer_elements.push(WriterElement {
        id,
        element: RoomElement {
            kind,
            rect: Rect {
                x,
                y,
                width: w,
                height: d,
            },
            rotation: 0.0,
            label: name,
            height: h,
            z_offset: z,
            storey: state.current_storey.clone(),
            color: state.current_color,
        },
    });

    Expr::Integer(id as i32)
}

// ---------------------------------------------------------------------------
// Lisp functions
// ---------------------------------------------------------------------------

/// (ifc-new) → T  — clear writer state, start fresh
fn ifc_new(ctx: &mut ForeignContext, _args: &[Expr]) -> Expr {
    if let Some(state) = ctx.user_data.downcast_mut::<IfcState>() {
        state.writer_elements.clear();
        state.next_writer_id = 1;
        state.project_info = ProjectInfo::default();
        state.current_storey = String::new();
        state.storey_elevations.clear();
        state.current_color = None;
    }
    ctx.drawing.entities.clear();
    Expr::T
}

/// (ifc-add-wall x y width depth height "name") → local-id
fn ifc_add_wall(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-WALL requires (x y width depth height [\"name\"])\n".to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::Wall, x, y, w, d, h, z, name)
}

/// (ifc-add-door x y width depth height "name") → local-id
fn ifc_add_door(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-DOOR requires (x y width depth height [\"name\"])\n".to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::Door, x, y, w, d, h, z, name)
}

/// (ifc-add-window x y width depth height "name") → local-id
fn ifc_add_window(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-WINDOW requires (x y width depth height [\"name\"])\n".to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::Window, x, y, w, d, h, z, name)
}

/// (ifc-add-slab x y width depth height [z] ["name"]) → local-id
fn ifc_add_slab(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-SLAB requires (x y width depth height [z] [\"name\"])\n"
                    .to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::Slab, x, y, w, d, h, z, name)
}

/// (ifc-add-roof x y width depth ridge-height [z] ["name"]) → local-id
/// Gable roof (Satteldach) — triangular profile extruded along depth.
fn ifc_add_roof(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-ROOF requires (x y width depth ridge-height [z] [\"name\"])\n"
                    .to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::RoofGable, x, y, w, d, h, z, name)
}

/// (ifc-add-roof-hip x y width depth ridge-height [z] ["name"]) → local-id
/// Hip/pyramid roof (Walmdach) — trapezoidal profile, good for pagodas.
fn ifc_add_roof_hip(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-ROOF-HIP requires (x y width depth height [z] [\"name\"])\n"
                    .to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::RoofHip, x, y, w, d, h, z, name)
}

/// (ifc-add-roof-barrel x y width depth height [z] ["name"]) → local-id
/// Barrel/arch roof (Tonnendach) — semicircular profile, for Quonset huts.
fn ifc_add_roof_barrel(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-ROOF-BARREL requires (x y width depth height [z] [\"name\"])\n"
                    .to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::RoofBarrel, x, y, w, d, h, z, name)
}

/// (ifc-add-column x y width depth height [z] ["name"]) → local-id
fn ifc_add_column(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-COLUMN requires (x y width depth height [z] [\"name\"])\n"
                    .to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::Column, x, y, w, d, h, z, name)
}

/// (ifc-add-beam x y width depth height [z] ["name"]) → local-id
fn ifc_add_beam(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-BEAM requires (x y width depth height [z] [\"name\"])\n"
                    .to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::Beam, x, y, w, d, h, z, name)
}

/// (ifc-add-stair x y width depth height [z] ["name"]) → local-id
fn ifc_add_stair(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-STAIR requires (x y width depth height [z] [\"name\"])\n"
                    .to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::Stair, x, y, w, d, h, z, name)
}

/// (ifc-add-railing x y width depth height [z] ["name"]) → local-id
fn ifc_add_railing(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let (x, y, w, d, h, z, name) = match parse_element_args(args) {
        Some(v) => v,
        None => {
            ctx.output.push(
                "Error: IFC-ADD-RAILING requires (x y width depth height [z] [\"name\"])\n"
                    .to_string(),
            );
            return Expr::Nil;
        }
    };
    add_element(ctx, ElementKind::Railing, x, y, w, d, h, z, name)
}

/// (ifc-add-furniture x y width depth height "category" "name") → local-id
fn ifc_add_furniture(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    if args.len() < 6 {
        ctx.output.push(
            "Error: IFC-ADD-FURNITURE requires (x y width depth height \"category\" [\"name\"])\n"
                .to_string(),
        );
        return Expr::Nil;
    }
    let x = match expr_to_f64(&args[0]) {
        Some(v) => v,
        None => return Expr::Nil,
    };
    let y = match expr_to_f64(&args[1]) {
        Some(v) => v,
        None => return Expr::Nil,
    };
    let w = match expr_to_f64(&args[2]) {
        Some(v) => v,
        None => return Expr::Nil,
    };
    let d = match expr_to_f64(&args[3]) {
        Some(v) => v,
        None => return Expr::Nil,
    };
    let h = match expr_to_f64(&args[4]) {
        Some(v) => v,
        None => return Expr::Nil,
    };
    let category = match expr_to_string(&args[5]) {
        Some(s) => s,
        None => {
            ctx.output
                .push("Error: IFC-ADD-FURNITURE category must be a string\n".to_string());
            return Expr::Nil;
        }
    };
    let name = args.get(6).and_then(expr_to_string);
    add_element(
        ctx,
        ElementKind::Furniture(category),
        x,
        y,
        w,
        d,
        h,
        0.0,
        name,
    )
}

/// (ifc-color r g b [a]) → T — set surface color for subsequent elements (0.0–1.0)
/// Alpha < 1.0 makes elements transparent (e.g., glass windows).
fn ifc_color(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    if args.len() < 3 {
        ctx.output
            .push("Error: IFC-COLOR requires (r g b [a]) with values 0.0-1.0\n".to_string());
        return Expr::Nil;
    }
    let r = match expr_to_f64(&args[0]) {
        Some(v) => v as f32,
        None => return Expr::Nil,
    };
    let g = match expr_to_f64(&args[1]) {
        Some(v) => v as f32,
        None => return Expr::Nil,
    };
    let b = match expr_to_f64(&args[2]) {
        Some(v) => v as f32,
        None => return Expr::Nil,
    };
    let a = args
        .get(3)
        .and_then(expr_to_f64)
        .map(|v| v as f32)
        .unwrap_or(1.0);
    if let Some(state) = ctx.user_data.downcast_mut::<IfcState>() {
        state.current_color = Some([r, g, b, a]);
    }
    Expr::T
}

/// (ifc-no-color) → T — clear current color, use viewer defaults
fn ifc_no_color(ctx: &mut ForeignContext, _args: &[Expr]) -> Expr {
    if let Some(state) = ctx.user_data.downcast_mut::<IfcState>() {
        state.current_color = None;
    }
    Expr::T
}

/// (ifc-storey "name" [elevation]) → T — set current storey for subsequent elements
fn ifc_storey(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let name = match args.first().and_then(expr_to_string) {
        Some(s) => s,
        None => {
            ctx.output
                .push("Error: IFC-STOREY requires a storey name string\n".to_string());
            return Expr::Nil;
        }
    };
    let elevation = args.get(1).and_then(expr_to_f64);
    if let Some(state) = ctx.user_data.downcast_mut::<IfcState>() {
        state.current_storey = name.clone();
        if let Some(elev) = elevation {
            // Remove old entry if exists, then add new
            state.storey_elevations.retain(|(n, _)| n != &name);
            state.storey_elevations.push((name, elev));
        }
    }
    Expr::T
}

/// (ifc-remove local-id) → T or Nil
fn ifc_remove(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let id = match args.first() {
        Some(Expr::Integer(i)) => *i as u32,
        _ => {
            ctx.output
                .push("Error: IFC-REMOVE requires an integer ID\n".to_string());
            return Expr::Nil;
        }
    };
    if let Some(state) = ctx.user_data.downcast_mut::<IfcState>() {
        let before = state.writer_elements.len();
        state.writer_elements.retain(|e| e.id != id);
        if state.writer_elements.len() < before {
            return Expr::T;
        }
    }
    Expr::Nil
}

/// (ifc-move local-id dx dy) → T or Nil
fn ifc_move(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let id = match args.first() {
        Some(Expr::Integer(i)) => *i as u32,
        _ => {
            ctx.output
                .push("Error: IFC-MOVE requires (id dx dy)\n".to_string());
            return Expr::Nil;
        }
    };
    let dx = match args.get(1).and_then(expr_to_f64) {
        Some(v) => v,
        None => return Expr::Nil,
    };
    let dy = match args.get(2).and_then(expr_to_f64) {
        Some(v) => v,
        None => return Expr::Nil,
    };
    if let Some(state) = ctx.user_data.downcast_mut::<IfcState>() {
        if let Some(we) = state.writer_elements.iter_mut().find(|e| e.id == id) {
            we.element.rect.x += dx;
            we.element.rect.y += dy;
            return Expr::T;
        }
    }
    Expr::Nil
}

/// (ifc-set-project "name" "site" "building" "storey" "author" "org") → T
fn ifc_set_project(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    if args.len() < 6 {
        ctx.output.push("Error: IFC-SET-PROJECT requires (\"name\" \"site\" \"building\" \"storey\" \"author\" \"org\")\n".to_string());
        return Expr::Nil;
    }
    let fields: Vec<String> = args.iter().take(6).filter_map(expr_to_string).collect();
    if fields.len() < 6 {
        ctx.output
            .push("Error: IFC-SET-PROJECT — all 6 arguments must be strings\n".to_string());
        return Expr::Nil;
    }
    if let Some(state) = ctx.user_data.downcast_mut::<IfcState>() {
        state.project_info = ProjectInfo {
            project_name: fields[0].clone(),
            site_name: fields[1].clone(),
            building_name: fields[2].clone(),
            storey_name: fields[3].clone(),
            author: fields[4].clone(),
            organization: fields[5].clone(),
        };
    }
    Expr::T
}

/// (ifc-save "output.ifc") → T or Nil
fn ifc_save(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let path = match args.first() {
        Some(Expr::String(s)) => s.clone(),
        _ => {
            ctx.output
                .push("Error: IFC-SAVE requires a file path string\n".to_string());
            return Expr::Nil;
        }
    };

    let state = match ctx.user_data.downcast_mut::<IfcState>() {
        Some(s) => s,
        None => return Expr::Nil,
    };

    if state.writer_elements.is_empty() {
        ctx.output.push("Error: no elements to save\n".to_string());
        return Expr::Nil;
    }

    // Compute bounding box from all elements
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut max_h: f64 = 0.0;

    for we in &state.writer_elements {
        let r = &we.element.rect;
        min_x = min_x.min(r.x);
        min_y = min_y.min(r.y);
        max_x = max_x.max(r.x + r.width);
        max_y = max_y.max(r.y + r.height);
        max_h = max_h.max(we.element.height);
    }

    let elements: Vec<RoomElement> = state
        .writer_elements
        .iter()
        .map(|we| we.element.clone())
        .collect();
    let project = state.project_info.clone();

    let bbox = Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    };
    let dims = RoomDimensions {
        width: max_x - min_x,
        depth: max_y - min_y,
        height: max_h,
    };

    let mut room = RoomData::new(elements, bbox, dims);
    room.project = project;
    room.storey_elevations = state.storey_elevations.clone();

    let ifc_content = write_ifc(&room);
    match std::fs::write(&path, ifc_content) {
        Ok(_) => {
            ctx.output.push(format!("IFC saved to {}\n", path));
            Expr::T
        }
        Err(e) => {
            ctx.output.push(format!("Error writing IFC: {}\n", e));
            Expr::Nil
        }
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register_all(interp: &mut Interpreter) {
    interp.register_fn("IFC-NEW", ifc_new);
    interp.register_fn("IFC-ADD-WALL", ifc_add_wall);
    interp.register_fn("IFC-ADD-DOOR", ifc_add_door);
    interp.register_fn("IFC-ADD-WINDOW", ifc_add_window);
    interp.register_fn("IFC-ADD-SLAB", ifc_add_slab);
    interp.register_fn("IFC-ADD-ROOF", ifc_add_roof);
    interp.register_fn("IFC-ADD-ROOF-HIP", ifc_add_roof_hip);
    interp.register_fn("IFC-ADD-ROOF-BARREL", ifc_add_roof_barrel);
    interp.register_fn("IFC-ADD-COLUMN", ifc_add_column);
    interp.register_fn("IFC-ADD-BEAM", ifc_add_beam);
    interp.register_fn("IFC-ADD-STAIR", ifc_add_stair);
    interp.register_fn("IFC-ADD-RAILING", ifc_add_railing);
    interp.register_fn("IFC-ADD-FURNITURE", ifc_add_furniture);
    interp.register_fn("IFC-COLOR", ifc_color);
    interp.register_fn("IFC-NO-COLOR", ifc_no_color);
    interp.register_fn("IFC-STOREY", ifc_storey);
    interp.register_fn("IFC-REMOVE", ifc_remove);
    interp.register_fn("IFC-MOVE", ifc_move);
    interp.register_fn("IFC-SET-PROJECT", ifc_set_project);
    interp.register_fn("IFC-SAVE", ifc_save);
}
