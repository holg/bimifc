// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Top toolbar: file open + discipline filter buttons.
//!
//! Layout (left → right):
//!   [Open IFC…]   [🏗 All] [🏛 Arch] [⚡ Elec] [🔧 Plumb] [💨 HVAC] [💡 Light]
//!   spacer
//!   Loaded: <filename> — <triangles> tri

// Bevy 0.18 renamed Event → Message at the ECS level: the writer side
// is now MessageWriter, kept in the prelude.
use bevy::prelude::{MessageWriter, Res, ResMut};
use bevy_egui::{egui, EguiContexts};
use bimifc_bevy::LoadIfcFileEvent;

use crate::state::{DisciplineFilter, LoadedFile};

/// System that draws the top toolbar each frame.
pub fn render_toolbar(
    mut contexts: EguiContexts,
    mut filter: ResMut<DisciplineFilter>,
    mut load_events: MessageWriter<LoadIfcFileEvent>,
    loaded: Res<LoadedFile>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // ── File controls ───────────────────────────────────────
            if ui.button("📂 Open IFC…").clicked() {
                // rfd is synchronous on the calling thread — fine for a
                // file-picker because the OS owns the modal anyway, and
                // we're already on the main thread inside an egui system.
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("IFC files", &["ifc", "ifcx"])
                    .pick_file()
                {
                    load_events.write(LoadIfcFileEvent { path });
                }
            }

            ui.separator();

            // ── Discipline filter buttons ───────────────────────────
            for variant in DisciplineFilter::ALL_VARIANTS {
                let label = format!("{} {}", variant.icon(), short_label(variant));
                if ui
                    .selectable_label(*filter == variant, label)
                    .on_hover_text(variant.label())
                    .clicked()
                {
                    *filter = variant;
                }
            }

            // ── Right-side status (loaded file + counts) ────────────
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(ref path) = loaded.path {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path);
                    ui.label(format!(
                        "{} — {} entities, {} triangles",
                        name, loaded.entity_count, loaded.triangle_count
                    ));
                } else {
                    ui.label("No file loaded");
                }
            });
        });
    });
}

/// Short label for the toolbar (the full label goes in the tooltip).
fn short_label(filter: DisciplineFilter) -> &'static str {
    match filter {
        DisciplineFilter::All => "All",
        DisciplineFilter::Architecture => "Arch",
        DisciplineFilter::Electrical => "Elec",
        DisciplineFilter::Plumbing => "Plumb",
        DisciplineFilter::Hvac => "HVAC",
        DisciplineFilter::Lighting => "Light",
    }
}
