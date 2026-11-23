//! Apple FFI functions for iOS and macOS
//!
//! These functions are called from Swift to control the Bevy app.

use crate::{
    mesh::{IfcMesh, IfcMeshSerialized},
    native_view::AppViews,
    EntityInfo, IfcSceneData, IfcViewerPlugin, ViewerSettings,
};
use bevy::prelude::*;
use std::ffi::c_void;
use std::sync::Mutex;

/// Opaque app handle for FFI
pub struct BevyApp {
    app: App,
}

/// Create a new Bevy app attached to a native view
///
/// # Safety
/// - `view_ptr` must be a valid pointer to a UIView (iOS) or NSView (macOS)
/// - The view must have a CAMetalLayer as its backing layer
/// - The caller must ensure the view outlives the BevyApp
#[unsafe(no_mangle)]
pub unsafe extern "C" fn create_bevy_app(
    view_ptr: *mut c_void,
    _max_fps: i32,
    scale_factor: f32,
) -> *mut BevyApp {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        build_bevy_app(view_ptr, scale_factor)
    })) {
        Ok(ptr) => ptr,
        Err(_) => {
            crate::log_info("[Viewer] panic while creating the viewer");
            std::ptr::null_mut()
        }
    }
}

fn build_bevy_app(view_ptr: *mut c_void, scale_factor: f32) -> *mut BevyApp {
    // Initialize logging
    #[cfg(debug_assertions)]
    {
        std::env::set_var("RUST_LOG", "info");
    }

    let mut app = App::new();

    // Create view object based on platform
    #[cfg(target_os = "ios")]
    let view_obj = crate::native_view::IOSViewObj {
        view: view_ptr,
        scale_factor,
    };

    #[cfg(target_os = "macos")]
    let view_obj = crate::native_view::MacOSViewObj {
        view: view_ptr,
        scale_factor,
    };
    // Both AppView (for the render surface) and AppViews (for resizes)
    // need one; they are plain pointer+scale records.
    #[cfg(target_os = "macos")]
    let view_obj_for_views = crate::native_view::MacOSViewObj {
        view: view_ptr,
        scale_factor,
    };
    #[cfg(target_os = "ios")]
    let view_obj_for_views = crate::native_view::IOSViewObj {
        view: view_ptr,
        scale_factor,
    };

    // Initialize app views manager
    let mut app_views = AppViews::new();

    // The window entity must carry what `bevy_render` extracts —
    // `Window`, `PrimaryWindow` and a `RawHandleWrapper` — or no surface
    // is ever created for it and the app renders into nothing while
    // happily reporting GPU uploads. WinitPlugin normally adds these.
    let app_view = crate::native_view::AppView::new(view_obj);
    let (logical_width, logical_height) = app_view.logical_resolution();
    let mut window = Window {
        title: "BIMIFC Viewer".to_string(),
        resolution: bevy::window::WindowResolution::new(
            logical_width.max(1.0) as u32,
            logical_height.max(1.0) as u32,
        ),
        present_mode: bevy::window::PresentMode::AutoVsync,
        ..default()
    };
    window.resolution.set_scale_factor(scale_factor);

    let wrapper = bevy::window::WindowWrapper::new(app_view);
    let handle = match bevy::window::RawHandleWrapper::new(&wrapper) {
        Ok(handle) => handle,
        Err(e) => {
            crate::log_info(&format!(
                "[Viewer] no window handle from the host view: {e:?}"
            ));
            return std::ptr::null_mut();
        }
    };

    let window_entity = app
        .world_mut()
        .spawn((window, handle, bevy::window::PrimaryWindow))
        .id();

    // Register the view for the resize system.
    #[cfg(any(target_os = "ios", target_os = "macos"))]
    {
        app_views.create_window(view_obj_for_views, window_entity);
    }

    // Insert resources before plugins
    app.insert_resource(IfcSceneData::default());
    app.insert_resource(ViewerSettings::default());
    app.insert_non_send_resource(app_views);

    // Default plugins MINUS WinitPlugin: the host app (AppKit/UIKit) owns
    // the event loop and the view. Leaving WinitPlugin in makes it try to
    // create its own event loop on a thread that has none, and it panics
    // inside a system — which, called through `enter_frame`, unwinds
    // across the C boundary and aborts the whole app.
    app.add_plugins(
        DefaultPlugins
            // The primary window is the host's view, spawned above.
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .build()
            .disable::<bevy::winit::WinitPlugin>(),
    );

    // Add IFC viewer plugin
    app.add_plugins(IfcViewerPlugin);
    app.add_plugins(crate::native_view::AppViewPlugin);

    // Without WinitPlugin there is no runner to do this for us: plugins
    // must be finished and cleaned up before the first `app.update()`,
    // otherwise the Main schedule has not been installed and the update
    // panics with "The schedule with the label Main was not found."
    app.finish();
    app.cleanup();

    let bevy_app = Box::new(BevyApp { app });
    Box::into_raw(bevy_app)
}

/// Run `f`, turning a panic into `None` instead of an abort. Used by the
/// entry points that build or mutate the app, for the reason described
/// on `enter_frame`.
fn guard<R>(what: &str, f: impl FnOnce() -> R) -> Option<R> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(v) => Some(v),
        Err(_) => {
            crate::log_info(&format!("[Viewer] panic in {what}"));
            None
        }
    }
}

/// Process a single frame update
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn enter_frame(bevy_app: *mut BevyApp) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;
    // Keep held buttons alive across the frame boundary (see
    // HELD_BUTTONS) before the systems read input.
    resend_held_buttons(app);
    // A panic inside a Bevy system must not unwind into Swift: an
    // `extern "C"` frame cannot unwind, so the process aborts with a
    // useless "panic in a function that cannot unwind". Catch it here,
    // log it once and let the host keep running (the view goes static
    // rather than taking the app down).
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        app.update();
        // Once, after the first frames: report whether the renderer
        // actually created a surface for our window. "Uploads fine but
        // shows nothing" means there is none.
        use std::sync::atomic::{AtomicU32, Ordering};
        static FRAME: AtomicU32 = AtomicU32::new(0);
        let n = FRAME.fetch_add(1, Ordering::Relaxed);
        if n == 5 {
            let world = app.world_mut();
            let windows = world
                .query_filtered::<(), (
                    With<Window>,
                    With<bevy::window::RawHandleWrapper>,
                    With<bevy::window::PrimaryWindow>,
                )>()
                .iter(world)
                .count();
            let cameras: Vec<String> = world
                .query::<(&Camera, Option<&bevy::camera::RenderTarget>)>()
                .iter(world)
                .map(|(c, t)| format!("active={} order={} target={:?}", c.is_active, c.order, t))
                .collect();
            let extracted = world
                .get_resource::<bevy::render::view::window::ExtractedWindows>()
                .map(|w| w.windows.len());
            crate::log_info(&format!(
                "[Viewer] frame {n}: windows={windows}, extracted={extracted:?}, cameras={cameras:?}"
            ));
        }
    }));
    if result.is_err() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static REPORTED: AtomicBool = AtomicBool::new(false);
        if !REPORTED.swap(true, Ordering::Relaxed) {
            crate::log_info("[Viewer] panic in a frame update — rendering stopped");
        }
    }
}

/// Release the Bevy app and free memory
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
/// - After calling this function, the pointer is invalid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn release_bevy_app(bevy_app: *mut BevyApp) {
    if bevy_app.is_null() {
        return;
    }

    let _ = Box::from_raw(bevy_app);
}

/// Load IFC geometry into the viewer
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
/// - `meshes_json` must be a valid null-terminated JSON string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn load_geometry(
    bevy_app: *mut BevyApp,
    meshes_json: *const std::ffi::c_char,
) -> bool {
    if bevy_app.is_null() || meshes_json.is_null() {
        return false;
    }

    let json_str = match std::ffi::CStr::from_ptr(meshes_json).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    // Deserialize to serialized format, then convert to Arc-based IfcMesh
    let serialized: Vec<IfcMeshSerialized> = match serde_json::from_str(json_str) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to parse meshes JSON: {}", e);
            return false;
        }
    };
    let meshes: Vec<IfcMesh> = serialized.into_iter().map(IfcMesh::from).collect();

    let app = &mut (*bevy_app).app;

    if let Some(mut scene_data) = app.world_mut().get_resource_mut::<IfcSceneData>() {
        scene_data.meshes = meshes;
        scene_data.dirty = true;
        true
    } else {
        false
    }
}

/// Load entity metadata
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
/// - `entities_json` must be a valid null-terminated JSON string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn load_entities(
    bevy_app: *mut BevyApp,
    entities_json: *const std::ffi::c_char,
) -> bool {
    if bevy_app.is_null() || entities_json.is_null() {
        return false;
    }

    let json_str = match std::ffi::CStr::from_ptr(entities_json).to_str() {
        Ok(s) => s,
        Err(_) => return false,
    };

    let entities: Vec<EntityInfo> = match serde_json::from_str(json_str) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to parse entities JSON: {}", e);
            return false;
        }
    };

    let app = &mut (*bevy_app).app;

    if let Some(mut scene_data) = app.world_mut().get_resource_mut::<IfcSceneData>() {
        scene_data.entities = entities;
        true
    } else {
        false
    }
}

/// Select an entity by ID
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn select_entity(bevy_app: *mut BevyApp, entity_id: u64) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;

    if let Some(mut selection) = app.world_mut().get_resource_mut::<crate::SelectionState>() {
        selection.selected.clear();
        selection.selected.insert(entity_id);
    }
}

/// Clear selection
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clear_selection(bevy_app: *mut BevyApp) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;

    if let Some(mut selection) = app.world_mut().get_resource_mut::<crate::SelectionState>() {
        selection.selected.clear();
        selection.hovered = None;
    }
}

/// Hide an entity
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn hide_entity(bevy_app: *mut BevyApp, entity_id: u64) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;

    if let Some(mut settings) = app.world_mut().get_resource_mut::<ViewerSettings>() {
        settings.hidden_entities.insert(entity_id);
    }
}

/// Show an entity
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn show_entity(bevy_app: *mut BevyApp, entity_id: u64) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;

    if let Some(mut settings) = app.world_mut().get_resource_mut::<ViewerSettings>() {
        settings.hidden_entities.remove(&entity_id);
    }
}

/// Show all entities
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn show_all(bevy_app: *mut BevyApp) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;

    if let Some(mut settings) = app.world_mut().get_resource_mut::<ViewerSettings>() {
        settings.hidden_entities.clear();
        settings.isolated_entities = None;
    }
}

/// Isolate entities (hide all others)
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
/// - `entity_ids` must be a valid array of `count` u64 values
#[unsafe(no_mangle)]
pub unsafe extern "C" fn isolate_entities(
    bevy_app: *mut BevyApp,
    entity_ids: *const u64,
    count: usize,
) {
    if bevy_app.is_null() || entity_ids.is_null() {
        return;
    }

    let ids = std::slice::from_raw_parts(entity_ids, count);
    let app = &mut (*bevy_app).app;

    if let Some(mut settings) = app.world_mut().get_resource_mut::<ViewerSettings>() {
        settings.isolated_entities = Some(ids.iter().copied().collect());
    }
}

/// Set camera home view
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn camera_home(bevy_app: *mut BevyApp) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;

    if let Some(mut controller) = app
        .world_mut()
        .get_resource_mut::<crate::CameraController>()
    {
        controller.home();
    }
}

/// Fit camera to show all geometry
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn camera_fit_all(bevy_app: *mut BevyApp) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;
    let world = app.world_mut();

    // Get bounds from scene data
    let bounds = world
        .get_resource::<IfcSceneData>()
        .and_then(|data| data.bounds.clone());

    if let (Some(bounds), Some(mut controller)) =
        (bounds, world.get_resource_mut::<crate::CameraController>())
    {
        controller.fit_bounds(bounds.min, bounds.max);
    }
}

/// Focus camera on a specific entity
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn camera_focus_entity(bevy_app: *mut BevyApp, entity_id: u64) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;

    // Set pending focus - the system will handle it
    if let Some(mut pending) = app
        .world_mut()
        .get_resource_mut::<crate::mesh::PendingFocus>()
    {
        pending.entity_id = Some(entity_id);
    }
}

/// Handle touch started event
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn touch_started(bevy_app: *mut BevyApp, x: f32, y: f32) {
    touch_event(bevy_app, x, y, TouchPhase::Started);
}

/// Handle touch moved event
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn touch_moved(bevy_app: *mut BevyApp, x: f32, y: f32) {
    touch_event(bevy_app, x, y, TouchPhase::Moved);
}

/// Handle touch ended event
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn touch_ended(bevy_app: *mut BevyApp, x: f32, y: f32) {
    touch_event(bevy_app, x, y, TouchPhase::Ended);
}

/// Handle touch cancelled event
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn touch_cancelled(bevy_app: *mut BevyApp, x: f32, y: f32) {
    touch_event(bevy_app, x, y, TouchPhase::Canceled);
}

/// Internal touch event handler
unsafe fn touch_event(bevy_app: *mut BevyApp, x: f32, y: f32, phase: TouchPhase) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;
    let world = app.world_mut();

    // Get the primary window entity
    let window_entity = world
        .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>()
        .iter(world)
        .next();

    if let Some(window_entity) = window_entity {
        let touch = TouchInput {
            phase,
            position: Vec2::new(x, y),
            window: window_entity,
            force: None,
            id: 0,
        };

        if let Some(mut messages) = world.get_resource_mut::<Messages<TouchInput>>() {
            messages.write(touch);
        }
    }
}

/// Load an IFC file's text directly into the viewer.
///
/// The JSON mesh path (`load_geometry`) expects the host to parse and
/// serialise geometry first. Swift apps already hold the IFC text they
/// exported, so this hands it over as-is and lets the viewer's own
/// loader do the work — the same route the web/native studio uses.
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
/// - `content` and `file_name` must be valid null-terminated UTF-8
#[unsafe(no_mangle)]
pub unsafe extern "C" fn load_ifc_text(
    bevy_app: *mut BevyApp,
    content: *const std::ffi::c_char,
    file_name: *const std::ffi::c_char,
) -> bool {
    if bevy_app.is_null() || content.is_null() {
        return false;
    }
    let Ok(content) = std::ffi::CStr::from_ptr(content).to_str() else {
        return false;
    };
    let name = if file_name.is_null() {
        "model.ifc".to_string()
    } else {
        std::ffi::CStr::from_ptr(file_name)
            .to_str()
            .unwrap_or("model.ifc")
            .to_string()
    };

    let app = &mut (*bevy_app).app;
    guard("load_ifc_text", || {
        // Clear the previous model so repeated loads replace rather than stack.
        if let Some(mut scene) = app.world_mut().get_resource_mut::<IfcSceneData>() {
            scene.meshes.clear();
            scene.entities.clear();
            scene.bounds = None;
            scene.dirty = true;
        }
        app.world_mut()
            .write_message(crate::loader::LoadIfcContentEvent {
                file_name: name,
                content: content.to_string(),
            });
    })
    .is_some()
}

/// Apply a discipline filter: 0 = all, 1 = architecture only, 2 =
/// electrical, 3 = plumbing, 4 = HVAC, 5 = lighting. Anything else is
/// treated as "all".
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_discipline_filter(bevy_app: *mut BevyApp, filter: u32) {
    if bevy_app.is_null() {
        return;
    }
    use bimifc_federation::{Discipline, ViewFilter};
    let filter = match filter {
        1 => ViewFilter::Architecture,
        2 => ViewFilter::Discipline(Discipline::Electrical),
        3 => ViewFilter::Discipline(Discipline::Plumbing),
        4 => ViewFilter::Discipline(Discipline::Hvac),
        5 => ViewFilter::Discipline(Discipline::Lighting),
        _ => ViewFilter::All,
    };
    let app = &mut (*bevy_app).app;
    if let Some(mut federation) = app.world_mut().get_resource_mut::<crate::FederationState>() {
        federation.scene.filter = filter;
    }
}

/// Resize the rendering surface after the host view's bounds changed.
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn resize_surface(bevy_app: *mut BevyApp, width: f32, height: f32) {
    if bevy_app.is_null() || width <= 0.0 || height <= 0.0 {
        return;
    }
    let app = &mut (*bevy_app).app;
    let mut windows = app.world_mut().query::<&mut Window>();
    for mut window in windows.iter_mut(app.world_mut()) {
        window.resolution.set(width, height);
    }
}

// ── Mouse input (desktop hosts) ────────────────────────────────────────
//
// The camera controller reads `ButtonInput<MouseButton>`, `MouseMotion`
// and `MouseWheel`, which winit normally fills. An embedded host owns the
// event loop, so it must forward them itself — without this the model is
// on screen but completely inert.

/// Buttons the host currently reports as held.
///
/// Bevy's `mouse_button_input_system` runs every frame, `clear()`s the
/// pressed set and rebuilds it from that frame's `MouseButtonInput`
/// messages. A button therefore only reads as held while messages keep
/// arriving — winit re-sends nothing, because its state *is* the
/// message stream. An embedded host that sends one press and then only
/// motion loses the button immediately, which is why orbit and pan did
/// nothing while zoom (wheel messages, read directly) worked.
static HELD_BUTTONS: Mutex<Vec<MouseButton>> = Mutex::new(Vec::new());

/// Re-send a press message for every held button, so the state survives
/// the next `clear()`. Called from `enter_frame`.
fn resend_held_buttons(app: &mut App) {
    let held = HELD_BUTTONS.lock().unwrap().clone();
    if held.is_empty() {
        return;
    }
    let world = app.world_mut();
    let Some(window) = primary_window(world) else {
        return;
    };
    if let Some(mut messages) =
        world.get_resource_mut::<Messages<bevy::input::mouse::MouseButtonInput>>()
    {
        for button in held {
            messages.write(bevy::input::mouse::MouseButtonInput {
                button,
                state: bevy::input::ButtonState::Pressed,
                window,
            });
        }
    }
}

/// Mouse button ids: 0 left, 1 right, 2 middle.
fn mouse_button_from(button: u32) -> MouseButton {
    match button {
        1 => MouseButton::Right,
        2 => MouseButton::Middle,
        _ => MouseButton::Left,
    }
}

/// Press or release a mouse button. `x`/`y` are in logical points with
/// the origin at the view's top-left (AppKit's flipped convention).
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mouse_button(
    bevy_app: *mut BevyApp,
    button: u32,
    pressed: bool,
    x: f32,
    y: f32,
) {
    if bevy_app.is_null() {
        return;
    }
    let app = &mut (*bevy_app).app;
    let button = mouse_button_from(button);
    {
        let world = app.world_mut();
        // The cursor position must be current before the press is
        // handled: picking reads it in the same frame.
        set_cursor_position(world, x, y);

        // Writing the resource directly is pointless: the next frame
        // clears it. The message below is the state.
        let _ = world;
    }
    {
        let mut held = HELD_BUTTONS.lock().unwrap();
        held.retain(|b| *b != button);
        if pressed {
            held.push(button);
        }
    }
    // `just_pressed` lives for exactly one frame and Bevy's own
    // `clear()` runs every frame. A host that presses and drags between
    // two frames therefore loses the press entirely: the camera sees
    // motion it never saw a button for, so orbit and pan do nothing
    // while zoom (which reads only wheel messages) works fine.
    //
    // Run one update while the press is fresh so the controller starts
    // its drag, matching what winit does by driving input and frames on
    // the same loop.

    let world = app.world_mut();
    if let Some(window_entity) = primary_window(world) {
        if let Some(mut messages) =
            world.get_resource_mut::<Messages<bevy::input::mouse::MouseButtonInput>>()
        {
            messages.write(bevy::input::mouse::MouseButtonInput {
                button,
                state: if pressed {
                    bevy::input::ButtonState::Pressed
                } else {
                    bevy::input::ButtonState::Released
                },
                window: window_entity,
            });
        }
    }
    // `just_pressed` lives for exactly one frame and Bevy clears it each
    // frame. A host that presses and drags between two frames loses the
    // press entirely: the camera then sees motion it never saw a button
    // for, so orbit and pan do nothing while zoom — which reads only
    // wheel messages — works fine. Run one update while the press is
    // fresh, as winit does by driving input and frames on one loop.
    if pressed {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.update()));
    }
}

/// Cursor moved to `x`/`y` (logical points), having moved by `dx`/`dy`.
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mouse_moved(bevy_app: *mut BevyApp, x: f32, y: f32, dx: f32, dy: f32) {
    if bevy_app.is_null() {
        return;
    }
    let app = &mut (*bevy_app).app;
    let world = app.world_mut();
    set_cursor_position(world, x, y);

    if let Some(mut messages) = world.get_resource_mut::<Messages<MouseMotion>>() {
        messages.write(MouseMotion {
            delta: Vec2::new(dx, dy),
        });
    }
    if let Some(window_entity) = primary_window(world) {
        if let Some(mut messages) = world.get_resource_mut::<Messages<bevy::window::CursorMoved>>()
        {
            messages.write(bevy::window::CursorMoved {
                window: window_entity,
                position: Vec2::new(x, y),
                delta: Some(Vec2::new(dx, dy)),
            });
        }
    }
}

/// Scroll / magnify. `lines` is positive when zooming in.
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mouse_wheel(bevy_app: *mut BevyApp, horizontal: f32, vertical: f32) {
    if bevy_app.is_null() {
        return;
    }
    let app = &mut (*bevy_app).app;
    let world = app.world_mut();
    let Some(window_entity) = primary_window(world) else {
        return;
    };
    if let Some(mut messages) = world.get_resource_mut::<Messages<MouseWheel>>() {
        messages.write(MouseWheel {
            unit: bevy::input::mouse::MouseScrollUnit::Line,
            x: horizontal,
            y: vertical,
            window: window_entity,
        });
    }
}

fn primary_window(world: &mut World) -> Option<Entity> {
    world
        .query_filtered::<Entity, With<bevy::window::PrimaryWindow>>()
        .iter(world)
        .next()
}

/// Keep `Window::cursor_position` in step — picking and the camera both
/// read it rather than the motion messages.
fn set_cursor_position(world: &mut World, x: f32, y: f32) {
    let mut windows = world.query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>();
    if let Ok(mut window) = windows.single_mut(world) {
        window.set_cursor_position(Some(Vec2::new(x, y)));
    }
}

/// Write a short description of the camera (azimuth, elevation,
/// distance, target) into `out`. For host-side checks that navigation
/// actually reaches the camera — a viewer that renders but ignores
/// input looks identical to one that works.
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
/// - `out` must point to at least `capacity` writable bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn camera_state_description(
    bevy_app: *mut BevyApp,
    out: *mut std::ffi::c_char,
    capacity: usize,
) {
    if bevy_app.is_null() || out.is_null() || capacity == 0 {
        return;
    }
    let app = &mut (*bevy_app).app;
    let text = app
        .world()
        .get_resource::<crate::camera::CameraController>()
        .map(|c| {
            format!(
                "az={:.4} el={:.4} d={:.3} t={:.2},{:.2},{:.2} drag={} gesture={:?} mode={:?}",
                c.azimuth,
                c.elevation,
                c.distance,
                c.target.x,
                c.target.y,
                c.target.z,
                c.is_dragging,
                c.drag_gesture,
                c.mode
            )
        })
        .unwrap_or_else(|| "no camera".to_string());

    let bytes = text.as_bytes();
    let n = bytes.len().min(capacity - 1);
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), out as *mut u8, n);
    *out.add(n) = 0;
}

/// Set theme (dark/light)
///
/// # Safety
/// - `bevy_app` must be a valid pointer returned by `create_bevy_app`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_theme(bevy_app: *mut BevyApp, dark: bool) {
    if bevy_app.is_null() {
        return;
    }

    let app = &mut (*bevy_app).app;

    if let Some(mut settings) = app.world_mut().get_resource_mut::<ViewerSettings>() {
        settings.theme = if dark {
            crate::Theme::Dark
        } else {
            crate::Theme::Light
        };
    }
}

use bevy::ecs::message::Messages;
use bevy::input::mouse::MouseButton;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::input::touch::{TouchInput, TouchPhase};
