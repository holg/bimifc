// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Keyboard input handling

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Input action
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    // Floor plan controls
    LevelUp,
    LevelDown,
    ZoomIn,
    ZoomOut,
    PanLeft,
    PanRight,
    PanUp,
    PanDown,
    ResetView,
    FitAll,

    // UI navigation
    CycleFocus,
    FocusViewport,
    FocusHierarchy,
    FocusProperties,

    // Hierarchy navigation
    TreeUp,
    TreeDown,
    TreeExpand,
    TreeCollapse,
    TreeSelect,

    // Panel visibility
    ToggleHierarchy,
    ToggleProperties,

    // Search
    StartSearch,
    CancelSearch,

    // Application
    Quit,
}

/// Map key event to action
pub fn map_key_to_action(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        // Level navigation (PgUp/PgDn or K/J)
        (KeyCode::PageUp, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => Some(Action::LevelUp),
        (KeyCode::PageDown, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => Some(Action::LevelDown),

        // Arrow keys for level navigation
        (KeyCode::Up, KeyModifiers::NONE) => Some(Action::LevelUp),
        (KeyCode::Down, KeyModifiers::NONE) => Some(Action::LevelDown),

        // Zoom
        (KeyCode::Char('+'), _) | (KeyCode::Char('='), KeyModifiers::NONE) => Some(Action::ZoomIn),
        (KeyCode::Char('-'), KeyModifiers::NONE) => Some(Action::ZoomOut),

        // Pan (WASD or arrow keys with shift)
        (KeyCode::Char('w'), KeyModifiers::NONE) => Some(Action::PanUp),
        (KeyCode::Char('s'), KeyModifiers::NONE) => Some(Action::PanDown),
        (KeyCode::Char('a'), KeyModifiers::NONE) => Some(Action::PanLeft),
        (KeyCode::Char('d'), KeyModifiers::NONE) => Some(Action::PanRight),
        (KeyCode::Left, KeyModifiers::SHIFT) => Some(Action::PanLeft),
        (KeyCode::Right, KeyModifiers::SHIFT) => Some(Action::PanRight),

        // View controls
        (KeyCode::Char('r'), KeyModifiers::NONE) => Some(Action::ResetView),
        (KeyCode::Char('f'), KeyModifiers::NONE) => Some(Action::FitAll),

        // Focus cycling
        (KeyCode::Tab, KeyModifiers::NONE) => Some(Action::CycleFocus),

        // Panel visibility
        (KeyCode::Char('1'), KeyModifiers::NONE) => Some(Action::ToggleHierarchy),
        (KeyCode::Char('2'), KeyModifiers::NONE) => Some(Action::ToggleProperties),

        // Hierarchy navigation (when focused)
        (KeyCode::Enter, KeyModifiers::NONE) => Some(Action::TreeSelect),

        // Search
        (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::StartSearch),
        (KeyCode::Esc, KeyModifiers::NONE) => Some(Action::CancelSearch),

        // Quit
        (KeyCode::Char('q'), KeyModifiers::NONE) => Some(Action::Quit),
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Quit),

        _ => None,
    }
}

/// Map key event to hierarchy navigation action
pub fn map_hierarchy_key(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => Some(Action::TreeUp),
        (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => Some(Action::TreeDown),
        (KeyCode::Right, _) | (KeyCode::Char('l'), KeyModifiers::NONE) => Some(Action::TreeExpand),
        (KeyCode::Left, _) | (KeyCode::Char('h'), KeyModifiers::NONE) => Some(Action::TreeCollapse),
        (KeyCode::Enter, _) => Some(Action::TreeSelect),
        (KeyCode::Tab, _) => Some(Action::CycleFocus),
        (KeyCode::Esc, _) => Some(Action::FocusViewport),
        _ => None,
    }
}
