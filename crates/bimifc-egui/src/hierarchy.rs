// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Left side panel — full IFC spatial tree (Project → Site → Building →
//! Storey → Space → Element), mirroring the leptos HierarchyPanel.
//!
//! Differences from leptos:
//! - egui's CollapsingHeader gives us the expand/collapse for free, so
//!   we don't need the FlatRow + virtual-scroll machinery (the panel
//!   tops out at a few thousand nodes; egui handles that comfortably).
//! - Search filter applied recursively, same matching rule as leptos:
//!   match against node.name OR node.entity_type; if any descendant
//!   matches, keep the ancestor visible.

use bevy::prelude::{Local, Res, ResMut};
use bevy_egui::{egui, EguiContexts};
use bimifc_bevy::SelectionState;
use bimifc_model::{SpatialNode, SpatialNodeType};

use crate::model::RichModel;

/// UI-only state local to this panel (search text, scroll position).
#[derive(Default)]
pub struct HierarchyState {
    search: String,
}

pub fn render_hierarchy(
    mut contexts: EguiContexts,
    rich: Res<RichModel>,
    mut selection: ResMut<SelectionState>,
    mut state: Local<HierarchyState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::SidePanel::left("hierarchy")
        .resizable(true)
        .default_width(280.0)
        .min_width(200.0)
        .show(ctx, |ui| {
            ui.heading("Model");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.add(
                    egui::TextEdit::singleline(&mut state.search)
                        .hint_text("filter (name or type)")
                        .desired_width(f32::INFINITY),
                );
            });
            ui.separator();

            let Some(root) = rich.spatial_tree.as_ref() else {
                ui.label("(open an IFC file)");
                return;
            };

            let query = state.search.to_lowercase();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    draw_node(ui, root, &query, &mut selection);
                });
        });
}

fn draw_node(
    ui: &mut egui::Ui,
    node: &SpatialNode,
    query: &str,
    selection: &mut SelectionState,
) {
    if !query.is_empty() && !matches_query(node, query) {
        return;
    }

    let id = node.id.0 as u64;
    let icon = node_icon(&node.node_type, &node.entity_type);
    let label = format!("{} {}", icon, display_label(node));

    if node.children.is_empty() {
        // Leaf — render as a selectable label, no collapsing header.
        let is_selected = selection.selected.contains(&id);
        let resp = ui.selectable_label(is_selected, label);
        if resp.clicked() {
            selection.selected.clear();
            selection.selected.insert(id);
        }
    } else {
        // Branch — collapsing header. Click on the header label
        // selects the node; child rows render inside.
        let header = egui::CollapsingHeader::new(label).id_salt(id);
        header.show(ui, |ui| {
            for child in &node.children {
                draw_node(ui, child, query, selection);
            }
        });
    }
}

/// Same matching rule as leptos's HierarchyPanel: keep a node when its
/// own name/type matches OR any descendant matches. We walk children
/// lazily; the recursion in `draw_node` then filters again per child.
fn matches_query(node: &SpatialNode, query: &str) -> bool {
    if node.name.to_lowercase().contains(query)
        || node.entity_type.to_lowercase().contains(query)
    {
        return true;
    }
    node.children.iter().any(|c| matches_query(c, query))
}

fn node_icon(node_type: &SpatialNodeType, entity_type: &str) -> &'static str {
    // Mirrors `bimifc-leptos::components::hierarchy_panel::get_node_icon`.
    // SpatialNodeType also has Facility / FacilityPart for IFC4X3
    // infrastructure files (bridges, roads), which leptos treats with
    // the same building icon — match that.
    match node_type {
        SpatialNodeType::Project => "📋",
        SpatialNodeType::Site => "🌍",
        SpatialNodeType::Building | SpatialNodeType::Facility => "🏢",
        SpatialNodeType::Storey | SpatialNodeType::FacilityPart => "📐",
        SpatialNodeType::Space => "🚪",
        SpatialNodeType::Element => entity_icon(entity_type),
    }
}

/// Trimmed-down copy of `bimifc-leptos::utils::get_entity_icon`. Picking
/// the most-common icons here; the leptos version has more, but each
/// extra emoji is a one-line addition when needed.
fn entity_icon(entity_type: &str) -> &'static str {
    let upper = entity_type.to_uppercase();
    if upper == "IFCCABLESEGMENT" || upper == "IFCCABLECARRIERSEGMENT" || upper == "IFCCABLECARRIERFITTING" {
        "⚡"
    } else if upper == "IFCPIPESEGMENT" || upper == "IFCPIPEFITTING" {
        "🚰"
    } else if upper == "IFCSPACEHEATER" {
        "🔥"
    } else if upper == "IFCAIRTERMINAL" {
        "🌬"
    } else if upper.contains("WALL") {
        "🧱"
    } else if upper.contains("SLAB") || upper.contains("FLOOR") {
        "⬜"
    } else if upper.contains("ROOF") {
        "🏠"
    } else if upper.contains("BEAM") {
        "➖"
    } else if upper.contains("COLUMN") {
        "⬛"
    } else if upper.contains("DOOR") {
        "🚪"
    } else if upper.contains("WINDOW") {
        "🪟"
    } else if upper.contains("STAIR") {
        "🪜"
    } else if upper.contains("FURNITURE") || upper.contains("FURNISHING") {
        "🪑"
    } else if upper.contains("PIPE") {
        "🚰"
    } else if upper.contains("DUCT") {
        "💨"
    } else if upper.contains("CABLE") || upper.contains("ELECTRIC") {
        "⚡"
    } else if upper.contains("LIGHT") {
        "💡"
    } else {
        "📦"
    }
}

fn display_label(node: &SpatialNode) -> String {
    if !node.name.is_empty() && node.name != "$" {
        if let Some(elev) = node.elevation {
            return format!("{} ({:.2} m)", node.name, elev);
        }
        return node.name.clone();
    }
    let short_type = node
        .entity_type
        .strip_prefix("Ifc")
        .unwrap_or(&node.entity_type);
    format!("{} #{}", short_type, node.id.0)
}
