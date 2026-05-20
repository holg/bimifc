// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Top toolbar — file open + MEP discipline filter + sources list.
//!
//! Mirrors the leptos toolbar group structure: File / Visibility / View
//! / Lighting / Discipline-filter / right-side status. Discipline state
//! lives in `bimifc-bevy::FederationState` (the shared federation
//! registry from `bimifc-federation`), so every viewer that consumes
//! the same Bevy resource sees the same filter.
//!
//! The right-side area lists the loaded sources with their inferred
//! discipline so the user can see at a glance which file is which —
//! and toggle per-source visibility independently of the discipline
//! filter (handy for "show me HVAC + Plumbing but not Cooling").

use bevy::prelude::{MessageWriter, Res, ResMut};
use bevy_egui::{egui, EguiContexts};
use bimifc_bevy::{FederationState, LoadIfcFileEvent, SelectionState};
use bimifc_federation::{Discipline, ViewFilter};

pub fn render_toolbar(
    mut contexts: EguiContexts,
    mut federation: ResMut<FederationState>,
    mut load_events: MessageWriter<LoadIfcFileEvent>,
    mut selection: ResMut<SelectionState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // ── File ────────────────────────────────────────────────
            if ui
                .button("📁 Open")
                .on_hover_text("Open IFC file — adds to the federated scene")
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("IFC files", &["ifc", "ifcx"])
                    .pick_file()
                {
                    load_events.write(LoadIfcFileEvent { path });
                }
            }

            ui.separator();

            // ── Visibility (selection-driven; native wiring is a TODO).
            let has_selection = !selection.selected.is_empty();
            if ui
                .button("👁")
                .on_hover_text("Show All — reset hidden/isolated")
                .clicked()
            {
                // TODO: thread to bimifc-bevy's visibility resource.
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("🎯"))
                .on_hover_text("Isolate selection")
                .clicked()
            { /* TODO */ }
            if ui
                .add_enabled(has_selection, egui::Button::new("🚫"))
                .on_hover_text("Hide selection")
                .clicked()
            { /* TODO */ }
            if has_selection
                && ui.button("✖").on_hover_text("Clear selection").clicked()
            {
                selection.selected.clear();
            }

            ui.separator();

            // ── View commands ───────────────────────────────────────
            if ui.button("🏠").on_hover_text("Home view").clicked() { /* TODO */ }
            if ui.button("⬚").on_hover_text("Fit all").clicked() { /* TODO */ }

            ui.separator();

            // ── Lighting toggle ─────────────────────────────────────
            if ui
                .button("💡")
                .on_hover_text("Toggle lighting mode (architectural / photometric / combined)")
                .clicked()
            {
                // TODO: drive PhotometricLightingPlugin
            }

            ui.separator();

            // ── MEP discipline filter ───────────────────────────────
            // Reads/writes FederationState::scene::filter directly so
            // every viewer that shares the resource (today: egui;
            // tomorrow: leptos web + ratatui via the same crate) sees
            // the toggle.
            for variant in ViewFilter::ALL_VARIANTS {
                let active = federation.scene.filter == variant;
                let label = format!("{} {}", variant.icon(), short_label(variant));
                if ui
                    .selectable_label(active, label)
                    .on_hover_text(variant.label())
                    .clicked()
                {
                    federation.scene.filter = variant;
                }
            }

            // ── Right-side: source list + filename status ───────────
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let counts = federation.scene.sources().len();
                ui.label(format!(
                    "{} source{}",
                    counts,
                    if counts == 1 { "" } else { "s" }
                ));
            });
        });
    });
}

fn short_label(filter: ViewFilter) -> &'static str {
    match filter {
        ViewFilter::All => "All",
        ViewFilter::Architecture => "Arch",
        ViewFilter::Discipline(Discipline::Electrical) => "Elec",
        ViewFilter::Discipline(Discipline::Plumbing) => "Plumb",
        ViewFilter::Discipline(Discipline::Hvac) => "HVAC",
        ViewFilter::Discipline(Discipline::Lighting) => "Light",
        ViewFilter::Discipline(Discipline::Other) => "Other",
    }
}
