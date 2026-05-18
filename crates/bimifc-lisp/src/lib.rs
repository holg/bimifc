// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! AutoLISP scripting engine for IFC BIM models
//!
//! Embeds the acadlisp AutoLISP interpreter with IFC-specific built-in
//! functions for querying entities, properties, spatial structure,
//! drawing IFC geometry as 2D plan views, and creating/saving IFC files.

mod draw_functions;
mod ifc_functions;
mod writer_functions;

use acadlisp::Interpreter;
use bimifc_geometry::GeometryRouter;
use bimifc_model::IfcModel;
use bimifc_writer::{ProjectInfo, RoomElement};
use std::sync::Arc;

/// Element tracked by the writer subsystem.
pub struct WriterElement {
    pub id: u32,
    pub element: RoomElement,
}

/// State stored in the interpreter's `user_data` field.
pub struct IfcState {
    pub model: Option<Arc<dyn IfcModel>>,
    pub router: Option<GeometryRouter>,
    pub writer_elements: Vec<WriterElement>,
    pub next_writer_id: u32,
    pub project_info: ProjectInfo,
    pub current_storey: String,
    pub storey_elevations: Vec<(String, f64)>,
    pub current_color: Option<[f32; 4]>,
}

impl IfcState {
    fn new() -> Self {
        Self {
            model: None,
            router: None,
            writer_elements: Vec::new(),
            next_writer_id: 1,
            project_info: ProjectInfo::default(),
            current_storey: String::new(),
            storey_elevations: Vec::new(),
            current_color: None,
        }
    }
}

/// Create an interpreter with all IFC functions registered (no model loaded yet).
pub fn create_ifc_interpreter() -> Interpreter {
    let mut interp = Interpreter::new();
    interp.user_data = Box::new(IfcState::new());
    ifc_functions::register_all(&mut interp);
    draw_functions::register_all(&mut interp);
    writer_functions::register_all(&mut interp);
    interp
}

/// Generate IFC file content from the interpreter's writer elements.
/// Returns `None` if no elements have been added.
pub fn generate_ifc(interp: &Interpreter) -> Option<String> {
    let state = interp.user_data.downcast_ref::<IfcState>()?;
    if state.writer_elements.is_empty() {
        return None;
    }

    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    let mut max_h: f64 = 0.0;

    for we in &state.writer_elements {
        let r = &we.element.rect;
        min_x = min_x.min(r.x);
        min_y = min_y.min(r.y);
        max_x = max_x.max(r.x + r.width);
        max_y = max_y.max(r.y + r.height);
        max_h = max_h.max(we.element.height);
    }

    let elements: Vec<RoomElement> = state
        .writer_elements
        .iter()
        .map(|we| we.element.clone())
        .collect();
    let bbox = bimifc_writer::Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    };
    let dims = bimifc_writer::RoomDimensions {
        width: max_x - min_x,
        depth: max_y - min_y,
        height: max_h,
    };

    let mut room = bimifc_writer::RoomData::new(elements, bbox, dims);
    room.project = state.project_info.clone();
    room.storey_elevations = state.storey_elevations.clone();
    Some(bimifc_writer::write_ifc(&room))
}

/// Create an interpreter with a pre-loaded IFC model.
pub fn create_ifc_interpreter_with_model(model: Arc<dyn IfcModel>) -> Interpreter {
    let mut interp = Interpreter::new();
    let router = GeometryRouter::with_default_processors_and_unit_scale(model.unit_scale());
    interp.user_data = Box::new(IfcState {
        model: Some(model),
        router: Some(router),
        writer_elements: Vec::new(),
        next_writer_id: 1,
        project_info: ProjectInfo::default(),
        current_storey: String::new(),
        storey_elevations: Vec::new(),
        current_color: None,
    });
    ifc_functions::register_all(&mut interp);
    draw_functions::register_all(&mut interp);
    writer_functions::register_all(&mut interp);
    interp
}
