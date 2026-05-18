// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! IFC built-in functions for the AutoLISP interpreter

use acadlisp::{Expr, ForeignContext, Interpreter};
use bimifc_geometry::GeometryRouter;
use bimifc_model::{EntityId, IfcModel, SpatialNode};
use bimifc_parser::parse_auto;
use std::sync::Arc;

use crate::IfcState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn get_model(ctx: &ForeignContext) -> Option<Arc<dyn IfcModel>> {
    ctx.user_data
        .downcast_ref::<IfcState>()
        .and_then(|s| s.model.clone())
}

pub(crate) fn expr_to_entity_id(e: &Expr) -> Option<EntityId> {
    match e {
        Expr::Integer(i) => Some(EntityId(*i as u32)),
        Expr::Real(r) => Some(EntityId(*r as u32)),
        _ => None,
    }
}

pub(crate) fn expr_to_string(e: &Expr) -> Option<String> {
    match e {
        Expr::String(s) => Some(s.clone()),
        Expr::Symbol(s) => Some(s.clone()),
        _ => None,
    }
}

fn entity_ids_to_list(ids: &[EntityId]) -> Expr {
    Expr::List(ids.iter().map(|id| Expr::Integer(id.0 as i32)).collect())
}

fn dotted_pair(key: &str, value: &str) -> Expr {
    Expr::List(vec![
        Expr::String(key.to_string()),
        Expr::String(value.to_string()),
    ])
}

fn spatial_node_to_expr(node: &SpatialNode) -> Expr {
    let children: Vec<Expr> = node.children.iter().map(spatial_node_to_expr).collect();
    Expr::List(vec![
        Expr::Integer(node.id.0 as i32),
        Expr::String(node.entity_type.clone()),
        Expr::String(node.name.clone()),
        Expr::List(children),
    ])
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register_all(interp: &mut Interpreter) {
    interp.register_fn("IFC-LOAD", ifc_load);
    interp.register_fn("IFC-ENTITY-COUNT", ifc_entity_count);
    interp.register_fn("IFC-ENTITIES", ifc_entities);
    interp.register_fn("IFC-ENTITIES-BY-TYPE", ifc_entities_by_type);
    interp.register_fn("IFC-ENTITY-TYPE", ifc_entity_type);
    interp.register_fn("IFC-ENTITY-NAME", ifc_entity_name);
    interp.register_fn("IFC-PROPERTY-SETS", ifc_property_sets);
    interp.register_fn("IFC-GET-PROPERTY", ifc_get_property);
    interp.register_fn("IFC-QUANTITIES", ifc_quantities);
    interp.register_fn("IFC-STOREYS", ifc_storeys);
    interp.register_fn("IFC-ELEMENTS-IN-STOREY", ifc_elements_in_storey);
    interp.register_fn("IFC-SEARCH", ifc_search);
    interp.register_fn("IFC-SPATIAL-TREE", ifc_spatial_tree);
    interp.register_fn("IFC-METADATA", ifc_metadata);
}

// ---------------------------------------------------------------------------
// Function implementations
// ---------------------------------------------------------------------------

/// (ifc-load "path.ifc") → T on success, Nil on failure
fn ifc_load(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let path = match args.first().and_then(expr_to_string) {
        Some(p) => p,
        None => {
            ctx.output
                .push("Error: IFC-LOAD requires a file path string\n".to_string());
            return Expr::Nil;
        }
    };

    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            ctx.output
                .push(format!("Error: cannot read '{}': {}\n", path, e));
            return Expr::Nil;
        }
    };

    match parse_auto(&content) {
        Ok(model) => {
            if let Some(state) = ctx.user_data.downcast_mut::<IfcState>() {
                let router =
                    GeometryRouter::with_default_processors_and_unit_scale(model.unit_scale());
                state.router = Some(router);
                state.model = Some(model);
            }
            Expr::T
        }
        Err(e) => {
            ctx.output.push(format!("Error: parse failed: {}\n", e));
            Expr::Nil
        }
    }
}

/// (ifc-entity-count) → integer
fn ifc_entity_count(ctx: &mut ForeignContext, _args: &[Expr]) -> Expr {
    match get_model(ctx) {
        Some(m) => Expr::Integer(m.resolver().entity_count() as i32),
        None => Expr::Nil,
    }
}

/// (ifc-entities) → list of integer IDs
fn ifc_entities(ctx: &mut ForeignContext, _args: &[Expr]) -> Expr {
    match get_model(ctx) {
        Some(m) => entity_ids_to_list(&m.resolver().all_ids()),
        None => Expr::Nil,
    }
}

/// (ifc-entities-by-type "IfcWall") → list of integer IDs
fn ifc_entities_by_type(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let type_name = match args.first().and_then(expr_to_string) {
        Some(s) => s,
        None => return Expr::Nil,
    };
    match get_model(ctx) {
        Some(m) => {
            let entities = m.resolver().find_by_type_name(&type_name);
            let ids: Vec<EntityId> = entities.iter().map(|e| e.id).collect();
            entity_ids_to_list(&ids)
        }
        None => Expr::Nil,
    }
}

/// (ifc-entity-type id) → string (e.g. "IfcWall")
fn ifc_entity_type(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let id = match args.first().and_then(expr_to_entity_id) {
        Some(id) => id,
        None => return Expr::Nil,
    };
    match get_model(ctx) {
        Some(m) => match m.resolver().get(id) {
            Some(entity) => Expr::String(format!("{:?}", entity.ifc_type)),
            None => Expr::Nil,
        },
        None => Expr::Nil,
    }
}

/// (ifc-entity-name id) → string or Nil
fn ifc_entity_name(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let id = match args.first().and_then(expr_to_entity_id) {
        Some(id) => id,
        None => return Expr::Nil,
    };
    match get_model(ctx) {
        Some(m) => match m.properties().name(id) {
            Some(name) => Expr::String(name),
            None => Expr::Nil,
        },
        None => Expr::Nil,
    }
}

/// (ifc-property-sets id) → (("PsetName" ("key" "val") ...) ...)
fn ifc_property_sets(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let id = match args.first().and_then(expr_to_entity_id) {
        Some(id) => id,
        None => return Expr::Nil,
    };
    let model = match get_model(ctx) {
        Some(m) => m,
        None => return Expr::Nil,
    };

    let psets = model.properties().property_sets(id);
    let result: Vec<Expr> = psets
        .iter()
        .map(|pset| {
            let mut items: Vec<Expr> = vec![Expr::String(pset.name.clone())];
            for prop in &pset.properties {
                items.push(dotted_pair(&prop.name, &prop.value));
            }
            Expr::List(items)
        })
        .collect();
    Expr::List(result)
}

/// (ifc-get-property id "PropertyName") → string or Nil
fn ifc_get_property(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let id = match args.first().and_then(expr_to_entity_id) {
        Some(id) => id,
        None => return Expr::Nil,
    };
    let name = match args.get(1).and_then(expr_to_string) {
        Some(s) => s,
        None => return Expr::Nil,
    };
    match get_model(ctx) {
        Some(m) => match m.properties().get_property(id, &name) {
            Some(prop) => Expr::String(prop.value),
            None => Expr::Nil,
        },
        None => Expr::Nil,
    }
}

/// (ifc-quantities id) → (("name" value "unit") ...)
fn ifc_quantities(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let id = match args.first().and_then(expr_to_entity_id) {
        Some(id) => id,
        None => return Expr::Nil,
    };
    let model = match get_model(ctx) {
        Some(m) => m,
        None => return Expr::Nil,
    };

    let quantities = model.properties().quantities(id);
    let result: Vec<Expr> = quantities
        .iter()
        .map(|q| {
            Expr::List(vec![
                Expr::String(q.name.clone()),
                Expr::Real(q.value),
                Expr::String(q.unit.clone()),
            ])
        })
        .collect();
    Expr::List(result)
}

/// (ifc-storeys) → ((id "name" elevation) ...)
fn ifc_storeys(ctx: &mut ForeignContext, _args: &[Expr]) -> Expr {
    let model = match get_model(ctx) {
        Some(m) => m,
        None => return Expr::Nil,
    };

    let storeys = model.spatial().storeys();
    let result: Vec<Expr> = storeys
        .iter()
        .map(|s| {
            Expr::List(vec![
                Expr::Integer(s.id.0 as i32),
                Expr::String(s.name.clone()),
                Expr::Real(s.elevation as f64),
            ])
        })
        .collect();
    Expr::List(result)
}

/// (ifc-elements-in-storey storey-id) → list of IDs
fn ifc_elements_in_storey(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let id = match args.first().and_then(expr_to_entity_id) {
        Some(id) => id,
        None => return Expr::Nil,
    };
    match get_model(ctx) {
        Some(m) => entity_ids_to_list(&m.spatial().elements_in_storey(id)),
        None => Expr::Nil,
    }
}

/// (ifc-search "query") → list of IDs
fn ifc_search(ctx: &mut ForeignContext, args: &[Expr]) -> Expr {
    let query = match args.first().and_then(expr_to_string) {
        Some(s) => s,
        None => return Expr::Nil,
    };
    match get_model(ctx) {
        Some(m) => entity_ids_to_list(&m.spatial().search(&query)),
        None => Expr::Nil,
    }
}

/// (ifc-spatial-tree) → nested list (id "type" "name" (children...))
fn ifc_spatial_tree(ctx: &mut ForeignContext, _args: &[Expr]) -> Expr {
    let model = match get_model(ctx) {
        Some(m) => m,
        None => return Expr::Nil,
    };
    match model.spatial().spatial_tree() {
        Some(tree) => spatial_node_to_expr(tree),
        None => Expr::Nil,
    }
}

/// (ifc-metadata) → (("schema" "IFC4") ("system" "Revit") ...)
fn ifc_metadata(ctx: &mut ForeignContext, _args: &[Expr]) -> Expr {
    let model = match get_model(ctx) {
        Some(m) => m,
        None => return Expr::Nil,
    };
    let meta = model.metadata();
    let mut pairs: Vec<Expr> = vec![dotted_pair("schema", &meta.schema_version)];
    if let Some(ref sys) = meta.originating_system {
        pairs.push(dotted_pair("system", sys));
    }
    if let Some(ref name) = meta.file_name {
        pairs.push(dotted_pair("file", name));
    }
    if let Some(ref desc) = meta.file_description {
        pairs.push(dotted_pair("description", desc));
    }
    if let Some(ref author) = meta.author {
        pairs.push(dotted_pair("author", author));
    }
    if let Some(ref org) = meta.organization {
        pairs.push(dotted_pair("organization", org));
    }
    Expr::List(pairs)
}
