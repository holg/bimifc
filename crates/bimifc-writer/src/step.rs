//! STEP entity builder — manages entity IDs and collects STEP data lines.

use std::fmt::Write;

/// Allocates sequential STEP entity IDs (#1, #2, ...).
pub struct IdAlloc {
    next: u32,
}

impl IdAlloc {
    pub fn new() -> Self {
        Self { next: 1 }
    }

    pub fn next(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
}

/// Accumulates STEP DATA section lines.
pub struct StepBuilder {
    lines: Vec<String>,
}

impl StepBuilder {
    pub fn new() -> Self {
        Self {
            lines: Vec::with_capacity(512),
        }
    }

    /// Convenience: format and push.
    pub fn entity(&mut self, id: u32, content: &str) {
        self.lines.push(format!("#{id}={content};"));
    }

    /// Consume and return all lines joined with newlines.
    pub fn into_data_section(self) -> String {
        self.lines.join("\n")
    }
}

/// Escape a string for STEP encoding (double single-quotes, escape backslashes).
pub fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "''")
}

/// Format a list of entity references: (#1,#2,#3)
pub fn ref_list(ids: &[u32]) -> String {
    let mut out = String::from("(");
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write!(out, "#{id}").unwrap();
    }
    out.push(')');
    out
}
