// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Status bar

use crate::renderer::FloorPlanStats;
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
    pub message: Option<String>,
}

impl StatusBar {
    pub fn new(fps: f32, stats: FloorPlanStats) -> Self {
        Self {
            fps,
            stats,
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

        // Build status line
        let mut spans = vec![
            Span::styled(
                format!(" FPS: {:.0}", self.fps),
                Style::default().fg(Color::Green),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!(
                    "Level {}/{}",
                    self.stats.current_level + 1,
                    self.stats.level_count.max(1)
                ),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                format!(
                    " ({} edges, {} px)",
                    format_number(self.stats.visible_edges),
                    format_number(self.stats.pixels_drawn),
                ),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        ];

        // Controls help
        spans.push(Span::styled(
            "[PgUp/Dn]",
            Style::default().fg(Color::Yellow),
        ));
        spans.push(Span::raw("level "));
        spans.push(Span::styled(
            "[+-]",
            Style::default().fg(Color::Yellow),
        ));
        spans.push(Span::raw("zoom "));
        spans.push(Span::styled(
            "[WASD]",
            Style::default().fg(Color::Yellow),
        ));
        spans.push(Span::raw("pan "));
        spans.push(Span::styled("[Q]", Style::default().fg(Color::Red)));
        spans.push(Span::raw("quit"));

        // Message if any
        if let Some(msg) = self.message {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(msg, Style::default().fg(Color::Magenta)));
        }

        let line = Line::from(spans);
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Format a number with K/M suffix
fn format_number(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
