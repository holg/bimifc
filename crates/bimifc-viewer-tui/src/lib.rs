// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Terminal-based 3D IFC viewer using ratatui
//!
//! This crate provides a terminal UI for viewing IFC models with:
//! - 3D viewport rendering using block characters
//! - Spatial hierarchy tree navigation
//! - Properties panel for selected entities
//! - Keyboard controls for camera orbit, pan, and zoom

pub mod app;
pub mod camera;
pub mod input;
pub mod renderer;
pub mod scene;
pub mod ui;

pub use app::App;
pub use camera::OrbitCamera;
pub use scene::Scene;
