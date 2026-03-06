//! Properties panel - shows selected entity details

use crate::bridge;
use crate::state::use_viewer_state;
use leptos::prelude::*;
use rustc_hash::FxHashSet;

/// Parsed photometric data ready for display
#[derive(Clone, Debug)]
struct PhotometryData {
    svg: String,
    luminaire_name: String,
    total_flux: f64,
    beam_angle: f64,
    field_angle: f64,
    max_intensity: f64,
    lor: f64,
    colour_temp: String,
    cri: String,
    cie_flux: String,
    wattage: f64,
    efficacy: f64,
}

/// Properties panel component
#[component]
pub fn PropertiesPanel() -> impl IntoView {
    let state = use_viewer_state();

    // Get selected entity (first one if multiple selected)
    let selected_entity = Memo::new(move |_| {
        let selected_ids = state.selection.selected_ids.get();
        let entities = state.scene.entities.get();
        selected_ids
            .iter()
            .next()
            .and_then(|id| entities.iter().find(|e| e.id == *id))
            .cloned()
    });

    let selection_count = Memo::new(move |_| state.selection.selected_ids.get().len());

    view! {
        <div class="properties-panel">
            {move || {
                let entity = selected_entity.get();
                let count = selection_count.get();

                if let Some(entity) = entity {
                    let photometry_ldt = entity.photometry_ldt.clone();

                    // Single entity selected
                    view! {
                        <div>
                            // Entity info section
                            <div class="property-section">
                                <div class="section-header">"Entity Info"</div>

                                <div class="property-row">
                                    <span class="property-label">"Type"</span>
                                    <span class="property-value">{entity.entity_type.clone()}</span>
                                </div>

                                {entity.name.clone().map(|name| view! {
                                    <div class="property-row">
                                        <span class="property-label">"Name"</span>
                                        <span class="property-value">{name}</span>
                                    </div>
                                })}

                                {entity.description.clone().map(|desc| view! {
                                    <div class="property-row">
                                        <span class="property-label">"Description"</span>
                                        <span class="property-value">{desc}</span>
                                    </div>
                                })}

                                {entity.global_id.clone().map(|gid| {
                                    let gid_clone = gid.clone();
                                    view! {
                                        <div class="property-row">
                                            <span class="property-label">"GlobalId"</span>
                                            <span class="property-value global-id">
                                                {gid}
                                                <button
                                                    class="copy-btn"
                                                    on:click=move |_| copy_to_clipboard(&gid_clone)
                                                    title="Copy to clipboard"
                                                >
                                                    "📋"
                                                </button>
                                            </span>
                                        </div>
                                    }
                                })}

                                {entity.storey.clone().map(|storey| view! {
                                    <div class="property-row">
                                        <span class="property-label">"Storey"</span>
                                        <span class="property-value">{storey}</span>
                                    </div>
                                })}

                                {entity.storey_elevation.map(|elev| view! {
                                    <div class="property-row">
                                        <span class="property-label">"Elevation"</span>
                                        <span class="property-value">{format!("{:.2} m", elev)}</span>
                                    </div>
                                })}
                            </div>

                            // Actions section
                            <div class="property-section">
                                <div class="section-header">"Actions"</div>
                                <div class="action-buttons">
                                    <ActionButtons entity_id=entity.id entity_type=entity.entity_type.clone() />
                                </div>
                            </div>

                            // Photometric data section (for entities with embedded LDT data)
                            {photometry_ldt.map(|ldt_content| view! {
                                <PhotometricSection ldt_content=ldt_content />
                            })}

                            // Property sets
                            {if entity.property_sets.is_empty() {
                                view! {
                                    <div class="property-section">
                                        <div class="section-header">"Property Sets"</div>
                                        <div class="empty-state small">
                                            <span class="empty-text">"No property sets"</span>
                                        </div>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div>
                                        {entity.property_sets.into_iter().map(|pset| view! {
                                            <div class="property-section">
                                                <div class="section-header">{pset.name}</div>
                                                {pset.properties.into_iter().map(|prop| view! {
                                                    <div class="property-row">
                                                        <span class="property-label">{prop.name}</span>
                                                        <span class="property-value">
                                                            {prop.value}
                                                            {prop.unit.map(|u| view! {
                                                                <span class="property-unit">{format!(" {}", u)}</span>
                                                            })}
                                                        </span>
                                                    </div>
                                                }).collect_view()}
                                            </div>
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}

                            // Quantities
                            {if entity.quantities.is_empty() {
                                view! {
                                    <div class="property-section">
                                        <div class="section-header">"Quantities"</div>
                                        <div class="empty-state small">
                                            <span class="empty-text">"No quantities"</span>
                                        </div>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="property-section">
                                        <div class="section-header">"Quantities"</div>
                                        {entity.quantities.into_iter().map(|qty| view! {
                                            <div class="property-row">
                                                <span class="property-label">{qty.name}</span>
                                                <span class="property-value">
                                                    {format!("{:.3}", qty.value)}
                                                    {if !qty.unit.is_empty() {
                                                        Some(view! {
                                                            <span class="property-unit">{format!(" {}", qty.unit)}</span>
                                                        })
                                                    } else {
                                                        None
                                                    }}
                                                </span>
                                            </div>
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        </div>
                    }.into_any()
                } else if count > 1 {
                    // Multiple selection
                    view! {
                        <div class="multi-selection">
                            <span class="selection-icon">"📑"</span>
                            <span class="selection-count">{format!("{} entities selected", count)}</span>
                            <MultiSelectionActions />
                        </div>
                    }.into_any()
                } else {
                    // No selection
                    view! {
                        <div class="empty-state">
                            <span class="empty-icon">"👆"</span>
                            <span class="empty-text">"No entity selected"</span>
                            <span class="empty-hint">"Click an entity to view its properties"</span>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}

/// Photometric data section - parses embedded LDT data and displays polar diagram
/// Supports two formats:
/// - Raw LDT/EULUMDAT string (from Pset_Photometry.EulumdatData)
/// - JSON with pre-rendered SVG and metrics (from IFC goniometric distribution data)
///   Multi-emitter fixtures produce multiple sources, each rendered separately.
#[component]
fn PhotometricSection(ldt_content: String) -> impl IntoView {
    // Check if content is pre-rendered JSON (from IFC goniometric sources)
    let items: Vec<PhotometryData> = if ldt_content.starts_with('{') {
        parse_ifc_photometry_items(&ldt_content)
    } else {
        match parse_ldt_data(&ldt_content) {
            Ok(data) => vec![data],
            Err(e) => {
                return view! {
                    <div class="property-section photometric-section">
                        <div class="section-header">"Photometric Data"</div>
                        <div class="photometric-error">
                            <span class="error-text">{format!("Parse error: {}", e)}</span>
                        </div>
                    </div>
                }
                .into_any();
            }
        }
    };

    let total = items.len();
    let multi = total > 1;

    view! {
        <div class="property-section photometric-section">
            <div class="section-header">"Photometric Data"</div>
            {items.into_iter().enumerate().map(|(idx, data)| {
                let svg = data.svg.clone();
                let label = if multi {
                    format!("Emitter {} of {}", idx + 1, total)
                } else {
                    String::new()
                };
                view! {
                    <div class="photometric-content" style={if idx > 0 { "border-top: 1px solid #333; padding-top: 8px; margin-top: 8px;" } else { "" }}>
                        {if multi {
                            Some(view! {
                                <div class="section-header" style="font-size: 0.85em; opacity: 0.7;">
                                    {label}
                                </div>
                            })
                        } else {
                            None
                        }}

                        // Polar diagram SVG
                        <div class="polar-diagram" inner_html=svg></div>

                        // Luminaire info
                        {if !data.luminaire_name.is_empty() {
                            Some(view! {
                                <div class="property-row">
                                    <span class="property-label">"Luminaire"</span>
                                    <span class="property-value">{data.luminaire_name.clone()}</span>
                                </div>
                            })
                        } else {
                            None
                        }}

                        // Key metrics — show non-zero values
                        {(data.total_flux > 0.0).then(|| view! {
                            <div class="property-row">
                                <span class="property-label">"Luminous Flux"</span>
                                <span class="property-value">{format!("{:.0} lm", data.total_flux)}</span>
                            </div>
                        })}
                        {(data.max_intensity > 0.0).then(|| view! {
                            <div class="property-row">
                                <span class="property-label">"Max Intensity"</span>
                                <span class="property-value">{format!("{:.0} cd", data.max_intensity)}</span>
                            </div>
                        })}
                        {(data.beam_angle > 0.0).then(|| view! {
                            <div class="property-row">
                                <span class="property-label">"Beam Angle"</span>
                                <span class="property-value">{format!("{:.1}\u{00b0}", data.beam_angle)}</span>
                            </div>
                        })}
                        {(data.field_angle > 0.0).then(|| view! {
                            <div class="property-row">
                                <span class="property-label">"Field Angle"</span>
                                <span class="property-value">{format!("{:.1}\u{00b0}", data.field_angle)}</span>
                            </div>
                        })}
                        {(data.lor > 0.0).then(|| view! {
                            <div class="property-row">
                                <span class="property-label">"Light Output"</span>
                                <span class="property-value">{format!("{:.1}%", data.lor)}</span>
                            </div>
                        })}
                        {(data.wattage > 0.0).then(|| view! {
                            <div class="property-row">
                                <span class="property-label">"Power"</span>
                                <span class="property-value">{format!("{:.0} W", data.wattage)}</span>
                            </div>
                        })}
                        {(data.efficacy > 0.0).then(|| view! {
                            <div class="property-row">
                                <span class="property-label">"Efficacy"</span>
                                <span class="property-value">{format!("{:.1} lm/W", data.efficacy)}</span>
                            </div>
                        })}
                        {if !data.colour_temp.is_empty() {
                            Some(view! {
                                <div class="property-row">
                                    <span class="property-label">"Colour Temp"</span>
                                    <span class="property-value">{data.colour_temp.clone()}</span>
                                </div>
                            })
                        } else {
                            None
                        }}
                        {if !data.cie_flux.is_empty() {
                            Some(view! {
                                <div class="property-row">
                                    <span class="property-label">"CIE Flux Code"</span>
                                    <span class="property-value">{data.cie_flux.clone()}</span>
                                </div>
                            })
                        } else {
                            None
                        }}
                        {if !data.cri.is_empty() {
                            Some(view! {
                                <div class="property-row">
                                    <span class="property-label">"CRI"</span>
                                    <span class="property-value">{data.cri.clone()}</span>
                                </div>
                            })
                        } else {
                            None
                        }}
                    </div>
                }
            }).collect::<Vec<_>>()}
        </div>
    }.into_any()
}

/// Decode IFC STEP encoded string sequences:
/// \X2\HHHH\X0\ → Unicode char, \S\c → extended char, '' → '
pub fn decode_ifc_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Check for \X2\ ... \X0\ (Unicode hex encoding)
            if chars.peek() == Some(&'X') {
                let mut seq = String::new();
                // Consume the potential \X2\ prefix
                seq.push(chars.next().unwrap_or(' ')); // 'X'
                seq.push(chars.next().unwrap_or(' ')); // '2' or other
                if seq == "X2" {
                    // Expect backslash
                    if chars.next() == Some('\\') {
                        // Read hex pairs until \X0\
                        let mut hex = String::new();
                        loop {
                            match chars.next() {
                                Some('\\') => {
                                    // Check for \X0\ terminator
                                    if chars.peek() == Some(&'X') {
                                        chars.next(); // X
                                        chars.next(); // 0
                                        chars.next(); // backslash
                                        break;
                                    } else {
                                        hex.push('\\');
                                    }
                                }
                                Some(ch) => hex.push(ch),
                                None => break,
                            }
                        }
                        // Decode hex pairs as Unicode code points (4 hex digits each)
                        let mut i = 0;
                        while i + 3 < hex.len() {
                            if let Ok(cp) = u32::from_str_radix(&hex[i..i + 4], 16) {
                                if let Some(ch) = char::from_u32(cp) {
                                    result.push(ch);
                                }
                            }
                            i += 4;
                        }
                    }
                } else {
                    result.push('\\');
                    for ch in seq.chars() {
                        result.push(ch);
                    }
                }
            } else if chars.peek() == Some(&'S') {
                // \S\c → ISO 8859-1 extended
                chars.next(); // S
                chars.next(); // backslash
                if let Some(ch) = chars.next() {
                    result.push(ch);
                }
            } else {
                result.push('\\');
            }
        } else if c == '\'' && chars.peek() == Some(&'\'') {
            // '' → '
            chars.next();
            result.push('\'');
        } else {
            result.push(c);
        }
    }

    result
}

/// Parse embedded LDT content and generate photometric data
fn parse_ldt_data(content: &str) -> Result<PhotometryData, String> {
    use eulumdat::diagram::{PolarDiagram, SvgTheme};
    use eulumdat::{Eulumdat, PhotometricSummary};

    // Decode IFC STEP string encoding (e.g. \X2\000A\X0\ → newline)
    let decoded = decode_ifc_string(content);
    let ldt = Eulumdat::parse(&decoded).map_err(|e| format!("{}", e))?;

    let polar = PolarDiagram::from_eulumdat(&ldt);
    let theme = SvgTheme::dark();
    let svg = polar.to_svg(280.0, 280.0, &theme);

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

    Ok(PhotometryData {
        svg,
        luminaire_name: ldt.luminaire_name.clone(),
        total_flux: summary.total_lamp_flux,
        beam_angle: summary.beam_angle,
        field_angle: summary.field_angle,
        max_intensity: summary.max_intensity,
        lor: summary.lor,
        colour_temp,
        cri,
        cie_flux: format!("{}", summary.cie_flux_codes),
        wattage: summary.total_wattage,
        efficacy: summary.luminaire_efficacy,
    })
}

/// Parse an LDT string and produce a JSON value for multi-source rendering.
/// Returns None if parsing fails.
pub fn parse_ldt_to_json(ldt_content: &str, name: &str) -> Option<serde_json::Value> {
    match parse_ldt_data(ldt_content) {
        Ok(data) => Some(serde_json::json!({
            "name": name,
            "svg": data.svg,
            "luminous_flux": data.total_flux,
            "max_intensity": data.max_intensity,
            "color_temperature": null,
            "emission_source": null,
        })),
        Err(_) => None,
    }
}

/// Parse pre-rendered IFC photometry JSON (from goniometric distribution data)
/// Supports multi-emitter format: {"sources": [{svg, luminous_flux, ...}, ...]}
fn parse_ifc_photometry_items(json: &str) -> Vec<PhotometryData> {
    #[derive(serde::Deserialize)]
    #[allow(dead_code)]
    struct IfcSource {
        name: Option<String>,
        svg: String,
        luminous_flux: Option<f64>,
        color_temperature: Option<f64>,
        max_intensity: Option<f64>,
        emission_source: Option<String>,
    }

    #[derive(serde::Deserialize)]
    struct IfcPhotometryMulti {
        sources: Vec<IfcSource>,
    }

    if let Ok(multi) = serde_json::from_str::<IfcPhotometryMulti>(json) {
        return multi
            .sources
            .into_iter()
            .map(|s| PhotometryData {
                svg: s.svg,
                luminaire_name: s.name.unwrap_or_default(),
                total_flux: s.luminous_flux.unwrap_or(0.0),
                beam_angle: 0.0,
                field_angle: 0.0,
                max_intensity: s.max_intensity.unwrap_or(0.0),
                lor: 0.0,
                colour_temp: s
                    .color_temperature
                    .map(|t| format!("{:.0} K", t))
                    .unwrap_or_default(),
                cri: String::new(),
                cie_flux: String::new(),
                wattage: 0.0,
                efficacy: 0.0,
            })
            .collect();
    }

    Vec::new()
}

/// Generate a polar diagram SVG from IFC light distribution data
/// Used for GLDF-exported IFC files that have inline IfcLightIntensityDistribution
pub fn generate_ifc_distribution_svg(
    distribution: &bimifc_parser::LightDistributionData,
) -> String {
    use std::f64::consts::FRAC_PI_2;

    let w: f64 = 280.0;
    let h: f64 = 280.0;
    let cx = w / 2.0;
    let cy = h / 2.0;
    let radius = (w.min(h) / 2.0) - 30.0;

    // Find max intensity across all planes for scaling
    let max_intensity = distribution
        .planes
        .iter()
        .flat_map(|p| p.intensities.iter().map(|(_, v)| *v))
        .fold(0.0_f64, f64::max)
        .max(1.0);

    let mut svg = String::with_capacity(4096);

    // SVG open tag
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" width=\"{}\" height=\"{}\">",
        w, h, w, h
    ));

    // Background
    svg.push_str(&format!(
        "<rect width=\"{}\" height=\"{}\" fill=\"#1a1a2e\" rx=\"8\"/>",
        w, h
    ));

    // Grid circles (25%, 50%, 75%, 100%)
    for frac in [0.25_f64, 0.5, 0.75, 1.0] {
        let r = radius * frac;
        svg.push_str(&format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{:.1}\" fill=\"none\" stroke=\"#333\" stroke-width=\"0.5\"/>",
            cx, cy, r
        ));
    }

    // Grid lines (0, 90, 180, 270)
    for angle_deg in [0.0_f64, 90.0, 180.0, 270.0] {
        let angle_rad = angle_deg.to_radians();
        let x2 = cx + radius * angle_rad.cos();
        let y2 = cy - radius * angle_rad.sin();
        svg.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#333\" stroke-width=\"0.5\"/>",
            cx, cy, x2, y2
        ));
    }

    // Angle labels
    for (angle, label) in [(0.0_f64, "0\u{00b0}"), (90.0, "90\u{00b0}"), (180.0, "180\u{00b0}")] {
        let angle_rad = -angle.to_radians() + FRAC_PI_2;
        let lx = cx + (radius + 15.0) * angle_rad.cos();
        let ly = cy - (radius + 15.0) * angle_rad.sin();
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" dominant-baseline=\"middle\" fill=\"#888\" font-size=\"10\">{}</text>",
            lx, ly, label
        ));
    }

    // Intensity label at max circle
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"start\" fill=\"#888\" font-size=\"9\">{:.0} cd</text>",
        cx + 3.0, cy - radius - 3.0, max_intensity
    ));

    let colors = ["#4fc3f7", "#ef5350", "#66bb6a", "#ffa726"];

    // Sort planes by angle for consistent rendering
    let mut sorted_planes: Vec<_> = distribution.planes.iter().collect();
    sorted_planes.sort_by(|a, b| a.main_angle.partial_cmp(&b.main_angle).unwrap());

    // Find standard C-plane pairs
    let c0_plane = sorted_planes.iter().find(|p| p.main_angle.abs() < 1.0);
    let c180_plane = sorted_planes.iter().find(|p| (p.main_angle - 180.0).abs() < 1.0);
    let c90_plane = sorted_planes.iter().find(|p| (p.main_angle - 90.0).abs() < 1.0);
    let c270_plane = sorted_planes.iter().find(|p| (p.main_angle - 270.0).abs() < 1.0);

    // Render C0-C180 pair (blue)
    if let Some(c0) = c0_plane {
        let path = build_polar_path(c0, c180_plane.copied(), cx, cy, radius, max_intensity);
        svg.push_str(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\" opacity=\"0.9\"/>",
            path, colors[0]
        ));
    }

    // Render C90-C270 pair (red, dashed)
    if let Some(c90) = c90_plane {
        let path = build_polar_path(c90, c270_plane.copied(), cx, cy, radius, max_intensity);
        svg.push_str(&format!(
            "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\" stroke-dasharray=\"4,3\" opacity=\"0.9\"/>",
            path, colors[1]
        ));
    }

    // If no C0/C90 pair found, render all planes individually
    if c0_plane.is_none() && c90_plane.is_none() {
        for (i, plane) in sorted_planes.iter().enumerate() {
            let path = build_single_plane_path(plane, cx, cy, radius, max_intensity);
            let color = colors[i % colors.len()];
            svg.push_str(&format!(
                "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\" opacity=\"0.9\"/>",
                path, color
            ));
        }
    }

    // Legend
    if c0_plane.is_some() {
        svg.push_str(&format!(
            "<rect x=\"10\" y=\"{:.0}\" width=\"10\" height=\"3\" fill=\"{}\"/>",
            h - 25.0, colors[0]
        ));
        svg.push_str(&format!(
            "<text x=\"24\" y=\"{:.0}\" fill=\"#ccc\" font-size=\"9\">C0-C180</text>",
            h - 22.0
        ));
    }
    if c90_plane.is_some() {
        svg.push_str(&format!(
            "<rect x=\"80\" y=\"{:.0}\" width=\"10\" height=\"3\" fill=\"{}\"/>",
            h - 25.0, colors[1]
        ));
        svg.push_str(&format!(
            "<text x=\"94\" y=\"{:.0}\" fill=\"#ccc\" font-size=\"9\">C90-C270</text>",
            h - 22.0
        ));
    }

    // Distribution type label
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"15\" text-anchor=\"middle\" fill=\"#aaa\" font-size=\"11\">{}</text>",
        cx, distribution.distribution_type
    ));

    svg.push_str("</svg>");
    svg
}

/// Build an SVG path for a C-plane pair (right half = primary, left half = opposite)
fn build_polar_path(
    primary: &bimifc_parser::DistributionPlane,
    opposite: Option<&bimifc_parser::DistributionPlane>,
    cx: f64,
    cy: f64,
    radius: f64,
    max_intensity: f64,
) -> String {
    use std::f64::consts::FRAC_PI_2;

    let mut path = String::new();

    // Right half: primary plane
    for (i, (angle, intensity)) in primary.intensities.iter().enumerate() {
        let gamma_rad = -angle.to_radians() + FRAC_PI_2;
        let r = (intensity / max_intensity) * radius;
        let x = cx + r * gamma_rad.cos();
        let y = cy - r * gamma_rad.sin();
        if i == 0 {
            path.push_str(&format!("M {x:.1} {y:.1}"));
        } else {
            path.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }

    // Left half: opposite plane (mirrored) or mirror of primary
    let opposite_intensities: Vec<(f64, f64)> = if let Some(opp) = opposite {
        opp.intensities.clone()
    } else {
        primary.intensities.clone()
    };

    for (angle, intensity) in opposite_intensities.iter().rev() {
        let gamma_rad = -angle.to_radians() + FRAC_PI_2;
        let r = (intensity / max_intensity) * radius;
        let x = cx - r * gamma_rad.cos(); // mirror X
        let y = cy - r * gamma_rad.sin();
        path.push_str(&format!(" L {x:.1} {y:.1}"));
    }

    path.push_str(" Z");
    path
}

/// Build an SVG path for a single plane (full circle sweep)
fn build_single_plane_path(
    plane: &bimifc_parser::DistributionPlane,
    cx: f64,
    cy: f64,
    radius: f64,
    max_intensity: f64,
) -> String {
    use std::f64::consts::FRAC_PI_2;

    let mut path = String::new();
    for (i, (angle, intensity)) in plane.intensities.iter().enumerate() {
        let gamma_rad = -angle.to_radians() + FRAC_PI_2;
        let r = (intensity / max_intensity) * radius;
        let x = cx + r * gamma_rad.cos();
        let y = cy - r * gamma_rad.sin();
        if i == 0 {
            path.push_str(&format!("M {x:.1} {y:.1}"));
        } else {
            path.push_str(&format!(" L {x:.1} {y:.1}"));
        }
    }
    path
}

/// Action buttons for single entity
#[component]
fn ActionButtons(entity_id: u64, entity_type: String) -> impl IntoView {
    let state = use_viewer_state();

    view! {
        <button
            class="action-btn"
            on:click=move |_| {
                bridge::save_focus(&bridge::FocusData { entity_id });
                bridge::log(&format!("Zoom to entity #{}", entity_id));
            }
            title="Zoom to entity"
        >
            "🔍 Zoom to"
        </button>

        <button
            class="action-btn"
            on:click=move |_| {
                state.visibility.isolate(entity_id);
            }
            title="Isolate entity"
        >
            "🎯 Isolate"
        </button>

        <button
            class="action-btn"
            on:click=move |_| {
                state.visibility.hide(entity_id);
            }
            title="Hide entity"
        >
            "👁‍🗨 Hide"
        </button>

        <button
            class="action-btn"
            on:click={
                let entity_type = entity_type.clone();
                move |_| {
                    // Select all entities of the same type
                    let same_type_ids: FxHashSet<u64> = state.scene.entities.get()
                        .iter()
                        .filter(|e| e.entity_type == entity_type)
                        .map(|e| e.id)
                        .collect();
                    for id in same_type_ids {
                        state.selection.add_to_selection(id);
                    }
                }
            }
            title="Select all of this type"
        >
            "📑 Select Similar"
        </button>
    }
}

/// Action buttons for multiple selection
#[component]
fn MultiSelectionActions() -> impl IntoView {
    let state = use_viewer_state();

    view! {
        <div class="action-buttons">
            <button
                class="action-btn"
                on:click=move |_| {
                    let ids = state.selection.selected_ids.get();
                    state.visibility.isolate_many(ids);
                }
            >
                "🎯 Isolate All"
            </button>

            <button
                class="action-btn"
                on:click=move |_| {
                    for id in state.selection.selected_ids.get().iter() {
                        state.visibility.hide(*id);
                    }
                }
            >
                "👁‍🗨 Hide All"
            </button>

            <button
                class="action-btn"
                on:click=move |_| {
                    state.selection.clear();
                }
            >
                "✖ Clear Selection"
            </button>
        </div>
    }
}

/// Copy text to clipboard using JS
fn copy_to_clipboard(text: &str) {
    let js_code = format!(
        "navigator.clipboard.writeText('{}').catch(e => console.warn('Copy failed:', e))",
        text.replace('\'', "\\'")
    );
    let _ = js_sys::eval(&js_code);
}
