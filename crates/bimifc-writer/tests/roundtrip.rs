//! Round-trip integration test: write IFC -> parse it back -> verify entities.

use bimifc_model::{IfcType, SpatialNodeType};
use bimifc_writer::*;

/// Build a sample room with walls, a door, a window, and furniture.
fn sample_room() -> RoomData {
    RoomData::new(
        vec![
            // North wall
            RoomElement {
                kind: ElementKind::Wall,
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 5.0,
                    height: 0.2,
                },
                rotation: 0.0,
                label: Some("North Wall".into()),
                height: 2.8,
                z_offset: 0.0,
                storey: String::new(),
                color: None,
            },
            // East wall
            RoomElement {
                kind: ElementKind::Wall,
                rect: Rect {
                    x: 4.8,
                    y: 0.0,
                    width: 0.2,
                    height: 4.0,
                },
                rotation: 0.0,
                label: Some("East Wall".into()),
                height: 2.8,
                z_offset: 0.0,
                storey: String::new(),
                color: None,
            },
            // South wall
            RoomElement {
                kind: ElementKind::Wall,
                rect: Rect {
                    x: 0.0,
                    y: 3.8,
                    width: 5.0,
                    height: 0.2,
                },
                rotation: 0.0,
                label: Some("South Wall".into()),
                height: 2.8,
                z_offset: 0.0,
                storey: String::new(),
                color: None,
            },
            // Door in the north wall
            RoomElement {
                kind: ElementKind::Door,
                rect: Rect {
                    x: 1.0,
                    y: 0.0,
                    width: 0.9,
                    height: 0.2,
                },
                rotation: 0.0,
                label: Some("Main Door".into()),
                height: 2.1,
                z_offset: 0.0,
                storey: String::new(),
                color: None,
            },
            // Window in the east wall
            RoomElement {
                kind: ElementKind::Window,
                rect: Rect {
                    x: 4.8,
                    y: 1.5,
                    width: 0.2,
                    height: 1.2,
                },
                rotation: 0.0,
                label: Some("East Window".into()),
                height: 1.5,
                z_offset: 0.0,
                storey: String::new(),
                color: None,
            },
            // Bed (furniture)
            RoomElement {
                kind: ElementKind::Furniture("bed".into()),
                rect: Rect {
                    x: 1.0,
                    y: 1.5,
                    width: 2.0,
                    height: 1.6,
                },
                rotation: 0.0,
                label: Some("Double Bed".into()),
                height: 0.6,
                z_offset: 0.0,
                storey: String::new(),
                color: None,
            },
            // Table (furniture, rotated)
            RoomElement {
                kind: ElementKind::Furniture("table".into()),
                rect: Rect {
                    x: 3.0,
                    y: 2.0,
                    width: 1.0,
                    height: 0.6,
                },
                rotation: 0.785,
                label: Some("Desk".into()),
                height: 0.75,
                z_offset: 0.0,
                storey: String::new(),
                color: None,
            },
        ],
        Rect {
            x: 0.0,
            y: 0.0,
            width: 5.0,
            height: 4.0,
        },
        RoomDimensions {
            width: 5.0,
            height: 2.8,
            depth: 4.0,
        },
    )
}

#[test]
fn roundtrip_entity_counts() {
    let room = sample_room();
    let ifc_content = write_ifc(&room);

    let model =
        bimifc_parser::parse_auto(&ifc_content).expect("parser should accept writer output");

    let resolver = model.resolver();

    // We created 3 walls, 1 door, 1 window, 2 furnishing elements
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcWallStandardCase),
        3,
        "expected 3 walls"
    );
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcDoor),
        1,
        "expected 1 door"
    );
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcWindow),
        1,
        "expected 1 window"
    );
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcFurnishingElement),
        2,
        "expected 2 furnishing elements"
    );

    // Spatial hierarchy entities: 1 each
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcProject),
        1,
        "expected 1 project"
    );
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcSite),
        1,
        "expected 1 site"
    );
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcBuilding),
        1,
        "expected 1 building"
    );
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcBuildingStorey),
        1,
        "expected 1 storey"
    );

    // Total entity count should be non-trivial (geometry + spatial + elements)
    assert!(
        resolver.entity_count() > 20,
        "expected substantial entity count, got {}",
        resolver.entity_count()
    );
}

#[test]
fn roundtrip_spatial_hierarchy() {
    let room = sample_room();
    let ifc_content = write_ifc(&room);

    let model =
        bimifc_parser::parse_auto(&ifc_content).expect("parser should accept writer output");

    let spatial = model.spatial();

    // The spatial tree should exist
    let tree = spatial.spatial_tree().expect("spatial tree should exist");

    // Root should be the project
    assert_eq!(tree.node_type, SpatialNodeType::Project);
    assert_eq!(tree.name, "RoomPlan Export");

    // Project -> Site
    assert_eq!(tree.children.len(), 1, "project should have 1 child (site)");
    let site = &tree.children[0];
    assert_eq!(site.node_type, SpatialNodeType::Site);
    assert_eq!(site.name, "Default Site");

    // Site -> Building
    assert_eq!(
        site.children.len(),
        1,
        "site should have 1 child (building)"
    );
    let building = &site.children[0];
    assert_eq!(building.node_type, SpatialNodeType::Building);
    assert_eq!(building.name, "Building");

    // Building -> Storey
    assert_eq!(
        building.children.len(),
        1,
        "building should have 1 child (storey)"
    );
    let storey = &building.children[0];
    assert_eq!(storey.node_type, SpatialNodeType::Storey);
    assert_eq!(storey.name, "Ground Floor");

    // Storey should contain the building elements as children
    // (7 elements: 3 walls + 1 door + 1 window + 2 furniture)
    assert_eq!(
        storey.children.len(),
        7,
        "storey should contain 7 elements, got {}",
        storey.children.len()
    );
}

#[test]
fn roundtrip_storeys() {
    let room = sample_room();
    let ifc_content = write_ifc(&room);

    let model =
        bimifc_parser::parse_auto(&ifc_content).expect("parser should accept writer output");

    let spatial = model.spatial();
    let storeys = spatial.storeys();

    assert_eq!(storeys.len(), 1, "expected exactly 1 storey");
    assert_eq!(storeys[0].name, "Ground Floor");
}
