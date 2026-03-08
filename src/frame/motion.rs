//! Motion commands for moving the cursor (Advance and Jump).

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::position::Position;

use super::Frame;

/// Commands for moving the cursor within the frame.
pub trait MotionCommands {
    /// Advance command - move to a different line.
    fn cmd_advance(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Jump command - move within the current line.
    fn cmd_jump(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Cursor left command (ZL).
    fn cmd_left(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Cursor right command (ZR).
    fn cmd_right(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Cursor up command (ZU).
    fn cmd_up(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Cursor down command (ZD).
    fn cmd_down(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Carriage return command (ZC).
    fn cmd_return(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Tab command (ZT) — move dot right to the next tab stop or margin.
    fn cmd_tab(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Backtab command (ZB) — move dot left to the previous tab stop or margin.
    fn cmd_backtab(&mut self, lead_param: LeadParam) -> CmdResult;
}

impl MotionCommands for Frame {
    fn cmd_advance(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.advance_fwd(1, false),
            LeadParam::Pint(n) => self.advance_fwd(n, false),
            LeadParam::Pindef => self.advance_end(),
            LeadParam::Minus => self.advance_back(1),
            LeadParam::Nint(n) => self.advance_back(n),
            LeadParam::Nindef => self.advance_begin(),
            LeadParam::Marker(id) => self.advance_to(self.get_mark(id)),
        }
    }

    fn cmd_jump(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.jump_fwd(1),
            LeadParam::Pint(n) => self.jump_fwd(n),
            LeadParam::Pindef => self.jump_end(),
            LeadParam::Minus => self.jump_back(1),
            LeadParam::Nint(n) => self.jump_back(n),
            LeadParam::Nindef => self.jump_begin(),
            LeadParam::Marker(id) => self.jump_to(self.get_mark(id)),
        }
    }

    fn cmd_left(&mut self, lead_param: LeadParam) -> CmdResult {
        let dot = self.dot();
        match lead_param {
            LeadParam::None | LeadParam::Plus => {
                if dot.column < 1 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(dot.line, dot.column - 1));
                CmdResult::Success
            }
            LeadParam::Pint(n) => {
                if dot.column < n {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(dot.line, dot.column - n));
                CmdResult::Success
            }
            LeadParam::Pindef => {
                // Go to left margin
                let dot = self.dot();
                if dot.column < self.left_margin {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                if dot.column > self.left_margin {
                    // Only set mark if we're actually moving
                    self.set_mark_at(MarkId::Equals, dot);
                    self.set_dot(Position::new(dot.line, self.left_margin));
                }
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_right(&mut self, lead_param: LeadParam) -> CmdResult {
        let dot = self.dot();
        match lead_param {
            LeadParam::None | LeadParam::Plus => {
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(dot.line, dot.column + 1));
                CmdResult::Success
            }
            LeadParam::Pint(n) => {
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(dot.line, dot.column + n));
                CmdResult::Success
            }
            LeadParam::Pindef => {
                // Go to right margin
                if dot.column >= self.right_margin {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                if dot.column + 1 < self.right_margin {
                    // Only set mark if we're actually moving
                    self.set_mark_at(MarkId::Equals, dot);
                    self.set_dot(Position::new(dot.line, self.right_margin.saturating_sub(1)));
                }
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_up(&mut self, lead_param: LeadParam) -> CmdResult {
        let dot = self.dot();
        match lead_param {
            LeadParam::None | LeadParam::Plus => {
                if dot.line < 1 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(dot.line - 1, dot.column));
                CmdResult::Success
            }
            LeadParam::Pint(n) => {
                if dot.line < n {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(dot.line - n, dot.column));
                CmdResult::Success
            }
            LeadParam::Pindef => {
                // Go to first line, preserving column
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(0, dot.column));
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_down(&mut self, lead_param: LeadParam) -> CmdResult {
        let dot = self.dot();
        let num_lines = self.line_count();
        match lead_param {
            LeadParam::None | LeadParam::Plus => {
                if dot.line + 1 >= num_lines {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(dot.line + 1, dot.column));
                CmdResult::Success
            }
            LeadParam::Pint(n) => {
                if dot.line + n >= num_lines {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(dot.line + n, dot.column));
                CmdResult::Success
            }
            LeadParam::Pindef => {
                // Go to last line, preserving column
                let last_line = if num_lines > 0 { num_lines - 1 } else { 0 };
                self.set_mark_at(MarkId::Equals, dot);
                self.set_dot(Position::new(last_line, dot.column));
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_return(&mut self, lead_param: LeadParam) -> CmdResult {
        // ZC: Advance n lines, go to left margin.
        // When on the last line, inserts a newline to extend the buffer.
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.return_fwd(1),
            LeadParam::Pint(n) => self.return_fwd(n),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_tab(&mut self, lead_param: LeadParam) -> CmdResult {
        let count = match lead_param {
            LeadParam::None | LeadParam::Plus => 1usize,
            LeadParam::Pint(n) => n,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        let max_col = self.tab_stops.len().saturating_sub(1);
        let mut new_col = self.dot().column;

        for _ in 0..count {
            if new_col >= max_col {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
            loop {
                new_col += 1;
                if new_col >= self.tab_stops.len() {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                if self.tab_stops[new_col]
                    || new_col == self.left_margin
                    || new_col == self.right_margin
                {
                    break;
                }
            }
        }

        let old_dot = self.dot();
        self.set_mark_at(MarkId::Equals, old_dot);
        self.set_dot(Position::new(old_dot.line, new_col));
        CmdResult::Success
    }

    fn cmd_backtab(&mut self, lead_param: LeadParam) -> CmdResult {
        let count = match lead_param {
            LeadParam::None | LeadParam::Plus => 1usize,
            LeadParam::Pint(n) => n,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        let mut new_col = self.dot().column;

        for _ in 0..count {
            if new_col == 0 {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
            loop {
                new_col -= 1;
                if self.tab_stops[new_col]
                    || new_col == self.left_margin
                    || new_col == self.right_margin
                {
                    break;
                }
                if new_col == 0 {
                    break;
                }
            }
        }

        let old_dot = self.dot();
        self.set_mark_at(MarkId::Equals, old_dot);
        self.set_dot(Position::new(old_dot.line, new_col));
        CmdResult::Success
    }
}

// Private implementation helpers for Advance
impl Frame {
    fn advance_fwd(&mut self, count: usize, allow_last: bool) -> CmdResult {
        let old_pos = self.dot();
        let new_line = old_pos.line + count;
        // Can't advance to last blank line if allow_last is false
        let last_line = if allow_last { new_line } else { new_line + 1 };
        if last_line >= self.line_count() && !allow_last {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        self.set_mark_at(MarkId::Equals, old_pos);
        self.set_dot(Position::new(new_line, 0));
        CmdResult::Success
    }

    fn return_fwd(&mut self, count: usize) -> CmdResult {
        let old_pos = self.dot();
        let new_line = old_pos.line + count;
        let num_lines = self.line_count();

        // If target is beyond the buffer, insert newlines to extend it
        if new_line >= num_lines {
            let lines_to_add = new_line - num_lines + 1;
            let last_line = num_lines.saturating_sub(1);
            let last_line_len = self.line_length_excluding_newline(last_line);
            self.insert_at(
                Position::new(last_line, last_line_len),
                &"\n".repeat(lines_to_add),
            );
        }

        let dot_col = if self.options.auto_indent {
            self.line_indent(old_pos.line)
        } else {
            self.left_margin
        };

        self.set_mark_at(MarkId::Equals, old_pos);
        self.set_dot(Position::new(new_line, dot_col));
        CmdResult::Success
    }

    fn line_indent(&self, line_number: usize) -> usize {
        let line = self.rope.line(line_number);
        if let Some(pos) = line.chars().position(|ch| !ch.is_whitespace()) {
            pos
        } else {
            self.left_margin
        }
    }

    fn advance_back(&mut self, count: usize) -> CmdResult {
        let old_pos = self.dot();
        if old_pos.line < count {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        self.set_mark_at(MarkId::Equals, old_pos);
        self.set_dot(Position::new(old_pos.line.saturating_sub(count), 0));
        CmdResult::Success
    }

    fn advance_begin(&mut self) -> CmdResult {
        self.set_mark_at(MarkId::Equals, self.dot());
        self.set_dot(Position::new(0, 0));
        CmdResult::Success
    }

    fn advance_end(&mut self) -> CmdResult {
        self.set_mark_at(MarkId::Equals, self.dot());
        self.set_dot(Position::new(self.line_count(), 0));
        CmdResult::Success
    }

    fn advance_to(&mut self, target: Option<Position>) -> CmdResult {
        if let Some(pos) = target {
            self.set_mark_at(MarkId::Equals, self.dot());
            self.set_dot(Position::new(pos.line, 0));
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::MarkNotDefined)
        }
    }
}

// Private implementation helpers for Jump
impl Frame {
    fn jump_fwd(&mut self, count: usize) -> CmdResult {
        let old_pos = self.dot();
        self.set_mark_at(MarkId::Equals, old_pos);
        self.set_dot(Position::new(old_pos.line, old_pos.column + count));
        CmdResult::Success
    }

    fn jump_back(&mut self, count: usize) -> CmdResult {
        let old_pos = self.dot();
        if old_pos.column < count {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        self.set_mark_at(MarkId::Equals, old_pos);
        self.set_dot(Position::new(
            old_pos.line,
            old_pos.column.saturating_sub(count),
        ));
        CmdResult::Success
    }

    fn jump_begin(&mut self) -> CmdResult {
        let old_pos = self.dot();
        self.set_mark_at(MarkId::Equals, old_pos);
        self.set_dot(Position::new(old_pos.line, 0));
        CmdResult::Success
    }

    fn jump_end(&mut self) -> CmdResult {
        let old_pos = self.dot();
        let line_len = self.line_length_excluding_newline(old_pos.line);
        if line_len < old_pos.column {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        self.set_mark_at(MarkId::Equals, old_pos);
        self.set_dot(Position::new(old_pos.line, line_len));
        CmdResult::Success
    }

    fn jump_to(&mut self, target: Option<Position>) -> CmdResult {
        if let Some(pos) = target {
            self.set_mark_at(MarkId::Equals, self.dot());
            self.set_dot(pos);
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::MarkNotDefined)
        }
    }
}
