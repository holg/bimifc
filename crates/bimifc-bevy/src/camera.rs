//! Camera system with orbit, pan, and zoom controls
//!
//! Provides a flexible camera controller similar to the TypeScript version.

#[cfg(target_arch = "wasm32")]
use crate::storage::save_camera;
use crate::storage::CameraStorage;
use bevy::ecs::message::MessageReader;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::input::touch::Touches;
use bevy::prelude::*;

/// System set for camera input (for ordering)
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CameraInputSet;

/// Camera controller plugin
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraController>()
            .init_resource::<crate::ViewerSettings>()
            // Crisper cascades on native; WebGL2 keeps Bevy's default.
            .insert_resource(bevy::light::DirectionalLightShadowMap {
                size: if cfg!(target_arch = "wasm32") {
                    2048
                } else {
                    4096
                },
            })
            .add_systems(Startup, setup_camera)
            .add_systems(
                Update,
                (apply_render_quality_system, fit_shadow_cascades_system),
            )
            .add_systems(
                Update,
                (
                    poll_camera_commands_system,
                    camera_input_system,
                    camera_touch_system,
                    camera_update_system,
                    camera_keyboard_system,
                )
                    .chain()
                    .in_set(CameraInputSet),
            );
    }
}

impl CameraPlugin {
    /// Get the system set for camera input (for ordering picking after camera)
    pub fn input_system_set() -> CameraInputSet {
        CameraInputSet
    }
}

/// Camera operating mode
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CameraMode {
    #[default]
    Orbit,
    Pan,
    Walk,
}

/// Camera controller resource
#[derive(Resource)]
pub struct CameraController {
    /// Gesture for the drag in progress, when a button implies one
    /// (right = pan, middle = orbit). `None` follows `mode`.
    pub drag_gesture: Option<CameraMode>,
    /// Current mode
    pub mode: CameraMode,
    /// Target point to orbit around
    pub target: Vec3,
    /// Distance from target
    pub distance: f32,
    /// Azimuth angle (horizontal rotation)
    pub azimuth: f32,
    /// Elevation angle (vertical rotation)
    pub elevation: f32,
    /// Damping factor for smooth movement (0.0 = instant, 1.0 = never moves)
    pub damping: f32,
    /// Velocity for inertia
    pub velocity: Vec3,
    /// Angular velocity for orbit inertia
    pub angular_velocity: Vec2,
    /// Whether camera is currently animating
    pub is_animating: bool,
    /// Animation target (for preset views)
    pub animation_target: Option<CameraAnimationTarget>,
    /// Field of view in degrees
    pub fov: f32,
    /// Near clipping plane
    pub near: f32,
    /// Far clipping plane
    pub far: f32,
    /// Walk mode speed
    pub walk_speed: f32,
    /// Orbit sensitivity
    pub orbit_sensitivity: f32,
    /// Pan sensitivity
    pub pan_sensitivity: f32,
    /// Zoom sensitivity
    pub zoom_sensitivity: f32,
    /// Is dragging (mouse down)
    pub is_dragging: bool,
    /// Last mouse position
    pub last_mouse_pos: Vec2,
    /// Mouse position when drag started (for click detection)
    pub drag_start_pos: Vec2,
    /// Did actual dragging occur (mouse moved significantly)?
    pub did_drag: bool,
    /// Was this a click (released without dragging)?
    pub just_clicked: bool,
    // Touch state
    /// Number of active touches last frame
    pub touch_count: usize,
    /// Last single-touch position (for computing deltas)
    pub last_touch_pos: Vec2,
    /// Last distance between two fingers (for pinch zoom)
    pub last_pinch_distance: f32,
    /// Last center of two fingers (for pan)
    pub last_two_touch_center: Vec2,
    /// Is a single-finger drag active?
    pub is_touch_dragging: bool,
    /// Position where touch started (for tap detection)
    pub touch_drag_start: Vec2,
    /// Did touch move significantly (not a tap)?
    pub touch_did_drag: bool,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            mode: CameraMode::Orbit,
            target: Vec3::ZERO,
            distance: 100.0,  // Start further back for IFC models (in mm)
            azimuth: 0.785,   // 45 degrees
            elevation: 0.615, // ~35 degrees (isometric)
            damping: 0.92,
            velocity: Vec3::ZERO,
            angular_velocity: Vec2::ZERO,
            is_animating: false,
            animation_target: None,
            fov: 45.0,
            near: 0.05,        // 5cm near plane — good depth precision for buildings
            far: 10000.0,      // 10km far plane for large IFC models
            walk_speed: 500.0, // 0.5m per frame for walking in mm-scale
            orbit_sensitivity: 0.005,
            drag_gesture: None,
            pan_sensitivity: 0.01,
            zoom_sensitivity: 0.02,
            is_dragging: false,
            last_mouse_pos: Vec2::ZERO,
            drag_start_pos: Vec2::ZERO,
            did_drag: false,
            just_clicked: false,
            touch_count: 0,
            last_touch_pos: Vec2::ZERO,
            last_pinch_distance: 0.0,
            last_two_touch_center: Vec2::ZERO,
            is_touch_dragging: false,
            touch_drag_start: Vec2::ZERO,
            touch_did_drag: false,
        }
    }
}

impl CameraController {
    /// Get camera position from spherical coordinates
    pub fn get_position(&self) -> Vec3 {
        let x = self.distance * self.elevation.cos() * self.azimuth.sin();
        let y = self.distance * self.elevation.sin();
        let z = self.distance * self.elevation.cos() * self.azimuth.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Set preset view
    pub fn set_preset_view(&mut self, azimuth: f32, elevation: f32) {
        self.animation_target = Some(CameraAnimationTarget {
            azimuth,
            elevation,
            distance: self.distance,
            target: self.target,
            duration: 0.5,
            elapsed: 0.0,
        });
        self.is_animating = true;
    }

    /// Set home/isometric view
    pub fn home(&mut self) {
        self.set_preset_view(0.785, 0.615); // 45°, 35.264°
    }

    /// Fit all - zoom to show entire scene
    pub fn fit_bounds(&mut self, min: Vec3, max: Vec3) {
        let center = (min + max) * 0.5;
        let size = max - min;
        let diagonal = size.length();

        // Calculate distance to fit the entire model
        let fov_rad = self.fov.to_radians();
        let distance = diagonal / (2.0 * (fov_rad / 2.0).tan());

        self.animation_target = Some(CameraAnimationTarget {
            azimuth: self.azimuth,
            elevation: self.elevation,
            distance: distance.max(1.0),
            target: center,
            duration: 0.5,
            elapsed: 0.0,
        });
        self.is_animating = true;
    }

    /// Frame selection - zoom to specific bounds
    pub fn frame(&mut self, min: Vec3, max: Vec3) {
        self.fit_bounds(min, max);
    }

    /// Zoom in
    pub fn zoom_in(&mut self) {
        self.distance = (self.distance * 0.8).max(1.0);
    }

    /// Zoom out
    pub fn zoom_out(&mut self) {
        self.distance = (self.distance * 1.25).min(500000.0);
    }

    /// Convert to storage format
    pub fn to_storage(&self) -> CameraStorage {
        CameraStorage {
            azimuth: self.azimuth,
            elevation: self.elevation,
            distance: self.distance,
            target: [self.target.x, self.target.y, self.target.z],
        }
    }

    /// Load from storage format
    pub fn from_storage(&mut self, storage: &CameraStorage) {
        self.azimuth = storage.azimuth;
        self.elevation = storage.elevation;
        self.distance = storage.distance;
        self.target = Vec3::new(storage.target[0], storage.target[1], storage.target[2]);
    }
}

/// Animation target for smooth camera transitions
#[derive(Clone, Debug)]
pub struct CameraAnimationTarget {
    pub azimuth: f32,
    pub elevation: f32,
    pub distance: f32,
    pub target: Vec3,
    pub duration: f32,
    pub elapsed: f32,
}

/// Marker component for the main camera
#[derive(Component)]
pub struct MainCamera;

/// Marker for architectural lighting entities (directional lights)
/// so they can be toggled by the photometric module.
#[derive(Component)]
pub struct ArchitecturalLight;

/// System to poll for camera commands from Yew UI
#[allow(unused_variables, unused_mut)]
fn poll_camera_commands_system(
    mut controller: ResMut<CameraController>,
    scene_data: Res<crate::IfcSceneData>,
) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(cmd) = crate::storage::load_camera_cmd() {
            crate::storage::clear_camera_cmd();

            match cmd.cmd.as_str() {
                "home" => {
                    controller.home();
                }
                "fit_all" => {
                    if let Some(ref bounds) = scene_data.bounds {
                        controller.fit_bounds(bounds.min, bounds.max);
                    }
                }
                "set_mode" => {
                    if let Some(mode) = cmd.mode {
                        controller.mode = match mode.as_str() {
                            "pan" => CameraMode::Pan,
                            "walk" => CameraMode::Walk,
                            _ => CameraMode::Orbit,
                        };
                    }
                }
                _ => {}
            }
        }
    }
}

/// Marker for the shadow-casting key (sun) light.
#[derive(Component)]
pub struct KeyLight;

/// Key light direction: ~55° above the horizon so it reaches down into rooms
/// (RoomPlan exports rarely have ceilings) while still throwing readable
/// shadows off walls and furniture. Its azimuth (~16° off +X) is deliberately
/// away from the default 45° home camera so the two facades facing the
/// viewer get clearly different light instead of a flat, equal shade.
const KEY_LIGHT_DIR: Vec3 = Vec3::new(0.96, 1.43, 0.28);

/// Shadow cascade layout for building-scale scenes (metres). The maximum
/// distance is later stretched by [`fit_shadow_cascades_system`] when a
/// scene is larger than this (e.g. mm-based IFC files).
fn building_cascades(max_distance: f32) -> bevy::light::CascadeShadowConfig {
    bevy::light::CascadeShadowConfigBuilder {
        num_cascades: 3,
        minimum_distance: 0.1,
        first_cascade_far_bound: 8.0 * (max_distance / 80.0).max(1.0),
        maximum_distance: max_distance,
        overlap_proportion: 0.3,
    }
    .build()
}

/// Setup the 3D camera and the architectural light rig
fn setup_camera(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    controller: Res<CameraController>,
    settings: Res<crate::ViewerSettings>,
) {
    use bevy::core_pipeline::tonemapping::Tonemapping;
    use bevy::light::EnvironmentMapLight;

    let position = controller.get_position();

    // Sky-lit ambient: a small procedural cubemap (blue-white sky, bright
    // horizon band, warm grey ground) as image-based light. Works on
    // WebGL2 too — it is a plain cube texture, no compute involved.
    let (diffuse_map, specular_map) = build_sky_environment(&mut images);

    let camera = commands
        .spawn((
            Camera3d::default(),
            Transform::from_translation(position).looking_at(controller.target, Vec3::Y),
            Projection::Perspective(PerspectiveProjection {
                fov: controller.fov.to_radians(),
                near: controller.near,
                far: controller.far,
                ..default()
            }),
            MainCamera,
            // AgX tonemapping for natural, filmic look
            Tonemapping::AgX,
            // Single cluster: all lights evaluated per fragment — best for stadium
            // floodlights where most lights illuminate the same area (the pitch).
            bevy::light::cluster::ClusterConfig::Single,
            EnvironmentMapLight {
                diffuse_map,
                specular_map,
                intensity: SKY_ENV_INTENSITY,
                rotation: Quat::IDENTITY,
                affects_lightmapped_mesh_diffuse: true,
            },
        ))
        .id();

    // Flat ambient: only a floor value now — the environment map carries
    // the sky/ground ambient with proper directionality.
    commands.spawn(AmbientLight {
        color: Color::srgb(0.9, 0.92, 1.0),
        brightness: 30.0,
        affects_lightmapped_meshes: true,
    });

    // Key directional light - warm sun from above, the only shadow caster.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.97, 0.92),
            // Bevy's default exposure is EV100 9.7 (≈ 1/1000): a white wall
            // facing this light at ~45° then sits near 0.9 before tonemapping
            // — bright, but with headroom so AgX keeps the palette colours
            // saturated instead of squashing everything towards white.
            illuminance: 2800.0,
            shadows_enabled: false, // set by apply_render_quality_system
            // Slightly more depth bias than Bevy's default so thin RoomPlan
            // walls (10 cm) don't self-shadow at grazing angles.
            shadow_depth_bias: 0.03,
            shadow_normal_bias: 1.2,
            affects_lightmapped_mesh_diffuse: true,
        },
        building_cascades(80.0),
        Transform::from_translation(KEY_LIGHT_DIR).looking_at(Vec3::ZERO, Vec3::Y),
        ArchitecturalLight,
        KeyLight,
    ));

    // Fill light from opposite side - cool, soft, no shadows. Deliberately
    // weak: the sky environment map carries the ambient now, and only that
    // term is darkened by shadows and SSAO — direct fills would wash both out.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.8, 0.88, 1.0),
            illuminance: 600.0,
            shadows_enabled: false,
            affects_lightmapped_mesh_diffuse: true,
            ..default()
        },
        Transform::from_xyz(-0.6, 0.5, -0.5).looking_at(Vec3::ZERO, Vec3::Y),
        ArchitecturalLight,
    ));

    // Rim/back light for edge definition
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.95, 0.95, 1.0),
            illuminance: 400.0,
            shadows_enabled: false,
            affects_lightmapped_mesh_diffuse: true,
            ..default()
        },
        Transform::from_xyz(-0.3, 0.8, -0.8).looking_at(Vec3::ZERO, Vec3::Y),
        ArchitecturalLight,
    ));

    // Bottom fill - subtle uplight so undersides are not pitch black
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.7, 0.75, 0.85),
            illuminance: 150.0,
            shadows_enabled: false,
            affects_lightmapped_mesh_diffuse: true,
            ..default()
        },
        Transform::from_xyz(0.0, -1.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
        ArchitecturalLight,
    ));

    apply_render_quality(&mut commands, camera, settings.render_quality);
}

/// Intensity (cd/m²) of the procedural sky environment map. Roughly half of
/// a lit wall's light comes from here: shadowed faces stay sky-lit rather
/// than black, and SSAO (which only darkens indirect light) has a real
/// ambient term to occlude in corners.
const SKY_ENV_INTENSITY: f32 = 1200.0;

/// Apply a [`crate::RenderQuality`] preset to the camera entity.
///
/// Native `Quality`: SSAO (needs `Msaa::Off`) + FXAA for edges.
/// Native `Fast` and every wasm preset: 4x MSAA, no SSAO (not supported on
/// WebGL2).  Shadows are toggled on the key light by the caller's system.
fn apply_render_quality(commands: &mut Commands, camera: Entity, quality: crate::RenderQuality) {
    use bevy::render::view::Msaa;
    let mut cam = commands.entity(camera);

    #[cfg(not(target_arch = "wasm32"))]
    {
        use bevy::anti_alias::fxaa::{Fxaa, Sensitivity};
        use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
        use bevy::pbr::{ScreenSpaceAmbientOcclusion, ScreenSpaceAmbientOcclusionQualityLevel};
        match quality {
            crate::RenderQuality::Quality => {
                cam.insert((
                    Msaa::Off,
                    ScreenSpaceAmbientOcclusion {
                        quality_level: ScreenSpaceAmbientOcclusionQualityLevel::High,
                        // Walls/furniture are thin — a smaller thickness
                        // avoids halos around free-standing objects.
                        constant_object_thickness: 0.15,
                    },
                    Fxaa {
                        enabled: true,
                        edge_threshold: Sensitivity::High,
                        edge_threshold_min: Sensitivity::High,
                    },
                ));
            }
            crate::RenderQuality::Fast => {
                cam.remove::<(
                    ScreenSpaceAmbientOcclusion,
                    DepthPrepass,
                    NormalPrepass,
                    Fxaa,
                )>()
                .insert(Msaa::Sample4);
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = quality;
        cam.insert(Msaa::Sample4);
    }
}

/// Re-apply the render quality preset whenever `ViewerSettings.render_quality`
/// changes (also runs once after startup so the key light's shadow flag
/// matches the preset).
fn apply_render_quality_system(
    mut commands: Commands,
    settings: Res<crate::ViewerSettings>,
    mut applied: Local<Option<crate::RenderQuality>>,
    camera: Query<Entity, With<MainCamera>>,
    mut key_light: Query<&mut DirectionalLight, With<KeyLight>>,
) {
    let quality = settings.render_quality;
    if *applied == Some(quality) {
        return;
    }
    let Ok(camera) = camera.single() else { return };
    let Ok(mut light) = key_light.single_mut() else {
        return;
    };

    light.shadows_enabled = matches!(quality, crate::RenderQuality::Quality);
    crate::log_info(&format!(
        "[Bevy] Render quality {:?}: shadows={}, ssao={}, msaa={}",
        quality,
        light.shadows_enabled,
        cfg!(not(target_arch = "wasm32")) && quality == crate::RenderQuality::Quality,
        if cfg!(not(target_arch = "wasm32")) && quality == crate::RenderQuality::Quality {
            "off"
        } else {
            "4x"
        },
    ));
    if applied.is_some() {
        // Startup already applied the camera components; only redo them on a
        // genuine change so we don't churn pipelines on the first frame.
        apply_render_quality(&mut commands, camera, quality);
    }
    *applied = Some(quality);
}

/// Stretch the shadow cascades to cover the loaded scene. Building-scale
/// (metre) scenes keep the 80 m default; mm-based or campus-scale files get
/// proportionally larger cascades so they are not left unshadowed.
fn fit_shadow_cascades_system(
    scene_data: Res<crate::IfcSceneData>,
    mut fitted: Local<Option<f32>>,
    mut key_light: Query<&mut bevy::light::CascadeShadowConfig, With<KeyLight>>,
) {
    let Some(bounds) = scene_data.bounds.as_ref() else {
        return;
    };
    let diagonal = (bounds.max - bounds.min).length();
    if !diagonal.is_finite() || diagonal <= 0.0 {
        return;
    }
    let wanted = (diagonal * 2.5).clamp(80.0, 20_000.0);
    if fitted.map(|f| (f - wanted).abs() < 1e-3).unwrap_or(false) {
        return;
    }
    let Ok(mut config) = key_light.single_mut() else {
        return;
    };
    *config = building_cascades(wanted);
    *fitted = Some(wanted);
}

// ---------------------------------------------------------------------------
// Procedural sky environment map
// ---------------------------------------------------------------------------

/// Sky radiance (linear RGB, roughly [0, 1]) for a world-space direction.
/// Blue-white overhead, a brighter haze band at the horizon, warm grey
/// ground bouncing light back up.
fn sky_radiance(dir: Vec3) -> Vec3 {
    let e = dir.y.clamp(-1.0, 1.0);
    let zenith = Vec3::new(0.52, 0.68, 1.0);
    let sky_horizon = Vec3::new(0.93, 0.95, 1.0);
    let ground_horizon = Vec3::new(0.58, 0.55, 0.50);
    let ground = Vec3::new(0.36, 0.34, 0.31);
    let smooth = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    if e >= 0.0 {
        sky_horizon.lerp(zenith, smooth(e / 0.75))
    } else {
        ground_horizon.lerp(ground, smooth(-e / 0.45))
    }
}

/// Direction for a texel on cubemap `face` (wgpu order: +X −X +Y −Y +Z −Z)
/// at normalised coordinates `u, v` in [-1, 1] (v grows downwards).
fn cubemap_direction(face: usize, u: f32, v: f32) -> Vec3 {
    match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    }
    .normalize()
}

fn linear_to_srgb_u8(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let s = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (s * 255.0 + 0.5) as u8
}

/// Build a cubemap image whose mip chain is filled by `radiance(direction,
/// mip_level)`.  Data is laid out layer-major (all mips of face 0, then face
/// 1, …) which is wgpu/Bevy's default `TextureDataOrder`.
fn build_cubemap(base_size: u32, mip_levels: u32, radiance: impl Fn(Vec3, u32) -> Vec3) -> Image {
    use bevy::asset::RenderAssetUsages;
    use bevy::image::ImageSampler;
    use bevy::render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    };

    let mut data = Vec::new();
    for face in 0..6 {
        for mip in 0..mip_levels {
            let size = (base_size >> mip).max(1);
            for y in 0..size {
                for x in 0..size {
                    let u = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
                    let v = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
                    let c = radiance(cubemap_direction(face, u, v), mip);
                    data.extend_from_slice(&[
                        linear_to_srgb_u8(c.x),
                        linear_to_srgb_u8(c.y),
                        linear_to_srgb_u8(c.z),
                        255,
                    ]);
                }
            }
        }
    }

    let mut image = Image::new(
        Extent3d {
            width: base_size,
            height: base_size,
            depth_or_array_layers: 6,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        // Keep the main-world copy too: the light-probe gatherer looks the
        // cubemaps up by handle every frame, and a RENDER_WORLD-only image
        // is unloaded after its first extraction.
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.mip_level_count = mip_levels;
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::Cube),
        ..default()
    });
    image.sampler = ImageSampler::linear();
    image
}

/// Generate the diffuse (irradiance) and specular (radiance, mipmapped)
/// cubemaps for [`EnvironmentMapLight`] from [`sky_radiance`].
fn build_sky_environment(images: &mut Assets<Image>) -> (Handle<Image>, Handle<Image>) {
    // Cosine-weighted hemisphere average of the sky = irradiance / π, which
    // is what Bevy's diffuse environment map expects.
    let samples: Vec<Vec3> = {
        let n = 400;
        let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
        (0..n)
            .map(|i| {
                let y = 1.0 - (i as f32 + 0.5) / n as f32 * 2.0;
                let r = (1.0 - y * y).max(0.0).sqrt();
                let phi = golden * i as f32;
                Vec3::new(r * phi.cos(), y, r * phi.sin())
            })
            .collect()
    };
    let irradiance = move |n: Vec3, _mip: u32| {
        let mut sum = Vec3::ZERO;
        let mut weight = 0.0;
        for &s in &samples {
            let w = n.dot(s).max(0.0);
            if w > 0.0 {
                sum += sky_radiance(s) * w;
                weight += w;
            }
        }
        if weight > 0.0 {
            sum / weight
        } else {
            sky_radiance(n)
        }
    };
    let diffuse = build_cubemap(8, 1, irradiance);

    // Specular: sharp sky at mip 0, progressively blended towards the
    // hemisphere average at coarser mips (what rough surfaces sample).
    let specular = build_cubemap(32, 6, |d, mip| {
        let rough = mip as f32 / 5.0;
        let avg = Vec3::new(0.62, 0.66, 0.74);
        sky_radiance(d).lerp(avg, rough * 0.6)
    });

    (images.add(diffuse), images.add(specular))
}

/// Handle mouse input for camera control
#[allow(unused_variables)]
fn camera_input_system(
    mouse_button: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    mut controller: ResMut<CameraController>,
    windows: Query<&Window>,
    measure_state: Res<crate::picking::MeasurementState>,
    // Check if mouse is over any UI element with Interaction (only when bevy-ui feature is enabled)
    #[cfg(feature = "bevy-ui")] ui_interactions: Query<&Interaction, With<Node>>,
) {
    let Ok(window) = windows.single() else { return };

    // Check if mouse is over any UI element (hovered or pressed)
    #[cfg(feature = "bevy-ui")]
    let mouse_over_ui = ui_interactions
        .iter()
        .any(|interaction| matches!(interaction, Interaction::Hovered | Interaction::Pressed));
    #[cfg(not(feature = "bevy-ui"))]
    let mouse_over_ui = false;

    // Check if measure tool is active — don't start camera drag for left-click
    let is_measure = measure_state.active;

    // Handle mouse button state - only start drag if not over UI and not measuring
    // Right- and middle-drag are the CAD convention for pan and orbit,
    // and an embedded host has no toolbar to switch modes with. They
    // override the current mode for the duration of the drag; the left
    // button keeps following `controller.mode` as before.
    if mouse_button.just_pressed(MouseButton::Right) && !mouse_over_ui {
        controller.is_dragging = true;
        controller.drag_gesture = Some(CameraMode::Pan);
    }
    if mouse_button.just_pressed(MouseButton::Middle) && !mouse_over_ui {
        controller.is_dragging = true;
        controller.drag_gesture = Some(CameraMode::Orbit);
    }
    if mouse_button.just_released(MouseButton::Right)
        || mouse_button.just_released(MouseButton::Middle)
    {
        controller.is_dragging = false;
        controller.drag_gesture = None;
    }

    if mouse_button.just_pressed(MouseButton::Left) && !mouse_over_ui && !is_measure {
        controller.is_dragging = true;
        controller.did_drag = false;
        controller.just_clicked = false; // Reset on press
        if let Some(pos) = window.cursor_position() {
            controller.last_mouse_pos = pos;
            controller.drag_start_pos = pos;
        }
    }
    if mouse_button.just_released(MouseButton::Left) {
        // Check if this was a click (no significant drag)
        if !controller.did_drag {
            controller.just_clicked = true;
        }
        controller.is_dragging = false;
    }
    // In measure mode, treat press as click (for immediate feedback)
    if is_measure && mouse_button.just_pressed(MouseButton::Left) && !mouse_over_ui {
        controller.just_clicked = true;
        if let Some(pos) = window.cursor_position() {
            controller.drag_start_pos = pos;
        }
    }

    // Handle mouse motion
    if controller.is_dragging {
        for ev in mouse_motion.read() {
            // Mark as drag if mouse moved significantly (more than 3 pixels)
            if ev.delta.length() > 3.0 {
                controller.did_drag = true;
            }

            match controller.drag_gesture.unwrap_or(controller.mode) {
                CameraMode::Orbit => {
                    controller.azimuth -= ev.delta.x * controller.orbit_sensitivity;
                    controller.elevation -= ev.delta.y * controller.orbit_sensitivity;
                    // Clamp elevation to avoid gimbal lock
                    controller.elevation = controller.elevation.clamp(-1.5, 1.5);
                    // Store angular velocity for inertia
                    controller.angular_velocity = ev.delta * controller.orbit_sensitivity;
                }
                CameraMode::Pan => {
                    // Calculate pan in camera space
                    let right = Vec3::new(controller.azimuth.cos(), 0.0, -controller.azimuth.sin());
                    let up = Vec3::Y;
                    let pan = right
                        * ev.delta.x
                        * controller.pan_sensitivity
                        * controller.distance
                        * 0.01
                        - up * ev.delta.y * controller.pan_sensitivity * controller.distance * 0.01;
                    controller.target += pan;
                }
                CameraMode::Walk => {
                    // First-person look
                    controller.azimuth -= ev.delta.x * controller.orbit_sensitivity * 0.5;
                    controller.elevation -= ev.delta.y * controller.orbit_sensitivity * 0.5;
                    controller.elevation = controller.elevation.clamp(-1.5, 1.5);
                }
            }
        }
    } else {
        // Apply damping to angular velocity when not dragging
        let damping = controller.damping;
        controller.angular_velocity *= damping;
        if controller.angular_velocity.length() > 0.0001 {
            controller.azimuth -= controller.angular_velocity.x;
            controller.elevation -= controller.angular_velocity.y;
            controller.elevation = controller.elevation.clamp(-1.5, 1.5);
        }
    }

    // Handle mouse wheel for zoom - only when NOT over UI
    if !mouse_over_ui {
        for ev in mouse_wheel.read() {
            let zoom_delta = ev.y * controller.zoom_sensitivity;
            controller.distance = (controller.distance * (1.0 - zoom_delta)).clamp(1.0, 500000.0);
        }
    }
}

/// Handle touch input for camera control (mobile/tablet)
///
/// Gesture mapping:
/// - 1 finger drag → orbit
/// - 2 finger drag → pan
/// - 2 finger pinch → zoom
/// - Tap (no drag) → select entity
fn camera_touch_system(touches: Res<Touches>, mut controller: ResMut<CameraController>) {
    let pressed: Vec<Vec2> = touches.iter().map(|t| t.position()).collect();
    let count = pressed.len();
    let prev_count = controller.touch_count;

    // === Single touch: orbit / tap ===
    if count == 1 {
        let pos = pressed[0];

        if prev_count == 0 {
            // Touch just started
            controller.is_touch_dragging = true;
            controller.touch_did_drag = false;
            controller.last_touch_pos = pos;
            controller.touch_drag_start = pos;
        } else if controller.is_touch_dragging && prev_count == 1 {
            // Continued single-finger drag → orbit
            let delta = pos - controller.last_touch_pos;

            if delta.length() > 2.0 {
                controller.touch_did_drag = true;
            }

            if controller.touch_did_drag {
                // Apply orbit (same math as mouse)
                controller.azimuth -= delta.x * controller.orbit_sensitivity;
                controller.elevation -= delta.y * controller.orbit_sensitivity;
                controller.elevation = controller.elevation.clamp(-1.5, 1.5);
                controller.angular_velocity = delta * controller.orbit_sensitivity;
            }

            controller.last_touch_pos = pos;
        }
    }

    // === Two touches: pan + pinch zoom ===
    if count == 2 {
        let center = (pressed[0] + pressed[1]) * 0.5;
        let distance = (pressed[0] - pressed[1]).length();

        if prev_count < 2 {
            // Just transitioned to two fingers — record baseline
            controller.last_two_touch_center = center;
            controller.last_pinch_distance = distance;
            // Cancel single-finger orbit
            controller.is_touch_dragging = false;
            controller.touch_did_drag = true; // prevent tap
        } else {
            // Pan: delta of center point
            let center_delta = center - controller.last_two_touch_center;
            let right = Vec3::new(controller.azimuth.cos(), 0.0, -controller.azimuth.sin());
            let up = Vec3::Y;
            let pan =
                right * center_delta.x * controller.pan_sensitivity * controller.distance * 0.01
                    - up * center_delta.y * controller.pan_sensitivity * controller.distance * 0.01;
            controller.target += pan;

            // Pinch zoom: ratio of finger distances
            if controller.last_pinch_distance > 10.0 {
                let zoom_ratio = distance / controller.last_pinch_distance;
                controller.distance = (controller.distance / zoom_ratio).clamp(1.0, 500000.0);
            }

            controller.last_two_touch_center = center;
            controller.last_pinch_distance = distance;
        }
    }

    // === Touch released: detect tap ===
    if count == 0 && prev_count > 0 {
        if controller.is_touch_dragging && !controller.touch_did_drag {
            // This was a tap — trigger picking
            controller.just_clicked = true;
            controller.drag_start_pos = controller.touch_drag_start;
        }
        controller.is_touch_dragging = false;
    }

    controller.touch_count = count;
}

/// Handle keyboard input for camera control
fn camera_keyboard_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut controller: ResMut<CameraController>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();

    // Walk mode movement (WASD)
    if controller.mode == CameraMode::Walk {
        let forward = Vec3::new(
            -controller.azimuth.sin() * controller.elevation.cos(),
            controller.elevation.sin(),
            -controller.azimuth.cos() * controller.elevation.cos(),
        )
        .normalize();
        let right = Vec3::new(controller.azimuth.cos(), 0.0, -controller.azimuth.sin());

        let mut movement = Vec3::ZERO;

        if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
            movement += forward;
        }
        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
            movement -= forward;
        }
        if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
            movement -= right;
        }
        if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
            movement += right;
        }
        if keyboard.pressed(KeyCode::KeyQ) {
            movement -= Vec3::Y;
        }
        if keyboard.pressed(KeyCode::KeyE) {
            movement += Vec3::Y;
        }

        if movement.length() > 0.0 {
            let walk_speed = controller.walk_speed;
            controller.target += movement.normalize() * walk_speed * dt;
        }
    }

    // Preset views (number keys)
    if keyboard.just_pressed(KeyCode::Digit1) {
        controller.set_preset_view(0.0, 0.0); // Front
    }
    if keyboard.just_pressed(KeyCode::Digit2) {
        controller.set_preset_view(std::f32::consts::PI, 0.0); // Back
    }
    if keyboard.just_pressed(KeyCode::Digit3) {
        controller.set_preset_view(-std::f32::consts::FRAC_PI_2, 0.0); // Left
    }
    if keyboard.just_pressed(KeyCode::Digit4) {
        controller.set_preset_view(std::f32::consts::FRAC_PI_2, 0.0); // Right
    }
    if keyboard.just_pressed(KeyCode::Digit5) {
        controller.set_preset_view(0.0, std::f32::consts::FRAC_PI_2 - 0.001); // Top
    }
    if keyboard.just_pressed(KeyCode::Digit6) {
        controller.set_preset_view(0.0, -std::f32::consts::FRAC_PI_2 + 0.001); // Bottom
    }
    if keyboard.just_pressed(KeyCode::KeyH) {
        controller.home(); // Isometric
    }
}

/// Update camera transform
fn camera_update_system(
    mut controller: ResMut<CameraController>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();

    // Handle animation
    if controller.animation_target.is_some() {
        // Extract animation target data to avoid borrow conflicts
        let animation_data = {
            let target = controller.animation_target.as_mut().unwrap();
            target.elapsed += dt;
            let t = (target.elapsed / target.duration).min(1.0);
            // Ease out cubic
            let t = 1.0 - (1.0 - t).powi(3);
            let completed = target.elapsed >= target.duration;
            (
                target.azimuth,
                target.elevation,
                target.distance,
                target.target,
                t,
                completed,
            )
        };

        let (target_azimuth, target_elevation, target_distance, target_pos, t, completed) =
            animation_data;

        controller.azimuth = lerp(controller.azimuth, target_azimuth, t);
        controller.elevation = lerp(controller.elevation, target_elevation, t);
        controller.distance = lerp(controller.distance, target_distance, t);
        controller.target = controller.target.lerp(target_pos, t);

        if completed {
            controller.animation_target = None;
            controller.is_animating = false;
        }
    }

    // Update camera transform
    if let Ok(mut transform) = camera.single_mut() {
        let position = controller.get_position();

        // Apply damping for smooth movement
        transform.translation = transform
            .translation
            .lerp(position, 1.0 - controller.damping.powi(2));
        transform.look_at(controller.target, Vec3::Y);
    }

    // Save camera state periodically (WASM)
    #[cfg(target_arch = "wasm32")]
    {
        // Only save occasionally to avoid flooding localStorage
        static mut SAVE_COUNTER: u32 = 0;
        unsafe {
            SAVE_COUNTER += 1;
            if SAVE_COUNTER % 30 == 0 {
                save_camera(&controller.to_storage());
            }
        }
    }
}

/// Linear interpolation
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
