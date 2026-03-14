//! The main Frame type that combines a Rope with marks and handles virtual space.

mod edit;
mod line_format;
mod motion;
pub(crate) mod params;
mod predicate;
mod search;
mod word;

pub use edit::{CaseMode, EditCommands};
pub use line_format::LineFormatCommands;
pub use motion::MotionCommands;
pub use predicate::PredicateCommands;
pub use search::SearchCommands;
pub use word::WordCommands;

use std::collections::HashMap;
use std::fmt;
use std::ops::RangeBounds;

use ropey::Rope;

use crate::code::CompiledCode;
use crate::file_io::FileHandle;
use crate::marks::{MarkId, MarkSet};
use crate::position::Position;

/// Number of tab-stop slots (columns 0..=TAB_STOPS_LEN-1).
pub(crate) const TAB_STOPS_LEN: usize = 401;

/// Build the default tab-stop array: stops at every column where `col % 8 == 0`.
pub(crate) fn default_tab_stops() -> Vec<bool> {
    (0..TAB_STOPS_LEN).map(|i| i % 8 == 0).collect()
}

/// Options flags for a frame (EP `O=` sub-command).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameOptions {
    pub auto_indent: bool, // O=I
    pub auto_wrap: bool,   // O=W
    pub newline: bool,     // O=N
}

/// Keyboard mode (EP `K=` sub-command). Global to the session, not per-frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardMode {
    #[default]
    Overtype,
    Insert,
    Command,
}

/// An editable text frame with support for virtual space and marks.
#[derive(Debug)]
pub struct Frame {
    /// The name of the frame.
    name: String,
    /// The underlying rope data structure.
    rope: Rope,
    /// All marks (including dot) in this frame.
    marks: MarkSet,
    /// Compiled code for the frame
    code: Option<CompiledCode>,
    /// Left margin column (0-based). Text should start at or after this column.
    pub left_margin: usize,
    /// Right margin: maximum line length. Lines should be at most this many characters.
    pub right_margin: usize,
    /// Frame to return to after ER (set by ED when switching to this frame).
    pub return_frame_name: Option<String>,
    /// Option flags (auto-indent, auto-wrap, newline).
    pub options: FrameOptions,
    /// Tab-stop bitmap, indexed by 0-based column (len = TAB_STOPS_LEN).
    pub tab_stops: Vec<bool>,
    /// Top scroll margin (rows from top of screen, default 0).
    pub margin_top: usize,
    /// Bottom scroll margin (rows from bottom of screen, default 0).
    pub margin_bottom: usize,
    /// Input file opened on this frame (FI / FE).
    pub input_file: Option<FileHandle>,
    /// Output file opened on this frame (FO / FE).
    pub output_file: Option<FileHandle>,
}

impl Default for Frame {
    fn default() -> Self {
        Self::new("")
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.rope)
    }
}

// Constructors
impl Frame {
    /// Create a new empty frame with default parameters.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            rope: Rope::new(),
            marks: MarkSet::new(),
            code: None,
            left_margin: 0,
            right_margin: 80,
            return_frame_name: None,
            options: FrameOptions::default(),
            tab_stops: default_tab_stops(),
            margin_top: 0,
            margin_bottom: 0,
            input_file: None,
            output_file: None,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(name: &str, s: &str) -> Self {
        let mut r = Rope::from_str(s);
        if !s.is_empty() && !s.ends_with('\n') {
            r.insert_char(r.len_chars(), '\n');
        }
        Self {
            name: name.into(),
            rope: r,
            marks: MarkSet::new(),
            code: None,
            left_margin: 0,
            right_margin: 80,
            return_frame_name: None,
            options: FrameOptions::default(),
            tab_stops: default_tab_stops(),
            margin_top: 0,
            margin_bottom: 0,
            input_file: None,
            output_file: None,
        }
    }
}

// Core Frame methods (used by command implementations)
impl Frame {
    pub fn name(&self) -> &str {
        &self.name
    }

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

    /// Clear all text content and marks, leaving frame settings (margins, tab stops, etc.) intact.
    ///
    /// After this call the frame is equivalent to a freshly created empty frame with the
    /// same name and settings.  Dot is reset to (0, 0).
    pub fn clear_content(&mut self) {
        self.rope = Rope::new();
        self.marks = MarkSet::new();
        self.code = None;
    }

    /// Get the compiled code for the frame
    pub fn get_code(&self) -> Option<&CompiledCode> {
        self.code.as_ref()
    }

    /// Set the compiled code for the frame
    pub fn set_code(&mut self, code: CompiledCode) {
        self.code = Some(code)
    }

    /// Unset the compiled code for the frame
    pub fn clear_code(&mut self) {
        self.code = None
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
        line_length_excluding_newline(&self.rope, line)
    }

    /// Get the length of a line excluding its newline character.
    /// Returns 0 if the line index is out of range.
    pub fn line_length_including_newline(&self, line: usize) -> usize {
        if line >= self.line_count() {
            return 0;
        }
        self.rope.line(line).len_chars()
    }

    /// Returns true if the specified line is empty or consists entirely of whitespace.
    /// Returns true if the line index is out of range.
    pub fn is_blank_line(&self, line: usize) -> bool {
        if line >= self.line_count() {
            return true;
        }
        self.rope.line(line).chars().all(|ch| ch.is_whitespace())
    }

    /// Get a slice of the underlying data.
    /// FIXME: Come up with a better way of copying bits of rope around.
    pub fn slice<R>(&self, char_range: R) -> String
    where
        R: RangeBounds<usize>,
    {
        self.rope.slice(char_range).to_string()
    }

    /// Get the full text of the frame.
    /// FIXME: Come up with a better way of copying bits of rope around.
    pub fn text(&self) -> String {
        self.rope.to_string()
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

    /// Build a [`crate::pattern::MatchCtx`] for the given line.
    ///
    /// Returns `None` if `line_idx` is out of range.
    pub fn make_match_ctx(&self, line_idx: usize) -> Option<crate::pattern::MatchCtx> {
        if line_idx >= self.line_count() {
            return None;
        }
        let line_chars: Vec<char> = self
            .rope
            .line(line_idx)
            .chars()
            .filter(|&c| c != '\n' && c != '\r')
            .collect();
        Some(crate::pattern::MatchCtx {
            line: line_chars,
            dot_col: self.dot().column,
            left_margin: self.left_margin,
            right_margin: self.right_margin,
            line_idx,
            marks: self.marks.clone(),
        })
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

/// Get the length of a line excluding any trailing newline character.
pub(crate) fn line_length_excluding_newline(rope: &Rope, line: usize) -> usize {
    if line >= rope.len_lines() {
        return 0;
    }

    let line_slice = rope.line(line);
    let mut len = line_slice.len_chars();
    if len > 0 && line_slice.char(len - 1) == '\n' {
        len -= 1;
    }
    if len > 0 && line_slice.char(len - 1) == '\r' {
        len -= 1;
    }
    len
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
    pub(crate) fn new() -> Self {
        Self {
            frames: HashMap::new(),
        }
    }

    /// Insert or replace a frame by name
    pub(crate) fn insert(&mut self, name: String, frame: Frame) {
        self.frames.insert(name, frame);
    }

    /// Look up a frame by name
    pub(crate) fn get(&self, name: &str) -> Option<&Frame> {
        self.frames.get(name)
    }

    /// Mutable look-up by name
    pub(crate) fn get_mut(&mut self, name: &str) -> Option<&mut Frame> {
        self.frames.get_mut(name)
    }

    /// Test whether a frame exists.
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.frames.contains_key(name)
    }

    /// Remove a frame by name, returning it if it existed.
    pub(crate) fn remove(&mut self, name: &str) -> Option<Frame> {
        self.frames.remove(name)
    }

    /// Return all frame names (for iteration, e.g. fixing return pointers).
    pub(crate) fn names(&self) -> Vec<String> {
        self.frames.keys().cloned().collect()
    }

    pub fn sorted_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.frames.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests;
