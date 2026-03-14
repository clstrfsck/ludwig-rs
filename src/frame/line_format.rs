//! Line formatting commands: YF (fill), YJ (justify), YC (centre), YL (left-align), YR (right-align).

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::position::Position;

use super::Frame;

/// Commands for margin-aware line formatting.
pub trait LineFormatCommands {
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
}

impl LineFormatCommands for Frame {
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
                        let fill_ratio = space_to_add as f64 / f64::from(holes);
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
                                debit -= f64::from(n);
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
                isize::midpoint(right as isize - left as isize - line_len as isize, first_ns as isize)
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
}

// Private helpers for line format commands
impl Frame {
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
            &format!("{sep}{text_to_move}"),
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
}
