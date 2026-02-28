// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Geometry Processors - Implementations for various IFC geometry types
//!
//! Each processor handles one or more types of IFC geometry representations.
//! Processors use the `EntityResolver` trait for entity lookups.

mod extruded_area_solid;
mod faceted_brep;
mod revolved_area_solid;
mod swept_disk_solid;
mod triangulated_face_set;

pub use extruded_area_solid::ExtrudedAreaSolidProcessor;
pub use faceted_brep::FacetedBrepProcessor;
pub use revolved_area_solid::RevolvedAreaSolidProcessor;
pub use swept_disk_solid::SweptDiskSolidProcessor;
pub use triangulated_face_set::TriangulatedFaceSetProcessor;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::GeometryProcessor;
    use crate::Point3;
    use bimifc_model::IfcType;

    #[test]
    fn test_extruded_area_solid_processor_creation() {
        let processor = ExtrudedAreaSolidProcessor::new();
        assert_eq!(
            processor.supported_types(),
            vec![IfcType::IfcExtrudedAreaSolid]
        );
    }

    #[test]
    fn test_triangulated_face_set_processor_creation() {
        let processor = TriangulatedFaceSetProcessor::new();
        assert_eq!(
            processor.supported_types(),
            vec![IfcType::IfcTriangulatedFaceSet]
        );
    }

    #[test]
    fn test_faceted_brep_processor_creation() {
        let processor = FacetedBrepProcessor::new();
        assert_eq!(
            processor.supported_types(),
            vec![IfcType::IfcFacetedBrep]
        );
    }

    #[test]
    fn test_swept_disk_solid_processor_creation() {
        let processor = SweptDiskSolidProcessor::new();
        assert_eq!(
            processor.supported_types(),
            vec![IfcType::IfcSweptDiskSolid]
        );
    }

    #[test]
    fn test_revolved_area_solid_processor_creation() {
        let processor = RevolvedAreaSolidProcessor::new();
        assert_eq!(
            processor.supported_types(),
            vec![IfcType::IfcRevolvedAreaSolid]
        );
    }

    #[test]
    fn test_faceted_brep_triangulate_triangle() {
        let processor = FacetedBrepProcessor::new();
        let points = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(0.5, 1.0, 0.0),
        ];
        let (positions, indices) = processor.triangulate_face(&points, &[]);

        assert_eq!(positions.len(), 9); // 3 vertices * 3 components
        assert_eq!(indices.len(), 3);   // 1 triangle * 3 indices
    }

    #[test]
    fn test_faceted_brep_triangulate_quad() {
        let processor = FacetedBrepProcessor::new();
        let points = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
        ];
        let (positions, indices) = processor.triangulate_face(&points, &[]);

        assert_eq!(positions.len(), 12); // 4 vertices * 3 components
        assert_eq!(indices.len(), 6);    // 2 triangles * 3 indices
    }

    #[test]
    fn test_faceted_brep_triangulate_with_hole() {
        let processor = FacetedBrepProcessor::new();
        let outer = vec![
            Point3::new(0.0, 0.0, 0.0),
            Point3::new(4.0, 0.0, 0.0),
            Point3::new(4.0, 4.0, 0.0),
            Point3::new(0.0, 4.0, 0.0),
        ];
        let hole = vec![
            Point3::new(1.0, 1.0, 0.0),
            Point3::new(3.0, 1.0, 0.0),
            Point3::new(3.0, 3.0, 0.0),
            Point3::new(1.0, 3.0, 0.0),
        ];
        let (positions, indices) = processor.triangulate_face(&outer, &[hole]);

        // Should have 8 vertices (4 outer + 4 hole)
        assert_eq!(positions.len(), 24);
        // Should have multiple triangles to fill the ring
        assert!(indices.len() >= 18); // At least 6 triangles
    }
}
