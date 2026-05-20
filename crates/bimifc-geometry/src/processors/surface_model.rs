// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Surface-model processor — handles `IfcFaceBasedSurfaceModel` and
//! `IfcShellBasedSurfaceModel`.
//!
//! Both are lists of face containers (`IfcConnectedFaceSet`,
//! `IfcOpenShell`, `IfcClosedShell`), each of which holds `IfcFace`s
//! with the same `bounds → poly-loop` structure as `IfcFacetedBrep`.
//! We delegate the per-face triangulation to
//! `FacetedBrepProcessor::triangulate_face_list` so the math + hole
//! handling stays in one place.
//!
//! ## Why this exists
//!
//! Revit MEP 2011 (Duplex-A-MEP.ifc) and most IFC2x3 federated
//! datasets use this representation for ducts, pipes, fixtures, and
//! cable runs — wrapped inside `IfcMappedItem` for instancing. Without
//! a processor, every MEP mesh came back empty and the discipline
//! filter had nothing to gate.

use crate::router::GeometryProcessor;
use crate::{Mesh, Result};
use bimifc_model::{DecodedEntity, EntityResolver, IfcType};

use super::FacetedBrepProcessor;

pub struct FaceBasedSurfaceModelProcessor {
    /// Reuses the face/poly-loop triangulation already proven in the
    /// `IfcFacetedBrep` path. Holding a copy is fine because the inner
    /// struct is zero-sized.
    brep: FacetedBrepProcessor,
}

impl FaceBasedSurfaceModelProcessor {
    pub fn new() -> Self {
        Self {
            brep: FacetedBrepProcessor::new(),
        }
    }
}

impl Default for FaceBasedSurfaceModelProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl GeometryProcessor for FaceBasedSurfaceModelProcessor {
    fn process(
        &self,
        entity: &DecodedEntity,
        resolver: &dyn EntityResolver,
        _unit_scale: f64,
    ) -> Result<Mesh> {
        // Both supported entity types carry a single attribute (index 0):
        // an aggregate of face-containers. The containers (Connected/
        // Open/Closed shell or face-set) each carry their face list
        // at their own index 0.
        let container_attr = entity
            .get(0)
            .and_then(|v| v.as_list())
            .ok_or_else(|| crate::Error::invalid_attribute(0, "Missing face-set list"))?;

        let mut combined = Mesh::new();
        for container_ref in container_attr {
            let container_id = match container_ref.as_entity_ref() {
                Some(id) => id,
                None => continue,
            };
            let container = match resolver.get(container_id) {
                Some(e) => e,
                None => continue,
            };
            // CfsFaces / inner face list — all variants put it at index 0.
            let faces = match container.get(0).and_then(|v| v.as_list()) {
                Some(f) => f,
                None => continue,
            };
            let mesh = self.brep.triangulate_face_list(faces, resolver);
            combined.merge(&mesh);
        }

        Ok(combined)
    }

    fn supported_types(&self) -> Vec<IfcType> {
        vec![
            IfcType::IfcFaceBasedSurfaceModel,
            IfcType::IfcShellBasedSurfaceModel,
        ]
    }
}
