// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Main application state and event loop

use crate::camera::OrbitCamera;
use crate::input::{map_hierarchy_key, map_key_to_action, Action};
use crate::renderer::{render_floorplan, FloorPlanStats, FloorPlanView, Framebuffer};
use crate::scene::Scene;
use crate::ui::{
    calculate_layout,
    hierarchy::{HierarchyPanel, HierarchyState},
    properties::{EntityProperties, PropertiesPanel},
    status::StatusBar,
    viewport::Viewport,
    Focus, LayoutConfig,
};

use anyhow::Result;
use bimifc_model::IfcModel;
use crossterm::event::{self, Event, KeyEvent};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::Stdout;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Application state
pub struct App {
    /// IFC model (if loaded)
    model: Option<Arc<dyn IfcModel>>,
    /// Scene for rendering
    scene: Scene,
    /// Camera controller (kept for compatibility)
    camera: OrbitCamera,
    /// Floor plan view settings
    floorplan_view: FloorPlanView,
    /// Framebuffer for rendering
    framebuffer: Framebuffer,
    /// Current render stats
    stats: FloorPlanStats,
    /// Current focus
    focus: Focus,
    /// Layout configuration
    layout: LayoutConfig,
    /// Hierarchy state
    hierarchy: HierarchyState,
    /// Selected entity properties
    selected_properties: Option<EntityProperties>,
    /// Properties scroll position
    properties_scroll: u16,
    /// Should quit
    should_quit: bool,
    /// Status message
    message: Option<String>,
    /// Frame times for FPS calculation
    frame_times: Vec<Duration>,
    /// Last frame instant
    last_frame: Instant,
}

impl App {
    /// Create a new app with test cube
    pub fn new() -> Self {
        let scene = Scene::test_cube();

        // Setup camera to fit the test cube
        let mut camera = OrbitCamera::new();
        camera.fit_bounds(scene.bounds_min, scene.bounds_max);

        let msg = format!(
            "Test cube: {} triangles, camera dist={:.1}",
            scene.triangles.len(),
            camera.distance
        );

        let mut floorplan_view = FloorPlanView::default();
        floorplan_view.fit_to_scene(&scene);

        Self {
            model: None,
            scene,
            camera,
            floorplan_view,
            framebuffer: Framebuffer::new(80, 24),
            stats: FloorPlanStats::default(),
            focus: Focus::Viewport,
            layout: LayoutConfig::default(),
            hierarchy: HierarchyState::default(),
            selected_properties: None,
            properties_scroll: 0,
            should_quit: false,
            message: Some(msg),
            frame_times: Vec::with_capacity(60),
            last_frame: Instant::now(),
        }
    }

    /// Create app with IFC model
    pub fn with_model(model: Arc<dyn IfcModel>) -> Self {
        // Build scene from model
        let scene = Scene::from_model(&model);

        // Build hierarchy
        let mut hierarchy = HierarchyState::default();
        if let Some(tree) = model.spatial().spatial_tree() {
            hierarchy.build_from_tree(tree);
        }

        // Setup camera to fit scene
        let mut camera = OrbitCamera::new();
        if scene.diagonal() > 0.0 {
            camera.fit_bounds(scene.bounds_min, scene.bounds_max);
        }

        let tri_count = scene.triangles.len();

        let mut floorplan_view = FloorPlanView::default();
        floorplan_view.fit_to_scene(&scene);

        Self {
            model: Some(model),
            scene,
            camera,
            floorplan_view,
            framebuffer: Framebuffer::new(80, 24),
            stats: FloorPlanStats::default(),
            focus: Focus::Viewport,
            layout: LayoutConfig::default(),
            hierarchy,
            selected_properties: None,
            properties_scroll: 0,
            should_quit: false,
            message: Some(format!("Loaded {} triangles", tri_count)),
            frame_times: Vec::with_capacity(60),
            last_frame: Instant::now(),
        }
    }

    /// Create app with IFC content and parsed model (more efficient)
    pub fn with_content(content: &str, model: Arc<bimifc_parser::ParsedModel>) -> Self {
        // Build scene from content using scanner
        let scene = Scene::from_content(content, &model);

        // Build hierarchy
        let mut hierarchy = HierarchyState::default();
        if let Some(tree) = model.spatial().spatial_tree() {
            hierarchy.build_from_tree(tree);
        }

        // Setup camera to fit scene
        let mut camera = OrbitCamera::new();
        if scene.diagonal() > 0.0 {
            camera.fit_bounds(scene.bounds_min, scene.bounds_max);
        }

        let tri_count = scene.triangles.len();
        let msg = format!(
            "{} tri, bounds: ({:.1},{:.1},{:.1})-({:.1},{:.1},{:.1}), dist={:.1}",
            tri_count,
            scene.bounds_min.x,
            scene.bounds_min.y,
            scene.bounds_min.z,
            scene.bounds_max.x,
            scene.bounds_max.y,
            scene.bounds_max.z,
            camera.distance
        );

        // Write debug info to file
        if let Ok(mut f) = std::fs::File::create("/tmp/ifc-tui-debug.log") {
            use std::io::Write;
            writeln!(f, "Scene: {} triangles", tri_count).ok();
            writeln!(
                f,
                "Bounds min: ({}, {}, {})",
                scene.bounds_min.x, scene.bounds_min.y, scene.bounds_min.z
            )
            .ok();
            writeln!(
                f,
                "Bounds max: ({}, {}, {})",
                scene.bounds_max.x, scene.bounds_max.y, scene.bounds_max.z
            )
            .ok();
            writeln!(
                f,
                "Camera distance: {}, target: ({}, {}, {})",
                camera.distance, camera.target.x, camera.target.y, camera.target.z
            )
            .ok();
            writeln!(
                f,
                "Camera pos: ({}, {}, {})",
                camera.position().x,
                camera.position().y,
                camera.position().z
            )
            .ok();
            writeln!(f, "Camera near: {}, far: {}", camera.near, camera.far).ok();
        }

        let mut floorplan_view = FloorPlanView::default();
        floorplan_view.fit_to_scene(&scene);

        Self {
            model: Some(model),
            scene,
            camera,
            floorplan_view,
            framebuffer: Framebuffer::new(80, 24),
            stats: FloorPlanStats::default(),
            focus: Focus::Viewport,
            layout: LayoutConfig::default(),
            hierarchy,
            selected_properties: None,
            properties_scroll: 0,
            should_quit: false,
            message: Some(msg),
            frame_times: Vec::with_capacity(60),
            last_frame: Instant::now(),
        }
    }

    /// Run the application main loop
    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        while !self.should_quit {
            // Calculate frame time
            let now = Instant::now();
            let frame_time = now - self.last_frame;
            self.last_frame = now;

            // Track frame times for FPS
            self.frame_times.push(frame_time);
            if self.frame_times.len() > 60 {
                self.frame_times.remove(0);
            }

            // Handle events (non-blocking with timeout for animation)
            if event::poll(Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                }
            }

            // Draw
            terminal.draw(|frame| {
                let area = frame.area();

                // Calculate layout
                let layout = calculate_layout(area, &self.layout);

                // Resize framebuffer to match viewport
                let viewport_inner_width = layout.viewport.width.saturating_sub(2) as usize;
                let viewport_inner_height = layout.viewport.height.saturating_sub(2) as usize;

                if viewport_inner_width > 0 && viewport_inner_height > 0 {
                    self.framebuffer
                        .resize(viewport_inner_width, viewport_inner_height);
                    self.camera.set_terminal_aspect(
                        viewport_inner_width as u16,
                        viewport_inner_height as u16,
                    );

                    // Render scene
                    let _selected_id = self.hierarchy.selected_id().map(|id| id.0 as u64);
                    self.stats =
                        render_floorplan(&mut self.framebuffer, &self.scene, &self.floorplan_view);
                }

                // Render hierarchy panel
                if let Some(hier_area) = layout.hierarchy {
                    let panel = HierarchyPanel::new(&self.hierarchy)
                        .focused(self.focus == Focus::Hierarchy);
                    frame.render_widget(panel, hier_area);
                }

                // Render viewport
                let viewport =
                    Viewport::new(&self.framebuffer).focused(self.focus == Focus::Viewport);
                frame.render_widget(viewport, layout.viewport);

                // Render properties panel
                if let Some(props_area) = layout.properties {
                    let panel = PropertiesPanel::new(self.selected_properties.as_ref())
                        .focused(self.focus == Focus::Properties)
                        .scroll(self.properties_scroll);
                    frame.render_widget(panel, props_area);
                }

                // Render status bar
                let fps = self.calculate_fps();
                let mut status = StatusBar::new(fps, self.stats.clone());
                if let Some(ref msg) = self.message {
                    status = status.with_message(msg.clone());
                }
                frame.render_widget(status, layout.status);
            })?;
        }

        Ok(())
    }

    /// Handle key input
    fn handle_key(&mut self, key: KeyEvent) {
        // Clear message on any key
        self.message = None;

        // Handle based on focus
        match self.focus {
            Focus::Hierarchy => {
                if let Some(action) = map_hierarchy_key(key) {
                    self.handle_action(action);
                }
            }
            _ => {
                if let Some(action) = map_key_to_action(key) {
                    self.handle_action(action);
                }
            }
        }
    }

    /// Handle action
    fn handle_action(&mut self, action: Action) {
        match action {
            // Floor plan navigation
            Action::LevelUp => {
                self.floorplan_view.next_level(&self.scene);
                self.message = Some(format!("Level up: Y={:.1}m", self.floorplan_view.slice_y));
            }
            Action::LevelDown => {
                self.floorplan_view.prev_level(&self.scene);
                self.message = Some(format!("Level down: Y={:.1}m", self.floorplan_view.slice_y));
            }
            Action::ZoomIn => {
                self.floorplan_view.zoom_in();
            }
            Action::ZoomOut => {
                self.floorplan_view.zoom_out();
            }
            Action::PanLeft => self.floorplan_view.pan_by(-1.0, 0.0, &self.scene),
            Action::PanRight => self.floorplan_view.pan_by(1.0, 0.0, &self.scene),
            Action::PanUp => self.floorplan_view.pan_by(0.0, -1.0, &self.scene),
            Action::PanDown => self.floorplan_view.pan_by(0.0, 1.0, &self.scene),
            Action::ResetView => {
                self.floorplan_view.fit_to_scene(&self.scene);
                self.message = Some("View reset".to_string());
            }
            Action::FitAll => {
                self.floorplan_view.fit_to_scene(&self.scene);
                self.message = Some("Fit all".to_string());
            }

            // Focus
            Action::CycleFocus => {
                self.focus = self.focus.next();
            }
            Action::FocusViewport => self.focus = Focus::Viewport,
            Action::FocusHierarchy => self.focus = Focus::Hierarchy,
            Action::FocusProperties => self.focus = Focus::Properties,

            // Hierarchy
            Action::TreeUp => {
                self.hierarchy.select_previous();
                self.update_selected_properties();
            }
            Action::TreeDown => {
                self.hierarchy.select_next();
                self.update_selected_properties();
            }
            Action::TreeExpand => {
                self.hierarchy.expand();
                self.rebuild_hierarchy();
            }
            Action::TreeCollapse => {
                self.hierarchy.collapse();
                self.rebuild_hierarchy();
            }
            Action::TreeSelect => {
                self.hierarchy.toggle_expand();
                self.rebuild_hierarchy();
                self.update_selected_properties();
            }

            // Panels
            Action::ToggleHierarchy => {
                self.layout.show_hierarchy = !self.layout.show_hierarchy;
            }
            Action::ToggleProperties => {
                self.layout.show_properties = !self.layout.show_properties;
            }

            // Search
            Action::StartSearch => {
                self.focus = Focus::Search;
                self.message = Some("Search: type to filter...".to_string());
            }
            Action::CancelSearch => {
                self.focus = Focus::Viewport;
                self.hierarchy.set_filter(String::new());
                self.rebuild_hierarchy();
            }

            // Quit
            Action::Quit => self.should_quit = true,
        }
    }

    /// Update selected entity properties
    fn update_selected_properties(&mut self) {
        if let (Some(id), Some(model)) = (self.hierarchy.selected_id(), &self.model) {
            self.selected_properties = EntityProperties::load(id, model);
            self.properties_scroll = 0;
        } else {
            self.selected_properties = None;
        }
    }

    /// Rebuild hierarchy tree after expand/collapse
    fn rebuild_hierarchy(&mut self) {
        if let Some(model) = &self.model {
            if let Some(tree) = model.spatial().spatial_tree() {
                self.hierarchy.build_from_tree(tree);
            }
        }
    }

    /// Calculate FPS from frame times
    fn calculate_fps(&self) -> f32 {
        if self.frame_times.is_empty() {
            return 0.0;
        }
        let total: Duration = self.frame_times.iter().sum();
        let avg = total.as_secs_f32() / self.frame_times.len() as f32;
        if avg > 0.0 {
            1.0 / avg
        } else {
            0.0
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
