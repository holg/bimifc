// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Left side panel: entity hierarchy grouped by storey.
//!
//! IfcSceneData carries a flat `Vec<EntityInfo>` plus per-entity storey
//! name. We don't have the full IfcRelAggregates tree on this side
//! (that lives in the parser, not the rendered scene), so we group by
//! storey for now — same approach as the existing bevy-ui hierarchy
//! panel in `bimifc-bevy/src/ui/hierarchy.rs`. The proper spatial tree
//! is a follow-up.

use bevy::prelude::{Res, ResMut};
use bevy_egui::{egui, EguiContexts};
use bimifc_bevy::{IfcSceneData, SelectionState};
use std::collections::BTreeMap;

/// Per-storey expansion state. Lives in app memory only (no persistence).
#[derive(Default)]
pub struct HierarchyState {
    /// Storey name → expanded?
    expanded: BTreeMap<String, bool>,
}

pub fn render_hierarchy(
    mut contexts: EguiContexts,
    scene: Option<Res<IfcSceneData>>,
    mut selection: ResMut<SelectionState>,
    mut hierarchy_state: bevy::prelude::Local<HierarchyState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::SidePanel::left("hierarchy")
        .resizable(true)
        .default_width(260.0)
        .min_width(180.0)
        .show(ctx, |ui| {
            ui.heading("Hierarchy");
            ui.separator();

            let Some(scene) = scene else {
                ui.label("(open an IFC file)");
                return;
            };

            if scene.entities.is_empty() {
                ui.label(format!("(loading… {} meshes)", scene.meshes.len()));
                return;
            }

            // Group entities by storey. Entities without a storey land
            // under an "<unassigned>" group so they're still reachable.
            let mut by_storey: BTreeMap<String, Vec<&bimifc_bevy::EntityInfo>> =
                BTreeMap::new();
            for entity in &scene.entities {
                let key = entity.storey.clone().unwrap_or_else(|| "<unassigned>".into());
                by_storey.entry(key).or_default().push(entity);
            }

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (storey_name, mut entities) in by_storey {
                    // Sort entities within each storey by type then name.
                    // Stable sort keeps insertion order as the tiebreaker.
                    entities.sort_by(|a, b| {
                        a.entity_type
                            .cmp(&b.entity_type)
                            .then_with(|| a.name.cmp(&b.name))
                    });

                    let expanded = hierarchy_state
                        .expanded
                        .entry(storey_name.clone())
                        .or_insert(false);

                    let header = format!("{} ({})", storey_name, entities.len());
                    let response = egui::CollapsingHeader::new(header)
                        .default_open(*expanded)
                        .show(ui, |ui| {
                            for entity in entities {
                                let label = display_label(entity);
                                let is_selected = selection.selected.contains(&entity.id);
                                let resp = ui.selectable_label(is_selected, label);
                                if resp.clicked() {
                                    // Single-select: clear + add
                                    selection.selected.clear();
                                    selection.selected.insert(entity.id);
                                }
                            }
                        });
                    *expanded = response.fully_open();
                }
            });
        });
}

fn display_label(entity: &bimifc_bevy::EntityInfo) -> String {
    let short_type = entity
        .entity_type
        .strip_prefix("Ifc")
        .unwrap_or(&entity.entity_type);
    match &entity.name {
        Some(name) if !name.is_empty() && name != "$" => {
            format!("{} · {}", short_type, name)
        }
        _ => format!("{} #{}", short_type, entity.id),
    }
}
