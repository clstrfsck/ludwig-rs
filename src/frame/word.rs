//! Word processing commands: YA (word advance), YD (word delete), YS (line squeeze).

use std::cmp::min;

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::position::{Position, line_length_excluding_newline};

use super::Frame;

/// Commands for word-level text processing.
pub trait WordCommands {
    /// YA: Move to the beginning of the Nth word.
    /// `0YA` = beginning of current word.
    /// `>YA` = beginning of next paragraph.
    /// `<YA` = beginning of current paragraph.
    fn cmd_word_advance(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Ditto Up ("): Copy character(s) from line above at current column.
    fn cmd_ditto_up(&mut self, lead_param: LeadParam) -> CmdResult;

    /// Ditto Down ('): Copy character(s) from line below at current column.
    fn cmd_ditto_down(&mut self, lead_param: LeadParam) -> CmdResult;
}

impl WordCommands for Frame {
    fn cmd_word_advance(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.word_advance_forward(1),
            LeadParam::Pint(0) => self.word_advance_backward(0),
            LeadParam::Pint(n) => self.word_advance_forward(n),
            LeadParam::Minus => self.word_advance_backward(1),
            LeadParam::Nint(n) => self.word_advance_backward(n),
            LeadParam::Pindef => self.word_advance_paragraph_forward(),
            LeadParam::Nindef => self.word_advance_paragraph_backward(),
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }

    fn cmd_ditto_up(&mut self, lead_param: LeadParam) -> CmdResult {
        let dot = self.dot();
        if dot.line == 0 {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        let count = match lead_param {
            LeadParam::None | LeadParam::Plus => 1,
            LeadParam::Pint(n) => n,
            LeadParam::Pindef => {
                let line_len = line_length_excluding_newline(&self.rope, dot.line - 1);
                if dot.column <= line_len {
                    line_len - dot.column
                } else {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            // FIXME: Negative values only work with overtype which is not implemented yet
            LeadParam::Minus => return CmdResult::Failure(CmdFailure::NotImplemented),
            LeadParam::Nint(_) => return CmdResult::Failure(CmdFailure::NotImplemented),
            LeadParam::Nindef => return CmdResult::Failure(CmdFailure::NotImplemented),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        self.ditto(-1, count)
    }

    fn cmd_ditto_down(&mut self, lead_param: LeadParam) -> CmdResult {
        let dot = self.dot();
        if dot.line + 1 >= self.lines() {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        let count = match lead_param {
            LeadParam::None | LeadParam::Plus => 1,
            LeadParam::Pint(n) => n,
            LeadParam::Pindef => {
                let line_len = line_length_excluding_newline(&self.rope, dot.line + 1);
                if dot.column <= line_len {
                    line_len - dot.column
                } else {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            // FIXME: Negative values only work with overtype which is not implemented yet
            LeadParam::Minus => return CmdResult::Failure(CmdFailure::NotImplemented),
            LeadParam::Nint(_) => return CmdResult::Failure(CmdFailure::NotImplemented),
            LeadParam::Nindef => return CmdResult::Failure(CmdFailure::NotImplemented),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        self.ditto(1, count)
    }
}

fn line_is_blank(rope: &ropey::Rope, line: usize) -> bool {
    rope.line(line).chars().all(|ch| ch.is_whitespace())
}

// Private helpers for word commands
impl Frame {
    /// Move forward to the start of the next paragraph.
    /// A paragraph is defined as a block of non-empty lines separated by empty lines.
    fn word_advance_paragraph_forward(&mut self) -> CmdResult {
        // Get to blank line between paragraphs
        let dot = self.dot();
        let mut line = dot.line;
        while line < self.lines() && !line_is_blank(&self.rope, line) {
            line += 1;
        }
        let new_pos = match self.find_next_word_start_from(Position::new(line, 0)) {
            Some(new_pos) => new_pos,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };

        self.set_mark_at(MarkId::Equals, dot);
        self.set_dot(new_pos);
        CmdResult::Success
    }

    fn word_advance_paragraph_backward(&mut self) -> CmdResult {
        let dot = self.dot();
        let mut line = dot.line;

        // Find non-blank line in paragraph
        while line > 0 && line_is_blank(&self.rope, line) {
            line -= 1;
        }
        // Find blank line separating this para from previous
        while line > 0 && !line_is_blank(&self.rope, line) {
            line -= 1;
        }
        // Find first non-blank
        while line_is_blank(&self.rope, line) {
            if line + 1 >= self.lines() {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
            line += 1;
        }
        // This line is not blank, so find first non-blank char
        let line_data = self.rope.line(line);
        let pos = line_data
            .chars()
            .position(|ch| !ch.is_whitespace())
            .unwrap_or(0);

        self.set_mark_at(MarkId::Equals, dot);
        self.set_dot(Position::new(line, pos));
        CmdResult::Success
    }

    /// Move backwards count words.  If count == 0, move to start of current word.
    fn word_advance_backward(&mut self, count: usize) -> CmdResult {
        let dot = self.dot();
        let mut line = dot.line;
        let mut pos = min(dot.column, line_length_excluding_newline(&self.rope, line));

        for i in 0..=count {
            // Find start of previous word (or current word if i==0)
            match self.find_prev_word_start_from(line, pos) {
                Some((new_line, new_pos)) => {
                    line = new_line;
                    pos = new_pos;
                }
                None => {
                    return if i == 0 {
                        CmdResult::Success // At beginning already
                    } else {
                        CmdResult::Failure(CmdFailure::OutOfRange)
                    };
                }
            }
        }

        self.set_mark_at(MarkId::Equals, dot);
        self.set_dot(Position::new(line, pos));
        CmdResult::Success
    }

    /// Find the start of the word to the left of (line, col).
    /// Returns None if at the beginning of the file.
    fn find_prev_word_start_from(&self, mut line: usize, mut col: usize) -> Option<(usize, usize)> {
        // Move to previous non-empty line if needed
        if col == 0 {
            (line, col) = self.goto_prev_nonempty_line(line)?;
        }

        // Skip whitespace backwards
        let line_chars = self.rope.line(line);
        while col > 0 && line_chars.char(col - 1).is_whitespace() {
            col -= 1;
        }

        // If still at whitespace, go to previous line
        if col == 0 && line_chars.char(0).is_whitespace() {
            (line, col) = self.goto_prev_nonempty_line(line)?;
        }

        // Scan backwards to find word start
        let line_chars = self.rope.line(line);
        while col > 0 && !line_chars.char(col - 1).is_whitespace() {
            col -= 1;
        }

        // Adjust for leading space
        if col < line_chars.len_chars() && line_chars.char(col) == ' ' {
            col += 1;
        }

        Some((line, col))
    }

    /// Move to the previous non-empty line. Returns None if at beginning.
    fn goto_prev_nonempty_line(&self, mut line: usize) -> Option<(usize, usize)> {
        loop {
            if line == 0 {
                return None;
            }
            line -= 1;
            let len = line_length_excluding_newline(&self.rope, line);
            if len > 0 {
                return Some((line, len));
            }
        }
    }

    /// Move forward to the start of the Nth word.
    fn word_advance_forward(&mut self, count: usize) -> CmdResult {
        let original_dot = self.dot();
        let mut pos = original_dot;

        for _ in 0..count {
            match self.find_next_word_start_from(pos) {
                Some(new_pos) => pos = new_pos,
                None => return CmdResult::Failure(CmdFailure::OutOfRange),
            }
        }

        self.set_mark_at(MarkId::Equals, original_dot);
        self.set_dot(pos);
        CmdResult::Success
    }

    /// Find the start of the next word after `pos`.
    fn find_next_word_start_from(&self, pos: Position) -> Option<Position> {
        let mut line = pos.line;
        let mut col = pos.column;

        // First, skip over the current word (non-space chars)
        while line < self.lines() {
            let line_len = line_length_excluding_newline(&self.rope, line);
            let line_start = self.rope.line_to_char(line);

            while col < line_len {
                let ch = self.rope.char(line_start + col);
                if ch.is_ascii_whitespace() {
                    break;
                }
                col += 1;
            }

            // Skip spaces
            while col < line_len {
                let ch = self.rope.char(line_start + col);
                if !ch.is_ascii_whitespace() {
                    return Some(Position::new(line, col));
                }
                col += 1;
            }

            // Move to next line
            line += 1;
            col = 0;

            // If next line starts with non-space, that's the word start
            if line < self.lines() {
                let next_line_len = line_length_excluding_newline(&self.rope, line);
                if next_line_len > 0 {
                    let next_line_start = self.rope.line_to_char(line);
                    let ch = self.rope.char(next_line_start);
                    if !ch.is_ascii_whitespace() {
                        return Some(Position::new(line, 0));
                    }
                }
            }
        }
        None
    }

    /// Ditto: copy character(s) from line above (direction=-1) or below (direction=1).
    fn ditto(&mut self, direction: isize, count: usize) -> CmdResult {
        let dot = self.dot();
        let source_line = (dot.line as isize + direction) as usize;

        if direction < 0 && dot.line == 0 {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        if direction > 0 && source_line >= self.lines() {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }

        let source_line_len = line_length_excluding_newline(&self.rope, source_line);
        let source_line_start = self.rope.line_to_char(source_line);

        let original_dot = dot;

        // Copy `count` chars from the source line at dot.column
        let mut chars_to_insert = String::new();
        for i in 0..count {
            let col = dot.column + i;
            let ch = if col < source_line_len {
                self.rope.char(source_line_start + col)
            } else {
                ' '
            };
            chars_to_insert.push(ch);
        }

        // Insert at dot
        self.insert(&chars_to_insert);
        self.set_mark(MarkId::Modified);
        self.set_mark_at(MarkId::Equals, original_dot);
        CmdResult::Success
    }
}
