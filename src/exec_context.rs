//! `ExecutionContext`: wraps `FrameSet` with interpreter state.
//!
//! This is the main "environment" passed through the interpreter.
//! Using a context type (rather than a bare `&mut Frame`) lets span commands
//! reach across frames and lets future phases (Phase 7) track recursion depth.

use crate::frame::Frame;
use crate::frame_set::FrameSet;

/// The execution environment for the Ludwig interpreter.
pub struct ExecutionContext<'a> {
    /// The set of all frames and the global span registry.
    pub frame_set: &'a mut FrameSet,
    /// Recursion depth counter (used in Phase 7 for EX/EN limit of 100).
    pub recursion_depth: u32,
}

impl<'a> ExecutionContext<'a> {
    pub fn new(frame_set: &'a mut FrameSet) -> Self {
        Self {
            frame_set,
            recursion_depth: 0,
        }
    }

    /// Immutable reference to the current frame.
    pub fn current_frame(&self) -> &Frame {
        self.frame_set.current_frame()
    }

    /// Mutable reference to the current frame.
    pub fn current_frame_mut(&mut self) -> &mut Frame {
        self.frame_set.current_frame_mut()
    }
}
