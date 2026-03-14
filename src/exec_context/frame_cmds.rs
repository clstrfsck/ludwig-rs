//! Frame navigation and parameter commands: ED, EK, ER, EP, {, }.

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::file_io;
use crate::frame::params::{EpDirective, EpKeyboardMode, EpOptionFlag, EpTabOp, parse_ep};
use crate::frame::{KeyboardMode, default_tab_stops};
use crate::frame_set::SPECIAL_FRAME_NAMES;
use crate::lead_param::LeadParam;
use crate::position::Position;
use crate::trail_param::TrailParam;

use super::ExecutionContext;

impl<'a> ExecutionContext<'a> {
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
}
