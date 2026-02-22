//! `FrameSet`: collection of named frames and the global span registry.
//!
//! Phase 5 initialises two frames ("LUDWIG" and "HEAP"). Phase 8 will extend
//! this with frame switching, a frame stack, and additional special frames.

use std::collections::HashMap;

use crate::frame::Frame;
use crate::span::SpanRegistry;

const DEFAULT_FRAME_NAME: &str = "LUDWIG";
const COMMAND_FRAME_NAME: &str = "COMMAND";
const HEAP_FRAME_NAME: &str = "HEAP";
const OOPS_FRAME_NAME: &str = "OOPS";
const SPECIAL_FRAME_NAMES: &[&str] = &[
    COMMAND_FRAME_NAME,
    HEAP_FRAME_NAME,
    OOPS_FRAME_NAME,
];

/// A collection of named [`Frame`]s plus the global [`SpanRegistry`].
pub struct FrameSet {
    frames: HashMap<String, Frame>,
    /// Global span registry — no two spans may share a name.
    pub spans: SpanRegistry,
    current_name: String,
}

impl FrameSet {
    /// Wrap an existing frame as the current ("LUDWIG") frame.
    /// A fresh empty HEAP frame is also created.
    pub fn new(main_frame: Frame) -> Self {
        let mut frames = HashMap::new();
        frames.insert(DEFAULT_FRAME_NAME.to_string(), main_frame);
        for &name in SPECIAL_FRAME_NAMES {
            frames.insert(name.to_string(), Frame::new());
        }
        Self {
            frames,
            spans: SpanRegistry::new(),
            current_name: DEFAULT_FRAME_NAME.to_string(),
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
        self.frames.get(&self.current_name)
            .expect("current frame must exist")
    }

    /// Mutable reference to the current frame.
    pub fn current_frame_mut(&mut self) -> &mut Frame {
        self.frames.get_mut(&self.current_name)
            .expect("current frame must exist")
    }

    /// Mutable reference to the HEAP frame.
    pub fn heap_frame_mut(&mut self) -> &mut Frame {
        self.frames.get_mut(HEAP_FRAME_NAME)
            .expect("HEAP frame must exist")
    }

    /// Immutable reference to a frame by name.
    pub fn get_frame(&self, name: &str) -> Option<&Frame> {
        self.frames.get(name)
    }

    /// Mutable reference to a frame by name.
    pub fn get_frame_mut(&mut self, name: &str) -> Option<&mut Frame> {
        self.frames.get_mut(name)
    }
}
