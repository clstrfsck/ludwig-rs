//! `ExecutionContext`: wraps `FrameSet` with interpreter state.
//!
//! This is the main "environment" passed through the interpreter.
//! Using a context type (rather than a bare `&mut Frame`) lets span commands
//! reach across frames and lets future phases (Phase 7) track recursion depth.

use crate::file_io;
use crate::frame::Frame;
use crate::frame::params::{EpDirective, EpKeyboardMode, EpOptionFlag, EpTabOp, parse_ep};
use crate::frame::{KeyboardMode, default_tab_stops};
use crate::frame_set::{FrameSet, SPECIAL_FRAME_NAMES};
use crate::marks::NUMBERED_MARK_RANGE;
use crate::span::Span;

use crate::{CmdFailure, CmdResult, LeadParam, MarkId, Position, TrailParam, compile};

/// The execution environment for the Ludwig interpreter.
pub(crate) struct ExecutionContext<'a> {
    /// The set of all frames and the global span registry.
    pub(crate) frame_set: &'a mut FrameSet,
    /// Current EX/EN nesting depth; capped at [`MAX_RECURSION_DEPTH`].
    pub(crate) recursion_depth: u32,
}

/// Maximum allowed EX/EN recursion depth (spec section 9.8).
pub(crate) const MAX_RECURSION_DEPTH: u32 = 100;

impl<'a> ExecutionContext<'a> {
    pub(crate) fn new(frame_set: &'a mut FrameSet) -> Self {
        Self {
            frame_set,
            recursion_depth: 0,
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

    /// SD — Span Define
    ///
    /// `[lead]SD/name/`
    /// Defines a span bounded by dot and another mark.
    /// Lead: None/Plus → mark 1; Pint(n) → mark n; Marker(Equals) → Equals mark.
    pub(crate) fn cmd_span_define(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        let span_name = match parse_span_name(&tpars[0]) {
            Some(n) => n,
            None => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        if self.frame_set.contains_frame(&span_name) {
            return CmdResult::Failure(CmdFailure::FrameExists);
        }

        // Resolve the second boundary mark ID.
        let second_mark = match lead {
            LeadParam::None | LeadParam::Plus => MarkId::Numbered(1),
            LeadParam::Pint(n) => {
                let n = match u8::try_from(n) {
                    Ok(v) if NUMBERED_MARK_RANGE.contains(&v) => v,
                    _ => return CmdResult::Failure(CmdFailure::SyntaxError),
                };
                MarkId::Numbered(n)
            }
            LeadParam::Marker(id) => id,
            LeadParam::Minus => {
                if let Some(old_span) = self.frame_set.remove_span(&span_name)
                    && let Some(old_frame) = self.frame_set.get_frame_mut(&old_span.frame_name)
                {
                    old_frame.unset_mark(old_span.mark_start);
                    old_frame.unset_mark(old_span.mark_end);
                    return CmdResult::Success;
                } else {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Get both positions from the current frame.
        let dot_pos = self.current_frame().dot();
        let other_pos = match self.current_frame().get_mark(second_mark) {
            Some(p) => p,
            None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
        };

        let (pos_start, pos_end) = if dot_pos <= other_pos {
            (dot_pos, other_pos)
        } else {
            (other_pos, dot_pos)
        };

        let current_frame_name = self.frame_set.current_name().to_string();

        // If a span with this name already exists, release its old bounds.
        if let Some(old_span) = self.frame_set.remove_span(&span_name)
            && let Some(old_frame) = self.frame_set.get_frame_mut(&old_span.frame_name)
        {
            old_frame.unset_mark(old_span.mark_start);
            old_frame.unset_mark(old_span.mark_end);
        }

        // Allocate new bounds in the current frame.
        let (id_start, id_end) = self.frame_set.alloc_span_bounds();
        self.frame_set
            .current_frame_mut()
            .set_mark_at(id_start, pos_start);
        self.frame_set
            .current_frame_mut()
            .set_mark_at(id_end, pos_end);

        self.frame_set
            .insert_span(&span_name, Span::new(current_frame_name, id_start, id_end));

        CmdResult::Success
    }

    /// SC — Span Copy
    ///
    /// `nSC/name/`
    /// Copies the span's text into the current frame at dot, N times (default 1).
    pub(crate) fn cmd_span_copy(&mut self, lead: LeadParam, tpar: &TrailParam) -> CmdResult {
        let span_or_frame_name = match parse_span_name(tpar) {
            Some(n) => n,
            None => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        let count = match lead {
            LeadParam::None | LeadParam::Plus => 1usize,
            LeadParam::Pint(n) => n,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Extract span text (ends the borrow on ctx.frame_set before the insert).
        if let Some(text) = self.read_span_or_frame_text(&span_or_frame_name) {
            for _ in 0..count {
                let dot = self.current_frame().dot();
                self.current_frame_mut().insert_at(dot, &text);
            }
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::SyntaxError)
        }
    }

    /// ST — Span Transfer
    ///
    /// `ST/name/`
    /// Moves the span's text from its source frame into the current frame at dot.
    /// The span's source region is deleted; the span marks collapse to the same point.
    pub(crate) fn cmd_span_transfer(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        let span_name = match parse_span_name(&tpars[0]) {
            Some(n) => n,
            None => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Collect all needed info before any mutation.
        let (text, src_frame_name, src_mark_start, src_mark_end, src_from, src_to) = {
            let span = match self.frame_set.get_span(&span_name) {
                Some(s) => s,
                None => return CmdResult::Failure(CmdFailure::OutOfRange),
            };
            let frame_name = span.frame_name.clone();
            let mark_start = span.mark_start;
            let mark_end = span.mark_end;
            let frame = match self.frame_set.get_frame(&frame_name) {
                Some(f) => f,
                None => return CmdResult::Failure(CmdFailure::OutOfRange),
            };
            let from = match frame.get_mark(mark_start) {
                Some(p) => p,
                None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
            };
            let to = match frame.get_mark(mark_end) {
                Some(p) => p,
                None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
            };
            let txt = {
                let start_idx = frame.to_char_index(&from);
                let end_idx = frame.to_char_index(&to);
                if start_idx < end_idx {
                    frame.slice(start_idx..end_idx)
                } else {
                    String::new()
                }
            };
            (txt, frame_name, mark_start, mark_end, from, to)
        };

        // Delete from source frame.
        {
            let src_frame = match self.frame_set.get_frame_mut(&src_frame_name) {
                Some(f) => f,
                None => return CmdResult::Failure(CmdFailure::OutOfRange),
            };
            src_frame.delete(src_from, src_to);
            // After delete, both src_mark_start and src_mark_end are now at src_from.
            // Reset them explicitly to be sure.
            src_frame.set_mark_at(src_mark_start, src_from);
            src_frame.set_mark_at(src_mark_end, src_from);
        }

        // Insert at current frame dot.
        let dot = self.current_frame().dot();
        self.current_frame_mut().insert_at(dot, &text);

        CmdResult::Success
    }

    /// SJ — Span Jump
    ///
    /// `[±]SJ/name/`
    /// Moves dot to the end (None/Plus) or start (Minus) of the span.
    /// Fails if the span is in a different frame.
    pub(crate) fn cmd_span_jump(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        let span_name = match parse_span_name(&tpars[0]) {
            Some(n) => n,
            None => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        let goto_end = match lead {
            LeadParam::None | LeadParam::Plus => true,
            LeadParam::Minus => false,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        let (target_pos, span_frame_name) = {
            let span = match self.frame_set.get_span(&span_name) {
                Some(s) => s,
                None => return CmdResult::Failure(CmdFailure::OutOfRange),
            };
            let frame = match self.frame_set.get_frame(&span.frame_name) {
                Some(f) => f,
                None => return CmdResult::Failure(CmdFailure::OutOfRange),
            };
            let mark = if goto_end {
                span.mark_end
            } else {
                span.mark_start
            };
            let pos = match frame.get_mark(mark) {
                Some(p) => p,
                None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
            };
            (pos, span.frame_name.clone())
        };

        // SJ only works when the span is in the current frame.
        if span_frame_name != self.frame_set.current_name() {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }

        let old_dot = self.current_frame().dot();
        self.current_frame_mut()
            .set_mark_at(MarkId::Equals, old_dot);
        self.current_frame_mut().set_dot(target_pos);

        CmdResult::Success
    }

    /// SA — Span Assign
    ///
    /// `SA/name/value/`   — assign literal text to span
    /// `SA$name$refspan$` — assign contents of another span to this span
    ///
    /// If the span does not exist, it is created in the HEAP frame.
    /// If it exists, its content is replaced in-place.
    pub(crate) fn cmd_span_assign(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        let span_name = match parse_span_name(&tpars[0]) {
            Some(n) => n,
            None => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Resolve value: span dereference if delim == '$', otherwise literal.
        let value = if tpars[1].delim == '$' {
            let ref_name = tpars[1].content.trim().to_uppercase();
            match self.read_span_or_frame_text(&ref_name) {
                Some(t) => t,
                None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
            }
        } else {
            tpars[1].content.clone()
        };

        // Check whether the target span already exists.
        if self.frame_set.contains_span(&span_name) {
            // Update existing span in-place.
            let (frame_name, mark_start, mark_end, from, _to) = {
                let span = self.frame_set.get_span(&span_name).unwrap();
                let fname = span.frame_name.clone();
                let ms = span.mark_start;
                let me = span.mark_end;
                let frame = self.frame_set.get_frame(&fname).unwrap();
                let from = match frame.get_mark(ms) {
                    Some(p) => p,
                    None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
                };
                let to = match frame.get_mark(me) {
                    Some(p) => p,
                    None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
                };
                (fname, ms, me, from, to)
            };

            {
                let frame = self.frame_set.get_frame_mut(&frame_name).unwrap();
                // Get fresh to after resolving above
                let to = frame.get_mark(mark_end).unwrap();
                frame.delete(from, to);
                frame.insert_at(from, &value);
                // After delete+insert both marks end up past the new text.
                // Reset mark_start back to 'from'.
                frame.set_mark_at(mark_start, from);
                // mark_end is already correct (it's at from + text_length).
            }
        } else {
            // Create new span in HEAP.
            let heap_name = self.frame_set.heap_name().to_string();
            let id_start;
            let id_end;

            {
                (id_start, id_end) = self.frame_set.alloc_span_bounds();
                let hf = self.frame_set.heap_frame_mut();
                let line_count = hf.line_count();
                let last_line = if line_count == 0 {
                    0
                } else {
                    line_count.saturating_sub(1)
                };
                let last_col = hf.line_length_including_newline(last_line);
                let insert_pos = Position::new(last_line, last_col);
                // Insert span text followed by a newline separator.
                hf.insert_at(insert_pos, &format!("{}\n", value));
                // mark_end points just before the separator newline.
                let mark_end_pos = insert_pos.after_text(&value);
                hf.set_mark_at(id_start, insert_pos);
                hf.set_mark_at(id_end, mark_end_pos);
            }

            self.frame_set
                .insert_span(&span_name, Span::new(heap_name, id_start, id_end));
        }

        CmdResult::Success
    }

    /// SI — Span Index
    ///
    /// Prints a listing of all spans and frames to stdout.
    /// (Interactive display deferred to Phase 10.)
    pub(crate) fn cmd_span_index(&mut self) -> CmdResult {
        let names = self.frame_set.sorted_span_names();
        for name in names {
            let preview = match self.read_span_or_frame_text(name) {
                Some(t) => {
                    let first_line = t.lines().next().unwrap_or("").to_string();
                    if first_line.len() > 31 || t.contains('\n') {
                        format!("{}...", &first_line[..first_line.len().min(28)])
                    } else {
                        first_line
                    }
                }
                None => String::from("<undefined>"),
            };
            println!("{:<32} {}", name, preview);
        }
        CmdResult::Success
    }

    /// SR — Span Recompile
    ///
    /// `SR/name/`
    /// Compiles the span's text as a Ludwig command string and caches the result.
    pub(crate) fn cmd_span_compile(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        let span_name = match parse_span_name(&tpars[0]) {
            Some(n) => n,
            None => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Read the span's text.
        let text = match self.read_span_or_frame_text(&span_name) {
            Some(t) => t,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };

        // Compile it.
        let compiled = match compile(&text) {
            Ok(c) => c,
            Err(_) => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Store the compiled code on the span.
        if let Some(span) = self.frame_set.get_span_mut(&span_name) {
            span.set_code(compiled);
            CmdResult::Success
        } else if let Some(frame) = self.frame_set.get_frame_mut(&span_name) {
            frame.set_code(compiled);
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::OutOfRange)
        }
    }

    // ─── Frame navigation commands ────────────────────────────────────────────

    /// ED — Edit Frame
    ///
    /// `ED/name/` — switch to an existing frame or create a new one.
    /// Empty name defaults to "LUDWIG".
    /// Sets the target frame's `return_frame_name` to the current frame.
    pub(crate) fn cmd_frame_edit(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        let raw = tpars[0].content.trim().to_uppercase();
        let target_name = if raw.is_empty() {
            "LUDWIG".to_string()
        } else {
            raw
        };

        // Fail if a span (not a frame) has this name.
        if self.frame_set.contains_span(&target_name) {
            return CmdResult::Failure(CmdFailure::SpanOfThatNameExists);
        }

        let current_name = self.frame_set.current_name().to_string();

        if self.frame_set.contains_frame(&target_name) {
            // Already on this frame — no-op.
            if target_name == current_name {
                return CmdResult::Success;
            }
            // Switch to the existing frame.
            if let Some(f) = self.frame_set.get_frame_mut(&target_name) {
                f.return_frame_name = Some(current_name);
            }
            self.frame_set.set_current_name(target_name);
        } else {
            // Create a new frame initialised from defaults.
            let mut new_frame = self.frame_set.create_frame_from_defaults(&target_name);
            new_frame.return_frame_name = Some(current_name);
            self.frame_set.insert_frame(&target_name, new_frame);
            self.frame_set.set_current_name(target_name);
        }

        CmdResult::Success
    }

    /// EK — Edit Kill
    ///
    /// `EK/name/` — destroy a named frame (must not be current or special).
    pub(crate) fn cmd_frame_kill(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        let raw = tpars[0].content.trim().to_uppercase();
        if raw.is_empty() {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if !self.frame_set.contains_frame(&raw) {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }

        // Can't kill the current frame.
        if raw == self.frame_set.current_name() {
            return CmdResult::Failure(CmdFailure::CantKillFrame);
        }

        // Can't kill special frames.
        if SPECIAL_FRAME_NAMES.contains(&raw.as_str()) {
            return CmdResult::Failure(CmdFailure::CantKillFrame);
        }

        // Remove all spans whose frame_name == target.
        let span_names_to_remove: Vec<String> = self
            .frame_set
            .sorted_span_names()
            .into_iter()
            .filter(|n| {
                self.frame_set
                    .get_span(n)
                    .is_some_and(|s| s.frame_name == raw)
            })
            .map(|s| s.to_string())
            .collect();

        for span_name in &span_names_to_remove {
            if let Some(span) = self.frame_set.remove_span(span_name) {
                // Marks live in the frame being killed — no need to unset them.
                let _ = span;
            }
        }

        // Fix return_frame_name in all surviving frames.
        let all_names = self.frame_set.frame_names();
        for name in &all_names {
            if name == &raw {
                continue;
            }
            if let Some(f) = self.frame_set.get_frame_mut(name)
                && f.return_frame_name.as_deref() == Some(&raw)
            {
                f.return_frame_name = None;
            }
        }

        // Remove the frame itself, cleaning up any temp output file.
        if let Some(frame) = self.frame_set.remove_frame(&raw)
            && let Some(ref fh) = frame.output_file
        {
            file_io::delete_temp(fh);
        }

        CmdResult::Success
    }

    /// ER — Edit Return
    ///
    /// `ER` / `NER` — return N levels up the frame-invocation chain.
    pub(crate) fn cmd_frame_return(&mut self, lead: LeadParam) -> CmdResult {
        let count = match lead {
            LeadParam::None | LeadParam::Plus => 1usize,
            LeadParam::Pint(n) => n,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        for _ in 0..count {
            let return_name = self.frame_set.current_frame().return_frame_name.clone();
            match return_name {
                None => return CmdResult::Failure(CmdFailure::NoReturnFrame),
                Some(name) => self.frame_set.set_current_name(name),
            }
        }

        CmdResult::Success
    }

    /// EP — Edit Parameters
    ///
    /// `EP'params'` — view or change editor parameters.
    /// Empty string is a no-op (display mode is interactive only).
    pub(crate) fn cmd_frame_parameters(&mut self, tpars: &[TrailParam]) -> CmdResult {
        let raw = tpars[0].content.trim().to_uppercase();
        if raw.is_empty() {
            return CmdResult::Success; // display mode — no-op in batch
        }

        let current_left = self.frame_set.current_frame().left_margin;
        let current_right = self.frame_set.current_frame().right_margin;
        let current_top = self.frame_set.current_frame().margin_top;
        let current_bottom = self.frame_set.current_frame().margin_bottom;

        let directives = match parse_ep(
            &raw,
            current_left,
            current_right,
            current_top,
            current_bottom,
        ) {
            Ok(d) => d,
            Err(()) => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        let dot_col = self.frame_set.current_frame().dot().column;

        for dir in directives {
            if let Err(()) = self.apply_ep_directive(dir, dot_col) {
                return CmdResult::Failure(CmdFailure::SyntaxError);
            }
        }

        CmdResult::Success
    }

    /// Apply a single parsed EP directive.
    fn apply_ep_directive(&mut self, dir: EpDirective, dot_col: usize) -> Result<(), ()> {
        match dir {
            EpDirective::KeyboardMode { mode, .. } => {
                self.frame_set.keyboard_mode = match mode {
                    EpKeyboardMode::Insert => KeyboardMode::Insert,
                    EpKeyboardMode::Overtype => KeyboardMode::Overtype,
                    EpKeyboardMode::Command => KeyboardMode::Command,
                };
            }

            EpDirective::Options { ops, set_initial } => {
                for op in ops {
                    let set = op.set;
                    match op.flag {
                        EpOptionFlag::AutoIndent => {
                            self.frame_set.current_frame_mut().options.auto_indent = set;
                            if set_initial {
                                self.frame_set.defaults.options.auto_indent = set;
                            }
                        }
                        EpOptionFlag::AutoWrap => {
                            self.frame_set.current_frame_mut().options.auto_wrap = set;
                            if set_initial {
                                self.frame_set.defaults.options.auto_wrap = set;
                            }
                        }
                        EpOptionFlag::Newline => {
                            self.frame_set.current_frame_mut().options.newline = set;
                            if set_initial {
                                self.frame_set.defaults.options.newline = set;
                            }
                        }
                        EpOptionFlag::Show => {} // display only — no-op in batch
                    }
                }
            }

            EpDirective::LrMargins {
                mut left_margin,
                mut right_margin,
                set_initial,
            } => {
                // Resolve '.' sentinel (value 0 means use dot column).
                if left_margin == 0 {
                    left_margin = dot_col;
                }
                if right_margin == 0 {
                    right_margin = dot_col + 1;
                }
                if left_margin >= right_margin {
                    return Err(());
                }
                self.frame_set.current_frame_mut().left_margin = left_margin;
                self.frame_set.current_frame_mut().right_margin = right_margin;
                if set_initial {
                    self.frame_set.defaults.left_margin = left_margin;
                    self.frame_set.defaults.right_margin = right_margin;
                }
            }

            EpDirective::TbMargins {
                margin_top,
                margin_bottom,
                set_initial,
            } => {
                self.frame_set.current_frame_mut().margin_top = margin_top;
                self.frame_set.current_frame_mut().margin_bottom = margin_bottom;
                if set_initial {
                    self.frame_set.defaults.margin_top = margin_top;
                    self.frame_set.defaults.margin_bottom = margin_bottom;
                }
            }

            EpDirective::Tabs { op, set_initial } => {
                self.apply_ep_tabs(op, dot_col, set_initial)?;
            }

            EpDirective::ScreenHeight { .. }
            | EpDirective::ScreenWidth { .. }
            | EpDirective::SpaceLimit { .. }
            | EpDirective::CmdIntroducer => {
                // Stored / no-op in this implementation.
            }
        }
        Ok(())
    }

    /// Apply a tab-stop operation.
    fn apply_ep_tabs(&mut self, op: EpTabOp, dot_col: usize, set_initial: bool) -> Result<(), ()> {
        match op {
            EpTabOp::Default => {
                let stops = default_tab_stops();
                self.frame_set.current_frame_mut().tab_stops = stops.clone();
                if set_initial {
                    self.frame_set.defaults.tab_stops = stops;
                }
            }

            EpTabOp::SetAtDot => {
                if dot_col < self.frame_set.current_frame().tab_stops.len() {
                    self.frame_set.current_frame_mut().tab_stops[dot_col] = true;
                    if set_initial && dot_col < self.frame_set.defaults.tab_stops.len() {
                        self.frame_set.defaults.tab_stops[dot_col] = true;
                    }
                }
            }

            EpTabOp::ClearAtDot => {
                if dot_col < self.frame_set.current_frame().tab_stops.len() {
                    self.frame_set.current_frame_mut().tab_stops[dot_col] = false;
                    if set_initial && dot_col < self.frame_set.defaults.tab_stops.len() {
                        self.frame_set.defaults.tab_stops[dot_col] = false;
                    }
                }
            }

            EpTabOp::Uniform { n } => {
                let stops: Vec<bool> = (0..crate::frame::TAB_STOPS_LEN)
                    .map(|i| i % n == 0)
                    .collect();
                self.frame_set.current_frame_mut().tab_stops = stops.clone();
                if set_initial {
                    self.frame_set.defaults.tab_stops = stops;
                }
            }

            EpTabOp::Explicit { cols } => {
                let mut stops = vec![false; crate::frame::TAB_STOPS_LEN];
                for col in cols {
                    if col < stops.len() {
                        stops[col] = true;
                    }
                }
                self.frame_set.current_frame_mut().tab_stops = stops.clone();
                if set_initial {
                    self.frame_set.defaults.tab_stops = stops;
                }
            }

            EpTabOp::Template => {
                let dot_line = self.frame_set.current_frame().dot().line;
                let line_chars: Vec<char> = {
                    match self.frame_set.current_frame().line_content(dot_line) {
                        None => return Ok(()),
                        Some(s) => s.chars().filter(|&c| c != '\n' && c != '\r').collect(),
                    }
                };
                let mut stops = vec![false; crate::frame::TAB_STOPS_LEN];
                for (i, &ch) in line_chars.iter().enumerate() {
                    if i == 0 {
                        stops[0] = ch != ' ';
                    } else if i < stops.len() {
                        stops[i] = ch != ' ' && line_chars[i - 1] == ' ';
                    }
                }
                self.frame_set.current_frame_mut().tab_stops = stops.clone();
                if set_initial {
                    self.frame_set.defaults.tab_stops = stops;
                }
            }

            EpTabOp::InsertRuler => {
                let left_m = self.frame_set.current_frame().left_margin;
                let right_m = self.frame_set.current_frame().right_margin;
                let stops = self.frame_set.current_frame().tab_stops.clone();
                let dot_line = self.frame_set.current_frame().dot().line;

                // Build ruler string of length `right_m`.
                let mut ruler: Vec<char> = vec![' '; right_m];
                for (i, &is_stop) in stops.iter().enumerate() {
                    if i < right_m && is_stop {
                        ruler[i] = 'T';
                    }
                }
                if left_m < right_m {
                    ruler[left_m] = 'L';
                }
                if right_m > 0 {
                    ruler[right_m - 1] = 'R';
                }

                let ruler_str: String = ruler.into_iter().collect();
                let insert_pos = Position::new(dot_line, 0);
                self.frame_set
                    .current_frame_mut()
                    .insert_at(insert_pos, &(ruler_str + "\n"));
            }

            EpTabOp::ReadRuler => {
                let dot_line = self.frame_set.current_frame().dot().line;
                let line_chars: Vec<char> = {
                    match self.frame_set.current_frame().line_content(dot_line) {
                        None => return Err(()),
                        Some(s) => s
                            .chars()
                            .map(|c| c.to_ascii_uppercase())
                            .filter(|&c| c != '\n' && c != '\r')
                            .collect(),
                    }
                };

                // Validate: only T, L, R, space; exactly one L before one R.
                let mut left_pos: Option<usize> = None;
                let mut right_pos: Option<usize> = None;
                for (i, &ch) in line_chars.iter().enumerate() {
                    match ch {
                        'L' => {
                            if left_pos.is_some() || right_pos.is_some() {
                                return Err(());
                            }
                            left_pos = Some(i);
                        }
                        'R' => {
                            if left_pos.is_none() || right_pos.is_some() {
                                return Err(());
                            }
                            right_pos = Some(i);
                        }
                        'T' | ' ' => {}
                        _ => return Err(()),
                    }
                }
                let left_col = match left_pos {
                    Some(p) => p,
                    None => return Err(()),
                };
                let right_col = match right_pos {
                    Some(p) => p + 1,
                    None => return Err(()),
                };

                // Extract tab stops.
                let mut stops = vec![false; crate::frame::TAB_STOPS_LEN];
                for (i, &ch) in line_chars.iter().enumerate() {
                    if ch == 'T' && i < stops.len() {
                        stops[i] = true;
                    }
                }

                // Delete the ruler line.
                let line_count = self.frame_set.current_frame().line_count();
                let next_line = (dot_line + 1).min(line_count);
                self.frame_set
                    .current_frame_mut()
                    .delete(Position::new(dot_line, 0), Position::new(next_line, 0));

                // Apply.
                self.frame_set.current_frame_mut().left_margin = left_col;
                self.frame_set.current_frame_mut().right_margin = right_col;
                self.frame_set.current_frame_mut().tab_stops = stops.clone();
                if set_initial {
                    self.frame_set.defaults.left_margin = left_col;
                    self.frame_set.defaults.right_margin = right_col;
                    self.frame_set.defaults.tab_stops = stops;
                }
            }
        }
        Ok(())
    }

    /// `{` — Set left margin to current dot column.
    /// `-{` — Reset left margin to default.
    pub(crate) fn cmd_set_margin_left(&mut self, lead: LeadParam) -> CmdResult {
        match lead {
            LeadParam::None | LeadParam::Plus => {
                let col = self.frame_set.current_frame().dot().column;
                if col >= self.frame_set.current_frame().right_margin {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.frame_set.current_frame_mut().left_margin = col;
            }
            LeadParam::Minus => {
                self.frame_set.current_frame_mut().left_margin =
                    self.frame_set.defaults.left_margin;
            }
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        }
        CmdResult::Success
    }

    /// `}` — Set right margin to dot column + 1.
    /// `-}` — Reset right margin to default.
    pub(crate) fn cmd_set_margin_right(&mut self, lead: LeadParam) -> CmdResult {
        match lead {
            LeadParam::None | LeadParam::Plus => {
                let col = self.frame_set.current_frame().dot().column + 1;
                if col <= self.frame_set.current_frame().left_margin {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.frame_set.current_frame_mut().right_margin = col;
            }
            LeadParam::Minus => {
                self.frame_set.current_frame_mut().right_margin =
                    self.frame_set.defaults.right_margin;
            }
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        }
        CmdResult::Success
    }

    // ─── File I/O commands ────────────────────────────────────────────────────

    /// FI — File Input
    ///
    /// `FI/path/` — open a file and load its content into the current frame.
    /// `-FI//`    — close the input file association.
    pub(crate) fn cmd_file_input(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                if self.frame_set.current_frame().input_file.is_none() {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }
                self.frame_set.current_frame_mut().input_file = None;
                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                if self.frame_set.current_frame().input_file.is_some() {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = std::path::PathBuf::from(&path_str);

                let mut fh = match file_io::open_input(&path) {
                    Ok(h) => h,
                    Err(_) => return CmdResult::Failure(CmdFailure::FileOpenError),
                };

                // Read all content (handle not yet stored on frame — no borrow conflict).
                let text = file_io::read_all(&mut fh);

                // Clear frame and load file content.
                let frame = self.frame_set.current_frame_mut();
                frame.clear_content();
                if !text.is_empty() {
                    frame.insert_at(Position::zero(), &text);
                }
                frame.set_dot(Position::zero());
                frame.input_file = Some(fh);

                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FO — File Output
    ///
    /// `FO/path/` — open an output file (writes frame content when closed).
    /// `-FO//`    — write all frame content to the output file, finalize, close.
    pub(crate) fn cmd_file_output(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                if self.frame_set.current_frame().output_file.is_none() {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }

                // Collect frame content and modified flag before taking the handle.
                let text = self.frame_set.current_frame().text();
                let modified = self
                    .frame_set
                    .current_frame()
                    .get_mark(MarkId::Modified)
                    .is_some();

                let mut fh = self
                    .frame_set
                    .current_frame_mut()
                    .output_file
                    .take()
                    .unwrap();

                if file_io::write_all(&mut fh, &text).is_err() {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                }
                if file_io::finalize_output(&mut fh, modified).is_err() {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                }

                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                if self.frame_set.current_frame().output_file.is_some() {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = if path_str.is_empty() {
                    // Fall back to the input file's path.
                    match self.frame_set.current_frame().input_file.as_ref() {
                        Some(fh) => fh.path.clone(),
                        None => return CmdResult::Failure(CmdFailure::SyntaxError),
                    }
                } else {
                    std::path::PathBuf::from(&path_str)
                };

                let fh = match file_io::open_output(&path, false, 1, false) {
                    Ok(h) => h,
                    Err(_) => return CmdResult::Failure(CmdFailure::FileOpenError),
                };

                self.frame_set.current_frame_mut().output_file = Some(fh);
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FE — File Edit
    ///
    /// `FE/path/` — open both input and output on the same file.
    /// `-FE//`    — write output, finalize, close both handles.
    pub(crate) fn cmd_file_edit(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                let has_input = self.frame_set.current_frame().input_file.is_some();
                let has_output = self.frame_set.current_frame().output_file.is_some();

                if !has_input && !has_output {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }

                // Finalize output if open.
                if has_output {
                    let text = self.frame_set.current_frame().text();
                    let modified = self
                        .frame_set
                        .current_frame()
                        .get_mark(MarkId::Modified)
                        .is_some();
                    let mut fh = self
                        .frame_set
                        .current_frame_mut()
                        .output_file
                        .take()
                        .unwrap();

                    if file_io::write_all(&mut fh, &text).is_err() {
                        return CmdResult::Failure(CmdFailure::FileOpenError);
                    }
                    if file_io::finalize_output(&mut fh, modified).is_err() {
                        return CmdResult::Failure(CmdFailure::FileOpenError);
                    }
                }

                // Close input.
                self.frame_set.current_frame_mut().input_file = None;

                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                let has_input = self.frame_set.current_frame().input_file.is_some();
                let has_output = self.frame_set.current_frame().output_file.is_some();

                if has_input || has_output {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = std::path::PathBuf::from(&path_str);

                // Open input and read all content.
                let mut input_fh = match file_io::open_input(&path) {
                    Ok(h) => h,
                    Err(_) => return CmdResult::Failure(CmdFailure::FileOpenError),
                };
                let text = file_io::read_all(&mut input_fh);

                // Open output.
                let output_fh = match file_io::open_output(&path, false, 1, false) {
                    Ok(h) => h,
                    Err(_) => return CmdResult::Failure(CmdFailure::FileOpenError),
                };

                // Load content into frame.
                let frame = self.frame_set.current_frame_mut();
                frame.clear_content();
                if !text.is_empty() {
                    frame.insert_at(Position::zero(), &text);
                }
                frame.set_dot(Position::zero());
                frame.input_file = Some(input_fh);
                frame.output_file = Some(output_fh);

                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FK — File Kill
    ///
    /// Deletes the per-frame output temp file without creating the real file.
    pub(crate) fn cmd_file_kill(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.current_frame().output_file.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let fh = self
            .frame_set
            .current_frame_mut()
            .output_file
            .take()
            .unwrap();
        file_io::delete_temp(&fh);

        CmdResult::Success
    }

    /// FB — File Back (rewind)
    ///
    /// Rewinds the per-frame input file and reloads the frame from the beginning.
    pub(crate) fn cmd_file_rewind(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.current_frame().input_file.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        // Take the handle out to allow mutable access to the frame simultaneously.
        let mut fh = self
            .frame_set
            .current_frame_mut()
            .input_file
            .take()
            .unwrap();

        if file_io::rewind(&mut fh).is_err() {
            // Put it back on error so the caller can still clean up.
            self.frame_set.current_frame_mut().input_file = Some(fh);
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }

        let text = file_io::read_all(&mut fh);

        let frame = self.frame_set.current_frame_mut();
        frame.clear_content();
        if !text.is_empty() {
            frame.insert_at(Position::zero(), &text);
        }
        frame.set_dot(Position::zero());
        frame.input_file = Some(fh);

        CmdResult::Success
    }

    /// FS — File Save
    ///
    /// Saves the current frame to its output file (if modified), then reopens
    /// both handles for continued editing.  No-op if the frame is unmodified.
    pub(crate) fn cmd_file_save(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.current_frame().output_file.is_none() {
            return CmdResult::Failure(CmdFailure::NoOutputFile);
        }

        // No-op if not modified.
        if self
            .frame_set
            .current_frame()
            .get_mark(MarkId::Modified)
            .is_none()
        {
            return CmdResult::Success;
        }

        // Collect frame text, then take the output handle.
        let text = self.frame_set.current_frame().text();
        let mut out = self
            .frame_set
            .current_frame_mut()
            .output_file
            .take()
            .unwrap();

        if file_io::write_all(&mut out, &text).is_err() {
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }

        // Remember path parameters before finalize consumes them.
        let path = out.path.clone();
        let entab = out.entab;
        let versions = out.versions;
        let purge = out.purge;

        if file_io::finalize_output(&mut out, true).is_err() {
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }

        // Reopen for continued editing.
        let mut new_input = match file_io::open_input(&path) {
            Ok(h) => h,
            Err(_) => return CmdResult::Failure(CmdFailure::FileOpenError),
        };
        let new_text = file_io::read_all(&mut new_input);

        let new_output = match file_io::open_output(&path, entab, versions, purge) {
            Ok(h) => h,
            Err(_) => return CmdResult::Failure(CmdFailure::FileOpenError),
        };

        let frame = self.frame_set.current_frame_mut();
        frame.clear_content();
        if !new_text.is_empty() {
            frame.insert_at(Position::zero(), &new_text);
        }
        frame.set_dot(Position::zero());
        frame.input_file = Some(new_input);
        frame.output_file = Some(new_output);

        CmdResult::Success
    }

    /// FT — File Table
    ///
    /// Displays the file table (no-op in batch mode).
    pub(crate) fn cmd_file_table(&mut self, _lead: LeadParam) -> CmdResult {
        // No-op in batch mode; interactive display deferred to Phase 10.
        CmdResult::Success
    }

    /// FGI — Global File Input
    ///
    /// `FGI/path/` — open global input file.
    /// `-FGI//`    — close global input file.
    pub(crate) fn cmd_fglobal_input(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                if self.frame_set.global_input.is_none() {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }
                self.frame_set.global_input = None;
                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                if self.frame_set.global_input.is_some() {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = std::path::PathBuf::from(&path_str);

                let fh = match file_io::open_input(&path) {
                    Ok(h) => h,
                    Err(_) => return CmdResult::Failure(CmdFailure::FileOpenError),
                };

                self.frame_set.global_input = Some(fh);
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FGO — Global File Output
    ///
    /// `FGO/path/` — open global output file.
    /// `-FGO//`    — flush, finalize, close global output file.
    pub(crate) fn cmd_fglobal_output(
        &mut self,
        lead: LeadParam,
        tpars: &[TrailParam],
    ) -> CmdResult {
        match lead {
            LeadParam::Minus => {
                if self.frame_set.global_output.is_none() {
                    return CmdResult::Failure(CmdFailure::FileNotOpen);
                }

                let mut fh = self.frame_set.global_output.take().unwrap();
                // Global output: no backups by default.
                if file_io::finalize_output(&mut fh, false).is_err() {
                    return CmdResult::Failure(CmdFailure::FileOpenError);
                }

                CmdResult::Success
            }
            LeadParam::None | LeadParam::Plus => {
                if self.frame_set.global_output.is_some() {
                    return CmdResult::Failure(CmdFailure::FileAlreadyOpen);
                }

                let path_str = tpars[0].content.trim().to_string();
                let path = std::path::PathBuf::from(&path_str);

                let fh = match file_io::open_output(&path, false, 0, false) {
                    Ok(h) => h,
                    Err(_) => return CmdResult::Failure(CmdFailure::FileOpenError),
                };

                self.frame_set.global_output = Some(fh);
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    /// FGR — Global File Read
    ///
    /// Reads N lines from the global input file and inserts them at the current dot.
    pub(crate) fn cmd_file_read(&mut self, lead: LeadParam) -> CmdResult {
        if self.frame_set.global_input.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let count = match lead {
            LeadParam::None | LeadParam::Plus => 1usize,
            LeadParam::Pint(n) => n,
            LeadParam::Pindef => usize::MAX,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Take the handle out to read from it, then put it back.
        let mut fh = self.frame_set.global_input.take().unwrap();
        let lines = file_io::read_lines(&mut fh, count);
        let at_eof = fh.at_eof;
        self.frame_set.global_input = Some(fh);

        if lines.is_empty() && at_eof {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }

        if lines.is_empty() {
            return CmdResult::Success;
        }

        // Build insertion text.
        let text: String = lines.iter().map(|l| format!("{}\n", l)).collect();

        let dot = self.frame_set.current_frame().dot();
        self.frame_set
            .current_frame_mut()
            .set_mark_at(MarkId::Equals, dot);
        self.frame_set.current_frame_mut().insert_at(dot, &text);

        let new_dot = self.frame_set.current_frame().dot();
        self.frame_set
            .current_frame_mut()
            .set_mark_at(MarkId::Modified, new_dot);

        CmdResult::Success
    }

    /// FGW — Global File Write
    ///
    /// Writes N lines from the current frame to the global output file.
    pub(crate) fn cmd_file_write(&mut self, lead: LeadParam) -> CmdResult {
        if self.frame_set.global_output.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let count = match lead {
            LeadParam::None | LeadParam::Plus => 1usize,
            LeadParam::Pint(n) => n,
            LeadParam::Pindef => usize::MAX,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        let dot_line = self.frame_set.current_frame().dot().line;
        let line_count = self.frame_set.current_frame().line_count();
        // Exclude the null (EOF) line.
        let content_lines = line_count.saturating_sub(1);

        let end_line = if count == usize::MAX {
            content_lines
        } else {
            (dot_line + count).min(content_lines)
        };

        // Collect lines to write.
        let mut lines = Vec::new();
        for line_idx in dot_line..end_line {
            let content = self
                .frame_set
                .current_frame()
                .line_content(line_idx)
                .map(|s| {
                    let s = s.to_string();
                    s.trim_end_matches('\n').trim_end_matches('\r').to_string()
                })
                .unwrap_or_default();
            lines.push(content);
        }

        // Write lines.
        let mut fh = self.frame_set.global_output.take().unwrap();
        if file_io::write_lines(&mut fh, &lines).is_err() {
            self.frame_set.global_output = Some(fh);
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }
        self.frame_set.global_output = Some(fh);

        // Advance dot to the line after the last written.
        let new_line = end_line.min(content_lines);
        self.frame_set
            .current_frame_mut()
            .set_dot(Position::new(new_line, 0));

        CmdResult::Success
    }

    /// FGB — Global File Back (rewind)
    ///
    /// Rewinds the global input file without affecting any frame.
    pub(crate) fn cmd_fglobal_rewind(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.global_input.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let mut fh = self.frame_set.global_input.take().unwrap();
        if file_io::rewind(&mut fh).is_err() {
            self.frame_set.global_input = Some(fh);
            return CmdResult::Failure(CmdFailure::FileOpenError);
        }
        self.frame_set.global_input = Some(fh);

        CmdResult::Success
    }

    /// FGK — Global File Kill
    ///
    /// Deletes the global output temp file without creating the real file.
    pub(crate) fn cmd_fglobal_kill(&mut self, lead: LeadParam) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        if self.frame_set.global_output.is_none() {
            return CmdResult::Failure(CmdFailure::FileNotOpen);
        }

        let fh = self.frame_set.global_output.take().unwrap();
        file_io::delete_temp(&fh);

        CmdResult::Success
    }

    /// Extracts the text of a span or frame by name.
    ///
    /// Returns `None` if neither a frame nor a span with that name exists,
    /// or if the span's boundary marks are not set in its frame.
    pub(crate) fn read_span_or_frame_text(&self, name: &str) -> Option<String> {
        if let Some(frame) = self.frame_set.get_frame(name) {
            Some(frame.text())
        } else if let Some(span) = self.frame_set.get_span(name) {
            let frame = self.frame_set.get_frame(&span.frame_name)?;
            let start = frame.get_mark(span.mark_start)?;
            let end = frame.get_mark(span.mark_end)?;
            let start_idx = frame.to_char_index(&start);
            let end_idx = frame.to_char_index(&end);
            if start_idx >= end_idx {
                Some(String::new())
            } else {
                Some(frame.slice(start_idx..end_idx))
            }
        } else {
            None
        }
    }
}

pub(crate) const MAX_SPAN_NAME_LEN: usize = 31;

/// Validate and normalise a span name from a trailing param.
pub(crate) fn parse_span_name(tpar: &TrailParam) -> Option<String> {
    let name = tpar.content.trim();
    if name.is_empty() || name.len() > MAX_SPAN_NAME_LEN {
        return None;
    }
    Some(name.to_uppercase())
}
