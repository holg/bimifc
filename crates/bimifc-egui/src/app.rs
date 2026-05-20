// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Egui shell plugin — installs the UI resources and wires the per-frame
//! UI systems. The 3D viewport is already provided by `IfcViewerPlugin`
//! from `bimifc-bevy`; we only add the surrounding chrome here.
//!
//! Federation state (the source list + active ViewFilter) lives in the
//! `FederationState` resource installed by `IfcViewerPlugin`, so we
//! don't init it ourselves. Each egui system reads/writes that
//! resource directly.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass, PrimaryEguiContext};
use bimifc_bevy::FederationState;

use crate::hierarchy::render_hierarchy;
use crate::properties::render_properties;
use crate::sources::render_sources;
use crate::toolbar::render_toolbar;

pub struct EguiAppPlugin;

impl Plugin for EguiAppPlugin {
    fn build(&self, app: &mut App) {
        // bevy_egui 0.39 only emits an EguiPrimaryContextPass for a
        // camera tagged `PrimaryEguiContext`. bimifc-bevy's camera is
        // spawned in its CameraPlugin's Startup without that tag, so
        // we add it post-startup. Without this, every panel system's
        // `contexts.ctx_mut()?` returns NoEntities and the UI silently
        // vanishes.
        app.add_systems(PostStartup, tag_primary_egui_camera)
            // Egui systems run inside the EguiPrimaryContextPass schedule
            // — bevy_egui 0.39's contract. Order is important so panels
            // claim screen space in a predictable layout: top toolbar
            // first, then sources at the bottom, then left/right side
            // panels, then the status bar. egui resolves layout in the
            // order systems run.
            .add_systems(
                EguiPrimaryContextPass,
                (
                    render_toolbar,
                    render_sources,
                    render_hierarchy,
                    render_properties,
                    render_status_bar,
                )
                    .chain(),
            );
    }
}

/// Add `PrimaryEguiContext` to the 3D camera that bimifc-bevy's
/// CameraPlugin spawned during Startup, so bevy_egui will run its
/// primary-context pass for it.
fn tag_primary_egui_camera(
    mut commands: Commands,
    cameras: Query<Entity, (With<Camera3d>, Without<PrimaryEguiContext>)>,
) {
    for cam in cameras.iter() {
        commands.entity(cam).insert(PrimaryEguiContext);
    }
}

/// Bottom status bar — shows the active discipline filter + a hint.
fn render_status_bar(mut contexts: EguiContexts, federation: Res<FederationState>) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(format!("View: {}", federation.scene.filter.label()));
            let total = federation.scene.total_triangles();
            let vis = federation.scene.visible_triangles();
            if total > 0 {
                ui.separator();
                ui.label(format!("{vis} / {total} triangles visible"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label("bimifc-egui — federated viewer");
            });
        });
    });
}
