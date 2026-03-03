//! Integration test: verify geometry processing on real IFC file
use bimifc_geometry::GeometryRouter;
use bimifc_model::{IfcModel, IfcType};
use bimifc_parser::ParsedModel;

#[test]
fn test_process_real_ifc_file() {
    let content = match std::fs::read_to_string("../../tests/models/First File Office Building.ifc")
    {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Skipping: test model not found");
            return;
        }
    };

    let model = ParsedModel::parse(&content, false, false).expect("Failed to parse");
    let resolver = model.resolver();
    let unit_scale = model.unit_scale();

    println!("Unit scale: {}", unit_scale);

    let router = GeometryRouter::with_default_processors_and_unit_scale(unit_scale);

    let mut processed = 0;
    let mut errors = 0;
    let mut empty = 0;
    let mut total_verts = 0;

    let types = [
        IfcType::IfcWall,
        IfcType::IfcWallStandardCase,
        IfcType::IfcSlab,
        IfcType::IfcColumn,
        IfcType::IfcBeam,
        IfcType::IfcDoor,
        IfcType::IfcWindow,
        IfcType::IfcRoof,
        IfcType::IfcPlate,
        IfcType::IfcRailing,
        IfcType::IfcBuildingElementProxy,
        IfcType::IfcCovering,
        IfcType::IfcOpeningElement,
        IfcType::IfcMember,
    ];

    for ifc_type in &types {
        let entities = resolver.entities_by_type(ifc_type);
        for entity in &entities {
            match router.process_element(entity, resolver) {
                Ok(mesh) => {
                    if !mesh.is_empty() {
                        processed += 1;
                        total_verts += mesh.vertex_count();
                        if processed <= 5 {
                            println!(
                                "  {:?} {}: {} verts, {} tris",
                                ifc_type,
                                entity.id,
                                mesh.vertex_count(),
                                mesh.triangle_count()
                            );
                        }
                    } else {
                        empty += 1;
                    }
                }
                Err(e) => {
                    errors += 1;
                    if errors <= 5 {
                        println!("  Error {:?} {}: {:?}", ifc_type, entity.id, e);
                    }
                }
            }
        }
    }

    println!("\nResults:");
    println!("  Processed: {} elements with geometry", processed);
    println!("  Empty: {} elements", empty);
    println!("  Errors: {}", errors);
    println!("  Total vertices: {}", total_verts);

    assert!(processed > 0, "Should process at least some geometry");
    assert!(total_verts > 0, "Should produce some vertices");
}
