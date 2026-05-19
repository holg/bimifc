// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Top toolbar — mirrors the leptos toolbar groups:
//!   📁 [open]          | 👁 🎯 🚫 [visibility] | 🏠 ⬚ [view]
//!   💡 [lighting]      | 🏗 🏛 ⚡ 🔧 💨 💡 [discipline filter]
//!   (spacer)           | 🌙/☀ ⌨ LISP [right-side]
//!
//! Tools (Select/Pan/Orbit/Walk/Measure/Section) are wired in the
//! leptos version through localStorage to the Bevy renderer. The egui
//! binary runs Bevy in-process, so we'd hook them directly into the
//! camera/picking resources — left as a follow-up because the parity
//! you need today is panels + the filter UI, not tool modes.

use bevy::prelude::{MessageWriter, Res, ResMut};
use bevy_egui::{egui, EguiContexts};
use bimifc_bevy::{LoadIfcFileEvent, SelectionState};

use crate::state::{DisciplineFilter, LoadedFile};

pub fn render_toolbar(
    mut contexts: EguiContexts,
    mut filter: ResMut<DisciplineFilter>,
    mut load_events: MessageWriter<LoadIfcFileEvent>,
    loaded: Res<LoadedFile>,
    mut selection: ResMut<SelectionState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            // ── File ────────────────────────────────────────────────
            if ui.button("📁 Open").on_hover_text("Open IFC file").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("IFC files", &["ifc", "ifcx"])
                    .pick_file()
                {
                    load_events.write(LoadIfcFileEvent { path });
                }
            }

            ui.separator();

            // ── Visibility (selection-driven) ───────────────────────
            // No "Show All" state to drive class-active from yet (the
            // Bevy native side doesn't expose hidden_ids via a resource
            // we read here). Buttons mutate SelectionState only for now.
            if ui
                .button("👁")
                .on_hover_text("Show All — reset hidden/isolated")
                .clicked()
            {
                // TODO: thread to bimifc-bevy's visibility resource.
            }
            let has_selection = !selection.selected.is_empty();
            if ui
                .add_enabled(has_selection, egui::Button::new("🎯"))
                .on_hover_text("Isolate selection")
                .clicked()
            {
                // TODO
            }
            if ui
                .add_enabled(has_selection, egui::Button::new("🚫"))
                .on_hover_text("Hide selection")
                .clicked()
            {
                // TODO
            }
            if has_selection
                && ui
                    .button("✖")
                    .on_hover_text("Clear selection")
                    .clicked()
            {
                selection.selected.clear();
            }

            ui.separator();

            // ── View commands ───────────────────────────────────────
            // bimifc-bevy's camera plugin reads commands from
            // localStorage in the wasm build; native uses keyboard.
            // These buttons stay visible for parity even though they're
            // not yet wired to the native camera controller.
            if ui.button("🏠").on_hover_text("Home view").clicked() {
                // TODO: send camera reset
            }
            if ui.button("⬚").on_hover_text("Fit all").clicked() {
                // TODO: send camera fit
            }

            ui.separator();

            // ── Lighting toggle ─────────────────────────────────────
            if ui
                .button("💡")
                .on_hover_text("Toggle lighting mode (architectural / photometric / combined)")
                .clicked()
            {
                // TODO: drive PhotometricLightingPlugin when "photometric"
                // feature is enabled on bimifc-bevy.
            }

            ui.separator();

            // ── MEP discipline filter ───────────────────────────────
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

            // ── Right-side: theme/shortcuts/LISP + loaded-file label ─
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Theme + shortcuts + LISP are stub-buttons today.
                // Calling them out so they're visible in the parity
                // demo — wiring follows the same pattern as the rest.
                ui.button("LISP").on_hover_text("AutoLISP REPL");
                ui.button("⌨").on_hover_text("Keyboard shortcuts");
                ui.button("🌙").on_hover_text("Toggle theme");

                ui.separator();

                if let Some(path) = &loaded.path {
                    let name = std::path::Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path);
                    ui.label(format!(
                        "{} — {} entities, {} tri",
                        name, loaded.entity_count, loaded.triangle_count
                    ));
                } else {
                    ui.label("No file loaded");
                }
            });
        });
    });
}

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
