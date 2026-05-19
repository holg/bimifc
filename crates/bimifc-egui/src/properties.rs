// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Right side panel — selected entity details.
//!
//! Layout mirrors `bimifc-leptos::components::properties_panel`:
//!   • Entity Info     — Type, Name, Description, GlobalId
//!   • Property Sets   — one collapsing section per IfcPropertySet
//!   • Quantities      — IfcElementQuantity values
//!   • Photometric     — only for IFCLIGHTFIXTURE (placeholder; the
//!                       polar-SVG render lives in leptos and is a
//!                       follow-up port)
//!
//! Data comes from `RichModel::details`, populated at file-load time.

use bevy::prelude::Res;
use bevy_egui::{egui, EguiContexts};
use bimifc_bevy::{IfcSceneData, SelectionState};

use crate::model::{EntityDetail, RichModel};

pub fn render_properties(
    mut contexts: EguiContexts,
    rich: Res<RichModel>,
    scene: Option<Res<IfcSceneData>>,
    selection: Res<SelectionState>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::SidePanel::right("properties")
        .resizable(true)
        .default_width(340.0)
        .min_width(240.0)
        .show(ctx, |ui| {
            ui.heading("Properties");
            ui.separator();

            // Pick the first selected entity (we never multi-select from
            // the hierarchy; picking might later — same single-entity
            // detail model as leptos).
            let selected_id = selection.selected.iter().next().copied();

            let Some(selected_id) = selected_id else {
                render_empty_state(ui, &rich, scene.as_deref());
                return;
            };

            let Some(detail) = rich.details.get(&selected_id) else {
                ui.label(format!("(no rich data for #{selected_id})"));
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "This entity has geometry but isn't in the cached \
                         property-set index. The cache is built at load time \
                         for IfcProduct subtypes only.",
                    )
                    .small()
                    .weak(),
                );
                return;
            };

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    render_entity_info(ui, detail);
                    render_property_sets(ui, detail);
                    render_quantities(ui, detail);
                    render_photometric_placeholder(ui, detail);
                });
        });
}

fn render_empty_state(
    ui: &mut egui::Ui,
    rich: &RichModel,
    scene: Option<&IfcSceneData>,
) {
    ui.label("(nothing selected)");
    ui.add_space(8.0);
    if let Some(path) = &rich.source_path {
        ui.label(
            egui::RichText::new(
                path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default(),
            )
            .strong(),
        );
    }
    if let Some(scene) = scene {
        ui.label(format!(
            "{} entities, {} cached details",
            scene.entities.len(),
            rich.details.len(),
        ));
        if let Some(bounds) = &scene.bounds {
            ui.label(format!(
                "Bounds: {:.1} × {:.1} × {:.1}",
                bounds.size().x,
                bounds.size().y,
                bounds.size().z,
            ));
        }
    }
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new("Click an entity in the Model tree to see its details.")
            .small()
            .weak(),
    );
}

fn render_entity_info(ui: &mut egui::Ui, detail: &EntityDetail) {
    section_header(ui, "Entity Info");
    egui::Grid::new("entity_info_grid")
        .num_columns(2)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            ui.label("Type");
            ui.label(&detail.entity_type);
            ui.end_row();

            ui.label("ID");
            ui.label(format!("#{}", detail.id));
            ui.end_row();

            if let Some(name) = &detail.name {
                if !name.is_empty() && name != "$" {
                    ui.label("Name");
                    ui.label(name);
                    ui.end_row();
                }
            }

            if let Some(desc) = &detail.description {
                if !desc.is_empty() && desc != "$" {
                    ui.label("Description");
                    ui.label(desc);
                    ui.end_row();
                }
            }

            if let Some(gid) = &detail.global_id {
                ui.label("GlobalId");
                // Show truncated with hover-full so it doesn't dominate.
                let short = if gid.len() > 12 {
                    format!("{}…", &gid[..10])
                } else {
                    gid.clone()
                };
                ui.label(short).on_hover_text(gid);
                ui.end_row();
            }
        });
    ui.add_space(6.0);
}

fn render_property_sets(ui: &mut egui::Ui, detail: &EntityDetail) {
    if detail.property_sets.is_empty() {
        return;
    }
    section_header(ui, "Property Sets");
    for pset in &detail.property_sets {
        egui::CollapsingHeader::new(&pset.name)
            .default_open(false)
            .show(ui, |ui| {
                if pset.properties.is_empty() {
                    ui.label(egui::RichText::new("(empty)").small().weak());
                    return;
                }
                egui::Grid::new(format!("pset_{}", pset.name))
                    .num_columns(2)
                    .spacing([12.0, 3.0])
                    .striped(true)
                    .show(ui, |ui| {
                        for prop in &pset.properties {
                            ui.label(&prop.name);
                            let value_str = match &prop.unit {
                                Some(unit) if !unit.is_empty() => {
                                    format!("{} {}", prop.value, unit)
                                }
                                _ => prop.value.clone(),
                            };
                            ui.label(value_str);
                            ui.end_row();
                        }
                    });
            });
    }
    ui.add_space(6.0);
}

fn render_quantities(ui: &mut egui::Ui, detail: &EntityDetail) {
    if detail.quantities.is_empty() {
        return;
    }
    section_header(ui, "Quantities");
    egui::Grid::new("quantities_grid")
        .num_columns(2)
        .spacing([12.0, 3.0])
        .striped(true)
        .show(ui, |ui| {
            for q in &detail.quantities {
                ui.label(&q.name);
                ui.label(format!("{:.3} {}", q.value, q.unit));
                ui.end_row();
            }
        });
    ui.add_space(6.0);
}

fn render_photometric_placeholder(ui: &mut egui::Ui, detail: &EntityDetail) {
    if !detail.entity_type.eq_ignore_ascii_case("IFCLIGHTFIXTURE") {
        return;
    }
    section_header(ui, "Photometric Data");
    // The leptos panel decodes Pset_Photometry.EulumdatData (base64 → LDT)
    // and renders a polar SVG. Porting that visualization to egui needs
    // either a custom Painter draw or piping the SVG through usvg/resvg.
    // Track in docs/egui-viewer-plan.md follow-ups.
    let has_pset = detail
        .property_sets
        .iter()
        .any(|p| p.name.contains("Photometry") || p.name.contains("photometric"));
    if has_pset {
        ui.label(
            egui::RichText::new(
                "Photometric property set present. Polar diagram rendering \
                 is a follow-up; see leptos PropertiesPanel for reference.",
            )
            .small(),
        );
    } else {
        ui.label(egui::RichText::new("No Pset_Photometry on this fixture.").small().weak());
    }
    ui.add_space(6.0);
}

/// Section header strip — same visual weight as the leptos
/// `.section-header` style: bold text + a thin separator underneath.
fn section_header(ui: &mut egui::Ui, label: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(label).strong().size(13.0));
    ui.separator();
}
