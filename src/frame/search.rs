//! Search commands: Next (N) and Bridge (BR).

use std::collections::HashSet;

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::position::{Position, line_length_excluding_newline};
use crate::trail_param::TrailParam;

use super::Frame;

/// Commands for searching within the frame.
pub trait SearchCommands {
    /// Next character command (N): find Nth occurrence of a character from a set.
    fn cmd_next(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult;

    /// Bridge command (BR): skip over consecutive occurrences of characters in a set.
    fn cmd_bridge(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult;

    /// Replace command (R): find and replace literal text.
    fn cmd_replace(
        &mut self,
        lead_param: LeadParam,
        search: &TrailParam,
        replace: &TrailParam,
    ) -> CmdResult;

    /// Get (Search) command (G): search for literal text.
    /// On success: dot → after match, Equals → start of match.
    fn cmd_get(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult;
}

impl SearchCommands for Frame {
    fn cmd_next(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult {
        let count = match lead_param {
            LeadParam::None | LeadParam::Plus => 1isize,
            LeadParam::Pint(n) => n as isize,
            LeadParam::Minus => -1,
            LeadParam::Nint(n) => -(n as isize),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        self.nextbridge(count, tpar, false)
    }

    fn cmd_bridge(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult {
        let count = match lead_param {
            LeadParam::None | LeadParam::Plus => 1isize,
            LeadParam::Minus => -1,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        self.nextbridge(count, tpar, true)
    }

    fn cmd_replace(
        &mut self,
        lead_param: LeadParam,
        search: &TrailParam,
        replace: &TrailParam,
    ) -> CmdResult {
        // Determine count and direction
        let (count, replace_all) = match lead_param {
            LeadParam::None | LeadParam::Plus => (1isize, false),
            LeadParam::Pint(n) => (n as isize, false),
            LeadParam::Minus => (-1, false),
            LeadParam::Nint(n) => (-(n as isize), false),
            LeadParam::Pindef => (1, true),   // >R: replace all forward
            LeadParam::Nindef => (-1, true),  // <R: replace all backward
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        if search.str.is_empty() {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }

        // Case sensitivity: / = insensitive, " = exact, others = insensitive
        let case_sensitive = search.dlm == '"';

        let original_dot = self.dot();
        let mut replacements = 0usize;

        if replace_all {
            // Replace all occurrences
            loop {
                let found = if count > 0 {
                    self.find_literal_forward(&search.str, case_sensitive)
                } else {
                    self.find_literal_backward(&search.str, case_sensitive)
                };
                match found {
                    Some((start, end)) => {
                        self.do_replace(start, end, &replace.str);
                        replacements += 1;
                    }
                    None => break,
                }
            }
        } else {
            // Replace count occurrences
            let abs_count = count.unsigned_abs();
            for _ in 0..abs_count {
                let found = if count > 0 {
                    self.find_literal_forward(&search.str, case_sensitive)
                } else {
                    self.find_literal_backward(&search.str, case_sensitive)
                };
                match found {
                    Some((start, end)) => {
                        self.do_replace(start, end, &replace.str);
                        replacements += 1;
                    }
                    None => {
                        if replacements == 0 {
                            return CmdResult::Failure(CmdFailure::OutOfRange);
                        }
                        break;
                    }
                }
            }
        }

        if replacements > 0 {
            self.set_mark(MarkId::Modified);
            self.set_mark_at(MarkId::Equals, original_dot);
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::OutOfRange)
        }
    }

    fn cmd_get(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult {
        let (count, forward) = match lead_param {
            LeadParam::None | LeadParam::Plus => (1usize, true),
            LeadParam::Pint(n) => (n, true),
            LeadParam::Minus => (1, false),
            LeadParam::Nint(n) => (n, false),
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        if tpar.str.is_empty() {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }

        let case_sensitive = tpar.dlm == '"';

        for _ in 0..count {
            let found = if forward {
                self.find_literal_forward(&tpar.str, case_sensitive)
            } else {
                self.find_literal_backward(&tpar.str, case_sensitive)
            };
            match found {
                Some((new_equals, new_dot)) => {
                    self.set_mark_at(MarkId::Equals, new_equals);
                    self.set_dot(new_dot);
                }
                None => {
                    return CmdResult::Failure(CmdFailure::OutOfRange);
                }
            }
        }

        CmdResult::Success
    }
}

/// Parse the trailing parameter into a character set.
///
/// Supports single characters and `..` ranges: `'abc'`, `'a..z'`, `'0..9A..Z'`.
fn parse_char_set(tpar: &TrailParam) -> HashSet<char> {
    let mut chars = HashSet::new();
    let s: Vec<char> = tpar.str.chars().collect();
    let len = s.len();
    let mut i = 0;
    while i < len {
        let ch1 = s[i];
        i += 1;
        // Check for range: ch1..ch2
        if i + 2 <= len && s[i] == '.' && s[i + 1] == '.' {
            let ch2 = s[i + 2];
            i += 3;
            // Insert all chars in the range (inclusive)
            let start = ch1 as u32;
            let end = ch2 as u32;
            if start <= end {
                for code in start..=end {
                    if let Some(c) = char::from_u32(code) {
                        chars.insert(c);
                    }
                }
            }
        } else {
            chars.insert(ch1);
        }
    }
    chars
}

impl Frame {
    /// Shared implementation for N (next) and BR (bridge) commands.
    ///
    /// `count` > 0 searches forward, `count` < 0 searches backward.
    /// For bridge mode, the character set is complemented (search for chars NOT in set).
    fn nextbridge(&mut self, count: isize, tpar: &TrailParam, bridge: bool) -> CmdResult {
        if count == 0 {
            // Zero count: just set Equals mark to current dot
            self.set_mark_at(MarkId::Equals, self.dot());
            return CmdResult::Success;
        }

        let chars = parse_char_set(tpar);
        let original_dot = self.dot();
        let num_lines = self.lines();

        if count > 0 {
            match self.nextbridge_forward(count as usize, &chars, bridge, num_lines) {
                Some(pos) => {
                    self.set_mark_at(MarkId::Equals, original_dot);
                    self.set_dot(pos);
                    CmdResult::Success
                }
                None => CmdResult::Failure(CmdFailure::OutOfRange),
            }
        } else {
            match self.nextbridge_backward((-count) as usize, &chars, bridge, num_lines) {
                Some(pos) => {
                    self.set_mark_at(MarkId::Equals, original_dot);
                    self.set_dot(pos);
                    CmdResult::Success
                }
                None => CmdResult::Failure(CmdFailure::OutOfRange),
            }
        }
    }

    /// Forward search: find count'th character matching the set.
    ///
    /// For N: skips char at dot, searches for chars IN set.
    /// For BR: does NOT skip char at dot, searches for chars NOT IN set.
    fn nextbridge_forward(
        &self,
        mut count: usize,
        chars: &HashSet<char>,
        bridge: bool,
        num_lines: usize,
    ) -> Option<Position> {
        let dot = self.dot();
        let mut line = dot.line;
        let mut col = dot.column;

        // N skips the character at dot; BR does not
        if !bridge {
            col += 1;
        }

        loop {
            if line >= num_lines {
                return None;
            }
            let line_len = line_length_excluding_newline(&self.rope, line);
            let line_start = self.rope.line_to_char(line);

            // Scan characters on this line starting from col
            let mut i = col;
            while i < line_len {
                let ch = self.rope.char(line_start + i);
                if char_matches(ch, chars, bridge) {
                    // Found a match
                    count -= 1;
                    if count == 0 {
                        return Some(Position::new(line, i));
                    }
                    // For N with count > 1, continue from next char
                    i += 1;
                    continue;
                }
                i += 1;
            }

            // Check for space at EOL (virtual space)
            if i == line_len && char_matches(' ', chars, bridge) {
                count -= 1;
                if count == 0 {
                    return Some(Position::new(line, line_len));
                }
            }

            // Move to next line
            line += 1;
            col = 0;
        }
    }

    /// Backward search: find count'th character matching the set.
    ///
    /// For N: starts from dot-1, searches for chars IN set.
    /// For BR: starts from dot-1, searches for chars NOT IN set.
    ///
    /// Returns position AFTER the found character (dot lands after the match).
    fn nextbridge_backward(
        &self,
        mut count: usize,
        chars: &HashSet<char>,
        bridge: bool,
        num_lines: usize,
    ) -> Option<Position> {
        let dot = self.dot();
        let mut line = dot.line;

        // Start column: for both N and BR, start from dot.column - 1.
        // But for bridge, we don't skip an additional char.
        // C++ ref: new_col = dot->col - 1; if (!bridge) new_col -= 1;
        // Ludwig is 1-indexed, ours is 0-indexed, so:
        //   N:  start at dot.column - 2 (skip dot's char and check one before)
        //   BR: start at dot.column - 1
        let start_offset: isize = if bridge {
            dot.column as isize - 1
        } else {
            dot.column as isize - 2
        };

        // If start_offset < 0, we need to go to previous line
        let mut col: isize = start_offset;

        loop {
            if line >= num_lines && line > 0 {
                // dot was past last line; go to last real line
                line = num_lines - 1;
                let line_len = line_length_excluding_newline(&self.rope, line);
                col = line_len as isize; // virtual space at EOL
            }

            if col < 0 {
                // Need to go to previous line
                if line == 0 {
                    // At the very beginning
                    if bridge {
                        // Bridge succeeds at current position
                        count -= 1;
                        if count == 0 {
                            // Position after found char: col+2 for N offset adjustment
                            // In the C++ code: new_col += 2 at the end
                            // Since we're at the beginning, result is column 0
                            return Some(Position::new(0, 0));
                        }
                    }
                    return None;
                }
                line -= 1;
                let line_len = line_length_excluding_newline(&self.rope, line);
                // Check virtual space (space at EOL) first
                col = line_len as isize;
            }

            let line_len = line_length_excluding_newline(&self.rope, line);
            let line_start = self.rope.line_to_char(line);

            // Check virtual space at EOL
            if col as usize > line_len {
                if char_matches(' ', chars, bridge) {
                    count -= 1;
                    if count == 0 {
                        return Some(Position::new(line, col as usize + 1));
                    }
                }
                col = line_len as isize - 1;
            }

            // Scan backward on this line
            while col >= 0 {
                let ch = self.rope.char(line_start + col as usize);
                if char_matches(ch, chars, bridge) {
                    count -= 1;
                    if count == 0 {
                        // Result position is AFTER the found char
                        return Some(Position::new(line, col as usize + 1));
                    }
                }
                col -= 1;
            }

            // Move to previous line
            if line == 0 {
                if bridge {
                    count -= 1;
                    if count == 0 {
                        return Some(Position::new(0, 0));
                    }
                }
                return None;
            }
            line -= 1;
            let prev_line_len = line_length_excluding_newline(&self.rope, line);
            col = prev_line_len as isize - 1;
        }
    }
}

// Private helpers for Replace command
impl Frame {
    /// Search forward from dot for a literal string.
    /// Returns (start_position, end_position) of match.
    /// Dot is NOT moved by this method — the caller handles positioning.
    fn find_literal_forward(
        &self,
        pattern: &str,
        case_sensitive: bool,
    ) -> Option<(Position, Position)> {
        let dot = self.dot();
        let num_lines = self.lines();
        let pat_chars: Vec<char> = pattern.chars().collect();
        let pat_len = pat_chars.len();

        if pat_len == 0 {
            return None;
        }

        let mut line = dot.line;
        let mut col = dot.column;

        while line < num_lines {
            let line_len = line_length_excluding_newline(&self.rope, line);
            let line_start = self.rope.line_to_char(line);

            while col + pat_len <= line_len {
                if self.matches_at_pos(line_start + col, &pat_chars, case_sensitive) {
                    return Some((
                        Position::new(line, col),
                        Position::new(line, col + pat_len),
                    ));
                }
                col += 1;
            }
            line += 1;
            col = 0;
        }
        None
    }

    /// Search backward from dot for a literal string.
    /// Returns (end_position, start_position) of match.
    fn find_literal_backward(
        &self,
        pattern: &str,
        case_sensitive: bool,
    ) -> Option<(Position, Position)> {
        let dot = self.dot();
        let pat_chars: Vec<char> = pattern.chars().collect();
        let pat_len = pat_chars.len();

        if pat_len == 0 {
            return None;
        }

        let mut line = dot.line;
        let line_len = line_length_excluding_newline(&self.rope, line);
        // Start searching from one position before dot (or end of line if dot is beyond)
        let effective_col = dot.column.min(line_len);
        let mut col = effective_col.saturating_sub(1) as isize;

        loop {
            let line_len = line_length_excluding_newline(&self.rope, line);
            let line_start = self.rope.line_to_char(line);

            while col >= 0 {
                let c = col as usize;
                if c + pat_len <= line_len
                    && self.matches_at_pos(line_start + c, &pat_chars, case_sensitive)
                {
                    return Some((
                        Position::new(line, c + pat_len),
                        Position::new(line, c),
                    ));
                }
                col -= 1;
            }

            if line == 0 {
                return None;
            }
            line -= 1;
            let prev_line_len = line_length_excluding_newline(&self.rope, line);
            col = prev_line_len.saturating_sub(1) as isize;
        }
    }

    /// Check if pattern matches at a specific char index in the rope.
    fn matches_at_pos(&self, char_idx: usize, pat_chars: &[char], case_sensitive: bool) -> bool {
        for (i, &pc) in pat_chars.iter().enumerate() {
            let rc = self.rope.char(char_idx + i);
            if case_sensitive {
                if rc != pc {
                    return false;
                }
            } else if !unicode_case_eq_char(rc, pc) {
                return false;
            }
        }
        true
    }

    /// Perform a replacement: delete from start to end, insert replacement text.
    /// Dot ends up after the replacement text.
    fn do_replace(&mut self, start: Position, end: Position, replacement: &str) {
        // Move dot to start, delete the matched text, insert replacement
        self.set_dot(start);
        self.delete(start, end);
        self.insert(replacement);
    }
}

/// Check if a character matches the search criteria.
///
/// For N (bridge=false): matches if char IS in the set.
/// For BR (bridge=true): matches if char is NOT in the set (complement).
fn char_matches(ch: char, chars: &HashSet<char>, bridge: bool) -> bool {
    chars.contains(&ch) ^ bridge
}

/// Unicode-aware, case-insensitive equality for single scalar values.
///
/// This compares lowercase expansions, so it handles mappings where
/// a single character lowercases to multiple code points.
fn unicode_case_eq_char(a: char, b: char) -> bool {
    a.to_lowercase().eq(b.to_lowercase())
}
