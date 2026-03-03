use bimifc_geometry::GeometryRouter;
use bimifc_model::{DecodedEntity, EntityResolver, MeshData};

/// Internal geometry context wrapping GeometryRouter
///
/// Created lazily on first `get_geometry()` call.
pub struct GeometryContext {
    router: GeometryRouter,
}

impl GeometryContext {
    pub fn new(unit_scale: f64) -> Self {
        Self {
            router: GeometryRouter::with_default_processors_and_unit_scale(unit_scale),
        }
    }

    /// Process a single element into MeshData
    pub fn process_element(
        &self,
        element: &DecodedEntity,
        resolver: &dyn EntityResolver,
    ) -> Option<MeshData> {
        match self.router.process_element(element, resolver) {
            Ok(mesh) => {
                let mesh_data = mesh.to_mesh_data();
                if mesh_data.is_empty() {
                    None
                } else {
                    Some(mesh_data)
                }
            }
            Err(_) => None,
        }
    }
}
