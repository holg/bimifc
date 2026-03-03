//! Photometric lighting integration for IFC fixtures.
//!
//! Spawns one SpotLight per sector (8 sectors, bundled from ~36-54 fixtures),
//! with color temperature and intensity derived from the embedded EULUMDAT data.
//! Press L to cycle: Architectural → Photometric → Combined.
//! Lights aim at pitch center; hide roof manually via eye icon for full effect.
//! Hiding a sector via the eye icon also turns off its SpotLight.

use bevy::prelude::*;
use eulumdat::Eulumdat;
use eulumdat_bevy::photometric::PhotometricData;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Mutex;

use crate::camera::ArchitecturalLight;

/// Pending light data transferred from the UI (Leptos) to Bevy.
///
/// Each PendingLight represents a sector (IfcElementAssembly group) at the
/// centroid of its fixtures, with `fixture_count` indicating how many heads
/// contribute to the combined intensity.
pub struct PendingLight {
    pub entity_id: u64,
    pub position: [f32; 3],
    pub ldt_content: String,
    pub beam_type: String,
    /// Number of fixtures bundled into this light (1 = single fixture, 6 = panel)
    pub fixture_count: u32,
    /// IFC entity IDs of all fixtures in this group (for visibility sync)
    pub fixture_ids: Vec<u64>,
}

/// Static storage for pending lights (Leptos → Bevy transfer).
static PENDING_LIGHTS: Mutex<Option<Vec<PendingLight>>> = Mutex::new(None);

/// Set pending lights from the UI layer.
pub fn set_pending_lights(lights: Vec<PendingLight>) {
    let count = lights.len();
    let mut guard = PENDING_LIGHTS.lock().unwrap();
    *guard = Some(lights);
    crate::log_info(&format!("[Photometric] Pending lights set: {}", count));
}

/// Take pending lights (consumes them).
fn take_pending_lights() -> Option<Vec<PendingLight>> {
    let mut guard = PENDING_LIGHTS.lock().unwrap();
    guard.take()
}

/// Lighting mode for the scene.
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum LightingMode {
    /// Original 4 directional lights, no photometric
    #[default]
    Architectural,
    /// IFC fixture lights only (roof hidden)
    Photometric,
    /// Dimmed architectural + photometric (roof hidden)
    Combined,
}

impl LightingMode {
    fn next(self) -> Self {
        match self {
            Self::Architectural => Self::Photometric,
            Self::Photometric => Self::Combined,
            Self::Combined => Self::Architectural,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Architectural => "Architectural",
            Self::Photometric => "Photometric",
            Self::Combined => "Combined",
        }
    }
}

/// Parsed fixture light ready to spawn.
struct FixtureLight {
    position: Vec3,
    color: Color,
    /// Luminous flux in lumens (combined for all fixtures in panel)
    lumens: f32,
    range: f32,
    /// IFC entity IDs of fixtures in this sector (for visibility sync)
    fixture_ids: Vec<u64>,
}

/// Cache for parsed LDT data (only 3 unique strings in BayArena).
#[derive(Resource, Default)]
struct LdtCache {
    parsed: HashMap<u64, Eulumdat>,
}

impl LdtCache {
    fn get_or_parse(&mut self, ldt_content: &str) -> Option<Eulumdat> {
        let hash = {
            let mut h = DefaultHasher::new();
            ldt_content.hash(&mut h);
            h.finish()
        };
        if let Some(cached) = self.parsed.get(&hash) {
            return Some(cached.clone());
        }
        match Eulumdat::parse(ldt_content) {
            Ok(ldt) => {
                self.parsed.insert(hash, ldt.clone());
                Some(ldt)
            }
            Err(e) => {
                crate::log_info(&format!("[Photometric] Failed to parse LDT: {}", e));
                None
            }
        }
    }
}

/// Marker for SpotLight entities spawned by this module.
/// Stores fixture IDs so visibility can be synced with the eye icon.
#[derive(Component)]
struct PhotometricFixtureLight {
    fixture_ids: Vec<u64>,
}

/// Photometric state: stores parsed fixture data, manages spawning.
#[derive(Resource, Default)]
struct PhotometricState {
    /// Parsed fixture lights, ready to spawn on mode switch
    fixtures: Vec<FixtureLight>,
    /// Whether fixture lights are currently spawned in the scene
    lights_active: bool,
}

/// Plugin for photometric IFC lighting.
pub struct PhotometricLightingPlugin;

impl Plugin for PhotometricLightingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LightingMode>()
            .init_resource::<LdtCache>()
            .init_resource::<PhotometricState>()
            .add_systems(
                Update,
                (
                    poll_pending_lights,
                    toggle_lighting_mode,
                    sync_light_visibility,
                ),
            );
    }
}

/// Poll for pending lights — parse LDT but don't spawn yet (wait for mode switch).
fn poll_pending_lights(mut cache: ResMut<LdtCache>, mut state: ResMut<PhotometricState>) {
    let Some(pending) = take_pending_lights() else {
        return;
    };

    let count = pending.len();
    let mut parsed = 0;

    for light in pending {
        let Some(ldt) = cache.get_or_parse(&light.ldt_content) else {
            continue;
        };

        let total_flux = ldt.total_flux() as f32;
        let lor = ldt.light_output_ratio() as f32;
        let color_temp = ldt.color_temperature().unwrap_or(4000.0);
        let color = eulumdat_bevy::photometric::kelvin_to_color(color_temp);
        let n = light.fixture_count.max(1) as f32;

        // Combined sector lumens (e.g. 36 × 6500 lm × 0.78 LOR ≈ 182,520 lm per sector)
        let sector_lumens = total_flux * lor * n;

        crate::log(&format!(
            "[Photometric] Sector '{}': {}lm × {} fixtures × {:.0}% LOR = {:.0}lm @ ({:.1}, {:.1}, {:.1})",
            light.beam_type, total_flux, light.fixture_count, lor * 100.0, sector_lumens,
            light.position[0], light.position[1], light.position[2],
        ));

        // IFC→Bevy coordinate swap: Bevy Y = IFC Z, Bevy Z = -IFC Y
        let bevy_pos = Vec3::new(light.position[0], light.position[2], -light.position[1]);

        state.fixtures.push(FixtureLight {
            position: bevy_pos,
            color,
            lumens: sector_lumens,
            range: 120.0,
            fixture_ids: light.fixture_ids,
        });
        parsed += 1;
    }

    crate::log_info(&format!(
        "[Photometric] Parsed {}/{} sector lights ({} unique LDT patterns). Press L to cycle modes.",
        parsed,
        count,
        cache.parsed.len()
    ));

    // Log all fixture positions for debugging
    for (i, f) in state.fixtures.iter().enumerate() {
        crate::log(&format!(
            "[Photometric]   Light {}: pos=({:.1}, {:.1}, {:.1}) lumens={:.0} range={:.0} ids={}",
            i,
            f.position.x,
            f.position.y,
            f.position.z,
            f.lumens,
            f.range,
            f.fixture_ids.len(),
        ));
    }
}

/// Toggle lighting mode with the L key or UI button (via localStorage command).
fn toggle_lighting_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mut mode: ResMut<LightingMode>,
    mut commands: Commands,
    arch_lights: Query<Entity, With<ArchitecturalLight>>,
    fixture_lights: Query<Entity, With<PhotometricFixtureLight>>,
    mut ambient: Query<&mut AmbientLight>,
    mut state: ResMut<PhotometricState>,
) {
    // Check both keyboard and UI command
    let key_pressed = keys.just_pressed(KeyCode::KeyL);
    let ui_cmd = crate::storage::load_lighting_cmd().is_some();
    if ui_cmd {
        crate::storage::clear_lighting_cmd();
    }

    if !(key_pressed || ui_cmd) {
        return;
    }

    if state.fixtures.is_empty() {
        crate::log_info(&format!(
            "[Photometric] Toggle requested but no fixtures loaded (key={}, ui={})",
            key_pressed, ui_cmd
        ));
        return;
    }

    *mode = mode.next();
    crate::log_info(&format!("[Photometric] Mode: {}", mode.label()));

    let needs_photometric = matches!(*mode, LightingMode::Photometric | LightingMode::Combined);

    // Spawn or despawn fixture lights as needed
    if needs_photometric && !state.lights_active {
        for (i, fixture) in state.fixtures.iter().enumerate() {
            // PointLight radiates in all directions — with only 8 lights this
            // illuminates both pitch and spectator stands realistically.
            // Bevy PointLight intensity is in lumens directly.
            let intensity = fixture.lumens * 8.0;

            crate::log(&format!(
                "[Photometric] Spawning light {}: pos=({:.1},{:.1},{:.1}) intensity={:.0} range={:.0}",
                i, fixture.position.x, fixture.position.y, fixture.position.z,
                intensity, fixture.range,
            ));

            commands.spawn((
                PointLight {
                    color: fixture.color,
                    intensity,
                    radius: 0.5,
                    range: fixture.range,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(fixture.position),
                PhotometricFixtureLight {
                    fixture_ids: fixture.fixture_ids.clone(),
                },
            ));
        }
        state.lights_active = true;
        crate::log_info(&format!(
            "[Photometric] Spawned {} PointLights",
            state.fixtures.len()
        ));
    } else if !needs_photometric && state.lights_active {
        let mut count = 0;
        for entity in fixture_lights.iter() {
            commands.entity(entity).despawn();
            count += 1;
        }
        state.lights_active = false;
        crate::log_info(&format!("[Photometric] Despawned {} PointLights", count));
    }

    // Toggle architectural lights (roof hiding is manual via eye icon)
    match *mode {
        LightingMode::Architectural => {
            for entity in arch_lights.iter() {
                commands.entity(entity).insert(Visibility::Inherited);
            }
            if let Ok(mut amb) = ambient.single_mut() {
                amb.brightness = 150.0;
            }
        }
        LightingMode::Photometric => {
            for entity in arch_lights.iter() {
                commands.entity(entity).insert(Visibility::Hidden);
            }
            if let Ok(mut amb) = ambient.single_mut() {
                amb.brightness = 80.0;
            }
        }
        LightingMode::Combined => {
            for entity in arch_lights.iter() {
                commands.entity(entity).insert(Visibility::Inherited);
            }
            if let Ok(mut amb) = ambient.single_mut() {
                amb.brightness = 60.0;
            }
        }
    }
}

/// Sync SpotLight visibility with the eye icon toggle.
/// If ANY fixture ID in a sector is hidden, the whole sector SpotLight is hidden.
fn sync_light_visibility(
    mut commands: Commands,
    fixture_lights: Query<(Entity, &PhotometricFixtureLight)>,
    mode: Res<LightingMode>,
) {
    if !matches!(*mode, LightingMode::Photometric | LightingMode::Combined) {
        return;
    }

    let Some(vis) = crate::storage::load_visibility() else {
        return;
    };

    let hidden: rustc_hash::FxHashSet<u64> = vis.hidden.iter().copied().collect();
    if hidden.is_empty() {
        // Ensure all lights are visible
        for (entity, _) in fixture_lights.iter() {
            commands.entity(entity).insert(Visibility::Inherited);
        }
        return;
    }

    for (entity, light) in fixture_lights.iter() {
        // Hide if any fixture in this sector is hidden
        let any_hidden = light.fixture_ids.iter().any(|id| hidden.contains(id));
        let vis = if any_hidden {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        commands.entity(entity).insert(vis);
    }
}
