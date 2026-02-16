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
}
