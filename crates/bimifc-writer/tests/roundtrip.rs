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
                psets: Vec::new(),
                material: None,
                identifier: None,
                mesh: None,
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
                psets: Vec::new(),
                material: None,
                identifier: None,
                mesh: None,
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
                psets: Vec::new(),
                material: None,
                identifier: None,
                mesh: None,
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
                psets: Vec::new(),
                material: None,
                identifier: None,
                mesh: None,
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
                psets: Vec::new(),
                material: None,
                identifier: None,
                mesh: None,
            },
            // Sink relabelled as a sanitary terminal (user IFC override)
            RoomElement {
                kind: ElementKind::SanitaryTerminal("sink".into()),
                rect: Rect {
                    x: 0.2,
                    y: 0.2,
                    width: 0.6,
                    height: 0.5,
                },
                rotation: 0.0,
                label: Some("Washbasin".into()),
                height: 0.85,
                z_offset: 0.0,
                storey: String::new(),
                color: None,
                psets: Vec::new(),
                material: None,
                identifier: None,
                mesh: None,
            },
            // Unclassifiable thing as a proxy (user IFC override)
            RoomElement {
                kind: ElementKind::Proxy("storage".into()),
                rect: Rect {
                    x: 4.0,
                    y: 0.2,
                    width: 0.8,
                    height: 0.4,
                },
                rotation: 0.0,
                label: Some("Server Rack".into()),
                height: 2.0,
                z_offset: 0.0,
                storey: String::new(),
                color: None,
                psets: Vec::new(),
                material: None,
                identifier: None,
                mesh: None,
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
                psets: Vec::new(),
                material: None,
                identifier: None,
                mesh: None,
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
                psets: Vec::new(),
                material: None,
                identifier: None,
                mesh: None,
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
        resolver.count_by_type(&IfcType::IfcWall),
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
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcSanitaryTerminal),
        1,
        "IFC4: a real sanitary terminal, not a flow terminal"
    );
    assert_eq!(
        resolver.count_by_type(&IfcType::IfcBuildingElementProxy),
        1,
        "expected 1 building element proxy"
    );
    assert_eq!(
        resolver.find_by_type_name("IFCSANITARYTERMINALTYPE").len(),
        1,
        "expected 1 sanitary terminal type"
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
    // (9 elements: 3 walls + 1 door + 1 window + 2 furniture
    //  + 1 sanitary terminal + 1 proxy)
    assert_eq!(
        storey.children.len(),
        9,
        "storey should contain 9 elements, got {}",
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

#[test]
fn roundtrip_light_fixtures() {
    let mut room = sample_room();
    room.light_fixtures.push(LightFixtureData {
        name: "Test Downlight".into(),
        x: 2.5,
        y: 2.0,
        z: 2.8,
        rotation: 0.0,
        flux_lm: 1200.0,
        cct_k: Some(3000.0),
        // "LDT" placeholder; real content is produced by the FFI layer.
        ldt_base64: Some("TERUIGNvbnRlbnQ=".into()),
        ldt_file_name: Some("test.ldt".into()),
        storey: String::new(),
        psets: Vec::new(),
        // A DALI-2 grow light: the control and horticultural data must
        // survive the round trip through IFC.
        control: Some(LightControlData {
            dimmable: true,
            protocol: Some("DALI-2".into()),
            range_percent: Some((1.0, 100.0)),
            cct_controllable: true,
            ..Default::default()
        }),
        horticulture: Some(HorticulturalData {
            ppf_umol_s: 1850.0,
            input_watts: Some(640.0),
            efficacy_umol_per_j: Some(2.89),
            spectrum: Some("full spectrum".into()),
            ..Default::default()
        }),
    });

    let ifc_content = write_ifc(&room);
    assert!(ifc_content.contains("IFCLIGHTFIXTURETYPE("));
    // Control + horticultural psets reach the file.
    assert!(ifc_content.contains("Pset_ATLA_Control"));
    assert!(ifc_content.contains("'DimmingMethod',$,IFCLABEL('DALI-2')"));
    assert!(ifc_content.contains("Pset_ATLA_Horticultural"));
    assert!(ifc_content.contains("'PhotosyntheticPhotonFlux'"));
    assert!(ifc_content.contains("Pset_LightFixtureTypeCommon"));
    assert!(ifc_content.contains("IFCLIGHTFIXTURE('"), "IFC4 default");
    assert!(ifc_content.contains("Pset_GLDF_LDTRawContent"));
    assert!(ifc_content.contains("LDT_0_Content"));

    let model =
        bimifc_parser::parse_auto(&ifc_content).expect("parser should accept writer output");
    let resolver = model.resolver();

    // IFC4: real occurrence classes for both.
    assert_eq!(resolver.count_by_type(&IfcType::IfcLightFixture), 1);
    assert_eq!(resolver.count_by_type(&IfcType::IfcSanitaryTerminal), 1);
    assert_eq!(resolver.count_by_type(&IfcType::IfcLightFixtureType), 1);

    // The lighting extractor must still find the fixture.
    let lighting = bimifc_parser::extract_lighting_data(resolver);
    assert_eq!(lighting.summary.total_fixtures, 1, "fixture found");
    let fixture = &lighting.light_fixtures[0];
    assert_eq!(fixture.name.as_deref(), Some("Test Downlight"));
    let ft = fixture
        .fixture_type
        .as_ref()
        .expect("fixture type resolved");
    assert_eq!(ft.name.as_deref(), Some("Test Downlight"));
    // Position round-trips (relative to bounding-box origin).
    assert!((fixture.position.2 - 2.8).abs() < 1e-6, "mount height");
}

#[test]
fn roundtrip_openings_types_and_quantities() {
    let room = sample_room();
    let ifc_content = write_ifc(&room);

    let model =
        bimifc_parser::parse_auto(&ifc_content).expect("parser should accept writer output");
    let resolver = model.resolver();

    // One opening per door/window, voiding the host wall and filled by it.
    assert_eq!(resolver.count_by_type(&IfcType::IfcOpeningElement), 2);
    assert_eq!(resolver.count_by_type(&IfcType::IfcRelVoidsElement), 2);
    assert_eq!(resolver.count_by_type(&IfcType::IfcRelFillsElement), 2);

    // Type objects (sanitary type comes from the sanitary element).
    assert_eq!(resolver.count_by_type(&IfcType::IfcWallType), 1);
    assert_eq!(resolver.find_by_type_name("IFCDOORSTYLE").len(), 1);
    assert_eq!(resolver.find_by_type_name("IFCWINDOWSTYLE").len(), 1);
    assert_eq!(resolver.find_by_type_name("IFCFURNITURETYPE").len(), 2);

    // Materials
    assert!(resolver.count_by_type(&IfcType::IfcRelAssociatesMaterial) >= 3);

    // Psets and base quantities are readable through the property reader.
    let props = model.properties();
    let walls = resolver.entities_by_type(&IfcType::IfcWall);
    let north = walls
        .iter()
        .find(|w| w.get_string(2) == Some("North Wall"))
        .expect("north wall present");
    let psets = props.property_sets(north.id);
    assert!(
        psets.iter().any(|p| p.name == "Pset_WallCommon"),
        "wall carries Pset_WallCommon, got {:?}",
        psets.iter().map(|p| p.name.clone()).collect::<Vec<_>>()
    );
    let qtys = props.quantities(north.id);
    let length = qtys
        .iter()
        .find(|q| q.name == "Length")
        .expect("Length quantity");
    assert!((length.value - 5.0).abs() < 1e-9);
    let net_side = qtys
        .iter()
        .find(|q| q.name == "NetSideArea")
        .expect("NetSideArea quantity");
    // 5 × 2.8 minus the 0.9 × 2.1 door
    assert!(
        (net_side.value - 12.11).abs() < 1e-6,
        "got {}",
        net_side.value
    );
}

#[test]
fn roundtrip_spaces_and_boundaries() {
    let mut room = sample_room();
    room.spaces.push(SpaceData {
        name: "101".into(),
        long_name: Some("Bedroom".into()),
        polygon: vec![(0.2, 0.2), (4.8, 0.2), (4.8, 3.8), (0.2, 3.8)],
        height: 0.0,
        z_offset: 0.0,
        storey: String::new(),
        psets: Vec::new(),
        identifier: None,
        slab_thickness: None,
        slab_only: false,
    });
    let ifc_content = write_ifc(&room);

    let model =
        bimifc_parser::parse_auto(&ifc_content).expect("parser should accept writer output");
    let resolver = model.resolver();

    assert_eq!(resolver.count_by_type(&IfcType::IfcSpace), 1);
    // north + east + south walls, door, window → 5 boundaries
    assert_eq!(resolver.count_by_type(&IfcType::IfcRelSpaceBoundary), 5);

    // The space hangs under the storey in the spatial tree.
    let spatial = model.spatial();
    let tree = spatial.spatial_tree().expect("spatial tree should exist");
    let storey = &tree.children[0].children[0].children[0];
    assert_eq!(storey.node_type, SpatialNodeType::Storey);
    let space = storey
        .children
        .iter()
        .find(|c| c.node_type == SpatialNodeType::Space)
        .expect("space under storey");
    assert_eq!(space.name, "101");

    // Openings are not spatially contained: still 9 elements + 1 space.
    assert_eq!(storey.children.len(), 10);

    let space_entity = &resolver.entities_by_type(&IfcType::IfcSpace)[0];
    let qtys = model.properties().quantities(space_entity.id);
    let area = qtys
        .iter()
        .find(|q| q.name == "GrossFloorArea")
        .expect("GrossFloorArea");
    assert!((area.value - 16.56).abs() < 1e-6, "got {}", area.value);
}

/// Doors and windows are assemblies of several solids; every one of them
/// must mesh through bimifc-geometry and together fill the element's box.
#[test]
fn roundtrip_door_and_window_assemblies_mesh() {
    use bimifc_geometry::router::GeometryRouter;

    let room = sample_room();
    let ifc_content = write_ifc(&room);
    let model =
        bimifc_parser::parse_auto(&ifc_content).expect("parser should accept writer output");
    let resolver = model.resolver();
    let router = GeometryRouter::with_default_processors();

    let body_item_count = |entity: &bimifc_model::DecodedEntity| -> usize {
        let shape = resolver.get(entity.get_ref(6).unwrap()).unwrap();
        let reps = shape
            .get(2)
            .and_then(|v| v.as_list().map(|l| l.to_vec()))
            .unwrap();
        reps.iter()
            .filter_map(|r| resolver.get(r.as_entity_ref()?))
            .filter(|rep| rep.get_string(1) == Some("Body"))
            .map(|rep| {
                rep.get(3)
                    .and_then(|v| v.as_list().map(|l| l.len()))
                    .unwrap_or(0)
            })
            .sum()
    };

    let extent = |mesh: &bimifc_geometry::mesh::Mesh| -> [f32; 3] {
        let (min, max) = mesh.bounds();
        [max.x - min.x, max.y - min.y, max.z - min.z]
    };
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3);

    let doors = resolver.entities_by_type(&IfcType::IfcDoor);
    let door = doors.first().expect("door");
    assert_eq!(body_item_count(door), 2, "door = frame + leaf");
    let door_mesh = router.process_element(door, resolver).expect("door meshes");
    assert!(door_mesh.triangle_count() >= 24, "both solids meshed");
    // Door rect is 0.9 × 0.2 in plan, 2.1 high.
    let e = extent(&door_mesh);
    assert!(
        close(e, [0.9, 0.216, 2.1]),
        "door assembly extent {e:?} (frame 8 mm proud of each face)"
    );
    let (min, _) = door_mesh.bounds();
    assert!(
        min.z.abs() < 1e-3,
        "door stands on the floor, min z {}",
        min.z
    );

    let windows = resolver.entities_by_type(&IfcType::IfcWindow);
    let window = windows.first().expect("window");
    assert_eq!(body_item_count(window), 2, "window = frame + pane");
    let window_mesh = router
        .process_element(window, resolver)
        .expect("window meshes");
    assert!(window_mesh.triangle_count() >= 24, "both solids meshed");
    // Window rect is 0.2 × 1.2 in plan (clear width along Y), 1.5 high.
    let e = extent(&window_mesh);
    assert!(
        close(e, [0.216, 1.2, 1.5]),
        "window assembly extent {e:?} (frame 8 mm proud of each face)"
    );
    let (min, _) = window_mesh.bounds();
    assert!(
        min.z.abs() < 1e-3,
        "window sits at its z offset, min z {}",
        min.z
    );
}
