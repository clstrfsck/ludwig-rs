//! Word processing commands: YA (word advance), YD (word delete), YS (line squeeze).

use std::cmp::min;

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::position::Position;

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

    /// YF: Fill line — move words so the line fits between left and right margins.
    /// Grabs words from the next line if the current line is too short.
    /// Splits the line if it is too long.
    fn cmd_line_fill(&mut self, lead_param: LeadParam) -> CmdResult;

    /// YJ: Justify line — expand by inserting spaces between words to fit exactly
    /// between the margins.  Does not modify the last line of a paragraph.
    fn cmd_line_justify(&mut self, lead_param: LeadParam) -> CmdResult;

    /// YC: Centre line — shift text so it is centred between the margins.
    fn cmd_line_centre(&mut self, lead_param: LeadParam) -> CmdResult;

    /// YL: Left-align line — remove leading spaces so the first word starts at
    /// the left margin.
    fn cmd_line_left(&mut self, lead_param: LeadParam) -> CmdResult;

    /// YR: Right-align line — add leading spaces so the last word ends at the
    /// right margin.
    fn cmd_line_right(&mut self, lead_param: LeadParam) -> CmdResult;

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
                if check_line >= self.line_count()
                    || self.line_length_excluding_newline(check_line) == 0
                {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            if start_line + count >= self.line_count() {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
        }

        // Main loop: process one line per iteration.
        while count > 0 {
            let line = self.dot().line;
            let line_len = self.line_length_excluding_newline(line);

            // Stop if current line is empty (pindef succeeds; pint checked by pre-check).
            if line_len == 0 {
                break;
            }

            // EOP check: must have a next line to advance dot into.
            if line + 1 >= self.line_count() {
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
                let line_len = self.line_length_excluding_newline(line);
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

    fn cmd_line_fill(&mut self, lead_param: LeadParam) -> CmdResult {
        let (mut count, is_pindef) = match lead_param {
            LeadParam::None | LeadParam::Plus => (1usize, false),
            LeadParam::Pint(n) => (n, false),
            LeadParam::Pindef => (usize::MAX, true),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Pre-check: verify count non-empty lines exist and one follows.
        if !is_pindef && count > 0 {
            let start = self.dot().line;
            for i in 0..count {
                let l = start + i;
                if l >= self.line_count() || self.line_length_excluding_newline(l) == 0 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            if start + count >= self.line_count() {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
        }

        while count > 0 {
            let line = self.dot().line;
            let line_len = self.line_length_excluding_newline(line);

            if line_len == 0 {
                break;
            }
            if line + 1 >= self.line_count() {
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }

            let right = self.right_margin;

            // Adjust to left margin if not the first line of a paragraph.
            if line > 0 && !self.is_blank_line(line - 1) {
                self.fill_adjust_margin(line);
            }

            let line_len = self.line_length_excluding_newline(line);
            let mut leave_dot_alone = false;

            if line_len > right {
                // Line is too long — split at the last space within the margin.
                if !self.fill_split_at_margin(line) {
                    if is_pindef {
                        break;
                    }
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
                // Account for the newly created line: process it on the next iteration.
                if !is_pindef {
                    count += 1;
                }
            } else {
                // Line is short — pull words from the next line in a loop.
                loop {
                    if line + 1 >= self.line_count() {
                        break;
                    }
                    if self.is_blank_line(line + 1) {
                        // Normalize the non-blank next-next line that follows.
                        break;
                    }

                    let cur_len = self.line_length_excluding_newline(line);
                    let space_avail = if right > cur_len + 1 {
                        right - cur_len - 1
                    } else {
                        0
                    };
                    if space_avail == 0 {
                        break;
                    }

                    if !self.fill_pull_one_chunk(line) {
                        break;
                    }

                    // Check if the next line is now empty.
                    if line + 1 >= self.line_count() {
                        break;
                    }
                    let next_len = self.line_length_excluding_newline(line + 1);
                    if next_len == 0 {
                        // Delete the now-empty next line.
                        self.delete(Position::new(line + 1, 0), Position::new(line + 2, 0));
                        count = count.saturating_sub(1);
                        if count == 0 {
                            leave_dot_alone = true;
                            break;
                        }
                        // Continue pulling from what is now the new next line.
                    } else {
                        // Normalize the next line's start to left_margin.
                        self.fill_normalize_line_start(line + 1);
                        break;
                    }
                }
            }

            count = count.saturating_sub(1);
            if !leave_dot_alone && line + 1 < self.line_count() {
                let lm = self.left_margin;
                self.set_dot(Position::new(line + 1, lm));
            }
            self.set_mark(MarkId::Modified);
        }

        if count == 0 || is_pindef {
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::OutOfRange)
        }
    }

    fn cmd_line_justify(&mut self, lead_param: LeadParam) -> CmdResult {
        let (mut count, is_pindef) = match lead_param {
            LeadParam::None | LeadParam::Plus => (1usize, false),
            LeadParam::Pint(n) => (n, false),
            LeadParam::Pindef => (usize::MAX, true),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Pre-check.
        if !is_pindef && count > 0 {
            let start = self.dot().line;
            for i in 0..count {
                let l = start + i;
                if l >= self.line_count() || self.line_length_excluding_newline(l) == 0 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            if start + count >= self.line_count() {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
        }

        while count > 0 {
            let line = self.dot().line;
            let line_len = self.line_length_excluding_newline(line);

            if line_len == 0 {
                break;
            }
            if line + 1 >= self.line_count() {
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }

            let right = self.right_margin;
            let left = self.left_margin;

            // Skip last line of paragraph (next line is blank).
            if self.line_length_excluding_newline(line + 1) != 0 {
                if line_len > right {
                    // Line too long to justify.
                    if is_pindef {
                        break;
                    }
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }

                let space_to_add = right - line_len;
                if space_to_add > 0 {
                    let lsc = self.rope.line_to_char(line);

                    // Find start of text (skip leading spaces from left_margin).
                    let mut pos = left;
                    while pos < line_len && self.rope.char(lsc + pos) == ' ' {
                        pos += 1;
                    }
                    let text_start = pos;

                    // Count inter-word gaps (holes).
                    let mut holes = 0i32;
                    let mut scan = text_start;
                    loop {
                        while scan < line_len && self.rope.char(lsc + scan) != ' ' {
                            scan += 1;
                        }
                        while scan < line_len && self.rope.char(lsc + scan) == ' ' {
                            scan += 1;
                        }
                        holes += 1;
                        if scan >= line_len {
                            break;
                        }
                    }
                    holes -= 1; // Last increment was at end of line, not a real hole.

                    if holes > 0 {
                        let fill_ratio = space_to_add as f64 / holes as f64;
                        let mut debit = 0.0f64;
                        let mut pos = text_start;

                        for _ in 0..holes {
                            // Find the next inter-word space.
                            let lsc = self.rope.line_to_char(line);
                            let cur_line_len = self.line_length_excluding_newline(line);
                            while pos < cur_line_len && self.rope.char(lsc + pos) != ' ' {
                                pos += 1;
                            }

                            debit += fill_ratio;
                            let n = (debit + 0.5) as i32;
                            if n > 0 {
                                self.insert_at(Position::new(line, pos), &" ".repeat(n as usize));
                                debit -= n as f64;
                            }

                            // Skip all spaces (original + any newly inserted).
                            let lsc = self.rope.line_to_char(line);
                            let cur_line_len = self.line_length_excluding_newline(line);
                            while pos < cur_line_len && self.rope.char(lsc + pos) == ' ' {
                                pos += 1;
                            }
                        }
                    }
                }
            }

            count = count.saturating_sub(1);
            let lm = self.left_margin;
            self.set_dot(Position::new(line + 1, lm));
            self.set_mark(MarkId::Modified);
        }

        if count == 0 || is_pindef {
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::OutOfRange)
        }
    }

    fn cmd_line_centre(&mut self, lead_param: LeadParam) -> CmdResult {
        let (mut count, is_pindef) = match lead_param {
            LeadParam::None | LeadParam::Plus => (1usize, false),
            LeadParam::Pint(n) => (n, false),
            LeadParam::Pindef => (usize::MAX, true),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Pre-check.
        if !is_pindef && count > 0 {
            let start = self.dot().line;
            for i in 0..count {
                let l = start + i;
                if l >= self.line_count() || self.line_length_excluding_newline(l) == 0 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            if start + count >= self.line_count() {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
        }

        while count > 0 {
            let line = self.dot().line;
            let line_len = self.line_length_excluding_newline(line);

            if line_len == 0 {
                break;
            }
            if line + 1 >= self.line_count() {
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }

            let right = self.right_margin;
            let left = self.left_margin;

            // Fail if line is out of valid range.
            if line_len <= left || line_len > right {
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }

            let lsc = self.rope.line_to_char(line);
            let first_ns = (0..line_len)
                .find(|&c| self.rope.char(lsc + c) != ' ')
                .unwrap_or(line_len);

            // Fail if text starts before the left margin.
            if first_ns < left {
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }

            // Compute spaces to add (may be negative, meaning spaces to remove).
            // Formula derived from C++ word_centre (translated to 0-based indexing):
            //   space_to_add = (right - left - line_len + first_ns) / 2 - (first_ns - left)
            let space_to_add: isize =
                (right as isize - left as isize - line_len as isize + first_ns as isize) / 2
                    - (first_ns as isize - left as isize);

            if space_to_add > 0 {
                self.insert_at(
                    Position::new(line, left),
                    &" ".repeat(space_to_add as usize),
                );
            } else if space_to_add < 0 {
                let to_remove = (-space_to_add) as usize;
                self.delete(
                    Position::new(line, left),
                    Position::new(line, left + to_remove),
                );
            }

            count = count.saturating_sub(1);
            let lm = self.left_margin;
            self.set_dot(Position::new(line + 1, lm));
            self.set_mark(MarkId::Modified);
        }

        if count == 0 || is_pindef {
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::OutOfRange)
        }
    }

    fn cmd_line_left(&mut self, lead_param: LeadParam) -> CmdResult {
        let (mut count, is_pindef) = match lead_param {
            LeadParam::None | LeadParam::Plus => (1usize, false),
            LeadParam::Pint(n) => (n, false),
            LeadParam::Pindef => (usize::MAX, true),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Pre-check.
        if !is_pindef && count > 0 {
            let start = self.dot().line;
            for i in 0..count {
                let l = start + i;
                if l >= self.line_count() || self.line_length_excluding_newline(l) == 0 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            if start + count >= self.line_count() {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
        }

        while count > 0 {
            let line = self.dot().line;
            let line_len = self.line_length_excluding_newline(line);

            if line_len == 0 {
                break;
            }
            if line + 1 >= self.line_count() {
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }

            let left = self.left_margin;

            if line_len > left {
                let lsc = self.rope.line_to_char(line);
                let first_ns = (0..line_len)
                    .find(|&c| self.rope.char(lsc + c) != ' ')
                    .unwrap_or(line_len);

                if first_ns < left {
                    // Text protrudes before left margin.
                    if is_pindef {
                        break;
                    }
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }

                // Remove excess leading spaces in [left, first_ns).
                if first_ns > left {
                    self.delete(Position::new(line, left), Position::new(line, first_ns));
                }
            } else {
                // Line too short.
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }

            count = count.saturating_sub(1);
            let lm = self.left_margin;
            self.set_dot(Position::new(line + 1, lm));
            self.set_mark(MarkId::Modified);
        }

        if count == 0 || is_pindef {
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::OutOfRange)
        }
    }

    fn cmd_line_right(&mut self, lead_param: LeadParam) -> CmdResult {
        let (mut count, is_pindef) = match lead_param {
            LeadParam::None | LeadParam::Plus => (1usize, false),
            LeadParam::Pint(n) => (n, false),
            LeadParam::Pindef => (usize::MAX, true),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Pre-check.
        if !is_pindef && count > 0 {
            let start = self.dot().line;
            for i in 0..count {
                let l = start + i;
                if l >= self.line_count() || self.line_length_excluding_newline(l) == 0 {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
            if start + count >= self.line_count() {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
        }

        while count > 0 {
            let line = self.dot().line;
            let line_len = self.line_length_excluding_newline(line);

            if line_len == 0 {
                break;
            }
            if line + 1 >= self.line_count() {
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }

            let right = self.right_margin;

            if line_len < right {
                let space_to_add = right - line_len;
                self.insert_at(Position::new(line, 0), &" ".repeat(space_to_add));
            } else if line_len > right {
                // Line too long to right-align.
                if is_pindef {
                    break;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
            // If line_len == right, no-op (already aligned).

            count = count.saturating_sub(1);
            let lm = self.left_margin;
            self.set_dot(Position::new(line + 1, lm));
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
                let line_len = self.line_length_excluding_newline(dot.line - 1);
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
        if dot.line + 1 >= self.line_count() {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        let count = match lead_param {
            LeadParam::None | LeadParam::Plus => 1,
            LeadParam::Pint(n) => n,
            LeadParam::Pindef => {
                let line_len = self.line_length_excluding_newline(dot.line + 1);
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

// Private helpers for word commands
impl Frame {
    /// Find the start of the word at (line, col).
    ///
    /// If the cursor is in the middle of a word, returns the word's first character;
    /// if it is already at a word start, it stays there; if it is in whitespace, it
    /// returns the start of the preceding word.
    fn find_current_word_start(&self, mut line: usize, mut col: usize) -> Option<(usize, usize)> {
        let mut line_len = self.line_length_excluding_newline(line);

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
            if col == 0
                && self.line_length_excluding_newline(line) > 0
                && lc.char(0).is_whitespace()
            {
                let (pl, _plen) = self.goto_prev_nonempty_line(line)?;
                line = pl;
                col = self.line_length_excluding_newline(line).saturating_sub(1);
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
            let ll = self.line_length_excluding_newline(line);
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
    fn find_word_delete_end(
        &self,
        word_start: Position,
        lead_param: LeadParam,
    ) -> Option<Position> {
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
                while line < self.line_count() && !self.is_blank_line(line) {
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
                while line > 0 && self.is_blank_line(line) {
                    line -= 1;
                }
                while line > 0 && !self.is_blank_line(line) {
                    line -= 1;
                }
                while self.is_blank_line(line) {
                    if line + 1 >= self.line_count() {
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
        while line < self.line_count() && !self.is_blank_line(line) {
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
        while line > 0 && self.is_blank_line(line) {
            line -= 1;
        }
        // Find blank line separating this para from previous
        while line > 0 && !self.is_blank_line(line) {
            line -= 1;
        }
        // Find first non-blank
        while self.is_blank_line(line) {
            if line + 1 >= self.line_count() {
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
        let mut pos = min(dot.column, self.line_length_excluding_newline(line));

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
            let len = self.line_length_excluding_newline(line);
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
        while line < self.line_count() {
            let line_len = self.line_length_excluding_newline(line);
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
            if line < self.line_count() {
                let next_line_len = self.line_length_excluding_newline(line);
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

    // ── YF helper: adjust a non-first-paragraph line's leading spaces to left_margin ──

    fn fill_adjust_margin(&mut self, line: usize) {
        let lsc = self.rope.line_to_char(line);
        let line_len = self.line_length_excluding_newline(line);
        let left = self.left_margin;

        let first_ns = (0..line_len)
            .find(|&c| self.rope.char(lsc + c) != ' ')
            .unwrap_or(line_len);

        if first_ns < left && first_ns < line_len {
            // Insert spaces to push text to left_margin.
            self.insert_at(Position::new(line, first_ns), &" ".repeat(left - first_ns));
        } else {
            // Remove any excess spaces that lie between left_margin and the text.
            let start = left;
            let line_len = self.line_length_excluding_newline(line);
            let lsc = self.rope.line_to_char(line);
            if start < line_len {
                let mut end = start;
                while end < line_len && self.rope.char(lsc + end) == ' ' {
                    end += 1;
                }
                if end > start {
                    self.delete(Position::new(line, start), Position::new(line, end));
                }
            }
        }
    }

    // ── YF helper: split a too-long line at the last word boundary within the margin ──

    /// Returns true on success, false if no valid split point exists.
    fn fill_split_at_margin(&mut self, line: usize) -> bool {
        let lsc = self.rope.line_to_char(line);
        let line_len = self.line_length_excluding_newline(line);
        let left = self.left_margin;
        let right = self.right_margin;

        // Start scanning from right_margin (first column past the right margin, 0-based).
        let mut end_col = right.min(line_len.saturating_sub(1));

        // Find the rightmost space at or before right_margin.
        loop {
            if self.rope.char(lsc + end_col) == ' ' {
                break;
            }
            if end_col == left {
                return false; // No split point found.
            }
            end_col -= 1;
        }
        // end_col now points at a space.

        // Scan backward to find the end of the last kept word.
        let mut overflow_start = end_col;
        while end_col > left && self.rope.char(lsc + end_col) == ' ' {
            end_col -= 1;
        }
        if end_col == left {
            return false; // Nothing to keep.
        }

        // Scan forward from overflow_start to find first char of overflow.
        while overflow_start < line_len && self.rope.char(lsc + overflow_start) == ' ' {
            overflow_start += 1;
        }
        if overflow_start >= line_len {
            return false; // No overflow text (line has only trailing spaces).
        }

        // Keep dot on the current line if it would end up in the overflow.
        let dot = self.dot();
        if dot.line == line && dot.column > end_col {
            self.set_dot(Position::new(line, end_col));
        }

        // Extract overflow text.
        let overflow: String = self
            .rope
            .line(line)
            .chars()
            .skip(overflow_start)
            .take_while(|&c| c != '\n')
            .collect();

        // Delete overflow from current line.
        self.delete(
            Position::new(line, overflow_start),
            Position::new(line, line_len),
        );

        // Insert a new line with left_margin indentation and overflow text.
        let insert_text = format!("\n{}{}", " ".repeat(self.left_margin), overflow);
        self.insert_at(Position::new(line, overflow_start), &insert_text);

        true
    }

    // ── YF helper: pull one "chunk" of fitting words from the next line ──

    /// Returns true if any words were moved.
    fn fill_pull_one_chunk(&mut self, line: usize) -> bool {
        let line_len = self.line_length_excluding_newline(line);
        let right = self.right_margin;

        if line_len >= right {
            return false;
        }
        let space_available = right - line_len - 1;
        if space_available == 0 {
            return false;
        }

        let next_line = line + 1;
        if next_line >= self.line_count() {
            return false;
        }

        let next_len = self.line_length_excluding_newline(next_line);
        let nlsc = self.rope.line_to_char(next_line);

        // Find first non-space on next line.
        let mut start = 0;
        while start < next_len && self.rope.char(nlsc + start) == ' ' {
            start += 1;
        }
        if start >= next_len {
            return false; // Next line is all spaces.
        }

        // Scan words on next line to find how many fit.
        let mut end = start;
        let mut old_end = start;
        loop {
            if end > next_len {
                break;
            }
            // Skip inter-word spaces.
            while end < next_len && self.rope.char(nlsc + end) == ' ' {
                end += 1;
            }
            // Skip word.
            while end < next_len && self.rope.char(nlsc + end) != ' ' {
                end += 1;
            }
            if end == next_len {
                end += 1; // Include the last word.
            }
            let chunk = end - start;
            if chunk > space_available {
                break; // Doesn't fit.
            }
            old_end = end;
            if end > next_len {
                break;
            }
        }

        if old_end == start {
            return false; // Nothing fits.
        }

        let actual_end = old_end.min(next_len);

        // Extract text to move.
        let text_to_move: String = self
            .rope
            .line(next_line)
            .chars()
            .skip(start)
            .take(actual_end - start)
            .collect();

        // Append to current line (with separator space if needed).
        let cur_len = self.line_length_excluding_newline(line);
        let sep = if cur_len > self.left_margin { " " } else { "" };
        self.insert_at(
            Position::new(line, cur_len),
            &format!("{}{}", sep, text_to_move),
        );

        // Remove [0, actual_end) from next line (leading spaces + moved words).
        self.delete(
            Position::new(next_line, 0),
            Position::new(next_line, actual_end),
        );

        true
    }

    // ── YF helper: normalize a line's leading spaces to left_margin ──

    fn fill_normalize_line_start(&mut self, line: usize) {
        let lsc = self.rope.line_to_char(line);
        let line_len = self.line_length_excluding_newline(line);
        let left = self.left_margin;

        let first_ns = (0..line_len)
            .find(|&c| self.rope.char(lsc + c) != ' ')
            .unwrap_or(line_len);

        if first_ns < left {
            self.insert_at(Position::new(line, first_ns), &" ".repeat(left - first_ns));
        } else if first_ns > left {
            self.delete(Position::new(line, left), Position::new(line, first_ns));
        }
    }

    /// Ditto: copy character(s) from line above (direction=-1) or below (direction=1).
    fn ditto(&mut self, direction: isize, count: usize) -> CmdResult {
        let dot = self.dot();
        let source_line = (dot.line as isize + direction) as usize;

        if direction < 0 && dot.line == 0 {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }
        if direction > 0 && source_line >= self.line_count() {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }

        let source_line_len = self.line_length_excluding_newline(source_line);
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
