// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! bimifc-egui — native desktop IFC viewer
//!
//! Bevy owns the window (3D viewport + camera + picking via the
//! existing `IfcViewerPlugin`), `bevy_egui` paints egui UI on top of
//! that for hierarchy, properties, and discipline-filter controls.
//!
//! See `docs/egui-viewer-plan.md` for the architectural overview.

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use bimifc_bevy::IfcViewerPlugin;

mod app;
mod hierarchy;
mod model;
mod properties;
mod state;
mod toolbar;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "bimifc — native viewer".to_string(),
                resolution: (1440u32, 900u32).into(),
                ..default()
            }),
            ..default()
        }))
        // Match the existing native bin's background so the egui shell
        // looks consistent with bevy-ui builds.
        .insert_resource(ClearColor(Color::srgb(0.1, 0.1, 0.15)))
        // bevy_egui handles input + rendering for egui inside the Bevy
        // render graph. Default settings give us a single-pass UI on
        // top of the 3D scene, which is what we want for a static shell.
        .add_plugins(EguiPlugin::default())
        // Existing bimifc-bevy plugin: parser bridge, camera, picking,
        // mesh upload, lighting, storage. We reuse every system as-is;
        // egui only sits on top.
        .add_plugins(IfcViewerPlugin)
        // Our own additions.
        .add_plugins(model::RichModelPlugin)
        .add_plugins(app::EguiAppPlugin)
        .run();
}
