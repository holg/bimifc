//! Texture layers for IFC4 texture maps (`IfcIndexedTriangleTextureMap`).
//!
//! An IFC face set can be textured by several `IfcSurfaceTexture`s on the same
//! texture coordinates — e.g. a façade photo, the façade at night and its
//! illuminance in false colour. The viewer shows one of them at a time: a
//! *layer*.
//!
//! Textured meshes stay in the ordinary opaque batch, so picking, hiding,
//! isolation and selection keep working unchanged. For that the batch samples
//! one image per layer, the **layer atlas**: the textures of all texture sets
//! side by side (set `k` of `K` in the column `u ∈ [k/K, (k+1)/K)`) above a
//! thin white strip. Untextured vertices point into the strip
//! ([`WHITE_UV`]), so `vertex colour × texture` leaves them unchanged.
//! The parser maps each face set's texture coordinates into its column
//! ([`atlas_uv`]) before the meshes reach Bevy.
//!
//! The images arrive as encoded bytes ([`set_pending_textures`], e.g. fetched
//! next to the IFC file); one layer is decoded and composed per frame, the
//! active one first. [`set_pending_texture_layer`] switches layers by swapping
//! the batch material's `base_color_texture`.

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use std::sync::Mutex;

use crate::mesh::BatchedMesh;

/// Height of the white strip at the bottom of every layer atlas, as a fraction
/// of the atlas height.
pub const TEXTURE_STRIP: f32 = 1.0 / 256.0;
/// Texture coordinate of untextured vertices: the middle of the white strip.
pub const WHITE_UV: [f32; 2] = [0.5, 1.0 - TEXTURE_STRIP * 0.5];
/// Widest layer atlas we build (WebGPU's guaranteed maxTextureDimension2D).
const MAX_ATLAS: u32 = 8192;

/// Map an IFC texture coordinate (S right, T up, X3D convention) of texture
/// set `set` out of `set_count` into the layer atlas (Bevy: v down).
pub fn atlas_uv(s: f32, t: f32, set: usize, set_count: usize) -> [f32; 2] {
    let k = set_count.max(1) as f32;
    [
        (set as f32 + s.clamp(0.0, 1.0)) / k,
        (1.0 - t.clamp(0.0, 1.0)) * (1.0 - TEXTURE_STRIP),
    ]
}

/// Encoded images of one model: `layers[j].1[k]` is the image of layer `j`
/// for texture set `k` (empty bytes = missing, drawn white).
#[derive(Default, Clone)]
pub struct TexturePayload {
    pub set_count: usize,
    pub layers: Vec<(String, Vec<Vec<u8>>)>,
    /// Layer to show first.
    pub active: usize,
}

static PENDING_TEXTURES: Mutex<Option<TexturePayload>> = Mutex::new(None);
static PENDING_LAYER: Mutex<Option<usize>> = Mutex::new(None);

/// Hand the encoded layer images to the renderer (any thread).
pub fn set_pending_textures(payload: TexturePayload) {
    if let Ok(mut p) = PENDING_TEXTURES.lock() {
        *p = Some(payload);
    }
}

/// Show layer `index` (ignored when out of range).
pub fn set_pending_texture_layer(index: usize) {
    if let Ok(mut p) = PENDING_LAYER.lock() {
        *p = Some(index);
    }
}

/// Decoded layer atlases and which one the batch shows.
#[derive(Resource, Default)]
pub struct TextureLayers {
    pub names: Vec<String>,
    /// One atlas per layer, `None` until decoded.
    pub images: Vec<Option<Handle<Image>>>,
    pub active: usize,
    /// Encoded input still to decode (one layer per frame).
    queue: Vec<(usize, Vec<Vec<u8>>)>,
    set_count: usize,
}

pub struct TexturePlugin;

impl Plugin for TexturePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TextureLayers>().add_systems(
            Update,
            (
                poll_pending_textures,
                decode_next_layer,
                poll_pending_layer,
                apply_texture_layer,
            )
                .chain(),
        );
    }
}

fn poll_pending_textures(mut layers: ResMut<TextureLayers>) {
    let Some(payload) = PENDING_TEXTURES.lock().ok().and_then(|mut p| p.take()) else {
        return;
    };
    let n = payload.layers.len();
    let active = payload.active.min(n.saturating_sub(1));
    layers.names = payload
        .layers
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    layers.images = vec![None; n];
    layers.active = active;
    layers.set_count = payload.set_count.max(1);
    // Active layer first, the others after it.
    let mut queue: Vec<(usize, Vec<Vec<u8>>)> = payload
        .layers
        .into_iter()
        .map(|(_, b)| b)
        .enumerate()
        .collect();
    queue.sort_by_key(|(i, _)| if *i == active { 0 } else { 1 + *i });
    queue.reverse(); // popped from the back
    layers.queue = queue;
    crate::log(&format!(
        "[Textures] {} layer(s) × {} texture set(s) queued",
        n, layers.set_count
    ));
}

fn decode_next_layer(mut layers: ResMut<TextureLayers>, mut images: ResMut<Assets<Image>>) {
    let Some((index, encoded)) = layers.queue.pop() else {
        return;
    };
    let set_count = layers.set_count;
    match compose_layer_atlas(&encoded, set_count) {
        Some(image) => {
            crate::log(&format!(
                "[Textures] layer '{}' ready: {}×{}",
                layers.names.get(index).map(String::as_str).unwrap_or("?"),
                image.width(),
                image.height()
            ));
            let handle = images.add(image);
            if let Some(slot) = layers.images.get_mut(index) {
                *slot = Some(handle);
            }
        }
        None => crate::log(&format!("[Textures] layer {} could not be decoded", index)),
    }
}

/// Decode the set images of one layer and put them side by side above the
/// white strip. Cells are as large as the largest image, capped so the atlas
/// stays within [`MAX_ATLAS`].
fn compose_layer_atlas(encoded: &[Vec<u8>], set_count: usize) -> Option<Image> {
    let decoded: Vec<Option<image::RgbaImage>> = encoded
        .iter()
        .map(|bytes| {
            if bytes.is_empty() {
                return None;
            }
            image::load_from_memory(bytes).ok().map(|i| i.to_rgba8())
        })
        .collect();
    let (mut cw, mut ch) = decoded.iter().flatten().fold((0u32, 0u32), |(w, h), i| {
        (w.max(i.width()), h.max(i.height()))
    });
    if cw == 0 || ch == 0 {
        return None;
    }
    let k = set_count.max(1) as u32;
    if cw * k > MAX_ATLAS {
        let s = MAX_ATLAS as f32 / (cw * k) as f32;
        cw = ((cw as f32 * s) as u32).max(1);
        ch = ((ch as f32 * s) as u32).max(1);
    }
    let strip = ((ch as f32 * TEXTURE_STRIP / (1.0 - TEXTURE_STRIP)).ceil() as u32).max(2);
    let (aw, ah) = (cw * k, ch + strip);
    let mut data = vec![255u8; (aw * ah * 4) as usize]; // white: missing cells and the strip
    for (set, img) in decoded.iter().enumerate().take(k as usize) {
        let Some(img) = img else { continue };
        let img = if img.width() != cw || img.height() != ch {
            image::imageops::resize(img, cw, ch, image::imageops::FilterType::Triangle)
        } else {
            img.clone()
        };
        let x0 = set as u32 * cw;
        for row in 0..ch {
            let src = &img.as_raw()[(row * cw * 4) as usize..((row + 1) * cw * 4) as usize];
            let dst = ((row * aw + x0) * 4) as usize;
            data[dst..dst + src.len()].copy_from_slice(src);
        }
    }
    let mut image = Image::new(
        Extent3d {
            width: aw,
            height: ah,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    Some(image)
}

fn poll_pending_layer(mut layers: ResMut<TextureLayers>) {
    let Some(index) = PENDING_LAYER.lock().ok().and_then(|mut p| p.take()) else {
        return;
    };
    if index < layers.names.len() {
        layers.active = index;
        // Decode the requested layer next if it is still waiting.
        if let Some(pos) = layers.queue.iter().position(|(i, _)| *i == index) {
            let item = layers.queue.remove(pos);
            layers.queue.push(item);
        }
    }
}

/// Keep the opaque batches' materials on the active layer (also after the
/// batches are rebuilt). Without layers, nothing is touched.
fn apply_texture_layer(
    layers: Res<TextureLayers>,
    batches: Query<(&BatchedMesh, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if layers.names.is_empty() {
        return;
    }
    let want = layers.images.get(layers.active).cloned().flatten();
    for (batch, handle) in &batches {
        if batch.is_transparent {
            continue;
        }
        let current = materials
            .get(&handle.0)
            .map(|m| m.base_color_texture.clone());
        if let Some(current) = current {
            if current != want {
                if let Some(m) = materials.get_mut(&handle.0) {
                    m.base_color_texture = want.clone();
                }
            }
        }
    }
}
