/*
 * bimifc embedded viewer — C API for Apple hosts.
 *
 * The Bevy-based IFC viewer rendered into a host NSView (macOS) or
 * UIView (iOS), so a Swift app uses the same renderer as the web and
 * native studio instead of maintaining a second scene graph.
 *
 * Implemented in crates/bimifc-bevy/src/ffi/apple.rs and linked into
 * libbimifc_ffi.a when bimifc-ffi is built with the `viewer` feature.
 *
 * Threading: every function must be called from the thread that owns the
 * view (the main thread on Apple platforms).
 */

#ifndef BIMIFC_VIEWER_H
#define BIMIFC_VIEWER_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque viewer instance. */
typedef struct BevyApp BevyApp;

/* Create a viewer drawing into `view_ptr` (an NSView* or UIView* whose
 * backing layer is a CAMetalLayer). Returns NULL on failure. */
BevyApp *create_bevy_app(void *view_ptr, int32_t max_fps, float scale_factor);

/* Advance and render one frame. Call once per display refresh. */
void enter_frame(BevyApp *app);

/* Destroy the viewer. The pointer is invalid afterwards. */
void release_bevy_app(BevyApp *app);

/* Replace the model with the given IFC text. Returns false if the
 * arguments are unusable. */
bool load_ifc_text(BevyApp *app, const char *content, const char *file_name);

/* Load geometry/entities as JSON (the older host-parses-first route). */
bool load_geometry(BevyApp *app, const char *meshes_json);
bool load_entities(BevyApp *app, const char *entities_json);

/* Discipline filter: 0 all, 1 architecture, 2 electrical, 3 plumbing,
 * 4 HVAC, 5 lighting. */
void set_discipline_filter(BevyApp *app, uint32_t filter);

/* Tell the viewer the drawable size changed (device pixels). */
void resize_surface(BevyApp *app, float width, float height);

/* Selection and visibility. */
void select_entity(BevyApp *app, uint64_t entity_id);
void clear_selection(BevyApp *app);
void hide_entity(BevyApp *app, uint64_t entity_id);
void show_entity(BevyApp *app, uint64_t entity_id);
void show_all(BevyApp *app);
void isolate_entities(BevyApp *app, const uint64_t *entity_ids, uintptr_t count);

/* Camera. */
void camera_home(BevyApp *app);
void camera_fit_all(BevyApp *app);
void camera_focus_entity(BevyApp *app, uint64_t entity_id);

/* Mouse input (desktop hosts). The host owns the event loop, so it must
 * forward these or the model renders but does not respond. Coordinates
 * are logical points, origin top-left. Buttons: 0 left, 1 right, 2 middle. */
void mouse_button(BevyApp *app, uint32_t button, bool pressed, float x, float y);
void mouse_moved(BevyApp *app, float x, float y, float dx, float dy);
void mouse_wheel(BevyApp *app, float horizontal, float vertical);

/* Pointer / touch input, in view coordinates. */
void touch_started(BevyApp *app, float x, float y);
void touch_moved(BevyApp *app, float x, float y);
void touch_ended(BevyApp *app, float x, float y);
void touch_cancelled(BevyApp *app, float x, float y);

/* Camera state as text, for host-side checks. */
void camera_state_description(BevyApp *app, char *out, size_t capacity);

/* Appearance. */
void set_theme(BevyApp *app, bool dark);

#ifdef __cplusplus
}
#endif

#endif /* BIMIFC_VIEWER_H */
