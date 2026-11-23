// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Import-side regression: a typed `IFCPOWERMEASURE` nominal value must
//! preserve its IFC measure type in `Property.value_type`, so it can
//! round-trip back to the same IFC type on export.

use bimifc_model::EntityId;

/// Minimal IFC2X3 file: one wall (#10) with a property set (#30) attached via
/// IfcRelDefinesByProperties (#40). The set carries a typed power measure and a
/// bare label so we can check both the typed and untyped cases.
const FIXTURE: &str = "\
ISO-10303-21;
HEADER;
FILE_DESCRIPTION((''),'2;1');
FILE_NAME('t.ifc','2026-01-01T00:00:00',(''),(''),'','','');
FILE_SCHEMA(('IFC2X3'));
ENDSEC;
DATA;
#1=IFCWALLSTANDARDCASE('0g$aP3lxz0sRr1MihNFaSY',$,'Wall',$,$,$,$,$);
#20=IFCPROPERTYSINGLEVALUE('RatedPower',$,IFCPOWERMEASURE(18.5),$);
#21=IFCPROPERTYSINGLEVALUE('FireRating',$,IFCLABEL('F90'),$);
#22=IFCPROPERTYSINGLEVALUE('Count',$,42,$);
#30=IFCPROPERTYSET('1g$aP3lxz0sRr1MihNFaSY',$,'Pset_WallCommon',$,(#20,#21,#22));
#40=IFCRELDEFINESBYPROPERTIES('2g$aP3lxz0sRr1MihNFaSY',$,$,$,(#1),#30);
ENDSEC;
END-ISO-10303-21;
";

#[test]
fn typed_power_measure_preserves_value_type() {
    let model = bimifc_parser::parse(FIXTURE).expect("parse fixture");
    let psets = model.properties().property_sets(EntityId(1));

    let pset = psets
        .iter()
        .find(|p| p.name == "Pset_WallCommon")
        .expect("Pset_WallCommon attached to wall #1");

    let power = pset.get("RatedPower").expect("RatedPower property");
    assert_eq!(power.value, "18.5");
    assert_eq!(
        power.value_type.as_deref(),
        Some("IFCPOWERMEASURE"),
        "typed measure name must be preserved for round-trip"
    );

    let label = pset.get("FireRating").expect("FireRating property");
    assert_eq!(label.value, "F90");
    assert_eq!(label.value_type.as_deref(), Some("IFCLABEL"));

    // A bare (non-typed) value carries no value_type.
    let count = pset.get("Count").expect("Count property");
    assert_eq!(count.value, "42");
    assert_eq!(count.value_type, None);
}
