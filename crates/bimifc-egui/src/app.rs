// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Egui shell plugin — installs the UI resources and wires the per-frame
//! UI systems. The 3D viewport is already provided by `IfcViewerPlugin`
//! from `bimifc-bevy`; we only add the surrounding chrome here.
//!
//! Walking-skeleton scope (per `docs/egui-viewer-plan.md`):
//!   ✅ window opens
//!   ✅ IFC can be loaded via File menu
//!   ✅ 3D shows
//!   ✅ discipline-filter buttons exist and update a shared resource
//!
//! Post-skeleton work (not in this commit): hierarchy panel, properties
//! panel, filter wired into the renderer, persistence, shortcuts.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};
use bimifc_bevy::IfcSceneData;

use crate::hierarchy::render_hierarchy;
use crate::properties::render_properties;
use crate::state::{DisciplineFilter, LoadedFile};
use crate::toolbar::render_toolbar;

pub struct EguiAppPlugin;

impl Plugin for EguiAppPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DisciplineFilter>()
            .init_resource::<LoadedFile>()
            // Egui systems run inside the EguiPrimaryContextPass schedule
            // — bevy_egui 0.39's contract: "draw UI now that egui has a
            // frame context available". Order matters for layout: the
            // top toolbar carves space first, then the side panels each
            // take their slice, then the bottom status bar, and what's
            // left in the middle is the 3D viewport. `.chain()` forces
            // that left-to-right declaration order.
            .add_systems(
                EguiPrimaryContextPass,
                (
                    render_toolbar,
                    render_status_bar,
                    render_hierarchy,
                    render_properties,
                )
                    .chain(),
            )
            // Mirror IfcSceneData → LoadedFile so the toolbar status
            // updates whenever a new file finishes parsing.
            .add_systems(Update, sync_loaded_file);
    }
}

/// Bottom status bar — short and quiet, holds the discipline-filter
/// label + any future hints (selected entity, picking ray, etc.).
fn render_status_bar(mut contexts: EguiContexts, filter: Res<DisciplineFilter>) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("View: {}", filter.label()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("bimifc-egui v0.1 (walking skeleton)");
            });
        });
    });
}

/// Watch IfcSceneData and reflect its size into LoadedFile. We don't
/// know the source path from here (the loader event carried it but the
/// resource only carries the parsed data) — TODO once the loader plugin
/// is extended to surface the source path.
fn sync_loaded_file(scene: Option<Res<IfcSceneData>>, mut loaded: ResMut<LoadedFile>) {
    let Some(scene) = scene else { return };
    if !scene.is_changed() {
        return;
    }
    loaded.entity_count = scene.entities.len();
    loaded.triangle_count = scene
        .meshes
        .iter()
        .map(|m| m.geometry.indices.len() / 3)
        .sum();
}
