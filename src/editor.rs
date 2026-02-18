//! The Editor wraps a Frame and provides a high-level interface for executing
//! compiled Ludwig commands.

use std::fmt;

use crate::{MarkId, code::*};
use crate::Frame;
use crate::interpreter;

/// An editor instance that wraps a Frame and provides command execution.
pub struct Editor {
    frame: Frame,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    /// Create a new empty editor.
    pub fn new() -> Self {
        Editor {
            frame: Frame::new(),
        }
    }

    /// Create an editor from a string.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Editor {
            frame: Frame::from_str(s),
        }
    }

    /// Get a reference to the current frame.
    pub fn current_frame(&self) -> &Frame {
        &self.frame
    }

    /// Get a mutable reference to the current frame.
    pub fn current_frame_mut(&mut self) -> &mut Frame {
        &mut self.frame
    }

    /// Check if the frame has been modified.
    pub fn modified(&self) -> bool {
        self.frame.get_mark(MarkId::Modified).is_some()
    }

    /// Execute compiled code against the frame.
    ///
    /// This delegates to the interpreter module which handles all control flow,
    /// exit handlers, and command dispatch.
    pub fn execute(&mut self, code: &CompiledCode) -> ExecOutcome {
        interpreter::execute(&mut self.frame, code)
    }
}

impl fmt::Display for Editor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;
    use crate::compiler::compile;

    // Helper: compile and execute, return outcome
    fn exec(content: &str, commands: &str) -> (Editor, ExecOutcome) {
        let mut editor = Editor::from_str(content);
        let code = compile(commands).unwrap();
        let outcome = editor.execute(&code);
        (editor, outcome)
    }

    // --- Basic dispatch ---

    #[test]
    fn test_execute_insert_command() {
        let (editor, outcome) = exec("hello\n", "i/world /");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "world hello\n");
    }

    #[test]
    fn test_execute_multiple_commands() {
        let (editor, outcome) = exec("hello world\n", "5ji/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello! world\n");
    }

    #[test]
    fn test_execute_commands_failure_stops() {
        let (editor, outcome) = exec("hello\n", "2ai/!/");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(editor.to_string(), "hello\n");
    }

    #[test]
    fn test_display_delegates_to_frame() {
        let editor = Editor::from_str("test content\n");
        assert_eq!(editor.to_string(), "test content\n");
    }

    #[test]
    fn test_new_empty_editor() {
        let editor = Editor::new();
        assert_eq!(editor.to_string(), "");
    }

    // --- Exit handler tests ---

    #[test]
    fn test_exit_handler_success_branch() {
        // A succeeds, so the success handler runs and inserts "ok"
        let (editor, outcome) = exec("line1\nline2\n", "A[I/ok/]");
        assert_eq!(outcome, ExecOutcome::Success);
        // A advances to next line (end of text), I inserts "ok" there
        assert_eq!(editor.to_string(), "line1\nokline2\n");
    }

    #[test]
    fn test_exit_handler_failure_branch() {
        // 2A fails on single-line content, so failure handler runs
        let (editor, outcome) = exec("hello\n", "2A[:I/fail/]");
        // The failure handler inserts "fail" (success outcome from handler)
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "failhello\n");
    }

    #[test]
    fn test_exit_handler_no_matching_branch() {
        // A succeeds, but only failure handler defined → outcome passes through
        let (_, outcome) = exec("hello\n", "A[:I/fail/]");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    // --- Compound command tests ---

    #[test]
    fn test_compound_times() {
        // 3(J) jumps 3 positions forward
        let (editor, outcome) = exec("hello world\n", "3(J)I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hel!lo world\n");
    }

    #[test]
    fn test_compound_indefinite() {
        // >(A) advances until it fails at end of file
        let (editor, outcome) = exec("line1\nline2\nline3\n", ">(A)[i/yes/:i/no/]");
        // The exit handler should run successfully
        assert_eq!(outcome, ExecOutcome::Success);
        // Should end up at the end, insert "no"
        assert_eq!(editor.to_string(), "line1\nline2\nnoline3\n");
    }

    #[test]
    fn test_compound_fails() {
        // >(A) advances until it fails at end of file
        let (editor, outcome) = exec("line1\nline2\nline3\n", ">(A)");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(editor.to_string(), "line1\nline2\nline3\n");
    }

    #[test]
    fn test_compound_succeeds_with_empty_exit_handler_1() {
        // >(A) advances until it fails at end of file
        let (editor, outcome) = exec("line1\nline2\nline3\n", ">(A)[:]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "line1\nline2\nline3\n");
    }

    #[test]
    fn test_compound_succeeds_with_empty_exit_handler_2() {
        // >(A) advances until it fails at end of file
        let (editor, outcome) = exec("line1\nline2\nline3\n", ">(A)[]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "line1\nline2\nline3\n");
    }

    #[test]
    fn test_compound_once() {
        let (editor, outcome) = exec("hello\n", "(5J)I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello!\n");
    }

    #[test]
    fn test_compound_times_failure() {
        // 10(A) on 2-line content should fail partway
        let (_, outcome) = exec("line1\nline2", "10(A)");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    // --- XS/XF/XA tests ---

    #[test]
    fn test_exit_success_in_compound() {
        // (A XS J) — A succeeds, XS exits the group with success, J does not run
        let (editor, outcome) = exec("line1\nline2", "(A XS 5J)I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        // XS exits the compound, then I runs. Dot is at line 1 col 0 after A.
        assert_eq!(editor.current_frame().dot(), Position::new(1, 1));
    }

    #[test]
    fn test_exit_failure_in_compound() {
        // (A XF J) — XF exits compound with failure
        let (_, outcome) = exec("line1\nline2", "(A XF J)");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_exit_abort() {
        let (_, outcome) = exec("hello", "XA");
        assert_eq!(outcome, ExecOutcome::Abort);
    }

    #[test]
    fn test_exit_abort_in_nested() {
        // XA propagates through everything
        let (_, outcome) = exec("hello", "(((XA)))");
        assert_eq!(outcome, ExecOutcome::Abort);
    }

    #[test]
    fn test_xs_multi_level() {
        // 2XS exits 2 levels
        let (editor, outcome) = exec("line1\nline2\nline3", "((A 2XS 5J))I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        // 2XS exits both inner and outer parens, then I runs
        assert_eq!(editor.current_frame().dot(), Position::new(1, 1));
    }

    #[test]
    fn test_xs_all_levels() {
        // >XS exits all levels, so I never runs
        let (editor, outcome) = exec("line1\nline2\n", "(((A >XS 5J)))I/!/");
        assert_eq!(outcome, ExecOutcome::ExitSuccessAll);
        assert_eq!(editor.to_string(), "line1\nline2\n");
        assert_eq!(editor.current_frame().dot(), Position::new(1, 0));
    }

    // --- Failure propagation ---

    #[test]
    fn test_failure_stops_sequence() {
        // 99A fails, I should not execute
        let (editor, outcome) = exec("hello\n", "99AI/!/");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(editor.to_string(), "hello\n");
    }

    // --- Exit handler on compound ---

    #[test]
    fn test_compound_exit_handler() {
        // >(A) fails after exhausting lines, failure handler inserts text
        let (editor, outcome) = exec("line1\nline2\n", ">(A)[:I/done/]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "line1\ndoneline2\n");
    }

    // --- XS/XF don't trigger exit handlers ---

    #[test]
    fn test_xs_propagates_through_handler() {
        // Exit commands propagate through handlers without triggering them
        let (_, outcome) = exec("hello", "XS[I/no/:I/no/]");
        // XS at top level becomes ExitSuccess{1} which has no compound to catch it
        assert_eq!(outcome, ExecOutcome::ExitSuccess { remaining: 1 });
    }

    #[test]
    fn test_eol_at_end_of_line() {
        let (_, outcome) = exec("hello\n", "5JEOL");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eol_not_at_end() {
        let (_, outcome) = exec("hello\n", "EOL");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_eol_inverted() {
        let (_, outcome) = exec("hello\n", "-EOL");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eol_inverted_at_end() {
        let (_, outcome) = exec("hello\n", "5J-EOL");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_eop_at_end() {
        // >A advances to the end (null line)
        let (_, outcome) = exec("hello\n", ">AEOP");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eop_not_at_end() {
        let (_, outcome) = exec("hello\nworld\n", "EOP");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_eop_inverted() {
        let (_, outcome) = exec("hello\nworld\n", "-EOP");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eof_same_as_eop() {
        let (_, outcome) = exec("hello\n", ">AEOF");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqc_equal() {
        // Dot is at column 0 (1-based = 1)
        let (_, outcome) = exec("hello\n", "EQC'1'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqc_not_equal() {
        let (_, outcome) = exec("hello\n", "EQC'5'");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_eqc_inverted() {
        let (_, outcome) = exec("hello\n", "-EQC'5'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqc_greater_or_equal() {
        // Dot at column 3 (1-based=4), test >=3 (1-based)
        let (_, outcome) = exec("hello\n", "3J>EQC'3'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqc_less_or_equal() {
        let (_, outcome) = exec("hello\n", "<EQC'3'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqs_match_case_insensitive() {
        let (_, outcome) = exec("Hello\n", "EQS/hello/");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqs_match_exact_case() {
        let (_, outcome) = exec("Hello\n", r#"EQS"Hello""#);
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqs_no_match_exact_case() {
        let (_, outcome) = exec("Hello\n", r#"EQS"hello""#);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_eqs_inverted() {
        let (_, outcome) = exec("Hello\n", "-EQS/world/");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqs_partial_match() {
        let (_, outcome) = exec("Hello World\n", "EQS/hello w/");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_mark_set_and_eqm() {
        // Set mark 1 at dot (0,0), advance, then test EQM
        let (_, outcome) = exec("hello\nworld\n", "M A EQM'1'");
        // After M: mark1=(0,0), after A: dot=(1,0), EQM'1' tests dot==(0,0) → false
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_mark_set_and_eqm_equal() {
        // Set mark, don't move, test equals
        let (_, outcome) = exec("hello\n", "M EQM'1'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_mark_set_numbered() {
        // Set mark 3, test against it
        let (_, outcome) = exec("hello\n", "3M EQM'3'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_mark_unset() {
        // Set mark 1, then unset it, then test — should fail (mark not defined)
        let (_, outcome) = exec("hello\n", "M -M EQM'1'");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_eqm_inverted() {
        // Set mark 1, advance, -EQM should succeed (dot != mark)
        let (_, outcome) = exec("hello\nworld\n", "M A -EQM'1'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqm_greater_or_equal() {
        // Set mark at (0,0), advance to (1,0), >EQM → dot >= mark → true
        let (_, outcome) = exec("hello\nworld\n", "M A >EQM'1'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqm_less_or_equal() {
        // Set mark at (0,0), don't advance, <EQM → dot <= mark → true
        let (_, outcome) = exec("hello\nworld\n", "M <EQM'1'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_predicate_in_loop() {
        // Use EOL in a loop to advance to end of line
        let (editor, outcome) = exec("hello\n", ">(-EOL J)");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(editor.current_frame().dot(), Position::new(0, 5));
    }

    #[test]
    fn test_replace_simple() {
        let (editor, outcome) = exec("hello world\n", "R/world/earth/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello earth\n");
    }

    #[test]
    fn test_replace_not_found() {
        let (_, outcome) = exec("hello world\n", "R/xyz/abc/");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_replace_case_insensitive() {
        let (editor, outcome) = exec("Hello World\n", "R/hello/goodbye/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "goodbye World\n");
    }

    #[test]
    fn test_replace_case_sensitive() {
        let (_, outcome) = exec("Hello World\n", r#"R"hello"goodbye""#);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_replace_case_sensitive_match() {
        let (editor, outcome) = exec("Hello World\n", r#"R"Hello"Goodbye""#);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "Goodbye World\n");
    }

    #[test]
    fn test_replace_multiple() {
        let (editor, outcome) = exec("aaa bbb aaa\n", "2R/aaa/ccc/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "ccc bbb ccc\n");
    }

    #[test]
    fn test_replace_all() {
        let (editor, outcome) = exec("aa bb aa bb aa\n", ">R/aa/cc/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "cc bb cc bb cc\n");
    }

    #[test]
    fn test_replace_with_empty() {
        let (editor, outcome) = exec("hello world\n", "R/world//");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello \n");
    }

    #[test]
    fn test_replace_with_longer() {
        let (editor, outcome) = exec("hi\n", "R/hi/hello world/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello world\n");
    }

    #[test]
    fn test_swap_line_default() {
        let (editor, outcome) = exec("line1\nline2\nline3\n", "SW");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "line2\nline1\nline3\n");
        // Dot follows original line (line1 moved to line 1)
        assert_eq!(editor.current_frame().dot(), Position::new(1, 0));
    }

    #[test]
    fn test_swap_line_backward() {
        let (editor, outcome) = exec("line1\nline2\nline3\n", "A-SW");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "line2\nline1\nline3\n");
        // Dot was on line 1 (line2), after -SW it's now on line 0
        assert_eq!(editor.current_frame().dot(), Position::new(0, 0));
    }

    #[test]
    fn test_swap_line_at_last_fails() {
        // Can't swap when on the last content line (no line below except null)
        let (_, outcome) = exec("line1\nline2\n", "ASW");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_swap_line_n_positions() {
        let (editor, outcome) = exec("line1\nline2\nline3\nline4\n", "2SW");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "line2\nline3\nline1\nline4\n");
        assert_eq!(editor.current_frame().dot(), Position::new(2, 0));
    }

    #[test]
    fn test_get_forward() {
        let (editor, outcome) = exec("hello world\n", "G/world/");
        assert_eq!(outcome, ExecOutcome::Success);
        // Dot → after "world" (col 11), Equals → start of "world" (col 6)
        assert_eq!(editor.current_frame().dot(), Position::new(0, 11));
        assert_eq!(
            editor.current_frame().get_mark(MarkId::Equals),
            Some(Position::new(0, 6))
        );
    }

    #[test]
    fn test_get_not_found() {
        let (_, outcome) = exec("hello world\n", "G/xyz/");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_get_case_insensitive() {
        let (editor, outcome) = exec("Hello World\n", "G/hello/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().dot(), Position::new(0, 5));
    }

    #[test]
    fn test_get_case_sensitive() {
        let (_, outcome) = exec("Hello World\n", r#"G"hello""#);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_get_nth_occurrence() {
        let (editor, outcome) = exec("aa bb aa bb\n", "2G/aa/");
        assert_eq!(outcome, ExecOutcome::Success);
        // Second "aa" is at col 6-8
        assert_eq!(editor.current_frame().dot(), Position::new(0, 8));
    }

    #[test]
    fn test_get_backward() {
        let (editor, outcome) = exec("hello world hello\n", ">J-G/hello/");
        assert_eq!(outcome, ExecOutcome::Success);
        // Searching backward from col 17, finds "hello" at col 12
        assert_eq!(editor.current_frame().dot(), Position::new(0, 12));
        assert_eq!(
            editor.current_frame().get_mark(MarkId::Equals),
            Some(Position::new(0, 17))
        );
    }

    #[test]
    fn test_word_advance_forward() {
        let (editor, outcome) = exec("hello world test\n", "YA");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().dot(), Position::new(0, 6));
    }

    #[test]
    fn test_word_advance_n() {
        let (editor, outcome) = exec("hello world test\n", "2YA");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().dot(), Position::new(0, 12));
    }

    #[test]
    fn test_word_advance_current() {
        // Move to middle of word, then 0YA to start of word
        let (editor, outcome) = exec("hello world\n", "3J 0YA");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().dot(), Position::new(0, 0));
    }

    #[test]
    fn test_word_advance_backward() {
        let (editor, outcome) = exec("hello world test\n", ">J -YA");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().dot(), Position::new(0, 6));
    }

    #[test]
    fn test_get_then_replace_at_match() {
        // Use G to find text, then use Equals mark for delete
        let (editor, outcome) = exec("hello world\n", "G/world/ =D");
        assert_eq!(outcome, ExecOutcome::Success);
        // G finds "world", dot=11, Equals=6. =D deletes from dot to Equals mark.
        assert_eq!(editor.to_string(), "hello \n");
    }
}
