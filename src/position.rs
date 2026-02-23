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

/// Get the length of a line excluding any trailing newline character.
/// FIXME: This is one of a handful of fns that take a Rope.
/// There should be some other abstraction that encapsulates Rope and provides
/// these utilities.
fn line_length_excluding_newline(rope: &Rope, line: usize) -> usize {
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

/// Calculate the effect of inserting text: (lines_added, end_column)
///
/// Uses Rope to handle multi-line text correctly.
pub(crate) fn calculate_insert_effect(text: &str) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }
    let r = Rope::from_str(text);
    let lines = r.len_lines();
    (lines - 1, line_length_excluding_newline(&r, lines - 1))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_insert_effect() {
        assert_eq!(calculate_insert_effect(""), (0, 0));
        assert_eq!(calculate_insert_effect("hello"), (0, 5));
        assert_eq!(calculate_insert_effect("hello\nworld"), (1, 5));
        assert_eq!(calculate_insert_effect("line1\nline2\n"), (2, 0));
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
