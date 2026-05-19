// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Rich model data needed by the egui panels — spatial tree, per-entity
//! property sets and quantities — kept as a Bevy resource so the
//! panels can read it without re-parsing.
//!
//! `bimifc-bevy`'s loader builds an `IfcSceneData` with a flat list of
//! id/type/name/storey, intentionally minimal so the renderer stays
//! lean. The leptos web viewer extracts the rest in its parse pass.
//! For panel parity in the egui binary we re-parse the same file with
//! the parser's `build_spatial=true, extract_properties=true` flags
//! and stash the results.
//!
//! Re-parsing is cheap: ~2.3s for the 198 MB medical clinic on native,
//! and it runs once per file open. The geometry pipeline in
//! `bimifc-bevy` already pays the full parse cost — we just don't want
//! to thread an `Arc<ParsedModel>` through their loader API.

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use bimifc_model::{EntityId, IfcModel, IfcType, SpatialNode};
use bimifc_parser::ParsedModel;
use log::warn;
use rustc_hash::FxHashMap;

use bimifc_bevy::LoadIfcFileEvent;

/// Per-entity rich detail used by the properties panel. Cached at file
/// load so the panel doesn't pay the property-set lookup cost on every
/// click.
#[derive(Clone, Debug, Default)]
pub struct EntityDetail {
    pub id: u64,
    pub entity_type: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub global_id: Option<String>,
    pub property_sets: Vec<bimifc_model::PropertySet>,
    pub quantities: Vec<bimifc_model::Quantity>,
}

/// Resource holding parsed data needed by the panels.
///
/// `None` before any file is loaded. When a `LoadIfcFileEvent` fires
/// we re-parse the same path in the background system and replace this
/// resource wholesale.
#[derive(Resource, Default)]
pub struct RichModel {
    pub source_path: Option<PathBuf>,
    pub spatial_tree: Option<SpatialNode>,
    /// Map from entity ID to richly-extracted details. Built lazily —
    /// the loader populates it for elements that have geometry.
    pub details: FxHashMap<u64, EntityDetail>,
}

/// Plugin: registers the resource + the re-parse system.
pub struct RichModelPlugin;

impl Plugin for RichModelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RichModel>()
            .add_systems(Update, reparse_on_load);
    }
}

/// Listens for `LoadIfcFileEvent` (the same message bimifc-bevy uses to
/// trigger its own load) and re-parses the file with spatial + property
/// extraction enabled so the panels have data to show.
///
/// Runs synchronously on the main thread for simplicity — parse time
/// dominates (~hundreds of ms to ~seconds) and the user already saw
/// the OS file-picker dialog. A spinner is overdue but out of scope
/// for the parity port.
fn reparse_on_load(
    mut load_events: MessageReader<LoadIfcFileEvent>,
    mut rich: ResMut<RichModel>,
) {
    for ev in load_events.read() {
        let path = ev.path.clone();
        match reparse(&path) {
            Ok(new_rich) => {
                *rich = new_rich;
            }
            Err(e) => {
                warn!("[egui] re-parse for panels failed: {e}");
            }
        }
    }
}

fn reparse(path: &PathBuf) -> Result<RichModel, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    // Both flags on: we want spatial_tree() and property_sets() to return
    // real data, not the empty defaults.
    let model: Arc<dyn IfcModel> = Arc::new(ParsedModel::parse(&content, true, true)?);

    let spatial_tree = model.spatial().spatial_tree().cloned();

    // For the properties panel we cache details for every entity that
    // would render. Two paths to find them — the resolver's
    // `types_present()` (post-`bimifc-model 0.3.0` change) gives us
    // every IfcType the file has, then we filter by `has_geometry()`.
    let mut details: FxHashMap<u64, EntityDetail> = FxHashMap::default();
    let resolver = model.resolver();
    let props = model.properties();
    let types: Vec<IfcType> = resolver.types_present();
    for t in &types {
        if !t.has_geometry() {
            continue;
        }
        for entity in resolver.entities_by_type(t) {
            let id = entity.id.0 as u64;
            let detail = EntityDetail {
                id,
                entity_type: t.name().to_string(),
                global_id: entity.get_string(0).map(str::to_string),
                name: entity.get_string(2).map(str::to_string),
                description: entity.get_string(3).map(str::to_string),
                property_sets: props.property_sets(EntityId(entity.id.0)),
                quantities: props.quantities(EntityId(entity.id.0)),
            };
            details.insert(id, detail);
        }
    }

    Ok(RichModel {
        source_path: Some(path.clone()),
        spatial_tree,
        details,
    })
}
