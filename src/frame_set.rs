//! `FrameSet`: collection of named frames and the global span registry.

use std::collections::HashMap;
use std::fmt;

use crate::MarkId;
use crate::code::{CompiledCode, ExecOutcome};
use crate::exec_context::ExecutionContext;
use crate::file_io::FileHandle;
use crate::frame::{Frame, FrameOptions, FrameRegistry, KeyboardMode, default_tab_stops};
use crate::interpreter;
use crate::screen::{BatchScreenBackend, ScreenBackend};
use crate::span::{Span, SpanRegistry};

pub const DEFAULT_FRAME_NAME: &str = "LUDWIG";
pub const COMMAND_FRAME_NAME: &str = "COMMAND";
pub const HEAP_FRAME_NAME: &str = "HEAP";
pub const OOPS_FRAME_NAME: &str = "OOPS";
pub const SPECIAL_FRAME_NAMES: &[&str] = &[COMMAND_FRAME_NAME, HEAP_FRAME_NAME, OOPS_FRAME_NAME];

/// Default parameters applied to newly-created frames.
/// Updated by EP with the `$` prefix.
#[derive(Debug, Clone)]
pub struct FrameDefaults {
    pub left_margin: usize,
    pub right_margin: usize,
    pub margin_top: usize,
    pub margin_bottom: usize,
    pub tab_stops: Vec<bool>,
    pub options: FrameOptions,
}

impl Default for FrameDefaults {
    fn default() -> Self {
        Self {
            left_margin: 0,
            right_margin: 79,
            margin_top: 0,
            margin_bottom: 0,
            tab_stops: default_tab_stops(),
            options: FrameOptions::default(),
        }
    }
}

/// A collection of named [`Frame`]s plus the global [`SpanRegistry`].
pub struct FrameSet {
    /// Global frame registry — no two frames may share a name.
    frames: FrameRegistry,
    /// Global span registry — no two spans may share a name.
    spans: SpanRegistry,
    current_name: String,
    next_bound_id: u32,
    /// Default parameters for newly-created frames (EP `$` prefix).
    pub defaults: FrameDefaults,
    /// Global keyboard mode (EP `K=`).
    pub keyboard_mode: KeyboardMode,
    /// Global input file (FGI) — shared across all frames.
    pub global_input: Option<FileHandle>,
    /// Global output file (FGO) — shared across all frames.
    pub global_output: Option<FileHandle>,
    /// Set to `true` by the `Q` command to signal that the editor should exit.
    pub quit_requested: bool,
    /// Set to `true` by the `UP` command to request suspension (interactive only).
    pub suspend_requested: bool,
    /// Set to `true` by the `US` command to request a subprocess shell (interactive only).
    pub subprocess_requested: bool,
    /// User-defined key bindings installed by the `UK` command.
    /// Key: canonical key name (e.g. "UP-ARROW", "a", "F1").
    /// Value: compiled procedure to execute when the key is pressed.
    pub user_key_bindings: HashMap<String, CompiledCode>,
}

impl FrameSet {
    /// Create a new empty frame set with the default main frame.
    pub fn empty() -> Self {
        Self::new(Frame::new(DEFAULT_FRAME_NAME))
    }

    /// Create a frame set from an initial buffer string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self::new(Frame::from_str(DEFAULT_FRAME_NAME, s))
    }

    /// Wrap an existing frame as the default / current frame.
    /// Fresh special frames are also created.
    pub fn new(main_frame: Frame) -> Self {
        let mut frames = FrameRegistry::new();
        let main_name = main_frame.name().to_string();
        frames.insert(main_name.clone(), main_frame);
        for &name in SPECIAL_FRAME_NAMES {
            frames.insert(name.into(), Frame::new(name));
        }
        Self {
            frames,
            spans: SpanRegistry::new(),
            current_name: main_name,
            next_bound_id: 0,
            defaults: FrameDefaults::default(),
            keyboard_mode: KeyboardMode::default(),
            global_input: None,
            global_output: None,
            quit_requested: false,
            suspend_requested: false,
            subprocess_requested: false,
            user_key_bindings: HashMap::new(),
        }
    }

    /// Create a new frame whose parameters are initialised from `defaults`.
    pub fn create_frame_from_defaults(&self, name: &str) -> Frame {
        let mut f = Frame::new(name);
        f.left_margin = self.defaults.left_margin;
        f.right_margin = self.defaults.right_margin;
        f.margin_top = self.defaults.margin_top;
        f.margin_bottom = self.defaults.margin_bottom;
        f.tab_stops = self.defaults.tab_stops.clone();
        f.options = self.defaults.options.clone();
        f
    }

    /// Set the current frame name directly (used by ED/ER).
    pub fn set_current_name(&mut self, name: String) {
        self.current_name = name;
    }

    /// Insert a frame by name (name is normalised to UPPERCASE).
    pub fn insert_frame(&mut self, name: &str, frame: Frame) {
        self.frames.insert(normalise(name), frame);
    }

    /// Remove a frame by name. Returns the removed frame if it existed.
    pub fn remove_frame(&mut self, name: &str) -> Option<Frame> {
        self.frames.remove(&normalise(name))
    }

    /// Return all frame names (for iterating to fix return pointers in EK).
    pub fn frame_names(&self) -> Vec<String> {
        self.frames.names()
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

    /// Check whether the current frame has been modified.
    pub fn modified(&self) -> bool {
        self.current_frame().get_mark(MarkId::Modified).is_some()
    }

    /// Execute compiled code with a caller-provided screen backend.
    ///
    /// Use this in interactive mode to inject the `InteractiveScreenBackend`
    /// so window commands update the viewport and output goes to the screen.
    pub fn execute_with_screen(
        &mut self,
        code: &CompiledCode,
        screen: &mut dyn ScreenBackend,
    ) -> ExecOutcome {
        let mut ctx = ExecutionContext::new(self, screen);
        interpreter::execute(&mut ctx, code)
    }

    /// Execute compiled code in batch mode (no-op screen backend, output to stdout).
    ///
    /// All existing callers and tests use this convenience wrapper.
    pub fn execute(&mut self, code: &CompiledCode) -> ExecOutcome {
        let mut screen = BatchScreenBackend;
        self.execute_with_screen(code, &mut screen)
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

    /// Get a list of all frame names, sorted case-insensitively.
    pub fn sorted_frame_names(&self) -> Vec<&str> {
        self.frames.sorted_names()
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

impl fmt::Display for FrameSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.current_frame())
    }
}

#[cfg(test)]
#[path = "frame_set/integration_tests.rs"]
mod integration_tests;
