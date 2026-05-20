// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared UI state for the egui shell.
//!
//! Discipline + view-filter state used to live here as a local enum;
//! it has moved to `bimifc-federation` so every viewer (egui, leptos
//! web, ratatui TUI, future ones) reads the same single source of
//! truth. The Bevy resource that holds it is
//! `bimifc-bevy::FederationState`, owned by `IfcViewerPlugin`.
//!
//! This module is left for any egui-only UI state that doesn't fit
//! into the federation registry. Currently empty.
