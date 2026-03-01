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
#[component]
fn PhotometricSection(ldt_content: String) -> impl IntoView {
    // Parse the LDT content synchronously (it's already in memory)
    let data = parse_ldt_data(&ldt_content);

    view! {
        <div class="property-section photometric-section">
            <div class="section-header">"Photometric Data"</div>
            {match data {
                Err(e) => view! {
                    <div class="photometric-error">
                        <span class="error-text">{format!("Parse error: {}", e)}</span>
                    </div>
                }.into_any(),
                Ok(data) => {
                    let svg = data.svg.clone();
                    view! {
                        <div class="photometric-content">
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

                            // Key metrics
                            <div class="property-row">
                                <span class="property-label">"Luminous Flux"</span>
                                <span class="property-value">{format!("{:.0} lm", data.total_flux)}</span>
                            </div>
                            <div class="property-row">
                                <span class="property-label">"Beam Angle"</span>
                                <span class="property-value">{format!("{:.1}\u{00b0}", data.beam_angle)}</span>
                            </div>
                            <div class="property-row">
                                <span class="property-label">"Field Angle"</span>
                                <span class="property-value">{format!("{:.1}\u{00b0}", data.field_angle)}</span>
                            </div>
                            <div class="property-row">
                                <span class="property-label">"Max Intensity"</span>
                                <span class="property-value">{format!("{:.0} cd", data.max_intensity)}</span>
                            </div>
                            <div class="property-row">
                                <span class="property-label">"Light Output"</span>
                                <span class="property-value">{format!("{:.1}%", data.lor)}</span>
                            </div>
                            <div class="property-row">
                                <span class="property-label">"Power"</span>
                                <span class="property-value">{format!("{:.0} W", data.wattage)}</span>
                            </div>
                            <div class="property-row">
                                <span class="property-label">"Efficacy"</span>
                                <span class="property-value">{format!("{:.1} lm/W", data.efficacy)}</span>
                            </div>
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
                    }.into_any()
                }
            }}
        </div>
    }
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
    use eulumdat::{Eulumdat, PhotometricSummary};
    use eulumdat::diagram::{PolarDiagram, SvgTheme};

    // Decode IFC STEP string encoding (e.g. \X2\000A\X0\ → newline)
    let decoded = decode_ifc_string(content);
    let ldt = Eulumdat::parse(&decoded)
        .map_err(|e| format!("{}", e))?;

    let polar = PolarDiagram::from_eulumdat(&ldt);
    let theme = SvgTheme::dark();
    let svg = polar.to_svg(280.0, 280.0, &theme);

    let summary = PhotometricSummary::from_eulumdat(&ldt);

    let colour_temp = ldt.lamp_sets.first()
        .map(|ls| ls.color_appearance.clone())
        .unwrap_or_default();
    let cri = ldt.lamp_sets.first()
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
