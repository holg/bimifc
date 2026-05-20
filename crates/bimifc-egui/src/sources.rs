// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! "Sources" panel — lists every loaded IFC file with its inferred
//! discipline, triangle count and per-source visibility toggle.
//!
//! Federated workflow:
//!   1. User loads LTU_A-House_redesign.ifc — registered as source #0,
//!      no whole-file discipline hint (the filename doesn't match a
//!      keyword), per-entity classification applies.
//!   2. User loads LTU_A-House_Air.ifc — registered as #1 with
//!      discipline = HVAC inferred from the filename.
//!   3. User loads LTU_A-House_Plumbing.ifc — source #2, Plumbing.
//!
//! Now the toolbar discipline buttons toggle WHOLE-FILE visibility for
//! the keyed sources (HVAC button → only show source #1 + the
//! architectural file), and per-entity tags still gate inside source #0.

use bevy::prelude::ResMut;
use bevy_egui::{egui, EguiContexts};
use bimifc_bevy::FederationState;
use bimifc_federation::Discipline;

pub fn render_sources(
    mut contexts: EguiContexts,
    mut federation: ResMut<FederationState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::TopBottomPanel::bottom("sources_panel")
        .resizable(true)
        .default_height(140.0)
        .min_height(40.0)
        .show(ctx, |ui| {
            ui.heading("Sources");
            if federation.scene.sources().is_empty() {
                ui.label(
                    egui::RichText::new("(no IFC loaded — File → Open to add a discipline file)")
                        .small()
                        .weak(),
                );
                return;
            }

            // Snapshot the sources list so the mutable borrow ends
            // before we reach back in via `source_mut`. egui's table
            // wants the data inline; we collect first, edit second.
            let snapshot: Vec<_> = federation
                .scene
                .sources()
                .iter()
                .enumerate()
                .map(|(idx, s)| {
                    (
                        idx,
                        s.display_name.clone(),
                        s.discipline,
                        s.entity_count,
                        s.triangle_count,
                        s.visible,
                    )
                })
                .collect();

            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    egui::Grid::new("sources_grid")
                        .num_columns(5)
                        .spacing([12.0, 4.0])
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label(egui::RichText::new("Visible").small().weak());
                            ui.label(egui::RichText::new("File").strong());
                            ui.label(egui::RichText::new("Discipline").strong());
                            ui.label(egui::RichText::new("Entities").small().weak());
                            ui.label(egui::RichText::new("Triangles").small().weak());
                            ui.end_row();

                            for (idx, name, disc, ecount, tcount, mut visible) in snapshot {
                                if ui.checkbox(&mut visible, "").changed() {
                                    if let Some(s) = federation
                                        .scene
                                        .source_mut(bimifc_federation::SourceId(idx as u32))
                                    {
                                        s.visible = visible;
                                    }
                                }
                                ui.label(format!("#{} {}", idx, name));
                                ui.label(discipline_label(disc));
                                ui.label(format!("{ecount}"));
                                ui.label(format!("{tcount}"));
                                ui.end_row();
                            }
                        });
                });
        });
}

fn discipline_label(d: Option<Discipline>) -> String {
    match d {
        None => "(per-entity)".to_string(),
        Some(d) => format!("{} {}", d.icon(), d.short()),
    }
}
