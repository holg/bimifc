// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Discipline enum, view filter, and source metadata. Kept in their
//! own module so the rest of the crate (and downstream viewers) can
//! `use bimifc_federation::{Discipline, ViewFilter, SourceInfo}` and
//! have everything they need for the toolbar UI without pulling in
//! the per-file scene tracking.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// MEP discipline categorisation for a renderable element.
///
/// Variants are deliberately small: this enum is copied into every
/// triangle in some renderers (the TUI does that), so we keep it a
/// 1-byte `u8` and skip a `Default::Other` overload. For "no filter
/// active", the front-end uses [`ViewFilter::All`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum Discipline {
    /// Architectural / structural / generic — shown under every
    /// filter except [`ViewFilter::Discipline`] for a *non*-Other
    /// discipline (where it still shows as context — see
    /// [`ViewFilter::shows`]).
    #[default]
    Other = 0,
    Electrical = 1,
    Plumbing = 2,
    Hvac = 3,
    Lighting = 4,
}

impl Discipline {
    /// Short single-token label (used by status bars and CLIs).
    pub fn short(self) -> &'static str {
        match self {
            Discipline::Other => "Other",
            Discipline::Electrical => "Electrical",
            Discipline::Plumbing => "Plumbing",
            Discipline::Hvac => "HVAC",
            Discipline::Lighting => "Lighting",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Discipline::Other => "📦",
            Discipline::Electrical => "⚡",
            Discipline::Plumbing => "🔧",
            Discipline::Hvac => "💨",
            Discipline::Lighting => "💡",
        }
    }
}

/// Viewport-level UI mode. The renderer asks [`ViewFilter::shows`] per
/// triangle (or per mesh, per source — whatever granularity the
/// renderer keeps) to decide visibility.
///
/// Architectural + per-discipline variants intentionally keep the
/// per-entity `Discipline::Other` triangles visible too, so the user
/// retains spatial orientation when filtering down to (say) just the
/// HVAC layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ViewFilter {
    /// Show every triangle. The default architectural+MEP combined view.
    #[default]
    All,
    /// Hide all MEP — only the building shell remains. Inverse of the
    /// other discipline filters.
    Architecture,
    /// Show triangles tagged with this discipline plus the always-
    /// visible architectural context.
    Discipline(Discipline),
}

impl ViewFilter {
    /// Should a triangle/mesh with the given tag be drawn?
    ///
    /// Rule summary:
    /// - `All`            → yes, always.
    /// - `Architecture`   → only if the tag is `Other`.
    /// - `Discipline(d)`  → if the tag is `d` OR `Other`.
    pub fn shows(self, tag: Discipline) -> bool {
        match self {
            ViewFilter::All => true,
            ViewFilter::Architecture => tag == Discipline::Other,
            ViewFilter::Discipline(d) => tag == d || tag == Discipline::Other,
        }
    }

    /// Human-readable label for status bars and tooltips.
    pub fn label(self) -> &'static str {
        match self {
            ViewFilter::All => "All disciplines",
            ViewFilter::Architecture => "Architecture only (no MEP)",
            ViewFilter::Discipline(Discipline::Electrical) => "Electrical only",
            ViewFilter::Discipline(Discipline::Plumbing) => "Plumbing only",
            ViewFilter::Discipline(Discipline::Hvac) => "HVAC only",
            ViewFilter::Discipline(Discipline::Lighting) => "Lighting only",
            ViewFilter::Discipline(Discipline::Other) => "Other only",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ViewFilter::All => "🏗",
            ViewFilter::Architecture => "🏛",
            ViewFilter::Discipline(d) => d.icon(),
        }
    }

    /// All variants in the order the toolbar should display them.
    pub const ALL_VARIANTS: [ViewFilter; 6] = [
        ViewFilter::All,
        ViewFilter::Architecture,
        ViewFilter::Discipline(Discipline::Electrical),
        ViewFilter::Discipline(Discipline::Plumbing),
        ViewFilter::Discipline(Discipline::Hvac),
        ViewFilter::Discipline(Discipline::Lighting),
    ];
}

/// Per-loaded-file metadata.
///
/// One [`SourceInfo`] per IFC file the user opened. Holds the on-disk
/// path (for display + reload), a stable id assigned at registration
/// time, and a discipline hint inferred from the filename. The hint
/// is `Some(d)` when it influences whole-file filtering — meshes from
/// this source whose own tag is `Other` get re-tagged with this hint
/// so the discipline toggle hides them too.
///
/// `None` discipline means "fall back to per-entity classification"
/// — the right answer for federated single-files like NBU_MedicalClinic.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceInfo {
    pub display_name: String,
    pub path: Option<String>,
    pub discipline: Option<Discipline>,
    pub entity_count: usize,
    pub triangle_count: usize,
    pub visible: bool,
}

impl SourceInfo {
    pub fn new(display_name: impl Into<String>) -> Self {
        Self {
            display_name: display_name.into(),
            path: None,
            discipline: None,
            entity_count: 0,
            triangle_count: 0,
            visible: true,
        }
    }
}

/// Infer a discipline from the on-disk filename. Returns `None` when
/// no keyword matches — caller then either treats the file as
/// per-entity-classified (the medical-clinic story) or asks the user
/// to pick.
///
/// The keyword list reflects real-world dataset naming we've seen:
/// the LTU A-House series, Revit-exported per-discipline files, and
/// the bayarena lighting fixture.
pub fn discipline_from_filename(path: impl AsRef<Path>) -> Option<Discipline> {
    let stem = path
        .as_ref()
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    // Order matters: more specific keywords first so e.g. "_air" wins
    // over a stray "_arch" match in compound filenames.
    if stem.contains("light") || stem.contains("lumin") || stem.contains("_lit") {
        return Some(Discipline::Lighting);
    }
    if stem.contains("electr") || stem.contains("_el") || stem.contains("_eled") || stem.contains("_elec") {
        return Some(Discipline::Electrical);
    }
    if stem.contains("hvac")
        || stem.contains("_air")
        || stem.contains("_duct")
        || stem.contains("ventil")
        || stem.contains("_heat")
        || stem.contains("_cool")
    {
        return Some(Discipline::Hvac);
    }
    if stem.contains("plumb")
        || stem.contains("sanit")
        || stem.contains("_pipe")
        || stem.contains("waste")
        || stem.contains("water")
    {
        return Some(Discipline::Plumbing);
    }
    if stem.contains("_arc")
        || stem.contains("_arch")
        || stem.contains("architect")
        || stem.contains("_shell")
        || stem.contains("_struct")
        || stem.contains("_str_")
    {
        return Some(Discipline::Other);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ltu_filenames_classify() {
        assert_eq!(
            discipline_from_filename("LTU_A-House_Air.ifc"),
            Some(Discipline::Hvac)
        );
        assert_eq!(
            discipline_from_filename("LTU_A-House_Heating.ifc"),
            Some(Discipline::Hvac)
        );
        assert_eq!(
            discipline_from_filename("LTU_A-House_Plumbing.ifc"),
            Some(Discipline::Plumbing)
        );
        assert_eq!(
            discipline_from_filename("LTU_A-House_Sanitation.ifc"),
            Some(Discipline::Plumbing)
        );
        // _redesign isn't a discipline keyword on its own — that's the
        // federated full model, no whole-file hint.
        assert_eq!(discipline_from_filename("LTU_A-House_redesign.ifc"), None);
    }

    #[test]
    fn architecture_keywords() {
        assert_eq!(
            discipline_from_filename("building_arc.ifc"),
            Some(Discipline::Other)
        );
        assert_eq!(
            discipline_from_filename("ProjectX_Architecture.ifc"),
            Some(Discipline::Other)
        );
    }

    #[test]
    fn view_filter_rules() {
        assert!(ViewFilter::All.shows(Discipline::Hvac));
        assert!(ViewFilter::All.shows(Discipline::Other));

        assert!(ViewFilter::Architecture.shows(Discipline::Other));
        assert!(!ViewFilter::Architecture.shows(Discipline::Hvac));

        let hvac = ViewFilter::Discipline(Discipline::Hvac);
        assert!(hvac.shows(Discipline::Hvac));
        // Architectural context is kept under any discipline filter.
        assert!(hvac.shows(Discipline::Other));
        // …but other MEP disciplines are hidden.
        assert!(!hvac.shows(Discipline::Plumbing));
    }
}
