//! Predicate commands: EOL, EOP, EOF, EQC, EQM, EQS, and Mark (M).

use std::cmp::Ordering;

use crate::cmd_result::{CmdFailure, CmdResult};
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::position::{Position, line_length_excluding_newline};
use crate::trail_param::TrailParam;

use super::Frame;

/// Commands that test conditions and succeed/fail based on predicates.
pub trait PredicateCommands {
    /// EOL: Test if dot is at end of line.
    /// Succeeds if dot.column >= line length (excluding newline).
    /// Leading `-` inverts the test (succeeds if NOT at EOL).
    fn cmd_eol(&mut self, lead_param: LeadParam) -> CmdResult;

    /// EOP: Test if dot is on the last (null) line.
    /// Leading `-` inverts.
    fn cmd_eop(&mut self, lead_param: LeadParam) -> CmdResult;

    /// EOF: Same as EOP - test if dot is on the last (null) line.
    /// Leading `-` inverts.
    fn cmd_eof(&mut self, lead_param: LeadParam) -> CmdResult;

    /// EQC: Test dot column against a number.
    /// `EQC'N'` succeeds if dot column == N (1-based in tpar).
    /// `-EQC'N'` succeeds if dot column != N.
    /// `>EQC'N'` succeeds if dot column >= N.
    /// `<EQC'N'` succeeds if dot column <= N.
    fn cmd_eqc(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult;

    /// EQM: Test dot position against a mark.
    /// `EQM'N'` succeeds if dot == mark N.
    /// `-EQM'N'` succeeds if dot != mark N.
    /// `>EQM'N'` succeeds if dot >= mark N.
    /// `<EQM'N'` succeeds if dot <= mark N.
    fn cmd_eqm(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult;

    /// EQS: Test text at dot against a string.
    /// `EQS/text/` succeeds if text at dot matches.
    /// Delimiter `/` = case-insensitive, `"` = exact case.
    /// Leading `-` inverts.
    fn cmd_eqs(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult;

    /// M: Set/unset marks.
    /// `M` sets mark 1 at dot. `NM` sets mark N at dot.
    /// `-M` unsets mark 1. `-NM` unsets mark N.
    fn cmd_mark(&mut self, lead_param: LeadParam) -> CmdResult;
}

impl PredicateCommands for Frame {
    fn cmd_eol(&mut self, lead_param: LeadParam) -> CmdResult {
        let dot = self.dot();
        let line_len = if dot.line < self.lines() {
            line_length_excluding_newline(&self.rope, dot.line)
        } else {
            0
        };
        let result = match lead_param {
            LeadParam::None | LeadParam::Plus => dot.column == line_len,
            LeadParam::Minus => dot.column != line_len,
            LeadParam::Pindef => dot.column >= line_len,
            LeadParam::Nindef => dot.column <= line_len,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        bool_result(result)
    }

    fn cmd_eop(&mut self, lead_param: LeadParam) -> CmdResult {
        let invert = match lead_param {
            LeadParam::None | LeadParam::Plus => false,
            LeadParam::Minus => true,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        let at_eop = self.is_at_eop();
        bool_result(at_eop ^ invert)
    }

    fn cmd_eof(&mut self, lead_param: LeadParam) -> CmdResult {
        // EOF is the same as EOP in Ludwig-rs as the file is always fully loaded in memory.
        self.cmd_eop(lead_param)
    }

    fn cmd_eqc(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult {
        let target_col = match parse_column_tpar(tpar) {
            Some(c) => c,
            None => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        let dot_col = self.dot().column;
        let result = match lead_param {
            LeadParam::None | LeadParam::Plus => dot_col == target_col,
            LeadParam::Minus => dot_col != target_col,
            LeadParam::Pindef => dot_col >= target_col,
            LeadParam::Nindef => dot_col <= target_col,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        bool_result(result)
    }

    fn cmd_eqm(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult {
        let mark_id = match parse_mark_tpar(tpar) {
            Some(id) => id,
            None => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        let mark_pos = match self.get_mark(mark_id) {
            Some(pos) => pos,
            None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
        };
        let dot = self.dot();
        let result = match lead_param {
            LeadParam::None | LeadParam::Plus => dot == mark_pos,
            LeadParam::Minus => dot != mark_pos,
            LeadParam::Pindef => dot >= mark_pos,
            LeadParam::Nindef => dot <= mark_pos,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        bool_result(result)
    }

    fn cmd_eqs(&mut self, lead_param: LeadParam, tpar: &TrailParam) -> CmdResult {
        let case_sensitive = tpar.dlm == '"';
        let cmp = self.text_compare_at(self.dot(), &tpar.str, case_sensitive);

        let result = match lead_param {
            LeadParam::None | LeadParam::Plus => cmp == Ordering::Equal,
            LeadParam::Minus => cmp != Ordering::Equal,
            LeadParam::Pindef => cmp != Ordering::Less,
            LeadParam::Nindef => cmp != Ordering::Greater,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };
        bool_result(result)
    }

    fn cmd_mark(&mut self, lead_param: LeadParam) -> CmdResult {
        match lead_param {
            LeadParam::None | LeadParam::Plus => {
                // M — set mark 1 at dot
                self.set_mark_at(MarkId::Numbered(1), self.dot());
                CmdResult::Success
            }
            LeadParam::Pint(n) => {
                // NM — set mark N at dot
                let n = n as u8;
                if !(1..=9).contains(&n) {
                    return CmdResult::Failure(CmdFailure::SyntaxError);
                }
                self.set_mark_at(MarkId::Numbered(n), self.dot());
                CmdResult::Success
            }
            LeadParam::Minus => {
                // -M — unset mark 1
                self.unset_mark(MarkId::Numbered(1));
                CmdResult::Success
            }
            LeadParam::Nint(n) => {
                // -NM — unset mark N
                let n = n as u8;
                if !(1..=9).contains(&n) {
                    return CmdResult::Failure(CmdFailure::SyntaxError);
                }
                self.unset_mark(MarkId::Numbered(n));
                CmdResult::Success
            }
            _ => CmdResult::Failure(CmdFailure::SyntaxError),
        }
    }
}

impl Frame {
    /// Check if dot is at end-of-page (last null line).
    fn is_at_eop(&self) -> bool {
        let num_lines = self.lines();
        if num_lines == 0 {
            return true;
        }
        // The last line is the null line (the one after the last newline)
        self.dot().line >= num_lines - 1
            && line_length_excluding_newline(&self.rope, num_lines - 1) == 0
    }

    /// Compare text at a given position with a pattern string.
    fn text_compare_at(&self, pos: Position, pattern: &str, case_sensitive: bool) -> Ordering {
        if pattern.is_empty() {
            return Ordering::Equal;
        }

        let line_start = self.rope.try_line_to_char(pos.line).ok();
        for (i, pattern_ch) in pattern.chars().enumerate() {
            let ach = line_start
                .and_then(|line_start| self.rope.get_char(line_start + pos.column + i))
                .map(|c| if case_sensitive { c } else { c.to_ascii_lowercase() })
                .unwrap_or(' ');
            let pch = if case_sensitive {
                pattern_ch
            } else {
                pattern_ch.to_ascii_lowercase()
            };
            match ach.cmp(&pch) {
                Ordering::Less => return Ordering::Less,
                Ordering::Greater => return Ordering::Greater,
                Ordering::Equal => {}
            }
        }
        Ordering::Equal
    }
}

/// Convert a boolean condition to a CmdResult.
fn bool_result(condition: bool) -> CmdResult {
    if condition {
        CmdResult::Success
    } else {
        CmdResult::Failure(CmdFailure::OutOfRange)
    }
}

/// Parse the trailing parameter for EQC as a 1-based column number,
/// converting to 0-based.
fn parse_column_tpar(tpar: &TrailParam) -> Option<usize> {
    let n: usize = tpar.str.trim().parse().ok()?;
    if n == 0 {
        return Some(0);
    }
    Some(n - 1)
}

/// Parse the trailing parameter for EQM as a mark identifier.
fn parse_mark_tpar(tpar: &TrailParam) -> Option<MarkId> {
    let s = tpar.str.trim();
    match s {
        "=" => Some(MarkId::Equals),
        "%" => Some(MarkId::Modified),
        _ => {
            let n: u8 = s.parse().ok()?;
            if (1..=9).contains(&n) {
                Some(MarkId::Numbered(n))
            } else {
                None
            }
        }
    }
}
