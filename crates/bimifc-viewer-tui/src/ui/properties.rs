// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Properties panel

use bimifc_model::{EntityId, IfcModel, PropertySet, Quantity};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use std::sync::Arc;

/// Properties to display for an entity
pub struct EntityProperties {
    pub id: EntityId,
    pub name: Option<String>,
    pub entity_type: String,
    pub global_id: Option<String>,
    pub description: Option<String>,
    pub property_sets: Vec<PropertySet>,
    pub quantities: Vec<Quantity>,
}

impl EntityProperties {
    /// Load properties for an entity from the model
    pub fn load(id: EntityId, model: &Arc<dyn IfcModel>) -> Option<Self> {
        let resolver = model.resolver();
        let entity = resolver.get(id)?;

        let props = model.properties();

        Some(Self {
            id,
            name: props.name(id),
            entity_type: entity.ifc_type.name().to_string(),
            global_id: props.global_id(id),
            description: props.description(id),
            property_sets: props.property_sets(id),
            quantities: props.quantities(id),
        })
    }
}

/// Properties panel widget
pub struct PropertiesPanel<'a> {
    properties: Option<&'a EntityProperties>,
    focused: bool,
    scroll: u16,
}

impl<'a> PropertiesPanel<'a> {
    pub fn new(properties: Option<&'a EntityProperties>) -> Self {
        Self {
            properties,
            focused: false,
            scroll: 0,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn scroll(mut self, scroll: u16) -> Self {
        self.scroll = scroll;
        self
    }
}

impl Widget for PropertiesPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Properties ");

        let inner = block.inner(area);
        block.render(area, buf);

        let Some(props) = self.properties else {
            // No selection
            let text =
                Paragraph::new("No entity selected").style(Style::default().fg(Color::DarkGray));
            text.render(inner, buf);
            return;
        };

        // Build content lines
        let mut lines: Vec<Line> = Vec::new();

        // Header
        lines.push(Line::from(vec![
            Span::styled("Type: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &props.entity_type,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("#{}", props.id.0)),
        ]));

        if let Some(ref name) = props.name {
            lines.push(Line::from(vec![
                Span::styled("Name: ", Style::default().fg(Color::DarkGray)),
                Span::raw(truncate(name, inner.width as usize - 8)),
            ]));
        }

        if let Some(ref guid) = props.global_id {
            lines.push(Line::from(vec![
                Span::styled("GUID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    truncate(guid, inner.width as usize - 8),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        if let Some(ref desc) = props.description {
            lines.push(Line::from(vec![
                Span::styled("Desc: ", Style::default().fg(Color::DarkGray)),
                Span::raw(truncate(desc, inner.width as usize - 8)),
            ]));
        }

        // Property sets
        for pset in &props.property_sets {
            lines.push(Line::raw("")); // Spacer
            lines.push(Line::from(vec![Span::styled(
                truncate(&pset.name, inner.width as usize - 2),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));

            for prop in &pset.properties {
                let value_str = if let Some(ref unit) = prop.unit {
                    format!("{} {}", prop.value, unit)
                } else {
                    prop.value.clone()
                };

                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        truncate(&prop.name, 15),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(": "),
                    Span::raw(truncate(&value_str, inner.width as usize - 20)),
                ]));
            }
        }

        // Quantities
        if !props.quantities.is_empty() {
            lines.push(Line::raw("")); // Spacer
            lines.push(Line::from(vec![Span::styled(
                "Quantities",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )]));

            for qty in &props.quantities {
                let formatted = qty.formatted();
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        truncate(&qty.name, 15),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::raw(": "),
                    Span::raw(truncate(&formatted, inner.width as usize - 20)),
                ]));
            }
        }

        // Render with scroll
        let text = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        text.render(inner, buf);
    }
}

/// Truncate string to max length
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}
