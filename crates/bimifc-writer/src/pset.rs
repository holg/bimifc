//! IFC property-set writer — generic `IFCPROPERTYSET` /
//! `IFCPROPERTYSINGLEVALUE` / `IFCRELDEFINESBYPROPERTIES` emission.
//!
//! Property sets are how IFC carries non-geometric data (electrical
//! ratings, maintenance values, marketing attributes, …). This module
//! is domain-agnostic: callers supply a Pset name plus typed
//! name/value pairs and get back the STEP entities, attached to any
//! product occurrence(s) via `IfcRelDefinesByProperties`.
//!
//! The shape mirrors the proven implementation in gldf-rs's
//! `step_writer.rs` (which predates this module); gldf-rs is expected
//! to migrate onto this API so there's one implementation.
//!
//! # bSDD property identity
//!
//! buildingSMART's data dictionary assigns stable GlobalIds to
//! standard properties (e.g. `2AvYrjYPr0sRr1MihNFaSY` = "access
//! clearance bottom"). `IFCPROPERTYSINGLEVALUE` has no GlobalId
//! attribute of its own — only rooted entities do — so the convention
//! used here is to carry the bSDD id in the property's *Description*
//! attribute as `bSDD:<id>`. Consumers that understand bSDD can match
//! on it; everyone else sees a harmless description string.

use crate::guid::new_global_id;
use crate::model::IfcSchema;
use crate::step::{escape, ref_list, IdAlloc, StepBuilder};

/// A typed IFC property value. Maps 1:1 onto the IFC defined types
/// used inside `IFCPROPERTYSINGLEVALUE`.
///
/// The measure variants matter to BIM tools: a wattage exported as
/// `IFCPOWERMEASURE` shows up with units in Revit/Solibri, whereas a
/// bare `IFCREAL` is just a number.
#[derive(Debug, Clone, PartialEq)]
pub enum IfcValue {
    /// `IFCLABEL` — short human-readable string (≤ 255 chars).
    Label(String),
    /// `IFCTEXT` — longer prose.
    Text(String),
    /// `IFCIDENTIFIER` — machine-oriented id string.
    Identifier(String),
    /// `IFCINTEGER`
    Integer(i64),
    /// `IFCREAL`
    Real(f64),
    /// `IFCBOOLEAN` — serialised as `.T.` / `.F.`.
    Boolean(bool),
    /// `IFCPOWERMEASURE` — watts.
    PowerMeasure(f64),
    /// `IFCELECTRICVOLTAGEMEASURE` — volts.
    ElectricVoltageMeasure(f64),
    /// `IFCELECTRICCURRENTMEASURE` — amperes.
    ElectricCurrentMeasure(f64),
    /// `IFCFREQUENCYMEASURE` — hertz.
    FrequencyMeasure(f64),
    /// `IFCMASSMEASURE` — kilograms.
    MassMeasure(f64),
    /// `IFCPOSITIVELENGTHMEASURE` — meters (project length unit).
    PositiveLengthMeasure(f64),
    /// `IFCTHERMODYNAMICTEMPERATUREMEASURE` — kelvin (CCT etc.).
    ThermodynamicTemperatureMeasure(f64),
    /// `IFCLUMINOUSFLUXMEASURE` — lumens.
    LuminousFluxMeasure(f64),
    /// `IFCPLANEANGLEMEASURE` — radians (cut-off angles etc.).
    PlaneAngleMeasure(f64),
}

impl IfcValue {
    /// Render as the STEP value expression inside
    /// `IFCPROPERTYSINGLEVALUE(name, desc, <here>, $)`.
    fn to_step(&self) -> String {
        match self {
            Self::Label(s) => format!("IFCLABEL('{}')", escape(s)),
            Self::Text(s) => format!("IFCTEXT('{}')", escape(s)),
            Self::Identifier(s) => format!("IFCIDENTIFIER('{}')", escape(s)),
            Self::Integer(n) => format!("IFCINTEGER({n})"),
            Self::Real(x) => format!("IFCREAL({})", fmt_real(*x)),
            Self::Boolean(b) => format!("IFCBOOLEAN({})", if *b { ".T." } else { ".F." }),
            Self::PowerMeasure(x) => format!("IFCPOWERMEASURE({})", fmt_real(*x)),
            Self::ElectricVoltageMeasure(x) => {
                format!("IFCELECTRICVOLTAGEMEASURE({})", fmt_real(*x))
            }
            Self::ElectricCurrentMeasure(x) => {
                format!("IFCELECTRICCURRENTMEASURE({})", fmt_real(*x))
            }
            Self::FrequencyMeasure(x) => format!("IFCFREQUENCYMEASURE({})", fmt_real(*x)),
            Self::MassMeasure(x) => format!("IFCMASSMEASURE({})", fmt_real(*x)),
            Self::PositiveLengthMeasure(x) => {
                format!("IFCPOSITIVELENGTHMEASURE({})", fmt_real(*x))
            }
            Self::ThermodynamicTemperatureMeasure(x) => {
                format!("IFCTHERMODYNAMICTEMPERATUREMEASURE({})", fmt_real(*x))
            }
            Self::LuminousFluxMeasure(x) => {
                format!("IFCLUMINOUSFLUXMEASURE({})", fmt_real(*x))
            }
            Self::PlaneAngleMeasure(x) => format!("IFCPLANEANGLEMEASURE({})", fmt_real(*x)),
        }
    }
}

/// STEP requires reals to carry a decimal point (`1.` not `1`).
///
/// Values are rounded to 1e-9 first so that binary float noise from
/// coordinate arithmetic (`0.6000000000000001`) doesn't leak into files.
pub(crate) fn fmt_real(x: f64) -> String {
    let rounded = (x * 1e9).round() / 1e9;
    // `-0.0` would print as "-0." — normalise to plain zero.
    let rounded = if rounded == 0.0 { 0.0 } else { rounded };
    let x = if rounded.is_finite() { rounded } else { x };
    let s = format!("{x}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.")
    }
}

/// One named property inside a Pset.
#[derive(Debug, Clone, PartialEq)]
pub struct IfcProperty {
    /// Property name as it appears in the Pset (e.g. `RatedInputPower`).
    pub name: String,
    /// Optional description. Conventionally carries the bSDD GlobalId
    /// as `bSDD:<22-char-id>` — see module docs.
    pub description: Option<String>,
    pub value: IfcValue,
}

impl IfcProperty {
    /// Shorthand constructor without a description.
    pub fn new(name: impl Into<String>, value: IfcValue) -> Self {
        Self {
            name: name.into(),
            description: None,
            value,
        }
    }

    /// Attach a description. Used for units that IFC has no measure
    /// type for — photon flux in µmol/s, say — where the number alone
    /// is ambiguous.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Constructor carrying a bSDD GlobalId in the description.
    pub fn with_bsdd(name: impl Into<String>, bsdd_id: &str, value: IfcValue) -> Self {
        Self {
            name: name.into(),
            description: Some(format!("bSDD:{bsdd_id}")),
            value,
        }
    }
}

/// Write a complete property set and attach it to `elements` via
/// `IFCRELDEFINESBYPROPERTIES`.
///
/// * Empty `props` → nothing is written, returns `None`.
/// * Empty `elements` → the `IFCPROPERTYSET` is written but no
///   rel-defines; the caller can reference the returned id from an
///   `IfcTypeObject.HasPropertySets` list instead (the IFC-correct way
///   to attach Psets to *type* objects rather than occurrences).
///
/// Returns the STEP id of the `IFCPROPERTYSET`.
pub fn write_property_set(
    ids: &mut IdAlloc,
    b: &mut StepBuilder,
    owner_history: u32,
    elements: &[u32],
    pset_name: &str,
    props: &[IfcProperty],
) -> Option<u32> {
    if props.is_empty() {
        return None;
    }

    let mut prop_ids: Vec<u32> = Vec::with_capacity(props.len());
    for p in props {
        let id = ids.next();
        let desc = match &p.description {
            Some(d) => format!("'{}'", escape(d)),
            None => "$".to_string(),
        };
        b.entity(
            id,
            &format!(
                "IFCPROPERTYSINGLEVALUE('{}',{},{},$)",
                escape(&p.name),
                desc,
                p.value.to_step()
            ),
        );
        prop_ids.push(id);
    }

    let pset = ids.next();
    b.entity(
        pset,
        &format!(
            "IFCPROPERTYSET('{}',#{owner_history},'{}',$,{})",
            new_global_id(),
            escape(pset_name),
            ref_list(&prop_ids)
        ),
    );

    if !elements.is_empty() {
        let rel = ids.next();
        b.entity(
            rel,
            &format!(
                "IFCRELDEFINESBYPROPERTIES('{}',#{owner_history},$,$,{},#{pset})",
                new_global_id(),
                ref_list(elements)
            ),
        );
    }

    Some(pset)
}

/// One base quantity inside an `IFCELEMENTQUANTITY`.
///
/// Maps onto the IFC2X3 `IfcPhysicalSimpleQuantity` subtypes. Units are
/// the project units (metres, square metres, cubic metres).
#[derive(Debug, Clone, PartialEq)]
pub enum IfcQuantity {
    /// `IFCQUANTITYLENGTH`
    Length(String, f64),
    /// `IFCQUANTITYAREA`
    Area(String, f64),
    /// `IFCQUANTITYVOLUME`
    Volume(String, f64),
    /// `IFCQUANTITYCOUNT`
    Count(String, f64),
}

impl IfcQuantity {
    /// STEP line for the given schema.
    ///
    /// IFC4 added a trailing optional `Formula` to every `IfcQuantity*`
    /// (4 → 5 attributes), so a 2x3-shaped quantity line is invalid
    /// IFC4. `IfcElementQuantity` itself is unchanged, which makes this
    /// easy to miss.
    fn to_step(&self, schema: IfcSchema) -> String {
        let (entity, name, value) = match self {
            Self::Length(n, v) => ("IFCQUANTITYLENGTH", n, v),
            Self::Area(n, v) => ("IFCQUANTITYAREA", n, v),
            Self::Volume(n, v) => ("IFCQUANTITYVOLUME", n, v),
            Self::Count(n, v) => ("IFCQUANTITYCOUNT", n, v),
        };
        let formula = if schema.is_ifc4() { ",$" } else { "" };
        format!(
            "{entity}('{}',$,$,{}{formula})",
            escape(name),
            fmt_real(*value)
        )
    }
}

/// Write an `IFCELEMENTQUANTITY` (IFC2X3 base quantities, conventionally
/// named `BaseQuantities`) and attach it to `elements` via
/// `IFCRELDEFINESBYPROPERTIES`. Same contract as [`write_property_set`]:
/// empty `quantities` writes nothing, empty `elements` skips the rel.
pub fn write_element_quantities(
    ids: &mut IdAlloc,
    b: &mut StepBuilder,
    owner_history: u32,
    elements: &[u32],
    qset_name: &str,
    quantities: &[IfcQuantity],
    schema: IfcSchema,
) -> Option<u32> {
    if quantities.is_empty() {
        return None;
    }

    let mut q_ids: Vec<u32> = Vec::with_capacity(quantities.len());
    for q in quantities {
        let id = ids.next();
        b.entity(id, &q.to_step(schema));
        q_ids.push(id);
    }

    let qset = ids.next();
    b.entity(
        qset,
        &format!(
            "IFCELEMENTQUANTITY('{}',#{owner_history},'{}',$,$,{})",
            new_global_id(),
            escape(qset_name),
            ref_list(&q_ids)
        ),
    );

    if !elements.is_empty() {
        let rel = ids.next();
        b.entity(
            rel,
            &format!(
                "IFCRELDEFINESBYPROPERTIES('{}',#{owner_history},$,$,{},#{qset})",
                new_global_id(),
                ref_list(elements)
            ),
        );
    }

    Some(qset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(props: &[IfcProperty], elements: &[u32]) -> String {
        let mut ids = IdAlloc::new();
        let mut b = StepBuilder::new();
        // Fake owner history at #1 so the test output references it.
        let oh = ids.next();
        b.entity(oh, "IFCOWNERHISTORY($,$,$,.ADDED.,$,$,$,0)");
        write_property_set(&mut ids, &mut b, oh, elements, "Pset_Test", props);
        b.into_data_section()
    }

    #[test]
    fn empty_props_writes_nothing() {
        let out = render(&[], &[42]);
        assert!(!out.contains("IFCPROPERTYSET"));
        assert!(!out.contains("IFCRELDEFINESBYPROPERTIES"));
    }

    #[test]
    fn writes_pset_and_rel() {
        let props = vec![
            IfcProperty::new("RatedInputPower", IfcValue::PowerMeasure(18.5)),
            IfcProperty::new("Dimmable", IfcValue::Boolean(true)),
        ];
        let out = render(&props, &[42]);
        assert!(out.contains("IFCPROPERTYSINGLEVALUE('RatedInputPower',$,IFCPOWERMEASURE(18.5),$)"));
        assert!(out.contains("IFCPROPERTYSINGLEVALUE('Dimmable',$,IFCBOOLEAN(.T.),$)"));
        assert!(out.contains("'Pset_Test'"));
        assert!(out.contains("IFCRELDEFINESBYPROPERTIES"));
        assert!(out.contains("(#42)"));
    }

    #[test]
    fn no_elements_skips_rel_but_returns_pset() {
        let mut ids = IdAlloc::new();
        let mut b = StepBuilder::new();
        let oh = ids.next();
        b.entity(oh, "IFCOWNERHISTORY($,$,$,.ADDED.,$,$,$,0)");
        let pset = write_property_set(
            &mut ids,
            &mut b,
            oh,
            &[],
            "Pset_TypeAttached",
            &[IfcProperty::new("X", IfcValue::Integer(1))],
        );
        assert!(pset.is_some());
        let out = b.into_data_section();
        assert!(out.contains("IFCPROPERTYSET"));
        assert!(!out.contains("IFCRELDEFINESBYPROPERTIES"));
    }

    #[test]
    fn bsdd_id_lands_in_description() {
        let props = vec![IfcProperty::with_bsdd(
            "median useful life",
            "2AvYrjYPr0sRr1MihNFaSY",
            IfcValue::Label("L80B50 70000h".into()),
        )];
        let out = render(&props, &[7]);
        assert!(out.contains("'bSDD:2AvYrjYPr0sRr1MihNFaSY'"));
        assert!(out.contains("IFCLABEL('L80B50 70000h')"));
    }

    #[test]
    fn writes_element_quantities() {
        let mut ids = IdAlloc::new();
        let mut b = StepBuilder::new();
        let oh = ids.next();
        b.entity(oh, "IFCOWNERHISTORY($,$,$,.ADDED.,$,$,$,0)");
        let q = write_element_quantities(
            &mut ids,
            &mut b,
            oh,
            &[9],
            "BaseQuantities",
            &[
                IfcQuantity::Length("Length".into(), 5.0),
                IfcQuantity::Area("GrossFootprintArea".into(), 1.0),
                IfcQuantity::Volume("GrossVolume".into(), 2.8),
            ],
            IfcSchema::Ifc2x3,
        );
        assert!(q.is_some());
        let out = b.into_data_section();
        assert!(out.contains("IFCQUANTITYLENGTH('Length',$,$,5.)"));
        assert!(out.contains("IFCQUANTITYAREA('GrossFootprintArea',$,$,1.)"));
        assert!(out.contains("IFCQUANTITYVOLUME('GrossVolume',$,$,2.8)"));
        assert!(out.contains("IFCELEMENTQUANTITY("));
        assert!(out.contains("'BaseQuantities'"));
        assert!(out.contains("(#9)"));
    }

    #[test]
    fn reals_always_carry_decimal_point() {
        assert_eq!(fmt_real(5.0), "5.");
        assert_eq!(fmt_real(5.25), "5.25");
        assert_eq!(fmt_real(0.0), "0.");
        assert_eq!(fmt_real(0.1 + 0.2), "0.3");
        assert_eq!(fmt_real(-0.6000000000000001), "-0.6");
        assert_eq!(fmt_real(-0.0), "0.");
        assert_eq!(fmt_real(-6.123e-17), "0.");
    }

    #[test]
    fn strings_are_step_escaped() {
        let props = vec![IfcProperty::new(
            "Name",
            IfcValue::Label("O'Brien \\ Sons".into()),
        )];
        let out = render(&props, &[7]);
        assert!(out.contains("IFCLABEL('O''Brien \\\\ Sons')"));
    }
}
