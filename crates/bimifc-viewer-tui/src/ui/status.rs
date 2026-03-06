// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Status bar

use crate::renderer::FloorPlanStats;
use crate::ui::viewport::ViewMode;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Widget,
};

/// Status bar widget
pub struct StatusBar {
    pub fps: f32,
    pub stats: FloorPlanStats,
    pub view_mode: ViewMode,
    pub message: Option<String>,
}

impl StatusBar {
    pub fn new(fps: f32, stats: FloorPlanStats, view_mode: ViewMode) -> Self {
        Self {
            fps,
            stats,
            view_mode,
            message: None,
        }
    }

    pub fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear background
        for x in area.x..area.right() {
            buf[(x, area.y)]
                .set_char(' ')
                .set_bg(Color::Rgb(30, 30, 45));
        }

        let mut spans = vec![
            Span::styled(
                format!(" [{}]", self.view_mode.label()),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("FPS: {:.0}", self.fps),
                Style::default().fg(Color::Green),
            ),
        ];

        // Show level info for floor plan modes
        if self.view_mode != ViewMode::Iso3D {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(
                format!(
                    "Level {}/{}",
                    self.stats.current_level + 1,
                    self.stats.level_count.max(1)
                ),
                Style::default().fg(Color::Yellow),
            ));
        }

        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));

        // Controls help
        spans.push(Span::styled("[V]", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw("view "));
        spans.push(Span::styled("[+-]", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw("zoom "));
        spans.push(Span::styled(
            if self.view_mode == ViewMode::Iso3D {
                "[\u{2190}\u{2191}\u{2192}\u{2193}]"
            } else {
                "[WASD]"
            },
            Style::default().fg(Color::Yellow),
        ));
        spans.push(Span::raw(if self.view_mode == ViewMode::Iso3D {
            "orbit "
        } else {
            "pan "
        }));
        spans.push(Span::styled("[Tab]", Style::default().fg(Color::Yellow)));
        spans.push(Span::raw("focus "));
        spans.push(Span::styled("[Q]", Style::default().fg(Color::Red)));
        spans.push(Span::raw("quit"));

        if let Some(msg) = self.message {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(msg, Style::default().fg(Color::Magenta)));
        }

        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}
