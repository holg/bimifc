// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Framebuffer for software rendering

use glam::Vec3;

/// Framebuffer with color, depth, and character output buffers
pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    /// Depth buffer (0=near, 1=far)
    pub depth: Vec<f32>,
    /// RGBA color per pixel
    pub color: Vec<[f32; 4]>,
    /// Normal at each pixel (for lighting)
    pub normal: Vec<Vec3>,
    /// Output characters for display
    pub chars: Vec<char>,
    /// RGB color for each character (ANSI true color)
    pub char_colors: Vec<[u8; 3]>,
    /// Entity ID at each pixel (for picking)
    pub entity_ids: Vec<u64>,
}

impl Framebuffer {
    /// Create a new framebuffer with given dimensions
    pub fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        Self {
            width,
            height,
            depth: vec![1.0; size],
            color: vec![[0.0, 0.0, 0.0, 0.0]; size],
            normal: vec![Vec3::ZERO; size],
            chars: vec![' '; size],
            char_colors: vec![[0, 0, 0]; size],
            entity_ids: vec![0; size],
        }
    }

    /// Resize the framebuffer
    pub fn resize(&mut self, width: usize, height: usize) {
        if self.width == width && self.height == height {
            return;
        }
        let size = width * height;
        self.width = width;
        self.height = height;
        self.depth.resize(size, 1.0);
        self.color.resize(size, [0.0, 0.0, 0.0, 0.0]);
        self.normal.resize(size, Vec3::ZERO);
        self.chars.resize(size, ' ');
        self.char_colors.resize(size, [0, 0, 0]);
        self.entity_ids.resize(size, 0);
        self.clear();
    }

    /// Clear the framebuffer
    pub fn clear(&mut self) {
        self.depth.fill(1.0);
        self.color.fill([0.1, 0.1, 0.15, 1.0]); // Dark background
        self.normal.fill(Vec3::ZERO);
        self.chars.fill('\u{00B7}'); // Middle dot as background
        self.char_colors.fill([40, 40, 50]); // Slightly visible background
        self.entity_ids.fill(0);
    }

    /// Get pixel index
    #[inline]
    pub fn index(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    /// Set pixel if closer than current depth
    #[inline]
    pub fn set_pixel(
        &mut self,
        x: usize,
        y: usize,
        depth: f32,
        color: [f32; 4],
        normal: Vec3,
        entity_id: u64,
    ) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }

        let idx = self.index(x, y);
        if depth < self.depth[idx] {
            self.depth[idx] = depth;
            self.color[idx] = color;
            self.normal[idx] = normal;
            self.entity_ids[idx] = entity_id;
            true
        } else {
            false
        }
    }

    /// Get entity at screen position
    pub fn entity_at(&self, x: usize, y: usize) -> Option<u64> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = self.index(x, y);
        let id = self.entity_ids[idx];
        if id > 0 {
            Some(id)
        } else {
            None
        }
    }
}
