// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! AutoLISP REPL panel — embedded scripting for IFC drawing and creation.

use crate::bridge::{self, CameraCommand};
use crate::state::use_viewer_state;
use leptos::prelude::*;
use std::cell::RefCell;

thread_local! {
    static INTERP: RefCell<acadlisp::Interpreter> = RefCell::new(bimifc_lisp::create_ifc_interpreter());
}

/// Run Lisp code on the thread-local interpreter, return (output, svg, result).
fn run_lisp(code: &str) -> (String, String, String) {
    INTERP.with(|cell| {
        let interp = &mut *cell.borrow_mut();
        let results = interp.run(code);

        let output = interp.output.join("");
        interp.output.clear();

        let result_str = results
            .iter()
            .filter(|r| **r != acadlisp::Expr::Nil)
            .map(|r| format!("{}", r))
            .collect::<Vec<_>>()
            .join("\n");

        let svg = if !interp.drawing.entities.is_empty() {
            generate_svg(&interp.drawing.entities)
        } else {
            String::new()
        };

        (output, svg, result_str)
    })
}

/// Generate IFC content string from the interpreter's writer elements.
fn generate_ifc_content() -> Option<String> {
    INTERP.with(|cell| bimifc_lisp::generate_ifc(&cell.borrow()))
}

/// Minimal SVG generator for plan-view lines.
fn generate_svg(entities: &[acadlisp::DrawEntity]) -> String {
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;

    for entity in entities {
        if let acadlisp::DrawEntity::Line { x1, y1, x2, y2, .. } = entity {
            min_x = min_x.min(*x1).min(*x2);
            min_y = min_y.min(*y1).min(*y2);
            max_x = max_x.max(*x1).max(*x2);
            max_y = max_y.max(*y1).max(*y2);
        }
    }

    if min_x >= max_x || min_y >= max_y {
        return String::new();
    }

    let padding = 10.0;
    let w = max_x - min_x;
    let h = max_y - min_y;
    let scale = if w > h { 400.0 / w } else { 400.0 / h };
    let svg_w = w * scale + 2.0 * padding;
    let svg_h = h * scale + 2.0 * padding;

    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100%\" viewBox=\"0 0 {:.0} {:.0}\" \
         style=\"background:#1a1a2e;border-radius:4px\">\n",
        svg_w, svg_h
    );
    svg.push_str("<style>\n");
    svg.push_str("  line { stroke: #00ff88; stroke-width: 0.5; }\n");
    svg.push_str("  line.wall { stroke: #ffffff; stroke-width: 1.0; }\n");
    svg.push_str("  line.slab { stroke: #555555; stroke-width: 0.3; }\n");
    svg.push_str("  line.door { stroke: #ff8800; stroke-width: 0.8; }\n");
    svg.push_str("  line.window { stroke: #00aaff; stroke-width: 0.8; }\n");
    svg.push_str("  line.furniture { stroke: #aa44ff; stroke-width: 0.6; }\n");
    svg.push_str("</style>\n");

    for entity in entities {
        if let acadlisp::DrawEntity::Line {
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
            let class = layer_css_class(layer);
            svg.push_str(&format!(
                "  <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" class=\"{}\"/>\n",
                sx1, sy1, sx2, sy2, class
            ));
        }
    }

    svg.push_str("</svg>");
    svg
}

fn layer_css_class(layer: &str) -> &str {
    let l = layer.to_ascii_lowercase();
    if l.contains("wall") {
        "wall"
    } else if l.contains("slab") || l.contains("floor") {
        "slab"
    } else if l.contains("door") {
        "door"
    } else if l.contains("window") {
        "window"
    } else if l.contains("furnish") {
        "furniture"
    } else {
        ""
    }
}

const EXAMPLES: &[(&str, &str)] = &[
    ("Einfamilienhaus", EXAMPLE_HOUSE),
    ("Quonset Hut", EXAMPLE_QUONSET),
    ("Office Tower", EXAMPLE_OFFICE),
    ("Yellow Crane Tower", EXAMPLE_CRANE_TOWER),
    ("Simple Room", EXAMPLE_SIMPLE_ROOM),
];

const EXAMPLE_SIMPLE_ROOM: &str = r#"(ifc-new)
(ifc-set-project "Simple Room" "Site" "Building" "GF" "Architect" "Studio")
(ifc-storey "Ground Floor" 0.0)
(ifc-color 0.75 0.75 0.73)
(ifc-add-slab 0 0 5 4 0.3 0 "Floor")
(ifc-color 0.95 0.93 0.88)
(ifc-add-wall 0 0 5 0.2 2.8 0.3 "North")
(ifc-add-wall 0 0 0.2 4 2.8 0.3 "West")
(ifc-add-wall 4.8 0 0.2 4 2.8 0.3 "East")
(ifc-add-wall 0 3.8 5 0.2 2.8 0.3 "South")
(ifc-color 0.55 0.35 0.18)
(ifc-add-door 1.5 0 1.0 0.2 2.1 0.3 "Door")
(ifc-color 0.60 0.78 0.90)
(ifc-add-window 3 3.8 1.2 0.2 1.5 0.3 "Window")"#;

const EXAMPLE_QUONSET: &str = r#"(ifc-new)
(ifc-set-project "Quonset Hut" "Airfield" "Hangar" "GF" "Engineer" "Corps")

; Quonset hut: semicircular arch roof on a rectangular base
(ifc-storey "Ground Floor" 0.0)
(ifc-color 0.75 0.75 0.73)
(ifc-add-slab 0 0 8 15 0.2 0 "Concrete Pad")

; End walls (gable ends with door)
(ifc-color 0.70 0.72 0.68)
(ifc-add-wall 0 0 8 0.15 4.0 0.2 "Front Wall")
(ifc-add-wall 0 14.85 8 0.15 4.0 0.2 "Rear Wall")
(ifc-color 0.40 0.42 0.38)
(ifc-add-door 3 0 2.0 0.15 3.0 0.2 "Hangar Door")
(ifc-color 0.60 0.78 0.90)
(ifc-add-window 1 14.85 1.5 0.15 1.5 0.2 "Rear Window")
(ifc-add-window 5.5 14.85 1.5 0.15 1.5 0.2 "Rear Window 2")

; Corrugated steel arch roof
(ifc-storey "Roof" 4.0)
(ifc-color 0.60 0.62 0.65)
(ifc-add-roof-barrel 0 0 8 15 3.0 4.0 "Steel Arch")"#;

const EXAMPLE_OFFICE: &str = r#"(ifc-new)
(ifc-set-project "Office Tower" "Downtown" "Tower" "GF" "Arch" "Firm")

; Helper: define floor height
; Ground floor = lobby, upper floors = offices

; === Ground Floor / Lobby ===
(ifc-storey "Lobby" 0.0)
(ifc-color 0.80 0.78 0.75)
(ifc-add-slab 0 0 12 12 0.4 0 "Foundation")
; Glass curtain walls
(ifc-color 0.50 0.65 0.80)
(ifc-add-wall 0 0 12 0.15 4.0 0.4 "Lobby North Glass")
(ifc-add-wall 0 0 0.15 12 4.0 0.4 "Lobby West Glass")
(ifc-add-wall 11.85 0 0.15 12 4.0 0.4 "Lobby East Glass")
(ifc-add-wall 0 11.85 12 0.15 4.0 0.4 "Lobby South Glass")
; Lobby entrance
(ifc-color 0.35 0.38 0.40)
(ifc-add-door 4.5 0 3.0 0.15 3.5 0.4 "Revolving Door")
; Core
(ifc-color 0.85 0.85 0.83)
(ifc-add-wall 4.5 4.5 3 0.15 4.0 0.4 "Core North")
(ifc-add-wall 4.5 4.5 0.15 3 4.0 0.4 "Core West")
(ifc-add-wall 7.35 4.5 0.15 3 4.0 0.4 "Core East")
(ifc-add-wall 4.5 7.35 3 0.15 4.0 0.4 "Core South")
(ifc-color 0.55 0.35 0.18)
(ifc-add-stair 5 5 2 2 4.0 0.4 "Elevator/Stairs")
; Reception
(ifc-color 0.45 0.45 0.50)
(ifc-add-furniture 2 2 3 1.2 1.1 "table" "Reception Desk")

; === Floor 1 ===
(ifc-storey "Floor 1" 4.4)
(ifc-color 0.80 0.78 0.75)
(ifc-add-slab 0 0 12 12 0.3 4.4 "F1 Slab")
(ifc-color 0.92 0.90 0.88)
(ifc-add-wall 0 0 12 0.12 3.2 4.7 "F1 North")
(ifc-add-wall 0 0 0.12 12 3.2 4.7 "F1 West")
(ifc-add-wall 11.88 0 0.12 12 3.2 4.7 "F1 East")
(ifc-add-wall 0 11.88 12 0.12 3.2 4.7 "F1 South")
; Core
(ifc-color 0.85 0.85 0.83)
(ifc-add-wall 4.5 4.5 3 0.12 3.2 4.7 "F1 Core N")
(ifc-add-wall 4.5 4.5 0.12 3 3.2 4.7 "F1 Core W")
(ifc-add-wall 7.38 4.5 0.12 3 3.2 4.7 "F1 Core E")
(ifc-add-wall 4.5 7.38 3 0.12 3.2 4.7 "F1 Core S")
; Windows
(ifc-color 0.55 0.70 0.85)
(ifc-add-window 1 0 2 0.12 2.0 4.7 "F1 Win N1")
(ifc-add-window 5 0 2 0.12 2.0 4.7 "F1 Win N2")
(ifc-add-window 9 0 2 0.12 2.0 4.7 "F1 Win N3")
; Office furniture
(ifc-color 0.55 0.35 0.18)
(ifc-add-furniture 1 1 1.6 0.8 0.75 "table" "Desk 1")
(ifc-add-furniture 1 3 1.6 0.8 0.75 "table" "Desk 2")
(ifc-add-furniture 9 1 1.6 0.8 0.75 "table" "Desk 3")
(ifc-add-furniture 9 3 1.6 0.8 0.75 "table" "Desk 4")

; === Floor 2 ===
(ifc-storey "Floor 2" 7.9)
(ifc-color 0.80 0.78 0.75)
(ifc-add-slab 0 0 12 12 0.3 7.9 "F2 Slab")
(ifc-color 0.92 0.90 0.88)
(ifc-add-wall 0 0 12 0.12 3.2 8.2 "F2 North")
(ifc-add-wall 0 0 0.12 12 3.2 8.2 "F2 West")
(ifc-add-wall 11.88 0 0.12 12 3.2 8.2 "F2 East")
(ifc-add-wall 0 11.88 12 0.12 3.2 8.2 "F2 South")
(ifc-color 0.55 0.70 0.85)
(ifc-add-window 1 0 2 0.12 2.0 8.2 "F2 Win N1")
(ifc-add-window 5 0 2 0.12 2.0 8.2 "F2 Win N2")
(ifc-add-window 9 0 2 0.12 2.0 8.2 "F2 Win N3")

; === Roof ===
(ifc-storey "Roof" 11.4)
(ifc-color 0.65 0.65 0.63)
(ifc-add-slab 0 0 12 12 0.3 11.4 "Roof Slab")"#;

const EXAMPLE_CRANE_TOWER: &str = r#"(ifc-new)
(ifc-set-project "Yellow Crane Tower" "Wuhan" "Tower" "GF" "Ancient" "China")

; Yellow Crane Tower (Huanghelou) — simplified 5-tier pagoda
; Each tier has upswept eaves represented by wider roof overhangs

; === Base Platform ===
(ifc-storey "Platform" 0.0)
(ifc-color 0.70 0.65 0.55)
(ifc-add-slab 0 0 16 16 1.0 0 "Stone Platform")

; === Tier 1 (largest) ===
(ifc-storey "Tier 1" 1.0)
(ifc-color 0.85 0.75 0.55)
(ifc-add-wall 3 3 10 0.3 5.0 1.0 "T1 North")
(ifc-add-wall 3 3 0.3 10 5.0 1.0 "T1 West")
(ifc-add-wall 12.7 3 0.3 10 5.0 1.0 "T1 East")
(ifc-add-wall 3 12.7 10 0.3 5.0 1.0 "T1 South")
; Columns
(ifc-color 0.65 0.20 0.15)
(ifc-add-column 3.3 3.3 0.4 0.4 5.0 1.0 "T1 Col NW")
(ifc-add-column 12.3 3.3 0.4 0.4 5.0 1.0 "T1 Col NE")
(ifc-add-column 3.3 12.3 0.4 0.4 5.0 1.0 "T1 Col SW")
(ifc-add-column 12.3 12.3 0.4 0.4 5.0 1.0 "T1 Col SE")
; Door
(ifc-color 0.55 0.25 0.15)
(ifc-add-door 7 3 2.0 0.3 3.5 1.0 "Main Gate")
; Upswept roof eaves
(ifc-color 0.55 0.22 0.12)
(ifc-add-roof-hip 1 1 14 14 4.0 6.0 "T1 Roof")

; === Tier 2 ===
(ifc-storey "Tier 2" 10.0)
(ifc-color 0.85 0.75 0.55)
(ifc-add-slab 3.5 3.5 9 9 0.3 10.0 "T2 Floor")
(ifc-add-wall 4 4 8 0.25 4.5 10.3 "T2 North")
(ifc-add-wall 4 4 0.25 8 4.5 10.3 "T2 West")
(ifc-add-wall 11.75 4 0.25 8 4.5 10.3 "T2 East")
(ifc-add-wall 4 11.75 8 0.25 4.5 10.3 "T2 South")
(ifc-color 0.65 0.20 0.15)
(ifc-add-column 4.2 4.2 0.35 0.35 4.5 10.3 "T2 Col NW")
(ifc-add-column 11.45 4.2 0.35 0.35 4.5 10.3 "T2 Col NE")
(ifc-add-column 4.2 11.45 0.35 0.35 4.5 10.3 "T2 Col SW")
(ifc-add-column 11.45 11.45 0.35 0.35 4.5 10.3 "T2 Col SE")
(ifc-color 0.55 0.22 0.12)
(ifc-add-roof-hip 2.5 2.5 11 11 3.5 14.8 "T2 Roof")

; === Tier 3 ===
(ifc-storey "Tier 3" 18.3)
(ifc-color 0.85 0.75 0.55)
(ifc-add-slab 4.5 4.5 7 7 0.25 18.3 "T3 Floor")
(ifc-add-wall 5 5 6 0.2 4.0 18.55 "T3 North")
(ifc-add-wall 5 5 0.2 6 4.0 18.55 "T3 West")
(ifc-add-wall 10.8 5 0.2 6 4.0 18.55 "T3 East")
(ifc-add-wall 5 10.8 6 0.2 4.0 18.55 "T3 South")
(ifc-color 0.55 0.22 0.12)
(ifc-add-roof-hip 4 4 8 8 3.0 22.55 "T3 Roof")

; === Tier 4 ===
(ifc-storey "Tier 4" 25.55)
(ifc-color 0.85 0.75 0.55)
(ifc-add-slab 5.5 5.5 5 5 0.2 25.55 "T4 Floor")
(ifc-add-wall 6 6 4 0.2 3.5 25.75 "T4 North")
(ifc-add-wall 6 6 0.2 4 3.5 25.75 "T4 West")
(ifc-add-wall 9.8 6 0.2 4 3.5 25.75 "T4 East")
(ifc-add-wall 6 9.8 4 0.2 3.5 25.75 "T4 South")
(ifc-color 0.55 0.22 0.12)
(ifc-add-roof-hip 5 5 6 6 2.5 29.25 "T4 Roof")

; === Tier 5 (top) ===
(ifc-storey "Tier 5" 31.75)
(ifc-color 0.90 0.80 0.60)
(ifc-add-slab 6.5 6.5 3 3 0.15 31.75 "T5 Floor")
(ifc-add-wall 6.8 6.8 2.4 0.15 3.0 31.9 "T5 North")
(ifc-add-wall 6.8 6.8 0.15 2.4 3.0 31.9 "T5 West")
(ifc-add-wall 9.05 6.8 0.15 2.4 3.0 31.9 "T5 East")
(ifc-add-wall 6.8 9.05 2.4 0.15 3.0 31.9 "T5 South")
(ifc-color 0.60 0.25 0.10)
(ifc-add-roof-hip 6 6 4 4 3.0 34.9 "Crown Roof")"#;

const EXAMPLE_HOUSE: &str = r#"(ifc-new)
(ifc-set-project "Einfamilienhaus" "Grundstueck" "Haus" "EG" "Architekt" "Buero")

; === Erdgeschoss / Ground Floor ===
(ifc-storey "Erdgeschoss" 0.0)
(ifc-color 0.75 0.75 0.73)
(ifc-add-slab 0 0 10 8 0.3 0 "Bodenplatte")
; Exterior walls - white plaster
(ifc-color 0.95 0.93 0.88)
(ifc-add-wall 0 0 10 0.3 2.8 0.3 "Nordwand")
(ifc-add-wall 0 0 0.3 8 2.8 0.3 "Westwand")
(ifc-add-wall 9.7 0 0.3 8 2.8 0.3 "Ostwand")
(ifc-add-wall 0 7.7 10 0.3 2.8 0.3 "Suedwand")
; Interior walls - lighter
(ifc-color 0.92 0.90 0.85)
(ifc-add-wall 4.5 0.3 0.15 4 2.8 0.3 "Trennwand Kueche")
(ifc-add-wall 4.5 4.3 0.15 3.4 2.8 0.3 "Trennwand Flur")
; Openings - wood brown door (thinner than wall, offset outward)
(ifc-color 0.55 0.35 0.18)
(ifc-add-door 2 -0.02 1.0 0.1 2.1 0.3 "Haustuer")
(ifc-add-door 4.5 5 0.15 0.1 2.1 0.3 "Zimmertuer")
; Windows - transparent glass
(ifc-color 0.60 0.78 0.92 0.35)
(ifc-add-window 6 -0.02 1.5 0.08 1.5 1.2 "Wohnzimmerfenster")
(ifc-add-window -0.02 3 0.08 1.2 1.5 1.2 "Kuechenfenster")
(ifc-add-window 6 7.72 1.5 0.08 1.5 1.2 "Suedfenster")
; Furniture - Wohnzimmer
(ifc-color 0.40 0.45 0.55)
(ifc-add-furniture 6 4 2.2 0.9 0.5 "sofa" "Sofa")
(ifc-color 0.55 0.35 0.18)
(ifc-add-furniture 6.5 2.5 1.0 0.5 0.45 "table" "Couchtisch")
; Furniture - Kueche
(ifc-color 0.85 0.85 0.85)
(ifc-add-furniture 1 1 0.6 0.6 0.9 "stove" "Herd")
(ifc-add-furniture 2 1 0.6 0.6 0.9 "sink" "Spuele")
(ifc-color 0.90 0.90 0.92)
(ifc-add-furniture 3 1 0.6 0.6 0.9 "refrigerator" "Kuehlschrank")
; Staircase - wood
(ifc-color 0.55 0.35 0.18)
(ifc-add-stair 8 5 1.2 3 2.8 0.3 "Treppe")

; === Obergeschoss / First Floor ===
(ifc-storey "Obergeschoss" 3.1)
(ifc-color 0.75 0.75 0.73)
(ifc-add-slab 0 0 10 8 0.3 3.1 "OG Decke")
; Exterior walls
(ifc-color 0.95 0.93 0.88)
(ifc-add-wall 0 0 10 0.3 2.6 3.4 "OG Nordwand")
(ifc-add-wall 0 0 0.3 8 2.6 3.4 "OG Westwand")
(ifc-add-wall 9.7 0 0.3 8 2.6 3.4 "OG Ostwand")
(ifc-add-wall 0 7.7 10 0.3 2.6 3.4 "OG Suedwand")
; Interior walls
(ifc-color 0.92 0.90 0.85)
(ifc-add-wall 5 0.3 0.15 3.5 2.6 3.4 "OG Flurwand")
(ifc-add-wall 5 4.5 0.15 3.2 2.6 3.4 "OG Badwand")
; Windows - transparent glass
(ifc-color 0.60 0.78 0.92 0.35)
(ifc-add-window 2 -0.02 1.5 0.08 1.5 4.3 "Schlafzimmerfenster")
(ifc-add-window 7 -0.02 1.5 0.08 1.5 4.3 "Arbeitszimmerfenster")
(ifc-add-window 2 7.72 1.2 0.08 1.2 4.3 "Badfenster")
; Schlafzimmer
(ifc-color 0.55 0.35 0.18)
(ifc-add-furniture 1 2 2.0 1.6 0.5 "bed" "Doppelbett")
(ifc-color 0.45 0.30 0.15)
(ifc-add-furniture 1 5.5 1.2 0.5 0.8 "storage" "Kleiderschrank")
; Arbeitszimmer
(ifc-color 0.55 0.35 0.18)
(ifc-add-furniture 7 2 1.4 0.7 0.75 "table" "Schreibtisch")
(ifc-color 0.30 0.30 0.30)
(ifc-add-furniture 7.5 3 0.5 0.5 0.45 "chair" "Stuhl")
; Bad
(ifc-color 0.92 0.92 0.95)
(ifc-add-furniture 1 6.5 0.6 0.4 0.85 "sink" "Waschbecken")
(ifc-add-furniture 2.5 6 1.7 0.75 0.6 "bathtub" "Badewanne")
(ifc-add-furniture 1 5.5 0.4 0.7 0.4 "toilet" "WC")

; === Dach / Roof ===
(ifc-storey "Dach" 6.0)
(ifc-color 0.65 0.32 0.22)
(ifc-add-roof 0 0 10 8 3.0 6.0 "Satteldach")"#;

/// Lisp REPL panel component
#[component]
pub fn LispPanel() -> impl IntoView {
    let state = use_viewer_state();
    let code = RwSignal::new(EXAMPLE_HOUSE.to_string());
    let output = RwSignal::new(String::new());
    let svg_content = RwSignal::new(String::new());
    let entity_count = RwSignal::new(0usize);

    let do_run = move || {
        let src = code.get();
        let (out, svg, result) = run_lisp(&src);

        let count = INTERP.with(|cell| cell.borrow().drawing.entities.len());
        entity_count.set(count);

        let mut display = String::new();
        if !out.is_empty() {
            display.push_str(&out);
        }
        if !result.is_empty() {
            if !display.is_empty() {
                display.push('\n');
            }
            display.push_str(&result);
        }
        output.set(display);
        svg_content.set(svg);
    };

    let do_send_3d = move || {
        if let Some(ifc_content) = generate_ifc_content() {
            let ifc_size = ifc_content.len();
            bridge::log_info(&format!("[LISP] Generated IFC: {} bytes", ifc_size));

            // Clear any stale cache so we always re-parse
            bridge::clear_model_cache();

            state.scene.set_file_name("lisp_model.ifc".to_string());
            state.loading.set_loading(true);
            state.loading.set_progress(crate::state::Progress {
                phase: "Parsing LISP model".to_string(),
                percent: 10.0,
            });

            match crate::components::toolbar::parse_and_process_ifc(&ifc_content, state) {
                Ok(_) => {
                    bridge::log_info("[LISP] Model sent to 3D viewer");
                    state.loading.set_loading(false);
                    state.loading.clear_progress();
                    bridge::save_camera_cmd(&CameraCommand {
                        cmd: "fit_all".to_string(),
                        mode: None,
                    });
                    output.update(|o| {
                        o.push_str(&format!("\n=> Sent to 3D viewer ({} bytes IFC)", ifc_size));
                    });
                }
                Err(e) => {
                    bridge::log_error(&format!("[LISP] Failed to send to 3D: {}", e));
                    state.loading.set_loading(false);
                    output.update(|o| {
                        o.push_str(&format!("\nError sending to 3D: {}", e));
                    });
                }
            }
        } else {
            output.update(|o| {
                o.push_str("\nNo elements to send — run code first");
            });
        }
    };

    view! {
        <div class="lisp-panel">
            // Header bar with title and action buttons
            <div style="display:flex;align-items:center;justify-content:space-between;padding:6px 8px;background:#252526;border-bottom:1px solid #3d3d3d;flex-shrink:0;min-height:32px">
                <span style="font-weight:600;font-size:12px;color:#0a84ff;letter-spacing:0.5px">"AutoLISP REPL"</span>
                <div style="display:flex;gap:6px;align-items:center">
                    <select
                        style="padding:3px 8px;font-size:11px;background:#2d2d2d;color:#fff;border:1px solid #4d4d4d;border-radius:4px;cursor:pointer"
                        on:change=move |ev| {
                            let idx: usize = event_target_value(&ev).parse().unwrap_or(0);
                            if idx < EXAMPLES.len() {
                                code.set(EXAMPLES[idx].1.to_string());
                            }
                        }
                    >
                        {EXAMPLES.iter().enumerate().map(|(i, (name, _))| {
                            view! { <option value={i.to_string()}>{*name}</option> }
                        }).collect::<Vec<_>>()}
                    </select>
                    <button
                        style="padding:4px 12px;font-size:12px;background:#2d2d2d;color:#fff;border:1px solid #4d4d4d;border-radius:4px;cursor:pointer"
                        on:click=move |_| {
                            run_lisp("(ifc-new)");
                            output.set(String::new());
                            svg_content.set(String::new());
                            entity_count.set(0);
                        }
                        title="Clear drawing and state"
                    >
                        "Clear"
                    </button>
                    <button
                        style="padding:4px 14px;font-size:12px;font-weight:700;background:#30d158;color:#000;border:1px solid #30d158;border-radius:4px;cursor:pointer"
                        on:click=move |_| do_run()
                        title="Run code (Ctrl+Enter)"
                    >
                        "Run"
                    </button>
                    <button
                        style="padding:4px 14px;font-size:12px;font-weight:700;background:#0a84ff;color:#fff;border:1px solid #0a84ff;border-radius:4px;cursor:pointer"
                        on:click=move |_| { do_run(); do_send_3d(); }
                        title="Run + Send to 3D viewer (Ctrl+Shift+Enter)"
                    >
                        "3D"
                    </button>
                    <button
                        style="background:none;border:none;color:#8e8e93;font-size:18px;cursor:pointer;padding:0 4px;line-height:1"
                        on:click=move |_| state.ui.toggle_lisp_panel()
                        title="Close REPL"
                    >
                        "x"
                    </button>
                </div>
            </div>
            // Body: editor + preview
            <div class="lisp-panel-body">
                <div class="lisp-editor-col">
                    <textarea
                        class="lisp-code-input"
                        prop:value=move || code.get()
                        on:input=move |ev| {
                            code.set(event_target_value(&ev));
                        }
                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                            if (ev.ctrl_key() || ev.meta_key()) && ev.key() == "Enter" {
                                ev.prevent_default();
                                do_run();
                                if ev.shift_key() {
                                    do_send_3d();
                                }
                            }
                        }
                        spellcheck="false"
                        placeholder="; Type AutoLISP code here...\n; Ctrl+Enter = Run | Ctrl+Shift+Enter = Run + 3D"
                    />
                    <div class="lisp-output-area">
                        <div class="lisp-output-header">
                            "Output"
                            <span class="lisp-entity-count">
                                {move || {
                                    let c = entity_count.get();
                                    if c > 0 { format!(" ({} lines)", c) } else { String::new() }
                                }}
                            </span>
                        </div>
                        <pre class="lisp-output">{move || output.get()}</pre>
                    </div>
                </div>
                <div class="lisp-preview-col">
                    <div class="lisp-preview-header">"Plan View"</div>
                    <div
                        class="lisp-svg-preview"
                        inner_html=move || svg_content.get()
                    />
                </div>
            </div>
        </div>
    }
}
