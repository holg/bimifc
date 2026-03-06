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
    viewport::{OrbitAngles, ViewMode, Viewport},
    Focus, LayoutConfig,
};

use anyhow::Result;
use bimifc_model::IfcModel;
use crossterm::event::{self, Event, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
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
    /// Framebuffer for legacy block-char rendering
    framebuffer: Framebuffer,
    /// Current render stats
    stats: FloorPlanStats,
    /// Current view mode
    view_mode: ViewMode,
    /// 3D orbit camera angles
    orbit: OrbitAngles,
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
    /// Tooltip from mouse hover (shown in status bar)
    tooltip: Option<String>,
    /// Last known layout areas (for mouse hit-testing)
    last_layout: Option<crate::ui::LayoutAreas>,
    /// Mouse drag state
    drag_start: Option<(u16, u16)>,
    /// Frame times for FPS calculation
    frame_times: Vec<Duration>,
    /// Last frame instant
    last_frame: Instant,
}

impl App {
    /// Create a new app with test cube
    pub fn new() -> Self {
        let scene = Scene::test_cube();

        let mut camera = OrbitCamera::new();
        camera.fit_bounds(scene.bounds_min, scene.bounds_max);

        let msg = format!(
            "Test cube: {} tri | [V] view mode [+-] zoom [WASD] pan [Q] quit",
            scene.triangles.len(),
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
            view_mode: ViewMode::Iso3D,
            orbit: OrbitAngles::default(),
            focus: Focus::Viewport,
            layout: LayoutConfig::default(),
            hierarchy: HierarchyState::default(),
            selected_properties: None,
            properties_scroll: 0,
            should_quit: false,
            message: Some(msg),
            tooltip: None,
            last_layout: None,
            drag_start: None,
            frame_times: Vec::with_capacity(60),
            last_frame: Instant::now(),
        }
    }

    /// Create app with IFC model
    pub fn with_model(model: Arc<dyn IfcModel>) -> Self {
        let scene = Scene::from_model(&model);

        let mut hierarchy = HierarchyState::default();
        if let Some(tree) = model.spatial().spatial_tree() {
            hierarchy.build_from_tree(tree);
        }

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
            view_mode: ViewMode::Iso3D,
            orbit: OrbitAngles::default(),
            focus: Focus::Viewport,
            layout: LayoutConfig::default(),
            hierarchy,
            selected_properties: None,
            properties_scroll: 0,
            should_quit: false,
            message: Some(format!(
                "{} tri | [V] view [+-] zoom [WASD] pan [Q] quit",
                tri_count
            )),
            tooltip: None,
            last_layout: None,
            drag_start: None,
            frame_times: Vec::with_capacity(60),
            last_frame: Instant::now(),
        }
    }

    /// Create app with IFC content and parsed model (more efficient)
    pub fn with_content(content: &str, model: Arc<bimifc_parser::ParsedModel>) -> Self {
        let scene = Scene::from_content(content, &model);

        let mut hierarchy = HierarchyState::default();
        if let Some(tree) = model.spatial().spatial_tree() {
            hierarchy.build_from_tree(tree);
        }

        let mut camera = OrbitCamera::new();
        if scene.diagonal() > 0.0 {
            camera.fit_bounds(scene.bounds_min, scene.bounds_max);
        }

        let tri_count = scene.triangles.len();
        let edge_count = scene.edges.len();
        let msg = format!(
            "{} tri, {} edges | [V] view [+-] zoom [WASD] pan [Q] quit",
            tri_count, edge_count,
        );

        let mut floorplan_view = FloorPlanView::default();
        floorplan_view.fit_to_scene(&scene);

        Self {
            model: Some(model),
            scene,
            camera,
            floorplan_view,
            framebuffer: Framebuffer::new(80, 24),
            stats: FloorPlanStats::default(),
            view_mode: ViewMode::Iso3D,
            orbit: OrbitAngles::default(),
            focus: Focus::Viewport,
            layout: LayoutConfig::default(),
            hierarchy,
            selected_properties: None,
            properties_scroll: 0,
            should_quit: false,
            message: Some(msg),
            tooltip: None,
            last_layout: None,
            drag_start: None,
            frame_times: Vec::with_capacity(60),
            last_frame: Instant::now(),
        }
    }

    /// Run the application main loop
    pub fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
        while !self.should_quit {
            let now = Instant::now();
            let frame_time = now - self.last_frame;
            self.last_frame = now;

            self.frame_times.push(frame_time);
            if self.frame_times.len() > 60 {
                self.frame_times.remove(0);
            }

            if event::poll(Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key),
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    _ => {}
                }
            }

            terminal.draw(|frame| {
                let area = frame.area();
                let layout = calculate_layout(area, &self.layout);

                // Store layout areas for mouse hit-testing
                self.last_layout = Some(crate::ui::LayoutAreas {
                    hierarchy: layout.hierarchy,
                    viewport: layout.viewport,
                    properties: layout.properties,
                    status: layout.status,
                });

                // Only render legacy framebuffer for BlockChar mode
                if self.view_mode == ViewMode::BlockChar {
                    let vp_w = layout.viewport.width.saturating_sub(2) as usize;
                    let vp_h = layout.viewport.height.saturating_sub(2) as usize;
                    if vp_w > 0 && vp_h > 0 {
                        self.framebuffer.resize(vp_w, vp_h);
                        self.camera.set_terminal_aspect(vp_w as u16, vp_h as u16);
                        self.stats = render_floorplan(
                            &mut self.framebuffer,
                            &self.scene,
                            &self.floorplan_view,
                        );
                    }
                }

                // Hierarchy panel
                if let Some(hier_area) = layout.hierarchy {
                    let panel = HierarchyPanel::new().focused(self.focus == Focus::Hierarchy);
                    frame.render_stateful_widget(panel, hier_area, &mut self.hierarchy);
                }

                // Get LDT content from selected entity (for polar diagram)
                let ldt_content = self
                    .selected_properties
                    .as_ref()
                    .and_then(|p| p.photometry.first())
                    .map(|ph| ph.ldt_content.as_str());

                // Viewport — Canvas-based for Iso3D/FloorPlan/Polar, legacy for BlockChar
                let viewport = Viewport::new(
                    &self.scene,
                    self.view_mode,
                    &self.floorplan_view,
                    &self.framebuffer,
                    &self.orbit,
                )
                .ldt_content(ldt_content)
                .focused(self.focus == Focus::Viewport);
                frame.render_widget(viewport, layout.viewport);

                // Properties panel
                if let Some(props_area) = layout.properties {
                    let panel = PropertiesPanel::new(self.selected_properties.as_ref())
                        .focused(self.focus == Focus::Properties)
                        .scroll(self.properties_scroll);
                    frame.render_widget(panel, props_area);
                }

                // Status bar
                let fps = self.calculate_fps();
                let mut status = StatusBar::new(fps, self.stats.clone(), self.view_mode);
                // Tooltip takes priority over message
                if let Some(ref tip) = self.tooltip {
                    status = status.with_message(tip.clone());
                } else if let Some(ref msg) = self.message {
                    status = status.with_message(msg.clone());
                }
                frame.render_widget(status, layout.status);
            })?;
        }

        Ok(())
    }

    /// Handle key input
    fn handle_key(&mut self, key: KeyEvent) {
        self.message = None;

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

    /// Handle mouse input
    fn handle_mouse(&mut self, mouse: MouseEvent) {
        let (col, row) = (mouse.column, mouse.row);
        let layout = match &self.last_layout {
            Some(l) => l,
            None => return,
        };

        // Determine which panel the mouse is over
        let over_hierarchy = layout
            .hierarchy
            .is_some_and(|r| r.contains((col, row).into()));
        let over_viewport = layout.viewport.contains((col, row).into());
        let over_properties = layout
            .properties
            .is_some_and(|r| r.contains((col, row).into()));

        match mouse.kind {
            // Left click — focus panel + select hierarchy item
            // Clicking already-selected hierarchy item toggles expand
            MouseEventKind::Down(MouseButton::Left) => {
                self.message = None;
                if over_hierarchy {
                    self.focus = Focus::Hierarchy;
                    let was_selected = self.click_hierarchy(col, row);
                    if was_selected {
                        self.hierarchy.toggle_expand();
                        self.rebuild_hierarchy();
                        self.update_selected_properties();
                    }
                } else if over_viewport {
                    self.focus = Focus::Viewport;
                    self.drag_start = Some((col, row));
                } else if over_properties {
                    self.focus = Focus::Properties;
                }
            }

            // Right-click on hierarchy to toggle expand
            MouseEventKind::Down(MouseButton::Right) => {
                if over_hierarchy {
                    self.focus = Focus::Hierarchy;
                    let _ = self.click_hierarchy(col, row);
                    self.hierarchy.toggle_expand();
                    self.rebuild_hierarchy();
                    self.update_selected_properties();
                }
            }

            // Drag for panning
            MouseEventKind::Drag(MouseButton::Left) => {
                if over_viewport {
                    if let Some((start_col, start_row)) = self.drag_start {
                        let dx = col as f32 - start_col as f32;
                        let dy = row as f32 - start_row as f32;
                        // Scale drag to scene units
                        let scale = 0.5 / self.floorplan_view.zoom;
                        self.floorplan_view.pan.x += dx * scale;
                        self.floorplan_view.pan.y += dy * scale;
                        self.drag_start = Some((col, row));
                    }
                }
            }

            MouseEventKind::Up(MouseButton::Left) => {
                self.drag_start = None;
            }

            // Scroll wheel for zoom (viewport) or scroll (hierarchy/properties)
            MouseEventKind::ScrollUp => {
                if over_viewport {
                    self.floorplan_view.zoom_in();
                } else if over_hierarchy {
                    self.hierarchy.select_previous();
                    self.update_selected_properties();
                } else if over_properties {
                    self.properties_scroll = self.properties_scroll.saturating_sub(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if over_viewport {
                    self.floorplan_view.zoom_out();
                } else if over_hierarchy {
                    self.hierarchy.select_next();
                    self.update_selected_properties();
                } else if over_properties {
                    self.properties_scroll = self.properties_scroll.saturating_add(3);
                }
            }

            // Mouse move — update tooltip for hierarchy hover
            MouseEventKind::Moved => {
                self.tooltip = None;
                if over_hierarchy {
                    if let Some(hier_area) = layout.hierarchy {
                        // Inner area (inside border)
                        let inner_top = hier_area.y + 1;
                        if row >= inner_top {
                            let list_row = (row - inner_top) as usize;
                            let item_index = list_row + self.hierarchy.scroll_offset();
                            if let Some(item) = self.hierarchy.items.get(item_index) {
                                let inner_w = hier_area.width.saturating_sub(2) as usize;
                                let display_w = inner_w.saturating_sub(item.depth * 2 + 6);
                                // Show tooltip if name is truncated
                                if item.name.len() > display_w {
                                    self.tooltip = Some(item.name.clone());
                                }
                            }
                        }
                    }
                }
            }

            _ => {}
        }
    }

    /// Handle click on hierarchy panel — select item at mouse position.
    /// Returns true if the clicked item was already selected (for toggle expand).
    fn click_hierarchy(&mut self, _col: u16, row: u16) -> bool {
        if let Some(hier_area) = self.last_layout.as_ref().and_then(|l| l.hierarchy) {
            let inner_top = hier_area.y + 1; // border
            if row >= inner_top {
                let list_row = (row - inner_top) as usize;
                let item_index = list_row + self.hierarchy.scroll_offset();
                let was_selected = self.hierarchy.selected == item_index;
                self.hierarchy.select_at(item_index);
                self.update_selected_properties();
                return was_selected;
            }
        }
        false
    }

    /// Handle action
    fn handle_action(&mut self, action: Action) {
        match action {
            // Floor plan / level navigation
            Action::LevelUp => {
                self.floorplan_view.next_level(&self.scene);
                self.message = Some(format!("Level up: Y={:.1}m", self.floorplan_view.slice_y));
            }
            Action::LevelDown => {
                self.floorplan_view.prev_level(&self.scene);
                self.message = Some(format!("Level down: Y={:.1}m", self.floorplan_view.slice_y));
            }
            Action::ZoomIn => self.floorplan_view.zoom_in(),
            Action::ZoomOut => self.floorplan_view.zoom_out(),
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

            // 3D orbit rotation
            Action::RotateLeft => self.orbit.rotate_left(),
            Action::RotateRight => self.orbit.rotate_right(),
            Action::RotateUp => self.orbit.rotate_up(),
            Action::RotateDown => self.orbit.rotate_down(),

            // View mode
            Action::ToggleViewMode => {
                self.view_mode = self.view_mode.next();
                self.message = Some(format!("View: {}", self.view_mode.label()));
            }

            // Focus
            Action::CycleFocus => self.focus = self.focus.next(),
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

    fn update_selected_properties(&mut self) {
        if let (Some(id), Some(model)) = (self.hierarchy.selected_id(), &self.model) {
            self.selected_properties = EntityProperties::load(id, model);
            self.properties_scroll = 0;
        } else {
            self.selected_properties = None;
        }
    }

    fn rebuild_hierarchy(&mut self) {
        if let Some(model) = &self.model {
            if let Some(tree) = model.spatial().spatial_tree() {
                self.hierarchy.build_from_tree(tree);
            }
        }
    }

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
