//! `ExecutionContext`: wraps `FrameSet` with interpreter state.
//!
//! This is the main "environment" passed through the interpreter.
//! Using a context type (rather than a bare `&mut Frame`) lets span commands
//! reach across frames and lets us track recursion depth.

use crate::frame::Frame;
use crate::frame_set::FrameSet;
use crate::screen::ScreenBackend;

mod file_cmds;
mod frame_cmds;
mod span_cmds;

pub(crate) use span_cmds::parse_span_name;

/// The execution environment for the Ludwig interpreter.
pub(crate) struct ExecutionContext<'a> {
    /// The set of all frames and the global span registry.
    pub(crate) frame_set: &'a mut FrameSet,
    /// Current EX/EN nesting depth; capped at [`MAX_RECURSION_DEPTH`].
    pub(crate) recursion_depth: u32,
    /// Display backend for window commands and output messages.
    pub(crate) screen: &'a mut dyn ScreenBackend,
}

/// Maximum allowed EX/EN recursion depth (spec section 9.8).
pub(crate) const MAX_RECURSION_DEPTH: u32 = 100;

impl<'a> ExecutionContext<'a> {
    pub(crate) fn new(frame_set: &'a mut FrameSet, screen: &'a mut dyn ScreenBackend) -> Self {
        Self {
            frame_set,
            recursion_depth: 0,
            screen,
        }
    }

    /// Immutable reference to the current frame.
    pub(crate) fn current_frame(&self) -> &Frame {
        self.frame_set.current_frame()
    }

    /// Mutable reference to the current frame.
    pub(crate) fn current_frame_mut(&mut self) -> &mut Frame {
        self.frame_set.current_frame_mut()
    }
}
