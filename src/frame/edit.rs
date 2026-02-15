//! Text editing commands (insert, delete, overtype).

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::position::{Position, line_length_excluding_newline};
use crate::trail_param::TrailParam;

use super::Frame;

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
        if lead_param == LeadParam::None || lead_param == LeadParam::Plus {
            let original_dot = self.dot();
            // We are not going to extend the line, so just clamp the dot to the actual text.
            let clamped_dot = original_dot.clamp_to_text(&self.rope);
            self.set_dot(clamped_dot);
            self.insert_at(Position::new(clamped_dot.line, clamped_dot.column), "\n");
            self.set_mark(MarkId::Modified);
            self.set_mark_at(MarkId::Last, original_dot);
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::SyntaxError)
        }
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
        self.set_mark_at(MarkId::Last, original_dot);
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
        self.set_mark_at(MarkId::Last, original_dot);
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
        self.set_mark_at(MarkId::Last, last);
        CmdResult::Success
    }
}

// Private implementation helpers for deletion
impl Frame {
    fn cmd_del_forward(&mut self, count: usize) -> CmdResult {
        if self.delete_forward(count) {
            self.set_mark(MarkId::Modified);
        }
        self.unset_mark(MarkId::Last);
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
        self.unset_mark(MarkId::Last);
        self.set_mark_at(MarkId::Dot, final_dot);
        CmdResult::Success
    }

    fn cmd_del_to_mark(&mut self, mark_id: MarkId) -> CmdResult {
        if let Some(mark_pos) = self.mark_position(mark_id) {
            if self.delete(self.dot(), mark_pos) {
                self.set_mark(MarkId::Modified);
            }
            self.unset_mark(MarkId::Last);
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
        self.set_mark_at(MarkId::Last, last);
        CmdResult::Success
    }
}
