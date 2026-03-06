// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Properties panel

use bimifc_model::{EntityId, IfcModel, PropertyReader, PropertySet, Quantity};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use std::sync::Arc;

/// Parsed photometric summary for TUI display
pub struct PhotometrySummary {
    pub luminaire_name: String,
    pub total_flux: f64,
    pub max_intensity: f64,
    pub beam_angle: f64,
    pub field_angle: f64,
    pub lor: f64,
    pub wattage: f64,
    pub efficacy: f64,
    pub colour_temp: String,
    pub cri: String,
    pub source_count: usize,
    /// Raw LDT content for polar diagram rendering
    pub ldt_content: String,
}

/// Properties to display for an entity
pub struct EntityProperties {
    pub id: EntityId,
    pub name: Option<String>,
    pub entity_type: String,
    pub global_id: Option<String>,
    pub description: Option<String>,
    pub property_sets: Vec<PropertySet>,
    pub quantities: Vec<Quantity>,
    pub photometry: Vec<PhotometrySummary>,
}

impl EntityProperties {
    /// Load properties for an entity from the model
    pub fn load(id: EntityId, model: &Arc<dyn IfcModel>) -> Option<Self> {
        let resolver = model.resolver();
        let entity = resolver.get(id)?;

        let props = model.properties();
        let all_psets = props.property_sets(id);

        // Extract photometric data from property sets (GLDF/bayarena style)
        let mut photometry = extract_photometry(&all_psets);

        // Fallback: try IFC-native goniometric light sources (Relux style)
        if photometry.is_empty() {
            photometry = extract_goniometric(id, props);
        }

        // Filter out raw GLDF/photometry property sets from display
        let property_sets: Vec<_> = all_psets
            .into_iter()
            .filter(|ps| {
                ps.name != "Pset_Photometry"
                    && !ps.name.starts_with("Pset_GLDF_LDT")
                    && ps.name != "Pset_GLDF_PhotometryFiles"
            })
            .collect();

        Some(Self {
            id,
            name: props.name(id),
            entity_type: entity.ifc_type.name().to_string(),
            global_id: props.global_id(id),
            description: props.description(id),
            property_sets,
            quantities: props.quantities(id),
            photometry,
        })
    }
}

/// Extract photometric summaries from property sets
fn extract_photometry(psets: &[PropertySet]) -> Vec<PhotometrySummary> {
    let mut results = Vec::new();

    // 1. Try Pset_Photometry.EulumdatData (bayarena style — IFC STEP encoded)
    for ps in psets {
        if ps.name == "Pset_Photometry" {
            if let Some(prop) = ps.properties.iter().find(|p| p.name == "EulumdatData") {
                let decoded = decode_ifc_string(&prop.value);
                if let Some(summary) = parse_ldt_summary(&decoded) {
                    results.push(summary);
                }
            }
        }
    }

    // 2. Try Pset_GLDF_LDTRawContent (GLDF style — base64-encoded LDT)
    if results.is_empty() {
        for ps in psets {
            if ps.name == "Pset_GLDF_LDTRawContent" {
                use base64::Engine;
                let mut content_props: Vec<_> = ps
                    .properties
                    .iter()
                    .filter(|p| p.name.contains("Content"))
                    .collect();
                content_props.sort_by(|a, b| a.name.cmp(&b.name));

                for (i, prop) in content_props.iter().enumerate() {
                    if let Ok(decoded) =
                        base64::engine::general_purpose::STANDARD.decode(prop.value.trim())
                    {
                        if let Ok(ldt_str) = String::from_utf8(decoded) {
                            if let Some(mut summary) = parse_ldt_summary(&ldt_str) {
                                summary.source_count = i + 1;
                                results.push(summary);
                            }
                        }
                    }
                }
            }
        }
    }

    results
}

/// Parse an LDT/EULUMDAT string and return a summary
fn parse_ldt_summary(content: &str) -> Option<PhotometrySummary> {
    use eulumdat::{Eulumdat, PhotometricSummary};

    let ldt = Eulumdat::parse(content).ok()?;
    let summary = PhotometricSummary::from_eulumdat(&ldt);

    let colour_temp = ldt
        .lamp_sets
        .first()
        .map(|ls| ls.color_appearance.clone())
        .unwrap_or_default();
    let cri = ldt
        .lamp_sets
        .first()
        .map(|ls| ls.color_rendering_group.clone())
        .unwrap_or_default();

    Some(PhotometrySummary {
        luminaire_name: ldt.luminaire_name.clone(),
        total_flux: summary.total_lamp_flux,
        max_intensity: summary.max_intensity,
        beam_angle: summary.beam_angle,
        field_angle: summary.field_angle,
        lor: summary.lor,
        wattage: summary.total_wattage,
        efficacy: summary.luminaire_efficacy,
        colour_temp,
        cri,
        source_count: 1,
        ldt_content: content.to_string(),
    })
}

/// Extract photometric summaries from IFC-native IfcLightSourceGoniometric entities
fn extract_goniometric(id: EntityId, props: &dyn PropertyReader) -> Vec<PhotometrySummary> {
    use eulumdat::PhotometricSummary;

    let sources = props.goniometric_sources(id);
    let mut results = Vec::new();

    for (i, src) in sources.iter().enumerate() {
        if src.planes.is_empty() {
            continue;
        }

        let ldt = bimifc_parser::goniometric_to_eulumdat(src);
        let ldt_content = ldt.to_ldt();
        let summary = PhotometricSummary::from_eulumdat(&ldt);

        results.push(PhotometrySummary {
            luminaire_name: src.name.clone(),
            total_flux: src.luminous_flux,
            max_intensity: summary.max_intensity,
            beam_angle: summary.beam_angle,
            field_angle: summary.field_angle,
            lor: summary.lor,
            wattage: summary.total_wattage,
            efficacy: summary.luminaire_efficacy,
            colour_temp: if src.colour_temperature > 0.0 {
                format!("{:.0}K", src.colour_temperature)
            } else {
                String::new()
            },
            cri: String::new(),
            source_count: i + 1,
            ldt_content,
        });
    }

    results
}

/// Decode IFC STEP string encoding (\X2\00E4\X0\ → ä, \S\x → Latin-1 high bit)
fn decode_ifc_string(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Check for \X2\ hex encoding
            if chars.peek() == Some(&'X') {
                let mut seq: String = "\\".to_string();
                seq.push(chars.next().unwrap_or_default());
                if chars.peek() == Some(&'2') {
                    seq.push(chars.next().unwrap_or_default());
                    if chars.peek() == Some(&'\\') {
                        chars.next();
                        // Read hex pairs until \X0\ end marker
                        let mut hex = String::new();
                        loop {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                                // Check for \X0\ end marker
                                let mut end_check = String::new();
                                for _ in 0..3 {
                                    if let Some(&nc) = chars.peek() {
                                        end_check.push(nc);
                                        chars.next();
                                    }
                                }
                                if end_check == "X0\\" {
                                    break;
                                }
                                hex.push('\\');
                                hex.push_str(&end_check);
                            } else if let Some(hc) = chars.next() {
                                hex.push(hc);
                            } else {
                                break;
                            }
                        }
                        // Decode hex pairs as UTF-16
                        let mut i = 0;
                        let hex_bytes: Vec<u8> = hex.bytes().collect();
                        while i + 3 < hex_bytes.len() {
                            if let Ok(code) = u16::from_str_radix(&hex[i..i + 4], 16) {
                                if let Some(ch) = char::from_u32(code as u32) {
                                    result.push(ch);
                                }
                            }
                            i += 4;
                        }
                        continue;
                    }
                }
                result.push_str(&seq);
                continue;
            }
            // Check for \S\ Latin-1 high bit
            if chars.peek() == Some(&'S') {
                chars.next();
                if chars.peek() == Some(&'\\') {
                    chars.next();
                    if let Some(nc) = chars.next() {
                        let code = nc as u32 + 128;
                        if let Some(ch) = char::from_u32(code) {
                            result.push(ch);
                            continue;
                        }
                    }
                }
            }
            // Check for \n → newline
            if chars.peek() == Some(&'n') {
                chars.next();
                result.push('\n');
                continue;
            }
            result.push(c);
        } else {
            result.push(c);
        }
    }

    result
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
        let w = inner.width as usize;

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
                Span::raw(truncate(name, w.saturating_sub(8))),
            ]));
        }

        if let Some(ref guid) = props.global_id {
            lines.push(Line::from(vec![
                Span::styled("GUID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    truncate(guid, w.saturating_sub(8)),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }

        if let Some(ref desc) = props.description {
            lines.push(Line::from(vec![
                Span::styled("Desc: ", Style::default().fg(Color::DarkGray)),
                Span::raw(truncate(desc, w.saturating_sub(8))),
            ]));
        }

        // Photometry section
        for (i, phot) in props.photometry.iter().enumerate() {
            lines.push(Line::raw(""));
            let title = if props.photometry.len() > 1 {
                format!("Photometry (Emitter {})", i + 1)
            } else {
                "Photometry".to_string()
            };
            lines.push(Line::from(vec![Span::styled(
                title,
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            )]));

            if !phot.luminaire_name.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("  Luminaire: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(truncate(&phot.luminaire_name, w.saturating_sub(14))),
                ]));
            }

            let flux_str = format!("{:.0} lm", phot.total_flux);
            lines.push(Line::from(vec![
                Span::styled("  Flux:      ", Style::default().fg(Color::DarkGray)),
                Span::styled(flux_str, Style::default().fg(Color::Yellow)),
            ]));

            let intensity_str = format!("{:.0} cd", phot.max_intensity);
            lines.push(Line::from(vec![
                Span::styled("  Max I:     ", Style::default().fg(Color::DarkGray)),
                Span::raw(intensity_str),
            ]));

            if phot.beam_angle > 0.0 {
                lines.push(Line::from(vec![
                    Span::styled("  Beam:      ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{:.1}\u{00b0}", phot.beam_angle)),
                ]));
            }
            if phot.field_angle > 0.0 {
                lines.push(Line::from(vec![
                    Span::styled("  Field:     ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{:.1}\u{00b0}", phot.field_angle)),
                ]));
            }

            if phot.lor > 0.0 {
                lines.push(Line::from(vec![
                    Span::styled("  LOR:       ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{:.1}%", phot.lor * 100.0)),
                ]));
            }

            if phot.wattage > 0.0 {
                lines.push(Line::from(vec![
                    Span::styled("  Wattage:   ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{:.1} W", phot.wattage)),
                ]));
            }
            if phot.efficacy > 0.0 {
                lines.push(Line::from(vec![
                    Span::styled("  Efficacy:  ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{:.1} lm/W", phot.efficacy)),
                ]));
            }

            if !phot.colour_temp.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("  CCT:       ", Style::default().fg(Color::DarkGray)),
                    Span::raw(truncate(&phot.colour_temp, w.saturating_sub(14))),
                ]));
            }
            if !phot.cri.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("  CRI:       ", Style::default().fg(Color::DarkGray)),
                    Span::raw(truncate(&phot.cri, w.saturating_sub(14))),
                ]));
            }
        }

        // Property sets
        for pset in &props.property_sets {
            lines.push(Line::raw("")); // Spacer
            lines.push(Line::from(vec![Span::styled(
                truncate(&pset.name, w.saturating_sub(2)),
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
                    Span::raw(truncate(&value_str, w.saturating_sub(20))),
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
                    Span::raw(truncate(&formatted, w.saturating_sub(20))),
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
