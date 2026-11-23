//! GLDF luminaire import + lux (radiosity) calculation — UniFFI surface.
//!
//! Bridges two sibling crates into the app-facing FFI:
//! * `gldf-rs` — parses `.gldf` containers (product XML, LDT photometry,
//!   L3D geometry) and extracts a renderable triangle mesh.
//! * `eulumdat` — LDT/IES photometry and the patch-radiosity engine used
//!   for room illuminance (validated against CIE 171 / DIALux).
//!
//! # Coordinate convention
//!
//! The lux calculator works in the radiosity engine's frame: **z-up**, room
//! box `[0,L]×[0,W]×[0,H]` with the floor at z=0, metres. Callers coming
//! from RoomPlan/SceneKit (y-up, room-centred) must map
//! `(x, y_height, z_depth)` → `(x − minX, z − minZ, y)` before calling in,
//! and mirror the mapping when reading grids back.

use std::sync::{Arc, Mutex};

use eulumdat::{
    compute_form_factors, direct_illuminance, illuminance_at_point, solve_radiosity, Eulumdat,
    FormFactors, IesExporter, IesParser, Luminaire, RadiosityResult, RoomMesh, SurfaceReflectances,
    Vec3,
};
use gldf_rs::{BufFile, GldfProduct, GldfToIfc};

use crate::MeshData;

// ═══════════════════════════════════════════════════════════════════════════
// Error type
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum LightingError {
    #[error("GLDF parse error: {message}")]
    ParseError { message: String },
    #[error("Not found: {message}")]
    NotFound { message: String },
    #[error("Calculation error: {message}")]
    CalcError { message: String },
}

// ═══════════════════════════════════════════════════════════════════════════
// GLDF luminaire
// ═══════════════════════════════════════════════════════════════════════════

/// A product variant inside a GLDF container.
#[derive(uniffi::Record)]
pub struct FfiGldfVariant {
    pub id: String,
    pub name: String,
    pub geometry_id: Option<String>,
    pub emitter_ids: Vec<String>,
}

/// Photometric key figures for one emitter.
#[derive(uniffi::Record)]
pub struct FfiPhotometrySummary {
    /// Total luminous flux in lumens (rated if present, else integrated
    /// from the LDT).
    pub flux_lm: f64,
    /// Correlated colour temperature in kelvin, when derivable.
    pub cct_k: Option<f64>,
    /// "fixed", "changeable", or "sensor".
    pub source_type: String,
}

/// A parsed GLDF luminaire container.
#[derive(uniffi::Object)]
pub struct GldfLuminaire {
    product: GldfProduct,
    files: Vec<BufFile>,
}

#[uniffi::export]
impl GldfLuminaire {
    /// Parse a `.gldf` container from raw bytes.
    #[uniffi::constructor]
    pub fn from_bytes(data: Vec<u8>) -> Result<Arc<Self>, LightingError> {
        let all =
            GldfProduct::load_gldf_from_buf_all(data).map_err(|e| LightingError::ParseError {
                message: e.to_string(),
            })?;
        Ok(Arc::new(Self {
            product: all.gldf,
            files: all.files,
        }))
    }

    /// Localized product name (first locale), falling back to the product number.
    pub fn name(&self) -> String {
        let meta = self.product.product_definitions.product_meta_data.as_ref();
        meta.and_then(|m| m.name.as_ref())
            .and_then(|n| n.locale.first())
            .map(|l| l.value.clone())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                meta.and_then(|m| m.product_number.as_ref())
                    .and_then(|n| n.locale.first())
                    .map(|l| l.value.clone())
            })
            .unwrap_or_default()
    }

    pub fn manufacturer(&self) -> String {
        self.product.header.manufacturer.clone()
    }

    /// Product variants with their geometry and emitter references.
    pub fn variants(&self) -> Vec<FfiGldfVariant> {
        self.product
            .product_definitions
            .variants
            .as_ref()
            .map(|variants| {
                variants
                    .variant
                    .iter()
                    .map(|v| {
                        let (geometry_id, emitter_ids) = v
                            .geometry
                            .as_ref()
                            .and_then(|g| g.model_geometry_reference.as_ref())
                            .map(|mgr| {
                                let ids = mgr
                                    .emitter_reference
                                    .iter()
                                    .map(|er| er.emitter_id.clone())
                                    .collect();
                                (Some(mgr.geometry_id.clone()), ids)
                            })
                            .unwrap_or((None, Vec::new()));
                        FfiGldfVariant {
                            id: v.id.clone(),
                            name: v
                                .name
                                .as_ref()
                                .and_then(|n| n.locale.first())
                                .map(|l| l.value.clone())
                                .unwrap_or_default(),
                            geometry_id,
                            emitter_ids,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// IDs of all emitters defined in the container (fallback when a variant
    /// carries no explicit emitter reference).
    pub fn emitter_ids(&self) -> Vec<String> {
        self.product
            .general_definitions
            .emitters
            .as_ref()
            .map(|es| es.emitter.iter().map(|e| e.id.clone()).collect())
            .unwrap_or_default()
    }

    /// Raw LDT (Eulumdat) photometry text for an emitter, when present.
    pub fn photometry_ldt(&self, emitter_id: String) -> Option<String> {
        let file_id = self.photometry_file_id(&emitter_id)?;
        let file_name = self.file_name_for_id(&file_id);
        let buf = self.files.iter().find(|f| {
            f.file_id.as_deref() == Some(file_id.as_str())
                || (file_name.is_some()
                    && f.name
                        .as_ref()
                        .zip(file_name.as_ref())
                        .map(|(n, fname)| n.contains(fname.as_str()))
                        .unwrap_or(false))
        })?;
        let content = buf.content.as_ref()?;
        Some(String::from_utf8_lossy(content).into_owned())
    }

    /// Photometric key figures for an emitter.
    pub fn photometry_summary(&self, emitter_id: String) -> Option<FfiPhotometrySummary> {
        let (source_type, rated_flux) = self.emitter_basics(&emitter_id)?;
        let ldt_flux = self
            .photometry_ldt(emitter_id)
            .and_then(|ldt| Eulumdat::parse(&ldt).ok())
            .map(|e| e.luminous_flux());
        let flux_lm = rated_flux
            .map(f64::from)
            .filter(|f| *f > 0.0)
            .or(ldt_flux)
            .unwrap_or(0.0);
        Some(FfiPhotometrySummary {
            flux_lm,
            cct_k: None,
            source_type,
        })
    }

    /// Triangulated L3D geometry of the luminaire (first model geometry),
    /// in metres, with computed smooth vertex normals.
    pub fn mesh(&self) -> Option<MeshData> {
        let mesh = GldfToIfc::extract_mesh_from_files(&self.product, &self.files)?;
        Some(convert_gldf_mesh(&mesh))
    }

    /// Raw bytes of the first product image in the container, for library UI.
    pub fn preview_image(&self) -> Option<Vec<u8>> {
        let image_file = self
            .product
            .general_definitions
            .files
            .file
            .iter()
            .find(|f| f.content_type.starts_with("image/"))?;
        let buf = self.files.iter().find(|f| {
            f.file_id.as_deref() == Some(image_file.id.as_str())
                || f.name
                    .as_ref()
                    .map(|n| n.contains(&image_file.file_name))
                    .unwrap_or(false)
        })?;
        buf.content.clone()
    }
}

impl GldfLuminaire {
    fn photometry_file_id(&self, emitter_id: &str) -> Option<String> {
        let emitters = self.product.general_definitions.emitters.as_ref()?;
        let emitter = emitters.emitter.iter().find(|e| e.id == emitter_id)?;
        let photometry_id = emitter
            .fixed_light_emitter
            .first()
            .map(|fle| &fle.photometry_reference.photometry_id)
            .or_else(|| {
                emitter
                    .changeable_light_emitter
                    .first()
                    .map(|cle| &cle.photometry_reference.photometry_id)
            })?;
        self.product
            .general_definitions
            .photometries
            .as_ref()?
            .photometry
            .iter()
            .find(|p| &p.id == photometry_id)?
            .photometry_file_reference
            .as_ref()
            .map(|pfr| pfr.file_id.clone())
    }

    fn file_name_for_id(&self, file_id: &str) -> Option<String> {
        self.product
            .general_definitions
            .files
            .file
            .iter()
            .find(|f| f.id == file_id)
            .map(|f| f.file_name.clone())
    }

    fn emitter_basics(&self, emitter_id: &str) -> Option<(String, Option<i32>)> {
        let emitters = self.product.general_definitions.emitters.as_ref()?;
        let emitter = emitters.emitter.iter().find(|e| e.id == emitter_id)?;
        if let Some(fle) = emitter.fixed_light_emitter.first() {
            return Some(("fixed".to_string(), fle.rated_luminous_flux));
        }
        if !emitter.changeable_light_emitter.is_empty() {
            return Some(("changeable".to_string(), None));
        }
        if !emitter.sensor_emitter.is_empty() {
            return Some(("sensor".to_string(), None));
        }
        None
    }
}

/// gldf-rs mesh (f64 vertices, 1-based triangle indices) → FFI MeshData
/// (flattened f32 positions, smooth vertex normals, 0-based indices).
fn convert_gldf_mesh(mesh: &gldf_rs::ifc::MeshData) -> MeshData {
    let vcount = mesh.vertices.len();
    let mut positions = Vec::with_capacity(vcount * 3);
    for &(x, y, z) in &mesh.vertices {
        positions.push(x as f32);
        positions.push(y as f32);
        positions.push(z as f32);
    }

    let mut indices = Vec::with_capacity(mesh.triangles.len() * 3);
    let mut normals = vec![0.0f32; vcount * 3];
    for &(a, b, c) in &mesh.triangles {
        // gldf-rs uses 1-based indices (IFC convention).
        let (ia, ib, ic) = (a as usize - 1, b as usize - 1, c as usize - 1);
        if ia >= vcount || ib >= vcount || ic >= vcount {
            continue;
        }
        indices.push(ia as u32);
        indices.push(ib as u32);
        indices.push(ic as u32);

        let va = mesh.vertices[ia];
        let vb = mesh.vertices[ib];
        let vc = mesh.vertices[ic];
        let e1 = (vb.0 - va.0, vb.1 - va.1, vb.2 - va.2);
        let e2 = (vc.0 - va.0, vc.1 - va.1, vc.2 - va.2);
        let n = (
            (e1.1 * e2.2 - e1.2 * e2.1) as f32,
            (e1.2 * e2.0 - e1.0 * e2.2) as f32,
            (e1.0 * e2.1 - e1.1 * e2.0) as f32,
        );
        for &i in &[ia, ib, ic] {
            normals[i * 3] += n.0;
            normals[i * 3 + 1] += n.1;
            normals[i * 3 + 2] += n.2;
        }
    }
    for chunk in normals.chunks_mut(3) {
        let len = (chunk[0] * chunk[0] + chunk[1] * chunk[1] + chunk[2] * chunk[2]).sqrt();
        if len > 1e-12 {
            chunk[0] /= len;
            chunk[1] /= len;
            chunk[2] /= len;
        }
    }

    let triangle_count = (indices.len() / 3) as u32;
    MeshData {
        positions,
        normals,
        indices,
        vertex_count: vcount as u32,
        triangle_count,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Light fixture input for IFC export
// ═══════════════════════════════════════════════════════════════════════════

/// A placed light fixture for IFC export. Position in the same plan frame
/// as `FfiRoomElement` rects (x/y), z = mounting height above the floor.
#[derive(uniffi::Record)]
pub struct FfiLightFixtureInput {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    /// Rotation about the vertical axis in radians.
    pub rotation: f64,
    pub flux_lm: f64,
    pub cct_k: Option<f64>,
    /// Raw LDT photometry text (base64-encoded into the IFC by the writer).
    pub ldt: Option<String>,
    pub ldt_file_name: Option<String>,
    /// Storey name this fixture sits on (must match one of the writer's
    /// storey names). `None` = the project's default storey, which files
    /// every luminaire under one floor — wrong for multi-storey scans.
    #[uniffi(default = None)]
    pub storey: Option<String>,
}

pub(crate) fn convert_ffi_light_fixture(
    fx: FfiLightFixtureInput,
) -> bimifc_writer::LightFixtureData {
    use base64::{engine::general_purpose::STANDARD, Engine};
    bimifc_writer::LightFixtureData {
        name: fx.name,
        x: fx.x,
        y: fx.y,
        z: fx.z,
        rotation: fx.rotation,
        flux_lm: fx.flux_lm,
        cct_k: fx.cct_k,
        ldt_base64: fx.ldt.map(|l| STANDARD.encode(l.as_bytes())),
        ldt_file_name: fx.ldt_file_name,
        storey: fx.storey.unwrap_or_default(),
        // Control and horticultural data reach the writer through the
        // placement's own psets today; see FfiLightFixtureInput.
        control: None,
        horticulture: None,
        psets: Vec::new(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LDT → IES conversion (for SceneKit SCNLight.iesProfileURL)
// ═══════════════════════════════════════════════════════════════════════════

/// µmol·m⁻²·s⁻¹ per lux for a set of luminaires.
///
/// Illuminance weights radiation by the eye's response, which barely
/// registers deep red; photon flux counts photons regardless of colour.
/// The ratio is therefore a property of the spectrum and cannot be a
/// constant — taking it as PPF ÷ luminous flux is exact per product,
/// since both figures come from the same measurement.
///
/// Every luminaire must state a photon flux: mixing grow lights with
/// plain ones gives a factor right for neither, and a confidently wrong
/// PPFD reading is worse than none. Returns `None` in that case.
///
/// One implementation, called by every consumer — the apps used to each
/// carry their own copy of this rule.
#[uniffi::export]
pub fn umol_per_lux(luminaires: Vec<FfiHorticulturalLuminaire>) -> Option<f64> {
    if luminaires.is_empty() {
        return None;
    }
    let mut ppf = 0.0;
    let mut lumens = 0.0;
    for l in &luminaires {
        if l.ppf_umol_s <= 0.0 || l.flux_lm <= 0.0 {
            return None;
        }
        ppf += l.ppf_umol_s;
        lumens += l.flux_lm;
    }
    (lumens > 0.0).then(|| ppf / lumens)
}

/// One luminaire's figures for [`umol_per_lux`].
#[derive(uniffi::Record)]
pub struct FfiHorticulturalLuminaire {
    pub flux_lm: f64,
    /// Photosynthetic photon flux, µmol/s over 400–700 nm.
    pub ppf_umol_s: f64,
}

/// Daily light integral, mol·m⁻²·d⁻¹ — the figure growers plan against.
#[uniffi::export]
pub fn daily_light_integral(ppfd_umol_m2_s: f64, photoperiod_hours: f64) -> f64 {
    ppfd_umol_m2_s * photoperiod_hours * 3600.0 / 1e6
}

/// Convert LDT (Eulumdat) photometry text to IES format.
#[uniffi::export]
pub fn ldt_to_ies(ldt: String) -> Result<String, LightingError> {
    let parsed = Eulumdat::parse(&ldt).map_err(|e| LightingError::ParseError {
        message: e.to_string(),
    })?;
    Ok(IesExporter::export(&parsed))
}

// ═══════════════════════════════════════════════════════════════════════════
// Bare photometry import (LDT / IES / ATLA XML)
// ═══════════════════════════════════════════════════════════════════════════

/// A photometry file read without a GLDF container: everything a
/// placement needs (the LDT text is what the lux solver consumes).
#[derive(uniffi::Record)]
pub struct FfiImportedPhotometry {
    /// Luminaire name from the file, or empty when it carries none.
    pub name: String,
    /// Manufacturer / identification field, empty when absent.
    pub manufacturer: String,
    pub flux_lm: f64,
    pub cct_k: Option<f64>,
    /// Normalised EULUMDAT text — the format everything downstream reads.
    pub ldt: String,
    /// Which format the bytes were in: "ldt", "ies" or "atla".
    pub source_format: String,
}

/// Parse a bare photometry file into EULUMDAT. Accepts EULUMDAT (.ldt),
/// IESNA LM-63 (.ies) and ATLA / TM-33 XML (.xml), detected from the
/// content so a wrong extension still works.
#[uniffi::export]
pub fn import_photometry(
    bytes: Vec<u8>,
    file_name: String,
) -> Result<FfiImportedPhotometry, LightingError> {
    let text = String::from_utf8_lossy(&bytes);
    let trimmed = text.trim_start();
    let lower = file_name.to_ascii_lowercase();

    let err = |what: &str, e: String| LightingError::ParseError {
        message: format!("{what}: {e}"),
    };

    // Content sniffing first: extensions lie more often than bytes do.
    let (ldt, format) = if trimmed.starts_with("<?xml") || trimmed.starts_with('<') {
        let doc = eulumdat::atla::parse(&text).map_err(|e| err("ATLA XML", e.to_string()))?;
        // The ATLA parser accepts any well-formed XML, so a document
        // without emitters is "valid" but carries no photometry.
        if doc.emitters.is_empty() {
            return Err(err("ATLA XML", "no emitters in document".into()));
        }
        (doc.to_eulumdat(), "atla")
    } else if is_ies(trimmed) || lower.ends_with(".ies") {
        let parsed = IesParser::parse(&text).map_err(|e| err("IES", e.to_string()))?;
        (parsed, "ies")
    } else {
        let parsed = Eulumdat::parse(&text).map_err(|e| err("EULUMDAT", e.to_string()))?;
        (parsed, "ldt")
    };

    let fallback = file_name
        .rsplit('/')
        .next()
        .unwrap_or(&file_name)
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(&file_name)
        .to_string();
    let name = if ldt.luminaire_name.trim().is_empty() {
        fallback
    } else {
        ldt.luminaire_name.trim().to_string()
    };
    // A placement needs light: reject anything that would render black.
    let flux_lm = ldt.luminous_flux();
    if !flux_lm.is_finite() || flux_lm <= 0.0 {
        return Err(err(
            "Photometry",
            format!("no luminous flux ({flux_lm} lm)"),
        ));
    }
    if ldt.intensities.is_empty() {
        return Err(err("Photometry", "no intensity distribution".into()));
    }

    let cct_k = ldt
        .lamp_sets
        .first()
        .and_then(|l| parse_cct(&l.color_appearance));

    Ok(FfiImportedPhotometry {
        name,
        manufacturer: ldt.identification.trim().to_string(),
        flux_lm,
        cct_k,
        ldt: ldt.to_ldt(),
        source_format: format.to_string(),
    })
}

/// IESNA LM-63 in any of its vintages: the 1991 form is bare `IESNA91`,
/// later ones `IESNA:LM-63-…` or `IES:LM-63-…`, and some writers emit
/// only the keyword block, which always precedes a `TILT=` line.
fn is_ies(trimmed: &str) -> bool {
    trimmed.starts_with("IESNA")
        || trimmed.starts_with("IES:LM-63")
        || trimmed
            .lines()
            .take(40)
            .any(|l| l.trim_start().starts_with("TILT="))
}

/// "4000K", "840", "3000 K" → kelvin when the colour appearance text
/// carries one (the LDT field is free-form).
fn parse_cct(appearance: &str) -> Option<f64> {
    let digits: String = appearance.chars().filter(|c| c.is_ascii_digit()).collect();
    match digits.len() {
        4 => digits
            .parse::<f64>()
            .ok()
            .filter(|k| (1500.0..=10000.0).contains(k)),
        3 => digits[1..]
            .parse::<f64>()
            .ok()
            .map(|hundreds| hundreds * 100.0),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Lux calculator (patch radiosity, box room)
// ═══════════════════════════════════════════════════════════════════════════

/// One placed luminaire for the lux solve. Position in calculator space
/// (z-up, metres, origin at a room corner — see module docs). `ldt` is the
/// raw Eulumdat text; luminaires sharing identical text share one parse.
#[derive(uniffi::Record)]
pub struct FfiLuxLuminaire {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub flux_lm: f64,
    pub ldt: String,
}

/// Workplane statistics over the evaluated grid.
#[derive(uniffi::Record)]
pub struct FfiLuxStats {
    pub e_avg: f64,
    pub e_min: f64,
    pub e_max: f64,
    /// EN-style overall uniformity U₀ = E_min / E_avg.
    pub u0: f64,
    pub iterations: u32,
}

struct LuxState {
    mesh: RoomMesh,
    ff: FormFactors,
    /// Luminaires grouped by distinct LDT (direct term is linear, so groups
    /// with different photometry are summed).
    groups: Vec<(Eulumdat, Vec<Luminaire>)>,
    result: Option<RadiosityResult>,
}

/// Room-illuminance calculator: box room, cached form factors, radiosity
/// interreflection. Construct once per room, `solve` per luminaire change,
/// `grid`/`stats` per readout.
#[derive(uniffi::Object)]
pub struct LuxCalculator {
    state: Mutex<LuxState>,
}

#[uniffi::export]
impl LuxCalculator {
    /// Build the room mesh and cache its form factors (the expensive part —
    /// geometry-only, reusable across solves).
    #[uniffi::constructor]
    pub fn new(
        length: f64,
        width: f64,
        height: f64,
        refl_ceiling: f64,
        refl_wall: f64,
        refl_floor: f64,
        divisions: u32,
    ) -> Arc<Self> {
        let mesh = RoomMesh::new(
            length.max(0.1),
            width.max(0.1),
            height.max(0.1),
            SurfaceReflectances {
                ceiling: refl_ceiling.clamp(0.0, 0.95),
                wall: refl_wall.clamp(0.0, 0.95),
                floor: refl_floor.clamp(0.0, 0.95),
            },
            divisions.clamp(4, 24) as usize,
        );
        let ff = compute_form_factors(&mesh);
        Arc::new(Self {
            state: Mutex::new(LuxState {
                mesh,
                ff,
                groups: Vec::new(),
                result: None,
            }),
        })
    }

    /// Solve direct + interreflected illuminance for the given luminaires.
    pub fn solve(&self, luminaires: Vec<FfiLuxLuminaire>) -> Result<(), LightingError> {
        let mut state = self.state.lock().unwrap();

        // Group by identical LDT text so each distinct photometry parses once.
        let mut groups: Vec<(String, Eulumdat, Vec<Luminaire>)> = Vec::new();
        for lum in luminaires {
            let l = Luminaire {
                pos: Vec3::new(lum.x, lum.y, lum.z),
                flux: lum.flux_lm,
            };
            if let Some(g) = groups.iter_mut().find(|(ldt, _, _)| *ldt == lum.ldt) {
                g.2.push(l);
            } else {
                let parsed = Eulumdat::parse(&lum.ldt).map_err(|e| LightingError::ParseError {
                    message: e.to_string(),
                })?;
                groups.push((lum.ldt, parsed, vec![l]));
            }
        }

        let n = state.mesh.patches.len();
        let mut direct = vec![0.0f64; n];
        for (_, ldt, lums) in &groups {
            for (d, v) in direct
                .iter_mut()
                .zip(direct_illuminance(&state.mesh, lums, ldt))
            {
                *d += v;
            }
        }

        let result = solve_radiosity(&state.mesh, &state.ff, &direct, 200, 1e-5);
        state.groups = groups.into_iter().map(|(_, e, l)| (e, l)).collect();
        state.result = Some(result);
        Ok(())
    }

    /// Illuminance (lux) on a horizontal, upward-facing grid at height `z`.
    /// Returns `nx*ny` values in row-major order (y rows, x columns), cell
    /// centres spanning the room. Requires a prior `solve`.
    pub fn grid(&self, z: f64, nx: u32, ny: u32) -> Result<Vec<f64>, LightingError> {
        let state = self.state.lock().unwrap();
        let result = state.result.as_ref().ok_or(LightingError::CalcError {
            message: "call solve() before grid()".to_string(),
        })?;

        let (nx, ny) = (nx.clamp(1, 512) as usize, ny.clamp(1, 512) as usize);
        let (l, w) = (state.mesh.length, state.mesh.width);
        let zero = zero_result(state.mesh.patches.len());
        let no_lums: [Luminaire; 0] = [];

        let mut out = Vec::with_capacity(nx * ny);
        for j in 0..ny {
            let y = (j as f64 + 0.5) / ny as f64 * w;
            for i in 0..nx {
                let x = (i as f64 + 0.5) / nx as f64 * l;
                let p = Vec3::new(x, y, z);
                let mut e = 0.0;
                // Direct per photometry group (zero exitance → no indirect).
                for (ldt, lums) in &state.groups {
                    e += illuminance_at_point(p, &state.mesh, &zero, lums, ldt);
                }
                // Indirect once (no luminaires → pure patch contribution).
                if let Some((ldt, _)) = state.groups.first() {
                    e += illuminance_at_point(p, &state.mesh, result, &no_lums, ldt);
                }
                out.push(e);
            }
        }
        Ok(out)
    }

    /// Workplane statistics at height `z`, over a 24×24 evaluation grid.
    pub fn stats(&self, z: f64) -> Result<FfiLuxStats, LightingError> {
        let iterations = {
            let state = self.state.lock().unwrap();
            state.result.as_ref().map(|r| r.iterations).unwrap_or(0) as u32
        };
        let values = self.grid(z, 24, 24)?;
        let count = values.len().max(1) as f64;
        let e_avg = values.iter().sum::<f64>() / count;
        let e_min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let e_max = values.iter().cloned().fold(0.0f64, f64::max);
        Ok(FfiLuxStats {
            e_avg,
            e_min: if e_min.is_finite() { e_min } else { 0.0 },
            e_max,
            u0: if e_avg > 1e-9 { e_min / e_avg } else { 0.0 },
            iterations,
        })
    }
}

fn zero_result(n: usize) -> RadiosityResult {
    RadiosityResult {
        incident: vec![0.0; n],
        direct: vec![0.0; n],
        exitance: vec![0.0; n],
        iterations: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ldt() -> String {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../eulumdat-rs/crates/eulumdat/tests/fixtures/alya_359lm.ldt"
        );
        std::fs::read_to_string(path).expect("eulumdat-rs fixture LDT")
    }

    fn sample_gldf() -> Vec<u8> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../gldf-rs/tests/data/Freestand_Belviso-2l3d.gldf"
        );
        std::fs::read(path).expect("gldf-rs sample container")
    }

    #[test]
    fn gldf_luminaire_parses_and_meshes() {
        let lum = GldfLuminaire::from_bytes(sample_gldf()).expect("parse gldf");
        assert!(!lum.manufacturer().is_empty(), "manufacturer");
        let variants = lum.variants();
        assert!(!variants.is_empty(), "variants");

        // Photometry must be reachable via variant emitters or the global list.
        let emitter_ids: Vec<String> = if variants[0].emitter_ids.is_empty() {
            lum.emitter_ids()
        } else {
            variants[0].emitter_ids.clone()
        };
        assert!(!emitter_ids.is_empty(), "emitter ids");
        let ldt = lum.photometry_ldt(emitter_ids[0].clone());
        assert!(ldt.is_some(), "ldt content");
        let summary = lum.photometry_summary(emitter_ids[0].clone()).unwrap();
        assert!(summary.flux_lm > 0.0, "flux from rated or LDT");

        // L3D geometry → triangle mesh with normals.
        let mesh = lum.mesh().expect("l3d mesh");
        assert!(mesh.triangle_count > 0);
        assert_eq!(mesh.positions.len(), mesh.normals.len());
        assert_eq!(mesh.indices.len() as u32, mesh.triangle_count * 3);
    }

    #[test]
    fn imports_ldt_ies_and_atla() {
        let ldt_text = sample_ldt();
        let reference = Eulumdat::parse(&ldt_text).expect("fixture parses");
        let flux = reference.luminous_flux();

        // EULUMDAT straight through.
        let from_ldt = import_photometry(ldt_text.clone().into_bytes(), "panel.ldt".into())
            .expect("ldt import");
        assert_eq!(from_ldt.source_format, "ldt");
        assert!((from_ldt.flux_lm - flux).abs() < 1e-6);
        assert!(!from_ldt.name.is_empty());
        assert!(
            Eulumdat::parse(&from_ldt.ldt).is_ok(),
            "output is valid LDT"
        );

        // IES converted to EULUMDAT.
        let ies = IesExporter::export(&reference);
        let from_ies =
            import_photometry(ies.clone().into_bytes(), "panel.ies".into()).expect("ies import");
        assert_eq!(from_ies.source_format, "ies");
        assert!(
            (from_ies.flux_lm - flux).abs() / flux < 0.05,
            "flux survives IES"
        );
        assert!(Eulumdat::parse(&from_ies.ldt).is_ok());

        // Content sniffing beats a wrong extension.
        let mislabelled =
            import_photometry(ies.into_bytes(), "panel.ldt".into()).expect("sniffed as IES");
        assert_eq!(mislabelled.source_format, "ies");

        // Name falls back to the file stem when the file carries none.
        let mut anonymous = reference.clone();
        anonymous.luminaire_name = String::new();
        let from_anon =
            import_photometry(anonymous.to_ldt().into_bytes(), "dir/Downlight.ldt".into())
                .expect("anonymous import");
        assert_eq!(from_anon.name, "Downlight");

        // Garbage is an error, never a panic.
        assert!(import_photometry(b"not photometry".to_vec(), "x.ldt".into()).is_err());
        assert!(import_photometry(Vec::new(), "x.ies".into()).is_err());
        assert!(
            import_photometry(b"<?xml version=\"1.0\"?><nope/>".to_vec(), "x.xml".into()).is_err()
        );
    }

    #[test]
    fn cct_parsed_from_colour_appearance() {
        assert_eq!(parse_cct("4000K"), Some(4000.0));
        assert_eq!(parse_cct("840"), Some(4000.0));
        assert_eq!(parse_cct("ww"), None);
    }

    #[test]
    fn ldt_to_ies_roundtrips() {
        let ies = ldt_to_ies(sample_ldt()).expect("convert");
        assert!(ies.contains("TILT="), "IES header present");
    }

    #[test]
    fn lux_calculator_sane_average() {
        // 4×3×2.8 m room, one 359 lm downlight at ceiling centre.
        let ldt = sample_ldt();
        let flux = Eulumdat::parse(&ldt).unwrap().luminous_flux();
        assert!(flux > 100.0);

        let calc = LuxCalculator::new(4.0, 3.0, 2.8, 0.7, 0.5, 0.2, 10);
        calc.solve(vec![FfiLuxLuminaire {
            x: 2.0,
            y: 1.5,
            z: 2.8,
            flux_lm: flux,
            ldt,
        }])
        .expect("solve");

        let stats = calc.stats(0.85).expect("stats");
        // Loose physical bounds: E_avg must be positive and below the
        // zero-loss ceiling flux/area = 359/12 ≈ 30 lx (plus interreflection
        // margin). Uniformity in (0, 1].
        assert!(stats.e_avg > 0.5, "e_avg={} too low", stats.e_avg);
        assert!(stats.e_avg < 60.0, "e_avg={} too high", stats.e_avg);
        assert!(stats.u0 > 0.0 && stats.u0 <= 1.0);
        assert!(stats.iterations > 0);

        let grid = calc.grid(0.85, 8, 6).expect("grid");
        assert_eq!(grid.len(), 48);
        // Centre must outshine the far corner for a single central downlight.
        let centre = grid[3 * 8 + 4];
        assert!(centre > grid[0], "centre {} vs corner {}", centre, grid[0]);
    }
}
