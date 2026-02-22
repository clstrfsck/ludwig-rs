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

    /// YD: Delete the same words that YA would advance over.
    fn cmd_word_delete(&mut self, lead_param: LeadParam) -> CmdResult;

    /// YS: Squeeze multiple consecutive spaces into single spaces within lines.
    fn cmd_line_squeeze(&mut self, lead_param: LeadParam) -> CmdResult;

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

    fn cmd_word_delete(&mut self, lead_param: LeadParam) -> CmdResult {
        if matches!(lead_param, LeadParam::Marker(_)) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        let original_dot = self.dot();

        // Step 1: Get to the start of the current word.
        // A single backward scan iteration that leaves dot at the beginning
        // of whatever word it's in.
        let word_start = match self.find_current_word_start(original_dot.line, original_dot.column)
        {
            Some((l, c)) => Position::new(l, c),
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };

        // Step 2: Advance word(s) from word_start to find the deletion endpoint.
        let advance_end = match self.find_word_delete_end(word_start, lead_param) {
            Some(pos) => pos,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };

        // Save advance_end column before any modification.
        // Used to re-establish the correct indentation if the deletion crossed a line.
        let advance_end_col = advance_end.column;

        // Step 3: Determine ordered deletion range.
        let (del_start, del_end) = if advance_end >= word_start {
            (word_start, advance_end)
        } else {
            (advance_end, word_start)
        };

        let start_line = del_start.line;
        let end_line = del_end.line;

        // Step 4: Delete text. If nothing is deleted (same position) treat as failure.
        if !self.delete(del_start, del_end) {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        self.set_dot(del_start);

        // Step 5: If the deletion crossed a line boundary, re-insert the newline.
        // The indent reproduces the original indentation of the advance-end position.
        if start_line != end_line {
            let indent = " ".repeat(advance_end_col);
            self.insert(&format!("\n{indent}"));
        }

        // Step 6: Update marks.
        self.set_mark_at(MarkId::Equals, original_dot);
        self.set_mark(MarkId::Modified);

        CmdResult::Success
    }

    fn cmd_line_squeeze(&mut self, lead_param: LeadParam) -> CmdResult {
        let (mut count, is_pindef) = match lead_param {
            LeadParam::Pindef => (usize::MAX, true),
            LeadParam::None | LeadParam::Plus => (1, false),
            LeadParam::Pint(n) => (n, false),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Pre-check (for non-pindef, non-zero count): verify that `count` consecutive
        // non-empty lines exist starting from dot.line, and that a line follows the
        // last one (needed to advance dot after processing).
        if !is_pindef && count > 0 {
            let start_line = self.dot().line;
            for i in 0..count {
                let check_line = start_line + i;
                if check_line >= self.lines()
                    || line_length_excluding_newline(&self.rope, check_line) == 0
                {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            if start_line + count >= self.lines() {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
        }

        // Main loop: process one line per iteration.
        while count > 0 {
            let line = self.dot().line;
            let line_len = line_length_excluding_newline(&self.rope, line);

            // Stop if current line is empty (pindef succeeds; pint checked by pre-check).
            if line_len == 0 {
                break;
            }

            // EOP check: must have a next line to advance dot into.
            if line + 1 >= self.lines() {
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }

            // Skip leading spaces (ASCII space only, matching C++ behaviour).
            let mut start = {
                let line_start = self.rope.line_to_char(line);
                let mut s = 0usize;
                while s < line_len && self.rope.char(line_start + s) == ' ' {
                    s += 1;
                }
                s
            };

            // Inner loop: find and squeeze runs of 2+ spaces.
            loop {
                let line_len = line_length_excluding_newline(&self.rope, line);
                let line_start = self.rope.line_to_char(line);

                // Skip the current word (non-space chars).
                while start < line_len && self.rope.char(line_start + start) != ' ' {
                    start += 1;
                }

                // If at or past end of line, this line is done.
                if start >= line_len {
                    break;
                }

                // Found a space; scan to end of the space run.
                let mut end = start;
                while end < line_len && self.rope.char(line_start + end) == ' ' {
                    end += 1;
                }

                if end - start > 1 {
                    // More than one space: delete [start, end-1), keeping the last space.
                    self.delete(Position::new(line, start), Position::new(line, end - 1));
                    // `start` stays at the same column (now the single remaining space).
                } else {
                    // Exactly one space: move past it.
                    start = end;
                }
            }

            // Advance to next line and record the modification.
            count = count.saturating_sub(1);
            self.set_dot(Position::new(line + 1, 0));
            self.set_mark(MarkId::Modified);
        }

        if count == 0 || is_pindef {
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::OutOfRange)
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
    /// Find the start of the word at (line, col).
    ///
    /// If the cursor is in the middle of a word, returns the word's first character;
    /// if it is already at a word start, it stays there; if it is in whitespace, it
    /// returns the start of the preceding word.
    fn find_current_word_start(&self, mut line: usize, mut col: usize) -> Option<(usize, usize)> {
        let mut line_len = line_length_excluding_newline(&self.rope, line);

        if col >= line_len {
            col = line_len.saturating_sub(1);
        }

        // If the line is empty go to the previous non-empty line.
        if line_len == 0 {
            let (pl, plen) = self.goto_prev_nonempty_line(line)?;
            line = pl;
            line_len = plen;
            col = line_len.saturating_sub(1);
        }

        // Step 2: skip whitespace backward.
        {
            let lc = self.rope.line(line);
            while col > 0 && lc.char(col).is_whitespace() {
                col -= 1;
            }
            // Step 3: if we are at col==0 and it is still whitespace, jump to the
            // previous non-empty line.
            if col == 0 && line_length_excluding_newline(&self.rope, line) > 0 && lc.char(0).is_whitespace() {
                let (pl, _plen) = self.goto_prev_nonempty_line(line)?;
                line = pl;
                col = line_length_excluding_newline(&self.rope, line).saturating_sub(1);
            }
        }

        // Step 4: scan backward while char(col)!=' ' and col > 0.
        {
            let lc = self.rope.line(line);
            while col > 0 && !lc.char(col).is_whitespace() {
                col -= 1;
            }
        }

        // Step 5: if now pointing at a space, advance one to reach the word start.
        {
            let lc = self.rope.line(line);
            let ll = line_length_excluding_newline(&self.rope, line);
            if col < ll && lc.char(col).is_whitespace() {
                col += 1;
            }
        }

        Some((line, col))
    }

    /// Compute the deletion end-point for YD, starting from `word_start`.
    ///
    /// Uses low-level finders directly to avoid side-effects on marks.
    /// Returns None if the advance is not possible (triggers failure).
    fn find_word_delete_end(&self, word_start: Position, lead_param: LeadParam) -> Option<Position> {
        match lead_param {
            LeadParam::None | LeadParam::Plus => self.find_next_word_start_from(word_start),
            LeadParam::Pint(0) => None, // delete 0 words → failure
            LeadParam::Pint(n) => {
                let mut pos = word_start;
                for _ in 0..n {
                    pos = self.find_next_word_start_from(pos)?;
                }
                Some(pos)
            }
            LeadParam::Pindef => {
                // Advance to start of next paragraph (same logic as word_advance_paragraph_forward).
                let mut line = word_start.line;
                while line < self.lines() && !line_is_blank(&self.rope, line) {
                    line += 1;
                }
                self.find_next_word_start_from(Position::new(line, 0))
            }
            LeadParam::Minus => {
                // Go back 1 word from word_start. One call to find_prev_word_start_from
                // suffices because word_start is already at a word boundary; the cursor-
                // convention col value happens to give the right previous-word result.
                let (l, c) = self.find_prev_word_start_from(word_start.line, word_start.column)?;
                Some(Position::new(l, c))
            }
            LeadParam::Nint(n) => {
                // Go back n words from word_start.
                let mut l = word_start.line;
                let mut c = word_start.column;
                for _ in 0..n {
                    let (nl, nc) = self.find_prev_word_start_from(l, c)?;
                    l = nl;
                    c = nc;
                }
                Some(Position::new(l, c))
            }
            LeadParam::Nindef => {
                // Advance to start of previous paragraph (same logic as word_advance_paragraph_backward).
                let mut line = word_start.line;
                while line > 0 && line_is_blank(&self.rope, line) {
                    line -= 1;
                }
                while line > 0 && !line_is_blank(&self.rope, line) {
                    line -= 1;
                }
                while line_is_blank(&self.rope, line) {
                    if line + 1 >= self.lines() {
                        return None;
                    }
                    line += 1;
                }
                let pos = self
                    .rope
                    .line(line)
                    .chars()
                    .position(|ch| !ch.is_whitespace())
                    .unwrap_or(0);
                Some(Position::new(line, pos))
            }
            _ => None,
        }
    }

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
