//! Position types for the editor.
//!
//! We use line/column positions rather than byte offsets to naturally support
//! "virtual space" - positions beyond the end of a line.

use ropey::Rope;

use crate::frame::line_length_excluding_newline;

/// A position in the frame, represented as line and column.
///
/// Both `line` and `column` are 0-indexed.
///
/// The column can extend beyond the actual line length (virtual space).
/// When this happens, operations that modify the frame will first
/// "materialize" the virtual space by padding with spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl Position {
    /// Create a new position.
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    /// Create a position at the start of the frame.
    pub fn zero() -> Self {
        Self { line: 0, column: 0 }
    }

    /// Calculate the position after inserting the given text at this position.
    pub fn after_text(&self, text: &str) -> Position {
        if text.is_empty() {
            return *self;
        }
        let r = Rope::from_str(text);
        let lines_added = r.len_lines() - 1;
        let last_line_col = line_length_excluding_newline(&r, r.len_lines() - 1);
        if lines_added == 0 {
            Position::new(self.line, self.column + last_line_col)
        } else {
            Position::new(self.line + lines_added, last_line_col)
        }
    }
}
