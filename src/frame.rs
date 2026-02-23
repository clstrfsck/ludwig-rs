//! The main Frame type that combines a Rope with marks and handles virtual space.

mod edit;
mod motion;
mod predicate;
mod search;
mod word;

pub use edit::{CaseMode, EditCommands};
pub use motion::MotionCommands;
pub use predicate::PredicateCommands;
pub use search::SearchCommands;
pub use word::WordCommands;

use std::collections::HashMap;
use std::fmt;

use ropey::Rope;

use crate::marks::{MarkId, MarkSet};
use crate::position::{Position, calculate_insert_effect};

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
        let mut r = Rope::from_str(s);
        if !s.is_empty() && !s.ends_with('\n') {
            r.insert_char(r.len_chars(), '\n');
        }
        Self {
            rope: r,
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

    pub fn get_mark(&self, id: MarkId) -> Option<Position> {
        self.marks.get(id)
    }

    /// Create a new mark at the current dot position
    fn set_mark(&mut self, id: MarkId) {
        self.marks.set(id, self.dot())
    }

    /// Set a mark at a specific position
    pub fn set_mark_at(&mut self, id: MarkId, pos: Position) {
        self.marks.set(id, pos)
    }

    /// Unset a mark
    pub fn unset_mark(&mut self, id: MarkId) {
        self.marks.unset(id);
    }

    /// Get the number of lines in the frame
    pub fn line_count(&self) -> usize {
        if self.rope.len_chars() == 0 {
            return 0;
        }
        self.rope.len_lines()
    }

    /// Get the content of a line as a RopeSlice, including the trailing newline.
    /// Returns None if the line index is out of range.
    pub fn line_content(&self, line: usize) -> Option<ropey::RopeSlice<'_>> {
        if line >= self.line_count() {
            return None;
        }
        Some(self.rope.line(line))
    }

    /// Get the length of a line excluding its newline character.
    /// Returns 0 if the line index is out of range.
    pub fn line_length_excluding_newline(&self, line: usize) -> usize {
        if line >= self.line_count() {
            return 0;
        }

        let line_slice = self.rope.line(line);
        let len = line_slice.len_chars();

        // Check for line endings and exclude them
        if len >= 1 {
            let last = line_slice.char(len - 1);
            if last == '\n' {
                if len >= 2 {
                    let second_last = line_slice.char(len - 2);
                    if second_last == '\r' {
                        return len - 2;
                    }
                }
                return len - 1;
            } else if last == '\r' {
                return len - 1;
            }
        }
        len
    }

    /// Get the length of a line excluding its newline character.
    /// Returns 0 if the line index is out of range.
    pub fn line_length_including_newline(&self, line: usize) -> usize {
        if line >= self.line_count() {
            return 0;
        }
        self.rope.line(line).len_chars()
    }

    /// Get a reference to the underlying rope (for advanced screen rendering).
    pub fn rope(&self) -> &Rope {
        &self.rope
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
        let line_len = self.line_length_excluding_newline(pos.line);
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
    pub fn insert_at(&mut self, pos: Position, text: &str) {
        if text.is_empty() {
            return;
        }

        // Materialize virtual space if needed
        self.materialize_virtual_space(pos);

        // Calculate the char index for insertion
        let char_idx = self.to_char_index(&pos);

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
        let line_len = self.line_length_excluding_newline(pos.line);
        let chars_after_cursor = line_len.saturating_sub(pos.column);

        // Count chars in text (handling multi-line text)
        let first_line_chars = Self::first_line_length(text);
        let chars_to_replace = first_line_chars.min(chars_after_cursor);

        let (pos, to_insert) = if chars_to_replace > 0 {
            let overwrite_position = self.to_char_index(&pos);
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
    pub fn delete(&mut self, from: Position, to: Position) -> bool {
        // Ensure from <= to
        let (from, to) = if from <= to { (from, to) } else { (to, from) };

        if self.clamp_to_text(&from) == self.clamp_to_text(&to) {
            return false; // Nothing to delete
        }

        self.materialize_virtual_space(from);
        let clamp_to = self.clamp_to_text(&to);

        let from_idx = self.to_char_index(&from);
        let to_idx = self.to_char_index(&clamp_to);

        // Delete from the rope
        self.rope.remove(from_idx..to_idx);

        // Update all marks
        self.marks.update_after_delete(from, clamp_to);
        true
    }

    fn first_line_length(text: &str) -> usize {
        text.find(['\r', '\n']).unwrap_or(text.len())
    }

    /// Convert this position to a char index in the rope.
    ///
    /// If the position is in virtual space, this returns the index at the
    /// end of the line (or end of the document if beyond the last line).
    pub fn to_char_index(&self, pos: &Position) -> usize {
        let total_lines = self.rope.len_lines();

        // Clamp line to valid range
        let line = pos.line.min(total_lines.saturating_sub(1));
        let line_start = self.rope.line_to_char(line);
        let line_len = self.line_length_excluding_newline(line);

        // Clamp column to actual line length
        let column = pos.column.min(line_len);

        line_start + column
    }

    /// Clamp this position to be within the actual text (no virtual space).
    pub fn clamp_to_text(&self, pos: &Position) -> Position {
        let total_lines = self.rope.len_lines();

        if total_lines == 0 {
            return Position::zero();
        }

        let line = pos.line.min(total_lines.saturating_sub(1));
        let line_len = self.line_length_excluding_newline(line);
        let column = pos.column.min(line_len);

        Position::new(line, column)
    }
}

/// Global registry of all frames, keyed by UPPERCASE name.
pub(crate) struct FrameRegistry {
    frames: HashMap<String, Frame>,
}

impl Default for FrameRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameRegistry {
    pub fn new() -> Self {
        Self {
            frames: HashMap::new(),
        }
    }

    /// Insert or replace a frame by name
    pub fn insert(&mut self, name: String, frame: Frame) {
        self.frames.insert(name, frame);
    }

    /// Look up a frame by name
    pub fn get(&self, name: &str) -> Option<&Frame> {
        self.frames.get(name)
    }

    /// Mutable look-up by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Frame> {
        self.frames.get_mut(name)
    }

    /// Test whether a frame exists.
    pub fn contains(&self, name: &str) -> bool {
        self.frames.contains_key(name)
    }
}

#[cfg(test)]
mod tests;
