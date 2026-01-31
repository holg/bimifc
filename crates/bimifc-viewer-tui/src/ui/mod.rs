// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! UI components for the TUI viewer

pub mod hierarchy;
pub mod properties;
pub mod status;
pub mod viewport;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// UI panel focus state
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Viewport,
    Hierarchy,
    Properties,
    Search,
}

impl Focus {
    /// Cycle to next focus
    pub fn next(self) -> Self {
        match self {
            Focus::Viewport => Focus::Hierarchy,
            Focus::Hierarchy => Focus::Properties,
            Focus::Properties => Focus::Viewport,
            Focus::Search => Focus::Hierarchy,
        }
    }
}

/// Layout configuration
pub struct LayoutConfig {
    pub show_hierarchy: bool,
    pub show_properties: bool,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            show_hierarchy: true,
            show_properties: true,
        }
    }
}

/// Layout areas
pub struct LayoutAreas {
    pub hierarchy: Option<Rect>,
    pub viewport: Rect,
    pub properties: Option<Rect>,
    pub status: Rect,
}

/// Calculate layout areas based on configuration
pub fn calculate_layout(area: Rect, config: &LayoutConfig) -> LayoutAreas {
    // Split into main area and status bar
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let main_area = vertical[0];
    let status_area = vertical[1];

    // Determine horizontal layout based on visible panels
    let (hierarchy, viewport, properties) = match (config.show_hierarchy, config.show_properties) {
        (true, true) => {
            // Both panels: [20%] [60%] [20%]
            let horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(20),
                    Constraint::Percentage(60),
                    Constraint::Percentage(20),
                ])
                .split(main_area);
            (Some(horizontal[0]), horizontal[1], Some(horizontal[2]))
        }
        (true, false) => {
            // Only hierarchy: [25%] [75%]
            let horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(75)])
                .split(main_area);
            (Some(horizontal[0]), horizontal[1], None)
        }
        (false, true) => {
            // Only properties: [75%] [25%]
            let horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
                .split(main_area);
            (None, horizontal[0], Some(horizontal[1]))
        }
        (false, false) => {
            // No panels: 100% viewport
            (None, main_area, None)
        }
    };

    LayoutAreas {
        hierarchy,
        viewport,
        properties,
        status: status_area,
    }
}
