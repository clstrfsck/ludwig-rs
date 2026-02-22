//! Span subsystem: named text regions bounded by live marks.
//!
//! Spans are global (no two spans share a name). The registry lives on
//! `FrameSet`, not on individual frames. Each span records which frame owns
//! its boundary marks so they can be resolved at runtime.

use std::collections::HashMap;

use crate::code::CompiledCode;
use crate::marks::MarkId;

/// A named text region in a frame, bounded by two live `SpanBound` marks.
pub struct Span {
    /// Name of the frame whose `MarkSet` holds the boundary marks.
    pub frame_name: String,
    /// Earlier boundary mark (`SpanBound` variant).
    pub mark_start: MarkId,
    /// Later boundary mark (`SpanBound` variant).
    pub mark_end: MarkId,
    /// Cached compiled body, populated by SR and used by EX/EN (Phase 7).
    pub code: Option<CompiledCode>,
}

/// Global registry of all spans, keyed by UPPERCASE name.
pub struct SpanRegistry {
    spans: HashMap<String, Span>,
}

impl Default for SpanRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SpanRegistry {
    pub fn new() -> Self {
        Self {
            spans: HashMap::new(),
        }
    }

    /// Insert or replace a span by name. Name is normalised to UPPERCASE.
    pub fn insert(&mut self, name: &str, span: Span) {
        self.spans.insert(normalise(name), span);
    }

    /// Look up a span by name (case-insensitive). O(1).
    pub fn get(&self, name: &str) -> Option<&Span> {
        self.spans.get(&normalise(name))
    }

    /// Mutable look-up by name (case-insensitive). O(1).
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Span> {
        self.spans.get_mut(&normalise(name))
    }

    /// Remove and return a span by name.
    pub fn remove(&mut self, name: &str) -> Option<Span> {
        self.spans.remove(&normalise(name))
    }

    /// Test whether a span exists.
    pub fn contains(&self, name: &str) -> bool {
        self.spans.contains_key(&normalise(name))
    }

    /// Return all span names in alphabetical order (for SI display).
    pub fn sorted_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.spans.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

fn normalise(s: &str) -> String {
    s.to_uppercase()
}
