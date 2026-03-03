// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Hierarchy tree panel

use bimifc_model::{EntityId, SpatialNode, SpatialNodeType};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Widget},
};
use std::collections::HashSet;

/// A flattened tree item for display
#[derive(Clone, Debug)]
pub struct TreeItem {
    pub id: EntityId,
    pub name: String,
    pub node_type: SpatialNodeType,
    pub depth: usize,
    pub has_children: bool,
    pub expanded: bool,
}

/// Hierarchy tree state
#[derive(Default)]
pub struct HierarchyState {
    /// Flattened visible items
    pub items: Vec<TreeItem>,
    /// Currently selected index
    pub selected: usize,
    /// Expanded node IDs
    pub expanded: HashSet<EntityId>,
    /// Search filter
    pub filter: String,
    /// List state for ratatui
    pub list_state: ListState,
}

impl HierarchyState {
    /// Build tree from spatial structure
    pub fn build_from_tree(&mut self, root: &SpatialNode) {
        self.items.clear();

        // Expand root by default
        self.expanded.insert(root.id);

        self.flatten_node(root, 0);

        // Update list state
        if !self.items.is_empty() {
            self.selected = self.selected.min(self.items.len() - 1);
            self.list_state.select(Some(self.selected));
        }
    }

    /// Flatten tree into visible items
    fn flatten_node(&mut self, node: &SpatialNode, depth: usize) {
        // Apply filter
        if !self.filter.is_empty() {
            let filter_lower = self.filter.to_lowercase();
            let name_matches = node.name.to_lowercase().contains(&filter_lower);
            let type_matches = node.entity_type.to_lowercase().contains(&filter_lower);

            // If filter active, only show matching nodes (and their parents are handled separately)
            if !name_matches && !type_matches && node.children.is_empty() {
                return;
            }
        }

        let is_expanded = self.expanded.contains(&node.id);

        self.items.push(TreeItem {
            id: node.id,
            name: node.name.clone(),
            node_type: node.node_type,
            depth,
            has_children: !node.children.is_empty(),
            expanded: is_expanded,
        });

        // Add children if expanded
        if is_expanded {
            for child in &node.children {
                self.flatten_node(child, depth + 1);
            }
        }
    }

    /// Toggle expansion of current node
    pub fn toggle_expand(&mut self) {
        if let Some(item) = self.items.get(self.selected) {
            if item.has_children {
                let id = item.id;
                if self.expanded.contains(&id) {
                    self.expanded.remove(&id);
                } else {
                    self.expanded.insert(id);
                }
            }
        }
    }

    /// Expand current node
    pub fn expand(&mut self) {
        if let Some(item) = self.items.get(self.selected) {
            if item.has_children && !item.expanded {
                self.expanded.insert(item.id);
            }
        }
    }

    /// Collapse current node
    pub fn collapse(&mut self) {
        if let Some(item) = self.items.get(self.selected) {
            if item.expanded {
                self.expanded.remove(&item.id);
            }
        }
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.list_state.select(Some(self.selected));
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.selected + 1 < self.items.len() {
            self.selected += 1;
            self.list_state.select(Some(self.selected));
        }
    }

    /// Get selected entity ID
    pub fn selected_id(&self) -> Option<EntityId> {
        self.items.get(self.selected).map(|item| item.id)
    }

    /// Set filter string
    pub fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }
}

/// Hierarchy panel widget
pub struct HierarchyPanel<'a> {
    state: &'a HierarchyState,
    focused: bool,
}

impl<'a> HierarchyPanel<'a> {
    pub fn new(state: &'a HierarchyState) -> Self {
        Self {
            state,
            focused: false,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for HierarchyPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Hierarchy ");

        let inner = block.inner(area);
        block.render(area, buf);

        // Create list items
        let items: Vec<ListItem> = self
            .state
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let indent = "  ".repeat(item.depth);

                // Icon for node type
                let icon = match item.node_type {
                    SpatialNodeType::Project => "P",
                    SpatialNodeType::Site => "S",
                    SpatialNodeType::Building => "B",
                    SpatialNodeType::Storey => "L",
                    SpatialNodeType::Space => "R",
                    SpatialNodeType::Element => "E",
                    SpatialNodeType::Facility => "F",
                    SpatialNodeType::FacilityPart => "f",
                };

                // Expand/collapse indicator
                let expand_char = if item.has_children {
                    if item.expanded {
                        "v"
                    } else {
                        ">"
                    }
                } else {
                    " "
                };

                // Truncate name if needed
                let max_name_len = inner.width.saturating_sub(item.depth as u16 * 2 + 6) as usize;
                let name = if item.name.len() > max_name_len {
                    format!("{}...", &item.name[..max_name_len.saturating_sub(3)])
                } else {
                    item.name.clone()
                };

                let line = Line::from(vec![
                    Span::raw(indent),
                    Span::styled(expand_char, Style::default().fg(Color::DarkGray)),
                    Span::raw(" "),
                    Span::styled(
                        icon,
                        Style::default()
                            .fg(node_type_color(item.node_type))
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::raw(name),
                ]);

                let style = if i == self.state.selected {
                    Style::default()
                        .bg(Color::Rgb(40, 40, 60))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(line).style(style)
            })
            .collect();

        // Render list
        let list = List::new(items);

        // Use StatefulWidget to render with scroll position
        let mut state = self.state.list_state.clone();
        ratatui::widgets::StatefulWidget::render(list, inner, buf, &mut state);
    }
}

/// Get color for node type
fn node_type_color(node_type: SpatialNodeType) -> Color {
    match node_type {
        SpatialNodeType::Project => Color::Yellow,
        SpatialNodeType::Site => Color::Green,
        SpatialNodeType::Building => Color::Cyan,
        SpatialNodeType::Storey => Color::Blue,
        SpatialNodeType::Space => Color::Magenta,
        SpatialNodeType::Element => Color::White,
        SpatialNodeType::Facility => Color::LightGreen,
        SpatialNodeType::FacilityPart => Color::LightBlue,
    }
}
