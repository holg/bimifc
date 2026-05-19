// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Right side panel: details of the selected entity.
//!
//! IfcSceneData only carries id/entity_type/name/storey per entity; the
//! richer property-set data is in the parser model, which we don't
//! currently expose to the renderer. So this panel shows the metadata
//! we DO have — full property-set extraction is a follow-up that needs
//! either passing the resolver into the egui app or extending
//! IfcSceneData to carry pre-extracted property sets.

use bevy::prelude::Res;
use bevy_egui::{egui, EguiContexts};
use bimifc_bevy::{IfcSceneData, SelectionState};

pub fn render_properties(
    mut contexts: EguiContexts,
    scene: Option<Res<IfcSceneData>>,
    selection: Res<SelectionState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::SidePanel::right("properties")
        .resizable(true)
        .default_width(320.0)
        .min_width(220.0)
        .show(ctx, |ui| {
            ui.heading("Properties");
            ui.separator();

            let Some(scene) = scene else {
                ui.label("(no scene loaded)");
                return;
            };

            // Single-selection model for now: take the first selected ID
            // (we never put more than one in there from the hierarchy
            // panel; picking might do multi-select later).
            let Some(&selected_id) = selection.selected.iter().next() else {
                ui.label("(nothing selected)");
                ui.add_space(8.0);
                ui.label(format!(
                    "Scene: {} entities, {} meshes",
                    scene.entities.len(),
                    scene.meshes.len(),
                ));
                if let Some(bounds) = &scene.bounds {
                    ui.label(format!(
                        "Bounds: {:.1} × {:.1} × {:.1}",
                        bounds.size().x,
                        bounds.size().y,
                        bounds.size().z,
                    ));
                }
                return;
            };

            let Some(entity) = scene.entities.iter().find(|e| e.id == selected_id) else {
                ui.label(format!("(entity #{} not in scene)", selected_id));
                return;
            };

            egui::Grid::new("properties_grid")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("ID");
                    ui.label(format!("#{}", entity.id));
                    ui.end_row();

                    ui.label("Type");
                    ui.label(&entity.entity_type);
                    ui.end_row();

                    if let Some(name) = &entity.name {
                        ui.label("Name");
                        ui.label(name);
                        ui.end_row();
                    }

                    if let Some(storey) = &entity.storey {
                        ui.label("Storey");
                        ui.label(storey);
                        ui.end_row();
                    }

                    if let Some(elev) = entity.storey_elevation {
                        ui.label("Elevation");
                        ui.label(format!("{:.2} m", elev));
                        ui.end_row();
                    }
                });

            ui.add_space(8.0);
            ui.separator();
            ui.label(
                egui::RichText::new(
                    "Property-set / quantity details live in the parser \
                     model and aren't surfaced to the renderer yet.",
                )
                .small()
                .weak(),
            );
        });
}
