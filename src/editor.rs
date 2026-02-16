use std::fmt;

use crate::frame::{CaseMode, EditCommands, MotionCommands};
use crate::{CmdFailure, MarkId, code::*};
use crate::{CmdResult, Frame, TrailParam};

pub struct Editor {
    frame: Frame,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

impl Editor {
    pub fn new() -> Self {
        Editor {
            frame: Frame::new(),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Editor {
            frame: Frame::from_str(s),
        }
    }

    pub fn current_frame(&self) -> &Frame {
        &self.frame
    }

    pub fn current_frame_mut(&mut self) -> &mut Frame {
        &mut self.frame
    }

    pub fn modified(&self) -> bool {
        self.frame.get_mark(MarkId::Modified).is_some()
    }

    /// Execute compiled code. Top-level entry point.
    pub fn execute(&mut self, code: &CompiledCode) -> ExecOutcome {
        for instr in &code.instructions {
            let outcome = self.execute_instruction(instr);
            match outcome {
                ExecOutcome::Success => continue,
                _ => return outcome,
            }
        }
        ExecOutcome::Success
    }

    /// Execute a single instruction.
    fn execute_instruction(&mut self, instr: &Instruction) -> ExecOutcome {
        match instr {
            Instruction::SimpleCmd {
                op,
                lead,
                tpar,
                exit_handler,
            } => {
                let result = self.dispatch_cmd(*op, *lead, tpar.as_ref());
                let outcome = if result.is_success() {
                    ExecOutcome::Success
                } else {
                    ExecOutcome::Failure
                };
                self.apply_exit_handler(outcome, exit_handler.as_ref())
            }
            Instruction::CompoundCmd {
                repeat,
                body,
                exit_handler,
            } => {
                let outcome = self.execute_compound(*repeat, body);
                self.apply_exit_handler(outcome, exit_handler.as_ref())
            }
            Instruction::ExitSuccess(levels) => match levels {
                ExitLevels::Count(n) => ExecOutcome::ExitSuccess { remaining: *n },
                ExitLevels::All => ExecOutcome::ExitSuccessAll,
            },
            Instruction::ExitFailure(levels) => match levels {
                ExitLevels::Count(n) => ExecOutcome::ExitFailure { remaining: *n },
                ExitLevels::All => ExecOutcome::ExitFailureAll,
            },
            Instruction::ExitAbort => ExecOutcome::Abort,
        }
    }

    /// Execute a compound command body based on RepeatCount.
    fn execute_compound(&mut self, repeat: RepeatCount, body: &CompiledCode) -> ExecOutcome {
        match repeat {
            RepeatCount::Once => {
                let outcome = self.execute(body);
                self.unwrap_exit_level(outcome)
            }
            RepeatCount::Times(n) => {
                for _ in 0..n {
                    let outcome = self.execute(body);
                    let outcome = self.unwrap_exit_level(outcome);
                    match outcome {
                        ExecOutcome::Success => continue,
                        _ => return outcome,
                    }
                }
                ExecOutcome::Success
            }
            RepeatCount::Indefinite => loop {
                let outcome = self.execute(body);
                let outcome = self.unwrap_exit_level(outcome);
                match outcome {
                    ExecOutcome::Success => continue,
                    other => return other,
                }
            },
        }
    }

    /// Decrement exit level counters at a compound command boundary.
    fn unwrap_exit_level(&self, outcome: ExecOutcome) -> ExecOutcome {
        match outcome {
            ExecOutcome::ExitSuccess { remaining } => {
                if remaining <= 1 {
                    ExecOutcome::Success
                } else {
                    ExecOutcome::ExitSuccess {
                        remaining: remaining - 1,
                    }
                }
            }
            ExecOutcome::ExitFailure { remaining } => {
                if remaining <= 1 {
                    ExecOutcome::Failure
                } else {
                    ExecOutcome::ExitFailure {
                        remaining: remaining - 1,
                    }
                }
            }
            other => other,
        }
    }

    /// Apply an exit handler to an outcome, running success/failure code as appropriate.
    fn apply_exit_handler(
        &mut self,
        outcome: ExecOutcome,
        handler: Option<&ExitHandler>,
    ) -> ExecOutcome {
        let handler = match handler {
            Some(h) => h,
            None => return outcome,
        };

        match &outcome {
            ExecOutcome::Success => {
                if let Some(code) = &handler.on_success {
                    self.execute(code)
                } else {
                    ExecOutcome::Success
                }
            }
            ExecOutcome::Failure => {
                if let Some(code) = &handler.on_failure {
                    self.execute(code)
                } else {
                    ExecOutcome::Success
                }
            }
            // XS/XF/XA/Abort propagate through handlers without triggering them
            _ => outcome,
        }
    }

    /// Dispatch a CmdOp to the appropriate Frame method.
    fn dispatch_cmd(
        &mut self,
        op: CmdOp,
        lead: crate::LeadParam,
        tpar: Option<&TrailParam>,
    ) -> CmdResult {
        let frame = &mut self.frame;
        match op {
            CmdOp::Advance => frame.cmd_advance(lead),
            CmdOp::Jump => frame.cmd_jump(lead),
            CmdOp::DeleteChar => frame.cmd_delete_char(lead),
            CmdOp::InsertText => frame.cmd_insert_text(lead, tpar.unwrap()),
            CmdOp::OvertypeText => frame.cmd_overtype_text(lead, tpar.unwrap()),
            CmdOp::InsertChar => frame.cmd_insert_char(lead),
            CmdOp::InsertLine => frame.cmd_insert_line(lead),
            CmdOp::SplitLine => frame.cmd_split_line(lead),
            CmdOp::DeleteLine => frame.cmd_delete_line(lead),
            CmdOp::CaseUp => frame.cmd_case_change(lead, CaseMode::Upper),
            CmdOp::CaseLow => frame.cmd_case_change(lead, CaseMode::Lower),
            CmdOp::CaseEdit => frame.cmd_case_change(lead, CaseMode::Edit),
            // FIXME: remove this when everything is implemented
            _ => CmdResult::Failure(CmdFailure::NotImplemented),
        }
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
        let (editor, outcome) = exec("hello", "i/world /");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "world hello");
    }

    #[test]
    fn test_execute_multiple_commands() {
        let (editor, outcome) = exec("hello world", "5ji/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello! world");
    }

    #[test]
    fn test_execute_commands_failure_stops() {
        let (editor, outcome) = exec("hello", "2ai/!/");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(editor.to_string(), "hello");
    }

    #[test]
    fn test_display_delegates_to_frame() {
        let editor = Editor::from_str("test content");
        assert_eq!(editor.to_string(), "test content");
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
        let (editor, outcome) = exec("hello", "2A[:I/fail/]");
        // The failure handler inserts "fail" (success outcome from handler)
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "failhello");
    }

    #[test]
    fn test_exit_handler_no_matching_branch() {
        // A succeeds, but only failure handler defined → outcome passes through
        let (_, outcome) = exec("hello", "A[:I/fail/]");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    // --- Compound command tests ---

    #[test]
    fn test_compound_times() {
        // 3(J) jumps 3 positions forward
        let (editor, outcome) = exec("hello world", "3(J)I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hel!lo world");
    }

    #[test]
    fn test_compound_indefinite() {
        // >(A) advances until it fails at end of file
        let (editor, outcome) = exec("line1\nline2\nline3", ">(A)[i/yes/:i/no/]");
        // The exit handler should run successfully
        assert_eq!(outcome, ExecOutcome::Success);
        // Should end up at the end, insert "no"
        assert_eq!(editor.to_string(), "line1\nline2\nnoline3");
    }

    #[test]
    fn test_compound_fails() {
        // >(A) advances until it fails at end of file
        let (editor, outcome) = exec("line1\nline2\nline3", ">(A)");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(editor.to_string(), "line1\nline2\nline3");
    }

    #[test]
    fn test_compound_succeeds_with_empty_exit_handler_1() {
        // >(A) advances until it fails at end of file
        let (editor, outcome) = exec("line1\nline2\nline3", ">(A)[:]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "line1\nline2\nline3");
    }

    #[test]
    fn test_compound_succeeds_with_empty_exit_handler_2() {
        // >(A) advances until it fails at end of file
        let (editor, outcome) = exec("line1\nline2\nline3", ">(A)[]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "line1\nline2\nline3");
    }

    #[test]
    fn test_compound_once() {
        let (editor, outcome) = exec("hello", "(5J)I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello!");
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
        let (editor, outcome) = exec("line1\nline2", "(((A >XS 5J)))I/!/");
        assert_eq!(outcome, ExecOutcome::ExitSuccessAll);
        assert_eq!(editor.to_string(), "line1\nline2");
        assert_eq!(editor.current_frame().dot(), Position::new(1, 0));
    }

    // --- Failure propagation ---

    #[test]
    fn test_failure_stops_sequence() {
        // 99A fails, I should not execute
        let (editor, outcome) = exec("hello", "99AI/!/");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(editor.to_string(), "hello");
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
