//! `FrameSet`: collection of named frames and the global span registry.

use crate::MarkId;
use crate::frame::{Frame, FrameRegistry};
use crate::span::{Span, SpanRegistry};

const DEFAULT_FRAME_NAME: &str = "LUDWIG";
const COMMAND_FRAME_NAME: &str = "COMMAND";
const HEAP_FRAME_NAME: &str = "HEAP";
const OOPS_FRAME_NAME: &str = "OOPS";
const SPECIAL_FRAME_NAMES: &[&str] = &[COMMAND_FRAME_NAME, HEAP_FRAME_NAME, OOPS_FRAME_NAME];

/// A collection of named [`Frame`]s plus the global [`SpanRegistry`].
pub struct FrameSet {
    /// Global frame registry — no two frames may share a name.
    frames: FrameRegistry,
    /// Global span registry — no two spans may share a name.
    spans: SpanRegistry,
    current_name: String,
    next_bound_id: u32,
}

impl FrameSet {
    /// Wrap an existing frame as the default / current frame.
    /// Fresh special frames are also created.
    pub fn new(main_frame: Frame) -> Self {
        let mut frames = FrameRegistry::new();
        frames.insert(DEFAULT_FRAME_NAME.into(), main_frame);
        for &name in SPECIAL_FRAME_NAMES {
            frames.insert(name.into(), Frame::new());
        }
        Self {
            frames,
            spans: SpanRegistry::new(),
            current_name: DEFAULT_FRAME_NAME.to_string(),
            next_bound_id: 0,
        }
    }

    /// Name of the current frame.
    pub fn current_name(&self) -> &str {
        &self.current_name
    }

    /// Name of the HEAP frame.
    pub fn heap_name(&self) -> &str {
        HEAP_FRAME_NAME
    }

    /// Immutable reference to the current frame.
    pub fn current_frame(&self) -> &Frame {
        self.frames
            .get(&self.current_name)
            .expect("current frame must exist")
    }

    /// Mutable reference to the current frame.
    pub fn current_frame_mut(&mut self) -> &mut Frame {
        self.frames
            .get_mut(&self.current_name)
            .expect("current frame must exist")
    }

    /// Mutable reference to the HEAP frame.
    pub fn heap_frame_mut(&mut self) -> &mut Frame {
        self.frames
            .get_mut(HEAP_FRAME_NAME)
            .expect("HEAP frame must exist")
    }

    /// Immutable reference to a frame by name.
    pub fn get_frame(&self, name: &str) -> Option<&Frame> {
        self.frames.get(&normalise(name))
    }

    /// Mutable reference to a frame by name.
    pub fn get_frame_mut(&mut self, name: &str) -> Option<&mut Frame> {
        self.frames.get_mut(&normalise(name))
    }

    /// Test whether a frame exists.
    pub fn contains_frame(&self, name: &str) -> bool {
        self.frames.contains(&normalise(name))
    }

    /// Look up a span by name (case-insensitive).
    pub fn get_span(&self, name: &str) -> Option<&Span> {
        self.spans.get(&normalise(name))
    }

    /// Look up a span by name (case-insensitive), mutable.
    pub fn get_span_mut(&mut self, name: &str) -> Option<&mut Span> {
        self.spans.get_mut(&normalise(name))
    }

    /// Insert or replace a span by name. Name is normalised to UPPERCASE.
    pub fn insert_span(&mut self, name: &str, span: Span) {
        self.spans.insert(normalise(name), span);
    }

    /// Remove a span by name, returning it if it existed.
    pub fn remove_span(&mut self, name: &str) -> Option<Span> {
        self.spans.remove(&normalise(name))
    }

    /// Test whether a span exists.
    pub fn contains_span(&self, name: &str) -> bool {
        self.spans.contains(&normalise(name))
    }

    /// Get a list of all span names, sorted case-insensitively.
    pub fn sorted_span_names(&self) -> Vec<&str> {
        self.spans.sorted_names()
    }

    /// Allocate two fresh `SpanBound` mark IDs. IDs are monotone and never reused.
    ///
    /// The returned `MarkId::SpanBound` values are NOT yet placed in the `MarkSet`;
    /// call [`set_mark_at`](Frame::set_mark_at) to record their positions.
    pub fn alloc_span_bounds(&mut self) -> (MarkId, MarkId) {
        let a = self.next_bound_id;
        self.next_bound_id += 1;
        let b = self.next_bound_id;
        self.next_bound_id += 1;
        (MarkId::SpanBound(a), MarkId::SpanBound(b))
    }
}

fn normalise(s: &str) -> String {
    s.to_uppercase()
}
