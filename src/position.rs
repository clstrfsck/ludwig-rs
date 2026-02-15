//! Position types for the editor.
//!
//! We use line/column positions rather than byte offsets to naturally support
//! "virtual space" - positions beyond the end of a line.

use ropey::Rope;

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

    /// Returns true if this position is in virtual space for the given rope.
    pub fn is_virtual(&self, rope: &Rope) -> bool {
        if self.line >= rope.len_lines() {
            return true;
        }
        let line_len = line_length_excluding_newline(rope, self.line);
        self.column > line_len
    }

    /// Convert this position to a char index in the rope.
    ///
    /// If the position is in virtual space, this returns the index at the
    /// end of the line (or end of the document if beyond the last line).
    pub fn to_char_index(&self, rope: &Rope) -> usize {
        let total_lines = rope.len_lines();

        // Clamp line to valid range
        let line = self.line.min(total_lines.saturating_sub(1));
        let line_start = rope.line_to_char(line);
        let line_len = line_length_excluding_newline(rope, line);

        // Clamp column to actual line length
        let column = self.column.min(line_len);

        line_start + column
    }

    /// Create a position from a char index in the rope.
    pub fn from_char_index(rope: &Rope, char_idx: usize) -> Self {
        let char_idx = char_idx.min(rope.len_chars());
        let line = rope.char_to_line(char_idx);
        let line_start = rope.line_to_char(line);
        let column = char_idx - line_start;
        Self { line, column }
    }

    /// Clamp this position to be within the actual text (no virtual space).
    pub fn clamp_to_text(&self, rope: &Rope) -> Self {
        let total_lines = rope.len_lines();

        if total_lines == 0 {
            return Self::zero();
        }

        let line = self.line.min(total_lines.saturating_sub(1));
        let line_len = line_length_excluding_newline(rope, line);
        let column = self.column.min(line_len);

        Self { line, column }
    }
}

/// Get the length of a line excluding any trailing newline character.
pub fn line_length_excluding_newline(rope: &Rope, line: usize) -> usize {
    if line >= rope.len_lines() {
        return 0;
    }

    let line_slice = rope.line(line);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_virtual_space() {
        let rope = Rope::from_str("hello\nworld\n");

        // Position within text
        let pos = Position::new(0, 3);
        assert!(!pos.is_virtual(&rope));

        // Position at end of line
        let pos = Position::new(0, 5);
        assert!(!pos.is_virtual(&rope));

        // Position beyond end of line (virtual)
        let pos = Position::new(0, 10);
        assert!(pos.is_virtual(&rope));

        // Position on non-existent line (virtual)
        let pos = Position::new(100, 0);
        assert!(pos.is_virtual(&rope));
    }

    #[test]
    fn test_position_to_char_index() {
        let rope = Rope::from_str("hello\nworld\n");

        // Normal positions
        assert_eq!(Position::new(0, 0).to_char_index(&rope), 0);
        assert_eq!(Position::new(0, 3).to_char_index(&rope), 3);
        assert_eq!(Position::new(1, 0).to_char_index(&rope), 6);
        assert_eq!(Position::new(1, 3).to_char_index(&rope), 9);

        // Virtual space clamps to end of line
        assert_eq!(Position::new(0, 100).to_char_index(&rope), 5);
    }

    #[test]
    fn test_position_clamp_to_text() {
        let rope = Rope::from_str("hello\nworld\n");

        // Clamp virtual column to line length
        assert_eq!(
            Position::new(0, 100).clamp_to_text(&rope),
            Position::new(0, 5)
        );

        // Clamp virtual line to last line
        assert_eq!(
            Position::new(10, 2).clamp_to_text(&rope),
            Position::new(2, 0)
        );
    }

    #[test]
    fn test_line_length() {
        let rope = Rope::from_str("hello\nworld\n");
        assert_eq!(line_length_excluding_newline(&rope, 0), 5);
        assert_eq!(line_length_excluding_newline(&rope, 1), 5);

        // Last line (empty after final newline)
        assert_eq!(line_length_excluding_newline(&rope, 2), 0);
    }

    #[test]
    fn test_line_length_crlf() {
        // Test CRLF line endings
        let rope = Rope::from_str("hello\r\nworld\r\n");
        assert_eq!(line_length_excluding_newline(&rope, 0), 5);
        assert_eq!(line_length_excluding_newline(&rope, 1), 5);

        // Last line (empty after final CRLF)
        assert_eq!(line_length_excluding_newline(&rope, 2), 0);

        // Mixed line endings
        let rope = Rope::from_str("hello\r\nworld\n");
        assert_eq!(line_length_excluding_newline(&rope, 0), 5);
        assert_eq!(line_length_excluding_newline(&rope, 1), 5);

        // Lone CR (old Mac style)
        let rope = Rope::from_str("hello\rworld");
        assert_eq!(line_length_excluding_newline(&rope, 0), 5);
    }

    #[test]
    fn test_unicode_newline_not_supported() {
        const UNICODE_LINE_BREAKS: &[char] =
            &['\u{000B}', '\u{000C}', '\u{0085}', '\u{2028}', '\u{2029}'];
        for &ch in UNICODE_LINE_BREAKS {
            let rope = Rope::from_str(&format!("line1{}line2", ch));
            assert_eq!(line_length_excluding_newline(&rope, 0), 11);
            assert_eq!(rope.len_lines(), 1);
        }
    }
}
