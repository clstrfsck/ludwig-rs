//! Motion commands for moving the cursor (Advance and Jump).

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::position::{Position, line_length_excluding_newline};

use super::Frame;

/// Commands for moving the cursor within the frame.
pub trait MotionCommands {
    /// Advance command - move to a different line.
    fn cmd_advance(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Jump command - move within the current line.
    fn cmd_jump(&mut self, lead_param: LeadParam) -> CmdResult;
}

impl MotionCommands for Frame {
    fn cmd_advance(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.advance_fwd(1),
            LeadParam::Pint(n) => self.advance_fwd(n),
            LeadParam::Pindef => self.advance_end(),
            LeadParam::Minus => self.advance_back(1),
            LeadParam::Nint(n) => self.advance_back(n),
            LeadParam::Nindef => self.advance_begin(),
            LeadParam::Marker(id) => self.advance_to(self.mark_position(id)),
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
            LeadParam::Marker(id) => self.jump_to(self.mark_position(id)),
        }
    }
}

// Private implementation helpers for Advance
impl Frame {
    fn advance_fwd(&mut self, count: usize) -> CmdResult {
        let old_pos = self.dot();
        let new_line = old_pos.line + count;
        if new_line >= self.lines() {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        self.set_mark_at(MarkId::Last, old_pos);
        self.set_dot(Position::new(new_line, 0));
        CmdResult::Success
    }

    fn advance_back(&mut self, count: usize) -> CmdResult {
        let old_pos = self.dot();
        if old_pos.line < count {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        self.set_mark_at(MarkId::Last, old_pos);
        self.set_dot(Position::new(old_pos.line.saturating_sub(count), 0));
        CmdResult::Success
    }

    fn advance_begin(&mut self) -> CmdResult {
        self.set_mark_at(MarkId::Last, self.dot());
        self.set_dot(Position::new(0, 0));
        CmdResult::Success
    }

    fn advance_end(&mut self) -> CmdResult {
        self.set_mark_at(MarkId::Last, self.dot());
        self.set_dot(Position::new(self.lines(), 0));
        CmdResult::Success
    }

    fn advance_to(&mut self, target: Option<Position>) -> CmdResult {
        if let Some(pos) = target {
            self.set_mark_at(MarkId::Last, self.dot());
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
        self.set_mark_at(MarkId::Last, old_pos);
        self.set_dot(Position::new(old_pos.line, old_pos.column + count));
        CmdResult::Success
    }

    fn jump_back(&mut self, count: usize) -> CmdResult {
        let old_pos = self.dot();
        if old_pos.column < count {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        self.set_mark_at(MarkId::Last, old_pos);
        self.set_dot(Position::new(
            old_pos.line,
            old_pos.column.saturating_sub(count),
        ));
        CmdResult::Success
    }

    fn jump_begin(&mut self) -> CmdResult {
        let old_pos = self.dot();
        self.set_mark_at(MarkId::Last, old_pos);
        self.set_dot(Position::new(old_pos.line, 0));
        CmdResult::Success
    }

    fn jump_end(&mut self) -> CmdResult {
        let old_pos = self.dot();
        let line_len = line_length_excluding_newline(&self.rope, old_pos.line);
        if line_len < old_pos.column {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        self.set_mark_at(MarkId::Last, old_pos);
        self.set_dot(Position::new(old_pos.line, line_len));
        CmdResult::Success
    }

    fn jump_to(&mut self, target: Option<Position>) -> CmdResult {
        if let Some(pos) = target {
            self.set_mark_at(MarkId::Last, self.dot());
            self.set_dot(pos);
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::MarkNotDefined)
        }
    }
}
