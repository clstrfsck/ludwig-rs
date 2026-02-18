//! Text editing commands (insert, delete, overtype).

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::position::{Position, line_length_excluding_newline};
use crate::trail_param::TrailParam;

use super::Frame;

/// The case-change mode for the `*` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseMode {
    Upper,
    Lower,
    Edit,
}

/// Commands for editing text in the frame.
pub trait EditCommands {
    /// Insert character(s) command.
    fn cmd_insert_char(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Insert line(s) command.
    fn cmd_insert_line(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Insert text command.
    fn cmd_insert_text(&mut self, lead_param: LeadParam, text: &TrailParam) -> CmdResult;

    /// Delete character(s) command.
    fn cmd_delete_char(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Overtype text command.
    fn cmd_overtype_text(&mut self, lead_param: LeadParam, text: &TrailParam) -> CmdResult;

    /// Split line command.
    fn cmd_split_line(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Case change command (*U, *L, *E).
    fn cmd_case_change(&mut self, lead_param: LeadParam, mode: CaseMode) -> CmdResult;

    /// Delete (kill) line(s) command (K).
    fn cmd_delete_line(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Rubout (backspace) command (ZZ).
    fn cmd_rubout(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Swap line command (SW).
    fn cmd_swap_line(&mut self, lead_param: LeadParam) -> CmdResult;
}

impl EditCommands for Frame {
    fn cmd_insert_char(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.insert_chars(1, false),
            LeadParam::Pint(n) => self.insert_chars(n, false),
            LeadParam::Minus => self.insert_chars(1, true),
            LeadParam::Nint(n) => self.insert_chars(n, true),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_insert_line(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.insert_lines(1, false),
            LeadParam::Pint(n) => self.insert_lines(n, false),
            LeadParam::Minus => self.insert_lines(1, true),
            LeadParam::Nint(n) => self.insert_lines(n, true),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_insert_text(&mut self, lead_param: LeadParam, text: &TrailParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.cmd_ins_text(1, &text.str),
            LeadParam::Pint(n) => self.cmd_ins_text(n, &text.str),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_delete_char(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.cmd_del_forward(1),
            LeadParam::Pint(n) => self.cmd_del_forward(n),
            LeadParam::Pindef => {
                self.cmd_del_forward(line_length_excluding_newline(&self.rope, self.dot().line))
            }
            LeadParam::Minus => self.cmd_del_backward(1),
            LeadParam::Nint(n) => self.cmd_del_backward(n),
            LeadParam::Nindef => self.cmd_del_backward(self.dot().column),
            LeadParam::Marker(id) => self.cmd_del_to_mark(id),
        }
    }

    fn cmd_overtype_text(&mut self, lead_param: LeadParam, text: &TrailParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.cmd_ovr_text(1, &text.str),
            LeadParam::Pint(n) => self.cmd_ovr_text(n, &text.str),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_split_line(&mut self, lead_param: LeadParam) -> CmdResult {
        if lead_param == LeadParam::None {
            let original_dot = self.dot();
            // We are not going to extend the line, so just clamp the dot to the actual text.
            let clamped_dot = original_dot.clamp_to_text(&self.rope);
            self.set_dot(clamped_dot);
            self.insert_at(Position::new(clamped_dot.line, clamped_dot.column), "\n");
            self.set_mark(MarkId::Modified);
            self.set_mark_at(MarkId::Equals, original_dot);
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::SyntaxError)
        }
    }

    fn cmd_case_change(&mut self, lead_param: LeadParam, mode: CaseMode) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.case_change_forward(1, mode),
            LeadParam::Pint(n) => self.case_change_forward(n, mode),
            LeadParam::Pindef => {
                let line_len = line_length_excluding_newline(&self.rope, self.dot().line);
                let count = line_len.saturating_sub(self.dot().column);
                self.case_change_forward(count, mode)
            }
            LeadParam::Minus => self.case_change_backward(1, mode),
            LeadParam::Nint(n) => self.case_change_backward(n, mode),
            LeadParam::Nindef => {
                let count = self.dot().column;
                self.case_change_backward(count, mode)
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_rubout(&mut self, lead_param: LeadParam) -> CmdResult {
        // ZZ: Delete the character to the left of Dot (backspace).
        // Insert mode behavior: equivalent to -J D (move back, delete forward).
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.cmd_del_backward(1),
            LeadParam::Pint(n) => self.cmd_del_backward(n),
            LeadParam::Pindef => self.cmd_del_backward(self.dot().column),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_delete_line(&mut self, lead_param: LeadParam) -> CmdResult {
        let num_lines = self.lines();
        let dot = self.dot();
        match lead_param {
            LeadParam::None | LeadParam::Plus => {
                if dot.line >= num_lines {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.kill_lines_forward(dot.line, 1)
            }
            LeadParam::Pint(n) => {
                if n == 0 {
                    return CmdResult::Success;
                }
                if dot.line + n + 1 > num_lines {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.kill_lines_forward(dot.line, n)
            }
            LeadParam::Pindef => {
                if dot.line >= num_lines {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                let count = num_lines - dot.line;
                self.kill_lines_forward(dot.line, count)
            }
            LeadParam::Minus => {
                if dot.line == 0 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.kill_lines_backward(dot.line - 1, 1)
            }
            LeadParam::Nint(n) => {
                if n == 0 {
                    return CmdResult::Success;
                }
                if n > dot.line {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.kill_lines_backward(dot.line - n, n)
            }
            LeadParam::Nindef => {
                if dot.line == 0 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.kill_lines_backward(0, dot.line)
            }
            LeadParam::Marker(id) => {
                if let Some(mark_pos) = self.mark_position(id) {
                    if mark_pos.line == dot.line {
                        // Mark is on the same line as dot, so nothing to delete.
                        CmdResult::Success
                    } else if mark_pos.line < dot.line {
                        // Mark is above dot, so delete backward (lines above dot).
                        let count = dot.line - mark_pos.line;
                        self.kill_lines_backward(mark_pos.line, count)
                    } else {
                        // Mark is below dot, so delete forward (lines below dot).
                        let count = mark_pos.line - dot.line;
                        self.kill_lines_forward(dot.line, count)
                    }
                } else {
                    CmdResult::Failure(CmdFailure::MarkNotDefined)
                }
            }
        }
    }

    fn cmd_swap_line(&mut self, lead_param: LeadParam) -> CmdResult {
        let dot = self.dot();
        let num_lines = self.lines();
        let (source_line, dest_line) = match lead_param {
            LeadParam::None | LeadParam::Plus => {
                // SW: move current line after line below
                if dot.line + 2 >= num_lines {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                (dot.line, dot.line + 1)
            }
            LeadParam::Pint(n) => {
                // nSW: move current line N positions down
                if dot.line + n + 1 >= num_lines {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                (dot.line, dot.line + n)
            }
            LeadParam::Minus => {
                // -SW: move current line before line above
                if dot.line == 0 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                (dot.line, dot.line - 1)
            }
            LeadParam::Nint(n) => {
                // -nSW: move current line N positions up
                if n > dot.line {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                (dot.line, dot.line - n)
            }
            LeadParam::Nindef => {
                // <SW: move current line to the top
                (dot.line, 0)
            }
            LeadParam::Pindef => {
                // >SW: move current line to the bottom
                (dot.line, num_lines - 1)
            }
            LeadParam::Marker(id) => {
                // @SW: move current line to the position of mark m
                if let Some(mark_pos) = self.mark_position(id) {
                    (dot.line, mark_pos.line)
                } else {
                    return CmdResult::Failure(CmdFailure::MarkNotDefined);
                }
            }
        };
        if source_line == dest_line {
            // No-op if trying to swap the same line
            return CmdResult::Success;
        }
        self.move_line(source_line, dest_line);
        let original_dot = dot;
        // After swap, dot follows the original line's new position
        self.set_dot(Position::new(dest_line, dot.column));
        // FIXME: Need to adjust marks
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, original_dot);
        CmdResult::Success
    }
}

// Private implementation helpers for insertion
impl Frame {
    fn insert_lines(&mut self, count: usize, move_dot: bool) -> CmdResult {
        if count == 0 {
            return CmdResult::Success;
        }
        let original_dot = self.dot();
        let insert_pos = Position::new(original_dot.line, 0);
        self.insert_at(insert_pos, &"\n".repeat(count));
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, original_dot);
        if !move_dot {
            // After insert_at, dot has been shifted down by count lines.
            // Reset it to the original position (now an empty inserted line).
            self.set_dot(Position::new(original_dot.line, original_dot.column));
        }
        CmdResult::Success
    }

    fn insert_chars(&mut self, count: usize, move_dot: bool) -> CmdResult {
        if count == 0 {
            return CmdResult::Success;
        }
        let original_dot = self.dot();
        self.insert(&" ".repeat(count));
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, original_dot);
        if !move_dot {
            // Dot moves when the text is inserted, so we need to move it back.
            self.set_dot(original_dot);
        }
        CmdResult::Success
    }

    fn cmd_ins_text(&mut self, count: usize, text: &str) -> CmdResult {
        if count == 0 {
            return CmdResult::Success;
        }
        let last = self.dot();
        self.insert(&text.repeat(count));
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, last);
        CmdResult::Success
    }
}

// Private implementation helpers for deletion
impl Frame {
    fn cmd_del_forward(&mut self, count: usize) -> CmdResult {
        if self.delete_forward(count) {
            self.set_mark(MarkId::Modified);
        }
        self.unset_mark(MarkId::Equals);
        CmdResult::Success
    }

    fn cmd_del_backward(&mut self, count: usize) -> CmdResult {
        let start_position = self.dot();
        if start_position.column < count {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        let final_dot = Position::new(
            start_position.line,
            start_position.column.saturating_sub(count),
        );
        if self.delete_backward(count) {
            self.set_mark_at(MarkId::Modified, final_dot);
        }
        self.unset_mark(MarkId::Equals);
        self.set_mark_at(MarkId::Dot, final_dot);
        CmdResult::Success
    }

    fn cmd_del_to_mark(&mut self, mark_id: MarkId) -> CmdResult {
        if let Some(mark_pos) = self.mark_position(mark_id) {
            if self.delete(self.dot(), mark_pos) {
                self.set_mark(MarkId::Modified);
            }
            self.unset_mark(MarkId::Equals);
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::MarkNotDefined)
        }
    }

    /// Delete `count` characters forward from dot.
    fn delete_forward(&mut self, count: usize) -> bool {
        let from = self.dot();
        let to = Position::new(from.line, from.column + count);
        self.delete(from, to)
    }

    /// Delete `count` characters backward from dot.
    fn delete_backward(&mut self, count: usize) -> bool {
        let to = self.dot();
        let from = Position::new(to.line, to.column.saturating_sub(count));
        self.delete(from, to)
    }
}

// Private implementation helpers for overtyping
impl Frame {
    fn cmd_ovr_text(&mut self, count: usize, text: &str) -> CmdResult {
        if count == 0 {
            return CmdResult::Success;
        }
        let last = self.dot();
        self.overtype(&text.repeat(count));
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, last);
        CmdResult::Success
    }
}

// Private implementation helpers for line deletion
impl Frame {
    /// Delete `count` lines starting at `from_line` (forward kill).
    /// Dot column is preserved; dot stays at `from_line` (now containing the
    /// content that was after the deleted range).
    fn kill_lines_forward(&mut self, from_line: usize, count: usize) -> CmdResult {
        let original_dot = self.dot();
        let to_line = from_line + count - 1;
        self.delete_line_range(from_line, to_line);
        // Dot column preserved, dot line stays at from_line (mark update handles shift)
        // But we need to restore the column since delete may have moved dot
        self.set_dot(Position::new(from_line, original_dot.column));
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, original_dot);
        CmdResult::Success
    }

    /// Delete `count` lines starting at `from_line` (backward kill — lines above dot).
    /// Dot stays on the same text content (its line number shifts up by `count`).
    fn kill_lines_backward(&mut self, from_line: usize, count: usize) -> CmdResult {
        let original_dot = self.dot();
        let to_line = from_line + count - 1;
        self.delete_line_range(from_line, to_line);
        // Dot was after the deleted range, so mark update already shifted it up.
        // Just preserve the column.
        self.set_dot(Position::new(original_dot.line - count, original_dot.column));
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, original_dot);
        CmdResult::Success
    }

    /// Delete lines `from_line` through `to_line` (inclusive) from the rope,
    /// updating marks appropriately.
    fn delete_line_range(&mut self, from_line: usize, to_line: usize) {
        let num_lines = self.lines();
        let is_last_line = to_line + 1 >= num_lines;

        if !is_last_line {
            // Normal case: delete from start of from_line to start of to_line+1.
            // This removes the lines and their trailing newlines.
            let from_pos = Position::new(from_line, 0);
            let to_pos = Position::new(to_line + 1, 0);
            self.delete(from_pos, to_pos);
        } else if from_line > 0 {
            // Deleting to the end; also delete the last line's newline
            let last_line_len = self.rope.line(to_line).len_chars();
            let from_pos = Position::new(from_line, 0);
            let to_pos = Position::new(to_line, last_line_len);
            self.delete(from_pos, to_pos);
        } else {
            // Deleting all lines from 0 to end.
            let last_line_len = line_length_excluding_newline(&self.rope, to_line);
            let from_pos = Position::new(0, 0);
            let to_pos = Position::new(to_line, last_line_len);
            self.delete(from_pos, to_pos);
        }
    }
}

// Private implementation helpers for swap line
impl Frame {
    /// Move the contents of one line to another position in the rope.
    fn move_line(&mut self, source_line: usize, dest_line: usize) {
        // Extract line text (including newline) for source_line
        let start = self.rope.line_to_char(source_line);
        let end = if source_line + 1 < self.lines() {
            self.rope.line_to_char(source_line + 1)
        } else {
            self.rope.len_chars()
        };
        let source_text = self.rope.slice(start..end).to_string();

        // Delete the source line
        self.rope.remove(start..end);

        // Insert the source text at the destination line
        let dest_start = self.rope.line_to_char(dest_line);
        self.rope.insert(dest_start, &source_text);
    }
}

// Private implementation helpers for case change
impl Frame {
    fn case_change_forward(&mut self, count: usize, mode: CaseMode) -> CmdResult {
        if count == 0 {
            return CmdResult::Success;
        }
        let dot = self.dot();
        let line_len = line_length_excluding_newline(&self.rope, dot.line);
        // In virtual space or nothing to change
        if dot.column >= line_len {
            return CmdResult::Success;
        }
        let actual_count = count.min(line_len - dot.column);
        let line_start = self.rope.line_to_char(dot.line);
        let start_idx = line_start + dot.column;

        self.apply_case_change(start_idx, actual_count, dot.column, mode);

        let original_dot = dot;
        self.set_dot(Position::new(dot.line, dot.column + actual_count));
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, original_dot);
        CmdResult::Success
    }

    fn case_change_backward(&mut self, count: usize, mode: CaseMode) -> CmdResult {
        if count == 0 {
            return CmdResult::Success;
        }
        let dot = self.dot();
        if dot.column == 0 {
            return CmdResult::Success;
        }
        let line_len = line_length_excluding_newline(&self.rope, dot.line);
        // Clamp dot to actual text for the purpose of the backward range
        let effective_col = dot.column.min(line_len);
        let actual_count = count.min(effective_col);
        let new_col = effective_col - actual_count;
        let line_start = self.rope.line_to_char(dot.line);
        let start_idx = line_start + new_col;

        self.apply_case_change(start_idx, actual_count, new_col, mode);

        let original_dot = dot;
        self.set_dot(Position::new(dot.line, new_col));
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, original_dot);
        CmdResult::Success
    }

    /// Apply case change to `count` chars starting at rope index `start_idx`.
    /// `start_column` is the column of `start_idx` on its line (used for *E logic).
    fn apply_case_change(
        &mut self,
        start_idx: usize,
        count: usize,
        start_column: usize,
        mode: CaseMode,
    ) {
        // Collect the characters we need to change
        let chars: Vec<char> = self
            .rope
            .chars_at(start_idx)
            .take(count)
            .collect();

        let new_chars: Vec<char> = match mode {
            CaseMode::Upper => chars.iter().map(|c| c.to_ascii_uppercase()).collect(),
            CaseMode::Lower => chars.iter().map(|c| c.to_ascii_lowercase()).collect(),
            CaseMode::Edit => {
                // For *E: if preceding char is a letter → lowercase, else → uppercase.
                // The "preceding char" for position i is the result of changing position i-1.
                let mut result = Vec::with_capacity(count);
                let mut prev = if start_column > 0 {
                    // Get the character before the range
                    self.rope.char(start_idx - 1)
                } else {
                    ' ' // Before column 0, treat as non-letter
                };
                for &ch in &chars {
                    let new_ch = if prev.is_ascii_alphabetic() {
                        ch.to_ascii_lowercase()
                    } else {
                        ch.to_ascii_uppercase()
                    };
                    prev = new_ch;
                    result.push(new_ch);
                }
                result
            }
        };

        // Check if anything actually changed
        if chars == new_chars {
            return;
        }

        // Replace in the rope character by character
        let new_str: String = new_chars.into_iter().collect();
        self.rope.remove(start_idx..start_idx + count);
        self.rope.insert(start_idx, &new_str);
    }
}
