// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! 3D viewport widget

use crate::renderer::Framebuffer;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};

/// 3D viewport widget that displays the framebuffer
pub struct Viewport<'a> {
    framebuffer: &'a Framebuffer,
    focused: bool,
}

impl<'a> Viewport<'a> {
    pub fn new(framebuffer: &'a Framebuffer) -> Self {
        Self {
            framebuffer,
            focused: false,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
}

impl Widget for Viewport<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Draw border
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" 3D Viewport ");

        let inner = block.inner(area);
        block.render(area, buf);

        // Render framebuffer contents
        let fb_width = self.framebuffer.width;
        let fb_height = self.framebuffer.height;

        for y in 0..inner.height as usize {
            for x in 0..inner.width as usize {
                if x >= fb_width || y >= fb_height {
                    continue;
                }

                let idx = y * fb_width + x;
                let ch = self.framebuffer.chars[idx];
                let [r, g, b] = self.framebuffer.char_colors[idx];

                let cell_x = inner.x + x as u16;
                let cell_y = inner.y + y as u16;

                if cell_x < area.right() && cell_y < area.bottom() {
                    buf[(cell_x, cell_y)]
                        .set_char(ch)
                        .set_fg(Color::Rgb(r, g, b));
                }
            }
        }
    }
}
