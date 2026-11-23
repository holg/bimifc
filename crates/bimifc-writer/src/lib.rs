//! bimifc-writer — IFC 2x3 STEP file generator
//!
//! Converts room geometry (walls, doors, windows, furniture) into valid IFC2X3
//! files that can be opened in BIM viewers like BIMvision, Solibri, Revit, etc.
//!
//! # Example
//!
//! ```rust
//! use bimifc_writer::*;
//!
//! let room = RoomData::new(
//!     vec![
//!         RoomElement {
//!             kind: ElementKind::Wall,
//!             rect: Rect { x: 0.0, y: 0.0, width: 5.0, height: 0.2 },
//!             rotation: 0.0,
//!             label: Some("North Wall".into()),
//!             height: 2.8,
//!             z_offset: 0.0,
//!             storey: String::new(),
//!             color: None,
//!             psets: Vec::new(),
//!             material: None,
//!             identifier: None,
//!             mesh: None,
//!         },
//!     ],
//!     Rect { x: 0.0, y: 0.0, width: 5.0, height: 4.0 },
//!     RoomDimensions { width: 5.0, height: 2.8, depth: 4.0 },
//! );
//!
//! let ifc_content = write_ifc(&room);
//! // IFC4 by default: IfcWallStandardCase is deprecated there.
//! assert!(ifc_content.contains("FILE_SCHEMA(('IFC4'))"));
//! assert!(ifc_content.contains("IFCWALL('"));
//! ```

mod guid;
mod model;
mod pset;
mod step;
mod writer;

pub use model::*;
pub use pset::{write_element_quantities, write_property_set, IfcProperty, IfcQuantity, IfcValue};
// `ElementPset` is defined in `model` and re-exported via `model::*`;
// `IfcProperty`/`IfcValue` come from `pset`.
pub use step::{IdAlloc, StepBuilder};
pub use writer::write_ifc;
