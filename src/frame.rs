//! The main Frame type that combines a Rope with marks and handles virtual space.

mod edit;
mod motion;

pub use edit::EditCommands;
pub use motion::MotionCommands;

use std::fmt;

use ropey::Rope;

use crate::marks::{MarkId, MarkSet};
use crate::position::{Position, line_length_excluding_newline};

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

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self {
            rope: Rope::from_str(s),
            marks: MarkSet::new(),
        }
    }
}

// Core Frame methods (used by command implementations)
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
            self.rope
                .insert(self.rope.len_chars(), &"\n".repeat(lines_to_add));
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

    /// Insert text at the current dot position.
    ///
    /// If dot is in virtual space, materializes the space first.
    /// Updates all marks appropriately.
    /// Dot ends up at the end of the inserted text.
    fn insert(&mut self, text: &str) {
        self.insert_at(self.dot(), text);
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
            self.rope
                .remove(overwrite_position..(overwrite_position + chars_to_replace));
            self.rope
                .insert(overwrite_position, &text[..chars_to_replace]);
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
