// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Per-entity discipline classification.
//!
//! Three lookup paths, tried in order; first non-`Other` wins:
//!
//! 1. **Concrete IFC type name** — works for IFC4+ files that use
//!    `IfcCableSegment` / `IfcPipeSegment` / `IfcSpaceHeater` etc.
//! 2. **Defining IFC TypeObject** — IFC2x3 exporters (MagiCAD,
//!    Plancal) leave the instance Name null and attach the
//!    discipline-revealing class on `IfcRelDefinesByType →
//!    IfcDuctSegmentType` etc. The TUI scene module already does
//!    this; the classifier here is the rule it consults.
//! 3. **Instance Name keyword** — Revit-style exports name things
//!    helpfully ("Rectangular Duct", "Pipe Types: Waste", "Troffer
//!    Light…"). Last-resort when neither type-class layer matches.
//!
//! These functions are pure / synchronous; the caller decides which
//! to invoke and in what order based on what data it has.

use crate::source::Discipline;

/// Map an IFC entity *class* name (the wire-format uppercase string)
/// to a discipline. Handles modern IFC4-and-later concrete subtypes.
/// Returns `Other` if the class isn't a known MEP leaf — fall through
/// to [`classify_by_type_name`] or [`classify_by_name`] next.
pub fn classify_by_entity_class(class_name_upper: &str) -> Discipline {
    match class_name_upper {
        "IFCCABLESEGMENT"
        | "IFCCABLECARRIERSEGMENT"
        | "IFCCABLECARRIERFITTING"
        | "IFCELECTRICAPPLIANCE"
        | "IFCELECTRICDISTRIBUTIONBOARD"
        | "IFCJUNCTIONBOX"
        | "IFCOUTLET"
        | "IFCSWITCHINGDEVICE" => Discipline::Electrical,
        "IFCPIPESEGMENT"
        | "IFCPIPEFITTING"
        | "IFCSANITARYTERMINAL"
        | "IFCVALVE"
        | "IFCWASTETERMINAL" => Discipline::Plumbing,
        "IFCAIRTERMINAL"
        | "IFCAIRTERMINALBOX"
        | "IFCSPACEHEATER"
        | "IFCDUCTSEGMENT"
        | "IFCDUCTFITTING"
        | "IFCFAN" => Discipline::Hvac,
        "IFCLIGHTFIXTURE" => Discipline::Lighting,
        _ => Discipline::Other,
    }
}

/// Resolve discipline from the *defining IFC TypeObject* of an entity.
/// Used when the instance class is a generic IFC2x3 flow class
/// (`IfcFlowSegment`, `IfcFlowFitting`, `IfcFlowTerminal`) and the
/// real discipline lives on the type object.
///
/// `type_class_upper` is the type-object's IFC class name (e.g.
/// `IFCDUCTSEGMENTTYPE`); `type_name` is the type-object's optional
/// Name attribute (e.g. `Some("Spiro kanaler, Frzinkade")`).
pub fn classify_by_type_name(type_class_upper: &str, type_name: Option<&str>) -> Discipline {
    match type_class_upper {
        "IFCDUCTSEGMENTTYPE"
        | "IFCDUCTFITTINGTYPE"
        | "IFCAIRTERMINALTYPE"
        | "IFCAIRTERMINALBOXTYPE"
        | "IFCFANTYPE"
        | "IFCHEATEXCHANGERTYPE"
        | "IFCCOILTYPE"
        | "IFCBOILERTYPE"
        | "IFCCHILLERTYPE"
        | "IFCDAMPERTYPE"
        | "IFCSPACEHEATERTYPE" => return Discipline::Hvac,
        "IFCPIPESEGMENTTYPE"
        | "IFCPIPEFITTINGTYPE"
        | "IFCSANITARYTERMINALTYPE"
        | "IFCVALVETYPE"
        | "IFCWASTETERMINALTYPE"
        | "IFCPUMPTYPE"
        | "IFCTANKTYPE" => return Discipline::Plumbing,
        "IFCCABLESEGMENTTYPE"
        | "IFCCABLECARRIERSEGMENTTYPE"
        | "IFCCABLECARRIERFITTINGTYPE"
        | "IFCJUNCTIONBOXTYPE"
        | "IFCSWITCHINGDEVICETYPE"
        | "IFCOUTLETTYPE" => return Discipline::Electrical,
        "IFCLIGHTFIXTURETYPE" | "IFCLAMPTYPE" => return Discipline::Lighting,
        _ => {}
    }
    // Type class wasn't conclusive — try the type object's Name.
    if let Some(n) = type_name {
        return classify_by_name(n);
    }
    Discipline::Other
}

/// Last-resort: keyword match on a free-form Name string.
///
/// The keyword list is observation-driven (Revit exports, MagiCAD,
/// Plancal). Add more keywords as new datasets surface — keep the
/// most specific terms first so e.g. "diffuser" wins before a stray
/// "_air" match elsewhere in a compound name.
pub fn classify_by_name(name: &str) -> Discipline {
    let lower = name.to_ascii_lowercase();
    if lower.contains("troffer") || lower.contains("fixture") || lower.contains(" lamp") {
        return Discipline::Lighting;
    }
    if lower.contains("diffuser")
        || lower.contains("duct")
        || lower.contains("hvac")
        || lower.contains("supply air")
        || lower.contains("return air")
        || lower.contains("vav")
        || lower.contains("fan ")
        || lower.contains("coil")
        || lower.contains("damper")
    {
        return Discipline::Hvac;
    }
    if lower.contains("pipe")
        || lower.contains("plumb")
        || lower.contains("sanitary")
        || lower.contains("waste")
        || lower.contains("water")
        || lower.contains("hydronic")
        || lower.contains("sprinkler")
    {
        return Discipline::Plumbing;
    }
    if lower.contains("cable")
        || lower.contains("conduit")
        || lower.contains("electric")
        || lower.contains("circuit")
        || lower.contains("panel")
        || lower.contains("junction")
        || lower.contains("outlet")
    {
        return Discipline::Electrical;
    }
    Discipline::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modern_entity_classes() {
        assert_eq!(classify_by_entity_class("IFCPIPESEGMENT"), Discipline::Plumbing);
        assert_eq!(classify_by_entity_class("IFCAIRTERMINAL"), Discipline::Hvac);
        assert_eq!(classify_by_entity_class("IFCLIGHTFIXTURE"), Discipline::Lighting);
        assert_eq!(classify_by_entity_class("IFCCABLESEGMENT"), Discipline::Electrical);
        assert_eq!(classify_by_entity_class("IFCWALL"), Discipline::Other);
    }

    #[test]
    fn ifc2x3_via_type_object() {
        // The LTU file pattern: IfcFlowSegment with IfcRelDefinesByType
        // pointing at an IfcDuctSegmentType named "Spiro kanaler".
        assert_eq!(
            classify_by_type_name("IFCDUCTSEGMENTTYPE", Some("Spiro kanaler, Frzinkade")),
            Discipline::Hvac
        );
        // Generic class but a discipline-revealing Name on the type.
        assert_eq!(
            classify_by_type_name("IFCPRODUCTTYPE", Some("Pipe Types: Waste 50mm")),
            Discipline::Plumbing
        );
        // No info either way.
        assert_eq!(classify_by_type_name("IFCRANDOMTYPE", None), Discipline::Other);
    }

    #[test]
    fn revit_name_heuristic() {
        assert_eq!(
            classify_by_name("Rectangular Duct:Mitered Elbows / Tees"),
            Discipline::Hvac
        );
        assert_eq!(
            classify_by_name("Pipe Types: Waste:577155"),
            Discipline::Plumbing
        );
        assert_eq!(
            classify_by_name("M_Troffer Light - Parabolic Rectangular"),
            Discipline::Lighting
        );
    }
}
