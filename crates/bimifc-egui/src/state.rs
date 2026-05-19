// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared UI state for the egui shell, kept as Bevy resources so both
//! the egui systems and any future Bevy-side filter systems see the
//! same value without a bridge layer.
//!
//! The discipline filter type is local to this crate for now; the plan
//! (`docs/egui-viewer-plan.md`) calls for moving it to `bimifc-model`
//! once we want both leptos and egui to share one source of truth.

use bevy::prelude::*;

/// MEP discipline view filter. Mirrors `MepView` from `bimifc-leptos`
/// and `ViewFilter` from `bimifc-viewer-tui`. Kept as a small Bevy
/// resource so any system can read it (e.g. a future filter that
/// hides non-matching meshes in the renderer).
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DisciplineFilter {
    /// Show everything (default architectural+MEP combined view).
    #[default]
    All,
    /// Hide all MEP — only the building shell remains.
    Architecture,
    Electrical,
    Plumbing,
    Hvac,
    Lighting,
}

impl DisciplineFilter {
    pub fn label(self) -> &'static str {
        match self {
            DisciplineFilter::All => "All disciplines",
            DisciplineFilter::Architecture => "Architecture only",
            DisciplineFilter::Electrical => "Electrical only",
            DisciplineFilter::Plumbing => "Plumbing only",
            DisciplineFilter::Hvac => "HVAC only",
            DisciplineFilter::Lighting => "Lighting only",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            DisciplineFilter::All => "🏗",
            DisciplineFilter::Architecture => "🏛",
            DisciplineFilter::Electrical => "⚡",
            DisciplineFilter::Plumbing => "🔧",
            DisciplineFilter::Hvac => "💨",
            DisciplineFilter::Lighting => "💡",
        }
    }

    pub const ALL_VARIANTS: [DisciplineFilter; 6] = [
        DisciplineFilter::All,
        DisciplineFilter::Architecture,
        DisciplineFilter::Electrical,
        DisciplineFilter::Plumbing,
        DisciplineFilter::Hvac,
        DisciplineFilter::Lighting,
    ];
}

/// Loaded-file display info — shown in the status bar so the user
/// knows what's in front of them.
#[derive(Resource, Default, Clone)]
pub struct LoadedFile {
    pub path: Option<String>,
    pub entity_count: usize,
    pub triangle_count: usize,
}
