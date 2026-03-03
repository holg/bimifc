// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Orbit camera controller for the TUI viewer

use glam::{Mat4, Vec3};

/// Orbit camera that rotates around a target point
pub struct OrbitCamera {
    /// Look-at target point
    pub target: Vec3,
    /// Distance from target
    pub distance: f32,
    /// Horizontal angle in radians
    pub azimuth: f32,
    /// Vertical angle in radians (clamped to avoid gimbal lock)
    pub elevation: f32,
    /// Field of view in degrees
    pub fov: f32,
    /// Aspect ratio (width/height, adjusted for terminal characters)
    pub aspect: f32,
    /// Near clipping plane
    pub near: f32,
    /// Far clipping plane
    pub far: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 100.0,
            azimuth: std::f32::consts::FRAC_PI_4,   // 45 degrees
            elevation: std::f32::consts::FRAC_PI_6, // 30 degrees
            fov: 60.0,
            aspect: 1.0,
            near: 0.1,
            far: 10000.0,
        }
    }
}

impl OrbitCamera {
    /// Create a new orbit camera
    pub fn new() -> Self {
        Self::default()
    }

    /// Get camera position in world space
    pub fn position(&self) -> Vec3 {
        let x = self.distance * self.elevation.cos() * self.azimuth.sin();
        let y = self.distance * self.elevation.sin();
        let z = self.distance * self.elevation.cos() * self.azimuth.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Get the view matrix (world to camera space)
    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_at_rh(self.position(), self.target, Vec3::Y)
    }

    /// Get the projection matrix (camera to clip space)
    pub fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov.to_radians(), self.aspect, self.near, self.far)
    }

    /// Get combined view-projection matrix
    pub fn view_projection_matrix(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    /// Rotate the camera horizontally
    pub fn rotate_horizontal(&mut self, delta: f32) {
        self.azimuth += delta;
        // Keep in range [0, 2*PI]
        while self.azimuth < 0.0 {
            self.azimuth += std::f32::consts::TAU;
        }
        while self.azimuth > std::f32::consts::TAU {
            self.azimuth -= std::f32::consts::TAU;
        }
    }

    /// Rotate the camera vertically
    pub fn rotate_vertical(&mut self, delta: f32) {
        self.elevation += delta;
        // Clamp to avoid gimbal lock (slightly less than 90 degrees)
        self.elevation = self.elevation.clamp(-1.5, 1.5);
    }

    /// Zoom in/out by changing distance
    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).max(0.1);
    }

    /// Pan the camera target
    pub fn pan(&mut self, right: f32, up: f32) {
        // Calculate camera right and up vectors
        let forward = (self.target - self.position()).normalize();
        let world_up = Vec3::Y;
        let right_vec = forward.cross(world_up).normalize();
        let up_vec = right_vec.cross(forward).normalize();

        let pan_speed = self.distance * 0.01;
        self.target += right_vec * right * pan_speed;
        self.target += up_vec * up * pan_speed;
    }

    /// Fit the camera to show bounds
    pub fn fit_bounds(&mut self, min: Vec3, max: Vec3) {
        let center = (min + max) * 0.5;
        let size = max - min;
        let diagonal = size.length();

        self.target = center;

        // Calculate distance to fit the model
        let fov_rad = self.fov.to_radians();
        self.distance = diagonal / (2.0 * (fov_rad / 2.0).tan());
        self.distance = self.distance.max(1.0);

        // Set near/far based on model size
        self.near = diagonal * 0.001;
        self.far = diagonal * 10.0;
    }

    /// Reset to isometric view
    pub fn reset(&mut self) {
        self.azimuth = std::f32::consts::FRAC_PI_4;
        self.elevation = std::f32::consts::FRAC_PI_6;
    }

    /// Set aspect ratio based on terminal dimensions
    /// Terminal characters are typically ~2x tall, so we adjust
    pub fn set_terminal_aspect(&mut self, width: u16, height: u16) {
        // Characters are roughly 2:1 (height:width ratio)
        // So a 80x24 terminal has effective aspect of 80/(24*2) = 1.67
        self.aspect = width as f32 / (height as f32 * 2.0);
    }
}
