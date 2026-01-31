// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shading: convert framebuffer to block characters

use super::framebuffer::Framebuffer;
use glam::Vec3;

/// Block characters for different intensity levels
const BLOCKS: [char; 5] = [' ', '\u{2591}', '\u{2592}', '\u{2593}', '\u{2588}'];
// ' ', '░', '▒', '▓', '█'

/// Light direction (from top-right-front)
const LIGHT_DIR: Vec3 = Vec3::new(0.5, 0.8, 0.3);

/// Ambient light intensity
const AMBIENT: f32 = 0.3;

/// Convert framebuffer colors and depths to characters
pub fn shade_framebuffer(fb: &mut Framebuffer, _camera_pos: Vec3) {
    let light_dir = LIGHT_DIR.normalize();

    for y in 0..fb.height {
        for x in 0..fb.width {
            let idx = fb.index(x, y);

            // Skip background pixels - keep them as cleared (dots)
            if fb.depth[idx] >= 1.0 {
                continue;
            }

            let color = fb.color[idx];
            let normal = fb.normal[idx];

            // Calculate lighting (Lambert diffuse + ambient)
            let ndotl = normal.dot(light_dir).max(0.0);
            let intensity = AMBIENT + (1.0 - AMBIENT) * ndotl;

            // Apply lighting to color
            let lit_r = (color[0] * intensity).min(1.0);
            let lit_g = (color[1] * intensity).min(1.0);
            let lit_b = (color[2] * intensity).min(1.0);

            // Convert to 8-bit RGB
            let r = (lit_r * 255.0) as u8;
            let g = (lit_g * 255.0) as u8;
            let b = (lit_b * 255.0) as u8;
            fb.char_colors[idx] = [r, g, b];

            // Choose block character based on intensity
            let luminance = 0.299 * lit_r + 0.587 * lit_g + 0.114 * lit_b;
            let block_idx = ((luminance * 4.0) as usize).min(4);
            fb.chars[idx] = BLOCKS[block_idx];
        }
    }
}
