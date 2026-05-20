// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Multi-file IFC federation — load several discipline files into one
//! scene, with each source carrying a discipline hint that the
//! [`ViewFilter`] uses to toggle visibility.
//!
//! ## Background
//!
//! A federated BIM model is the architectural pattern where every
//! discipline (architecture, structure, HVAC, plumbing, electrical,
//! lighting) lives in its own IFC file and shares a common coordinate
//! system. Project review then means opening all of them together and
//! turning layers on and off.
//!
//! The four bimifc viewers (leptos web, egui native, ratatui TUI,
//! SwiftUI macOS app) were each carrying a partial copy of a
//! `Discipline` enum and a partial copy of a name-keyword classifier.
//! This crate centralises both so the viewers stay in sync and so
//! single-source-of-truth lives in `core/`, not in any one front-end.
//!
//! ## Layered model
//!
//! - [`Discipline`] — per-entity tag (what *is* this triangle?).
//! - [`SourceInfo`] — one entry per loaded IFC file. Carries a path,
//!   a stable [`SourceId`], and the discipline hint we inferred from
//!   the filename (overridable by the user).
//! - [`ViewFilter`] — UI-mode state: `All` / `Architecture` / a
//!   specific [`Discipline`]. The [`ViewFilter::shows`] predicate
//!   combines the per-source override and the per-entity tag so the
//!   renderer can answer "should I draw this?" with one call.
//!
//! Each viewer then composes the small surface here with its own UI
//! and renderer:
//!
//! - the renderer (bimifc-bevy or any custom one) tags each spawned
//!   mesh with a `SourceId` + a `Discipline` and asks
//!   [`ViewFilter::shows`] before drawing;
//! - the file-load path infers the discipline via
//!   [`discipline_from_filename`] and lets the user pick a different
//!   value in a popup;
//! - the toolbar reads/writes the shared [`ViewFilter`] state.

pub mod classify;
pub mod scene;
pub mod source;

pub use classify::{classify_by_entity_class, classify_by_name, classify_by_type_name};
pub use scene::{FederatedScene, SourceId};
pub use source::{discipline_from_filename, Discipline, SourceInfo, ViewFilter};
