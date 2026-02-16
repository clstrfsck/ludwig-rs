//! Mark system for tracking positions in the frame.
//!
//! Marks are named positions that automatically update when the frame is modified.

use std::collections::HashMap;

use crate::position::Position;

/// A unique identifier for a mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkId {
    Dot,
    Equals,
    Last,
    Modified,
    Numbered(u8),
}

/// Manages all marks in a frame.
#[derive(Debug)]
pub struct MarkSet {
    marks: HashMap<MarkId, Position>,
}

pub const NUMBERED_MARK_RANGE: std::ops::RangeInclusive<u8> = 1u8..=9u8;

impl Default for MarkSet {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkSet {
    /// Create a new empty mark set with the dot mark at position (0, 0).
    pub fn new() -> Self {
        let mut marks = HashMap::new();
        marks.insert(MarkId::Dot, Position::zero());
        Self { marks }
    }

    /// Get a mark by ID.
    pub fn get(&self, id: MarkId) -> Option<Position> {
        self.marks.get(&id).copied()
    }

    /// Set the position of a mark.
    pub fn set(&mut self, id: MarkId, position: Position) {
        if let MarkId::Numbered(n) = id
            && !NUMBERED_MARK_RANGE.contains(&n)
        {
            return;
        }
        self.marks.insert(id, position);
    }

    /// Unset the position of a mark.
    pub fn unset(&mut self, id: MarkId) {
        if id != MarkId::Dot {
            self.marks.remove(&id);
        }
    }

    /// Get the dot position.
    pub fn dot(&self) -> Position {
        *self.marks.get(&MarkId::Dot).unwrap()
    }

    /// Set the dot position.
    pub fn set_dot(&mut self, position: Position) {
        self.set(MarkId::Dot, position);
    }

    /// Update all marks after an insertion.
    ///
    /// - `at`: The position where text was inserted
    /// - `lines_added`: Number of complete lines added (from newlines in inserted text)
    /// - `end_column`: The column position after the insertion on the final line
    pub fn update_after_insert(&mut self, at: Position, lines_added: usize, end_column: usize) {
        for pos in self.marks.values_mut() {
            // Marks before the insertion point don't move
            if pos.line < at.line || (pos.line == at.line && pos.column < at.column) {
                continue;
            }

            // Marks after insertion point on the same line
            if pos.line == at.line {
                if lines_added > 0 {
                    // Text was split, mark moves to new line
                    pos.line = at.line + lines_added;
                    pos.column = pos.column - at.column + end_column;
                } else {
                    // Single line insert, shift column
                    pos.column += end_column;
                }
                continue;
            }

            // Marks on lines after the insertion line
            pos.line += lines_added;
        }
    }

    /// Update all marks after a deletion.
    ///
    /// - `from`: Start position of deletion (inclusive)
    /// - `to`: End position of deletion (exclusive)
    pub fn update_after_delete(&mut self, from: Position, to: Position) {
        // Ensure from <= to
        let (from, to) = if from <= to { (from, to) } else { (to, from) };

        for pos in self.marks.values_mut() {
            // Marks before the deletion range don't move
            if *pos <= from {
                continue;
            }

            // Marks in the deletion range move to start of deletion
            if *pos < to {
                *pos = from;
                continue;
            }

            // Marks after the deletion range
            if pos.line == to.line {
                // Same line as end of deletion
                pos.line = from.line;
                pos.column = from.column + (pos.column - to.column);
            } else {
                // Lines after the deletion
                pos.line -= to.line - from.line;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mark_creation() {
        let mut marks = MarkSet::new();

        // Dot should exist by default
        assert_eq!(marks.dot(), Position::zero());

        // Create a new mark
        let id = MarkId::Numbered(1);
        assert_eq!(marks.get(id), None);
        marks.set(id, Position::new(5, 10));
        assert_eq!(marks.get(id).unwrap(), Position::new(5, 10));
    }

    #[test]
    fn test_insert_updates_marks() {
        let mut marks = MarkSet::new();

        // Set dot at line 1, column 5
        marks.set_dot(Position::new(1, 5));

        // Create a mark after dot
        let after_mark = MarkId::Numbered(1);
        marks.set(after_mark, Position::new(1, 10));

        // Create a mark before dot (should not move)
        let before_mark = MarkId::Numbered(2);
        marks.set(before_mark, Position::new(0, 3));

        // Simulate inserting "abc" (3 chars, no newlines) at line 1, column 3
        marks.update_after_insert(Position::new(1, 3), 0, 3);

        // Before mark: unchanged
        assert_eq!(marks.get(before_mark).unwrap(), Position::new(0, 3));

        // dot: column shifted by 3
        assert_eq!(marks.dot(), Position::new(1, 8));

        // After mark: column shifted by 3
        assert_eq!(marks.get(after_mark).unwrap(), Position::new(1, 13));
    }

    #[test]
    fn test_insert_with_newlines() {
        let mut marks = MarkSet::new();

        marks.set_dot(Position::new(0, 5));
        let after = MarkId::Numbered(1);
        marks.set(after, Position::new(0, 8));

        // Insert text with a newline at column 3: "ab\ncd" -> 1 line added, ends at column 2
        marks.update_after_insert(Position::new(0, 3), 1, 2);

        // Dot was at column 5, which is after the insert point at column 3
        // After "ab\n", we're on line 1. The remaining "cd" is 2 chars.
        // Dot's new position: line 1, column = (5 - 3) + 2 = 4
        assert_eq!(marks.dot(), Position::new(1, 4));

        // After mark at column 8: line 1, column = (8 - 3) + 2 = 7
        assert_eq!(marks.get(after).unwrap(), Position::new(1, 7));
    }

    #[test]
    fn test_delete_updates_marks() {
        let mut marks = MarkSet::new();

        marks.set_dot(Position::new(1, 10));
        let inside = MarkId::Numbered(1);
        marks.set(inside, Position::new(1, 7));
        let before = MarkId::Numbered(2);
        marks.set(before, Position::new(0, 5));

        // Delete from (1, 5) to (1, 8)
        marks.update_after_delete(Position::new(1, 5), Position::new(1, 8));

        // Before: unchanged
        assert_eq!(marks.get(before).unwrap(), Position::new(0, 5));

        // Inside deletion: moves to start of deletion
        assert_eq!(marks.get(inside).unwrap(), Position::new(1, 5));

        // After deletion: column adjusted
        // Dot was at 10, deletion removed 3 chars (5 to 8)
        // New column: 5 + (10 - 8) = 7
        assert_eq!(marks.dot(), Position::new(1, 7));
    }
}
