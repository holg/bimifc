// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Federation registry — owns the list of loaded sources and answers
//! "should this mesh be visible right now?" for the renderer.
//!
//! Renderers are expected to:
//!   1. Call [`FederatedScene::register`] when a new IFC file finishes
//!      parsing. Get back a [`SourceId`] and stash it on every mesh
//!      produced from that file.
//!   2. Call [`FederatedScene::visible`] each frame with the mesh's
//!      `(SourceId, Discipline)` pair to decide whether to draw.
//!   3. Update [`FederatedScene::filter`] in response to toolbar
//!      clicks. Reading the filter back is also how the bottom status
//!      bar shows "Showing HVAC only — 12,372 / 297,478 triangles".
//!
//! The registry is a plain `serde`-able struct so each viewer can
//! store it however it likes — as a Bevy `Resource`, as a leptos
//! signal, as a `Mutex<FederatedScene>` for the SwiftUI FFI, etc.
//! We don't pull a runtime in (no Bevy, no leptos) so this stays
//! a pure-data dependency.

use serde::{Deserialize, Serialize};

use crate::source::{Discipline, SourceInfo, ViewFilter};

/// Opaque stable identifier for a loaded file. The first file gets
/// id `0`, the next `1`, etc. The renderer copies this onto each
/// mesh it spawns from that file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub struct SourceId(pub u32);

/// Registry of loaded IFC files + the current viewport filter.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FederatedScene {
    sources: Vec<SourceInfo>,
    /// Current viewport filter. Renderers read this each frame.
    pub filter: ViewFilter,
}

impl FederatedScene {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new source. The returned [`SourceId`] is `sources.len()`
    /// at insertion time — call sites must preserve insertion order if
    /// they remap sources.
    pub fn register(&mut self, info: SourceInfo) -> SourceId {
        let id = SourceId(self.sources.len() as u32);
        self.sources.push(info);
        id
    }

    pub fn sources(&self) -> &[SourceInfo] {
        &self.sources
    }

    pub fn sources_mut(&mut self) -> &mut [SourceInfo] {
        &mut self.sources
    }

    pub fn source(&self, id: SourceId) -> Option<&SourceInfo> {
        self.sources.get(id.0 as usize)
    }

    pub fn source_mut(&mut self, id: SourceId) -> Option<&mut SourceInfo> {
        self.sources.get_mut(id.0 as usize)
    }

    pub fn remove(&mut self, id: SourceId) -> Option<SourceInfo> {
        let idx = id.0 as usize;
        if idx >= self.sources.len() {
            return None;
        }
        // Tombstone with an empty placeholder so existing SourceIds
        // remain valid (renderers' mesh tags stay correct). Caller
        // should also tell the renderer to drop the meshes for this
        // source — see the renderer-side counterpart.
        Some(std::mem::replace(&mut self.sources[idx], SourceInfo::new("(removed)")))
    }

    pub fn clear(&mut self) {
        self.sources.clear();
    }

    /// Decide whether a mesh from `source` with per-entity tag
    /// `entity_discipline` should be drawn under the current filter.
    ///
    /// The rule combines three signals:
    /// 1. Per-source `visible` toggle (user hid the whole file).
    /// 2. Whole-file discipline override (filename hint or user-set).
    /// 3. Per-entity discipline tag (the federated-single-file path).
    ///
    /// Whole-file override wins over per-entity tag — that's the
    /// usecase of opening LTU_A-House_Air.ifc as "HVAC": every mesh
    /// in that file is treated as HVAC regardless of its individual
    /// classification.
    pub fn visible(&self, source: SourceId, entity_discipline: Discipline) -> bool {
        let Some(info) = self.source(source) else {
            // Unknown source → treat as visible so we don't drop
            // meshes that were spawned before registration (race
            // window during loading).
            return self.filter.shows(entity_discipline);
        };
        if !info.visible {
            return false;
        }
        // Whole-file discipline overrides the per-entity tag, except
        // when the override is `Other` (architectural file): then we
        // still defer to the per-entity tag so MEP entities embedded
        // in an arch model — rare but real — keep being classified
        // correctly.
        let effective = match info.discipline {
            Some(d) if d != Discipline::Other => d,
            _ => entity_discipline,
        };
        self.filter.shows(effective)
    }

    pub fn total_triangles(&self) -> usize {
        self.sources.iter().map(|s| s.triangle_count).sum()
    }

    pub fn visible_triangles(&self) -> usize {
        // Whole-file aggregate — useful for "12K of 200K shown" status.
        self.sources
            .iter()
            .filter(|s| s.visible)
            .filter(|s| match s.discipline {
                Some(d) => self.filter.shows(d),
                None => true,
            })
            .map(|s| s.triangle_count)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn register(scene: &mut FederatedScene, name: &str, d: Option<Discipline>) -> SourceId {
        let mut info = SourceInfo::new(name);
        info.discipline = d;
        info.triangle_count = 100;
        scene.register(info)
    }

    #[test]
    fn whole_file_discipline_overrides_entity() {
        let mut scene = FederatedScene::new();
        // LTU pattern: open Air.ifc as a whole-file HVAC source.
        let air = register(&mut scene, "LTU_A-House_Air.ifc", Some(Discipline::Hvac));

        scene.filter = ViewFilter::Discipline(Discipline::Hvac);
        // Entity tag is "Other" (the renderer hasn't classified it) —
        // whole-file override says HVAC, filter matches, so visible.
        assert!(scene.visible(air, Discipline::Other));

        scene.filter = ViewFilter::Discipline(Discipline::Plumbing);
        // Now the user wants plumbing — HVAC source is hidden whole.
        assert!(!scene.visible(air, Discipline::Other));
    }

    #[test]
    fn per_entity_classification_when_no_override() {
        let mut scene = FederatedScene::new();
        let federated = register(&mut scene, "NBU_MedicalClinic.ifc", None);

        scene.filter = ViewFilter::Discipline(Discipline::Hvac);
        assert!(scene.visible(federated, Discipline::Hvac));
        assert!(scene.visible(federated, Discipline::Other)); // arch context
        assert!(!scene.visible(federated, Discipline::Plumbing));
    }

    #[test]
    fn whole_file_visibility_toggle() {
        let mut scene = FederatedScene::new();
        let air = register(&mut scene, "Air.ifc", Some(Discipline::Hvac));
        scene.filter = ViewFilter::All;
        assert!(scene.visible(air, Discipline::Hvac));
        scene.source_mut(air).unwrap().visible = false;
        assert!(!scene.visible(air, Discipline::Hvac));
    }
}
