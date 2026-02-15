//! The main Frame type that combines a Rope with marks and handles virtual space.

use std::fmt;

use ropey::Rope;

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::{MarkId, MarkSet};
use crate::position::{line_length_excluding_newline, Position};
use crate::trail_param::TrailParam;

/// An editable text frame with support for virtual space and marks.
#[derive(Debug, Default)]
pub struct Frame {
    /// The underlying rope data structure.
    rope: Rope,
    /// All marks (including dot) in this frame.
    marks: MarkSet,
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.rope)
    }
}

// Constructors
impl Frame {
    /// Create a new empty frame.
    pub fn new() -> Self {
        Self {
            rope: Rope::new(),
            marks: MarkSet::new(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        Self {
            rope: Rope::from_str(s),
            marks: MarkSet::new(),
        }
    }
}

// Editor commands
impl Frame {
    pub fn cmd_advance(&mut self, lead_param: LeadParam) -> CmdResult {
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

    pub fn cmd_jump(&mut self, lead_param: LeadParam) -> CmdResult {
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

    pub fn cmd_delete_char(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.cmd_del_forward(1),
            LeadParam::Pint(n) => self.cmd_del_forward(n),
            LeadParam::Pindef => self.cmd_del_forward(line_length_excluding_newline(&self.rope, self.dot().line)),
            LeadParam::Minus => self.cmd_del_backward(1),
            LeadParam::Nint(n) => self.cmd_del_backward(n),
            LeadParam::Nindef => self.cmd_del_backward(self.dot().column),
            LeadParam::Marker(id) => self.cmd_del_to_mark(id)
        }
    }

    pub fn cmd_insert_text(&mut self, lead_param: LeadParam, text: &TrailParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.cmd_ins_text(1, &text.str),
            LeadParam::Pint(n) => self.cmd_ins_text(n, &text.str),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    pub fn cmd_insert_char(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.insert_chars(1, false),
            LeadParam::Pint(n) => self.insert_chars(n, false),
            LeadParam::Minus => self.insert_chars(1, true),
            LeadParam::Nint(n) => self.insert_chars(n, true),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    pub fn cmd_overtype_text(&mut self, lead_param: LeadParam, text: &TrailParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.cmd_ovr_text(1, &text.str),
            LeadParam::Pint(n) => self.cmd_ovr_text(n, &text.str),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }
}

// Advance
impl Frame {
    fn advance_fwd(&mut self, count: usize) -> CmdResult {
        let old_pos = self.dot();
        let new_line = old_pos.line + count;
        if new_line > self.lines() {
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

// Jump
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
        self.set_dot(Position::new(old_pos.line, old_pos.column.saturating_sub(count)));
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

// Delete
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
        let final_dot = Position::new(start_position.line, start_position.column.saturating_sub(count));
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

    /// Delete text from `from` to `to` (exclusive).
    ///
    /// Positions are clamped to actual text (virtual space is ignored).
    /// Updates all marks appropriately.
    fn delete(&mut self, from: Position, to: Position) -> bool {
        // Ensure from <= to
        let (from, to) = if from <= to { (from, to) } else { (to, from) };

        if from.clamp_to_text(&self.rope) == to.clamp_to_text(&self.rope) {
            return false; // Nothing to delete
        }

        self.materialize_virtual_space(from);
        let clamp_to = to.clamp_to_text(&self.rope);

        let from_idx = from.to_char_index(&self.rope);
        let to_idx = clamp_to.to_char_index(&self.rope);

        // Delete from the rope
        self.rope.remove(from_idx..to_idx);

        // Update all marks
        self.marks.update_after_delete(from, clamp_to);
        true
    }
}

// Insert / Overwrite
impl Frame {
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

    /// Materialize virtual space at a position by padding with spaces.
    ///
    /// If the position is not in virtual space, this is a no-op.
    /// Returns the position (unchanged, since the virtual position is now real).
    ///
    /// Note: This does NOT update marks for the space padding, because the spaces
    /// are being added to "catch up" to where marks already are in virtual space.
    /// Marks in virtual space on this line are conceptually already past the line end,
    /// so adding spaces to reach them doesn't change their logical position.
    fn materialize_virtual_space(&mut self, pos: Position) {
        let total_lines = self.rope.len_lines();

        // First, add lines if needed
        if pos.line >= total_lines {
            // Need to add newlines to reach the desired line
            let lines_to_add = pos.line - total_lines + 1;

            // Make sure the last line ends with a newline before adding more
            let len = self.rope.len_chars();
            if len > 0 {
                let last_char = self.rope.char(len - 1);
                if last_char != '\n' && last_char != '\r' {
                    self.rope.insert_char(len, '\n');
                }
            }

            // Add the required newlines
            self.rope.insert(self.rope.len_chars(), &"\n".repeat(lines_to_add));
        }

        // Now pad the line with spaces if needed
        let line_len = line_length_excluding_newline(&self.rope, pos.line);
        if pos.column > line_len {
            let spaces_needed = pos.column - line_len;
            let line_start = self.rope.line_to_char(pos.line);
            let insert_pos = line_start + line_len;

            self.rope.insert(insert_pos, &" ".repeat(spaces_needed));
        }
    }

    /// Insert text at the current dot position.
    ///
    /// If dot is in virtual space, materializes the space first.
    /// Updates all marks appropriately.
    /// Dot ends up at the end of the inserted text.
    fn insert(&mut self, text: &str) {
        self.insert_at(self.dot(), text);
    }

    /// Insert text at a specific position.
    ///
    /// If the position is in virtual space, materializes the space first.
    /// Updates all marks appropriately.
    fn insert_at(&mut self, pos: Position, text: &str) {
        if text.is_empty() {
            return;
        }

        // Materialize virtual space if needed
        self.materialize_virtual_space(pos);

        // Calculate the char index for insertion
        let char_idx = pos.to_char_index(&self.rope);

        // Insert the text
        self.rope.insert(char_idx, text);

        // Calculate how the insertion affects positions
        let (lines_added, end_column) = calculate_insert_effect(text);

        // Update all marks
        self.marks.update_after_insert(pos, lines_added, end_column);
    }

    /// Overtype (replace) text at the current dot position.
    ///
    /// This replaces existing characters with the new text.
    /// If dot is in virtual space, materializes the space first.
    /// If the text extends beyond the line, the extra characters are inserted.
    fn overtype(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }

        let pos = self.dot();

        // Materialize virtual space if needed
        self.materialize_virtual_space(pos);

        // Figure out how many characters we can replace on this line
        let line_len = line_length_excluding_newline(&self.rope, pos.line);
        let chars_after_cursor = line_len.saturating_sub(pos.column);

        // Count chars in text (handling multi-line text)
        let first_line_chars = line_length_excluding_newline(&Rope::from_str(text), 0);
        let chars_to_replace = first_line_chars.min(chars_after_cursor);

        let (pos, to_insert) = if chars_to_replace > 0 {
            let overwrite_position = pos.to_char_index(&self.rope);
            self.rope.remove(overwrite_position..(overwrite_position + chars_to_replace));
            self.rope.insert(overwrite_position, &text[..chars_to_replace]);
            // Dot moves to the end of the overwritten part
            let new_dot = Position::new(pos.line, pos.column + chars_to_replace);
            self.set_dot(new_dot);
            (new_dot, &text[chars_to_replace..])
        } else {
            (pos, text)
        };

        // Now insert the text
        self.insert_at(pos, to_insert);
    }
}

// Miscellaneous methods
impl Frame {
    pub fn dot(&self) -> Position {
        self.marks.dot()
    }

    pub fn set_dot(&mut self, position: Position) {
        let use_line = position.line.min(self.rope.len_lines().saturating_sub(1));
        self.marks.set_dot(Position::new(use_line, position.column));
    }

    /// Create a new mark at the current dot position.
    fn set_mark(&mut self, id: MarkId) {
        self.marks.set(id, self.dot())
    }

    /// Create a new mark at a specific position.
    fn set_mark_at(&mut self, id: MarkId, pos: Position) {
        self.marks.set(id, pos)
    }

    /// Unset a mark.
    fn unset_mark(&mut self, id: MarkId) {
        self.marks.unset(id);
    }

    /// Get the position of a mark.
    fn mark_position(&self, id: MarkId) -> Option<Position> {
        self.marks.get(id)
    }

    fn lines(&self) -> usize {
        if self.rope.len_chars() == 0 {
            return 0;
        }
        let lines = self.rope.len_lines().saturating_sub(1);
        if line_length_excluding_newline(&self.rope, lines) > 0 {
            lines + 1
        } else {
            lines
        }
    }
}

/// Calculate the effect of inserting text: (lines_added, end_column)
///
/// Uses Rope to handle multi-line text correctly.
fn calculate_insert_effect(text: &str) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }
    let r = Rope::from_str(text);
    let lines = r.len_lines();
    (lines - 1, line_length_excluding_newline(&r, lines - 1))
}

#[cfg(test)]
mod tests;
