use std::time::Instant;
use bimifc_parser::ParsedModel;
use bimifc_model::{EntityResolver, IfcModel, IfcType};

fn main() {
    let path = std::env::args().nth(1).expect("need IFC path");
    let t0 = Instant::now();
    let content = std::fs::read_to_string(&path).expect("read");
    let read_time = t0.elapsed();
    let size_mb = content.len() as f64 / 1024.0 / 1024.0;
    println!("Read {size_mb:.1} MB in {read_time:?}");

    let t1 = Instant::now();
    let model = ParsedModel::parse(&content, true, true).expect("parse");
    let parse_time = t1.elapsed();
    println!("Parsed in {parse_time:?}");

    let resolver = model.resolver();
    println!("Total entities: {}", resolver.entity_count());

    let t2 = Instant::now();
    let types: Vec<IfcType> = resolver.types_present();
    let types_time = t2.elapsed();
    println!("types_present() in {types_time:?}, {} distinct types", types.len());

    // Count MEP / lighting
    let mep_classes = [
        IfcType::IfcLightFixture,
        IfcType::IfcCableSegment, IfcType::IfcCableCarrierSegment, IfcType::IfcCableCarrierFitting,
        IfcType::IfcPipeSegment, IfcType::IfcPipeFitting,
        IfcType::IfcSpaceHeater, IfcType::IfcAirTerminal,
        IfcType::IfcFlowSegment, IfcType::IfcFlowFitting, IfcType::IfcFlowTerminal,
        IfcType::IfcEnergyConversionDevice,
    ];
    for c in &mep_classes {
        let n = resolver.count_by_type(c);
        if n > 0 {
            println!("  {c:?}: {n}");
        }
    }

    // Geometry-eligible product count
    let t3 = Instant::now();
    let mut prod_count = 0usize;
    let geo_types: Vec<IfcType> = resolver.types_present();
    for t in &geo_types {
        if t.has_geometry() {
            prod_count += resolver.count_by_type(t);
        }
    }
    println!("Geometry-eligible products: {} (in {:?})", prod_count, t3.elapsed());
}
