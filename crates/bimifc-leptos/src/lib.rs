//! BIMIFC Leptos UI Components
//!
//! Leptos-based UI components for the BIMIFC viewer.
//! Provides a reactive UI that integrates with the Bevy 3D renderer.

pub mod bridge;
pub mod components;
pub mod state;
pub mod utils;

// Re-exports
pub use bridge::{
    clear_model_cache,
    // Cache exports
    compute_file_hash,
    init_debug_from_url,
    is_bevy_loaded,
    is_bevy_loading,
    is_model_cached,
    is_unified_mode,
    load_bevy_viewer,
    load_cached_model,
    save_camera_cmd,
    save_focus,
    save_geometry,
    save_model_to_cache,
    save_palette,
    save_section,
    save_selection,
    save_visibility,
    CacheEntry,
    CacheIndex,
    CachedModel,
    CameraCommand,
    ColorPalette,
    EntityData,
    FocusData,
    GeometryData,
    SectionData,
    SelectionData,
    VisibilityData,
};
pub use components::{App, ViewerLayout};
pub use state::{
    provide_viewer_state, use_viewer_state, EntityInfo, MeasurePoint, Measurement, Progress,
    PropertySet, PropertyValue, QuantityValue, SectionAxis, SpatialNode, SpatialNodeType,
    StoreyInfo, Theme, Tool, ViewerState,
};
