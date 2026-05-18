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
//!         },
//!     ],
//!     Rect { x: 0.0, y: 0.0, width: 5.0, height: 4.0 },
//!     RoomDimensions { width: 5.0, height: 2.8, depth: 4.0 },
//! );
//!
//! let ifc_content = write_ifc(&room);
//! assert!(ifc_content.contains("IFCWALLSTANDARDCASE"));
//! ```

mod guid;
mod model;
mod step;
mod writer;

pub use model::*;
pub use writer::write_ifc;
