//! Command execution engine for Ludwig compiled code.
//!
//! This module interprets compiled Ludwig commands (from [`CompiledCode`]) and executes
//! them against a [`Frame`]. It handles control flow including compound commands with
//! repetition, exit handlers, and exit level unwinding (XS/XF/XA).

use crate::frame::{CaseMode, EditCommands, MotionCommands, SearchCommands};
use crate::{CmdFailure, CmdResult, Frame, LeadParam, TrailParam};
use crate::code::*;

/// Execute compiled code against a frame. Top-level entry point.
///
/// Executes each instruction sequentially until completion or until
/// a failure/exit occurs.
pub fn execute(frame: &mut Frame, code: &CompiledCode) -> ExecOutcome {
    for instr in &code.instructions {
        let outcome = execute_instruction(frame, instr);
        match outcome {
            ExecOutcome::Success => continue,
            _ => return outcome,
        }
    }
    ExecOutcome::Success
}

/// Execute a single instruction.
fn execute_instruction(frame: &mut Frame, instr: &Instruction) -> ExecOutcome {
    match instr {
        Instruction::SimpleCmd {
            op,
            lead,
            tpar,
            exit_handler,
        } => {
            let result = dispatch_cmd(frame, *op, *lead, tpar.as_ref());
            let outcome = if result.is_success() {
                ExecOutcome::Success
            } else {
                ExecOutcome::Failure
            };
            apply_exit_handler(frame, outcome, exit_handler.as_ref())
        }
        Instruction::CompoundCmd {
            repeat,
            body,
            exit_handler,
        } => {
            let outcome = execute_compound(frame, *repeat, body);
            apply_exit_handler(frame, outcome, exit_handler.as_ref())
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
fn execute_compound(frame: &mut Frame, repeat: RepeatCount, body: &CompiledCode) -> ExecOutcome {
    match repeat {
        RepeatCount::Once => {
            let outcome = execute(frame, body);
            unwrap_exit_level(outcome)
        }
        RepeatCount::Times(n) => {
            for _ in 0..n {
                let outcome = execute(frame, body);
                let outcome = unwrap_exit_level(outcome);
                match outcome {
                    ExecOutcome::Success => continue,
                    _ => return outcome,
                }
            }
            ExecOutcome::Success
        }
        RepeatCount::Indefinite => loop {
            let outcome = execute(frame, body);
            let outcome = unwrap_exit_level(outcome);
            match outcome {
                ExecOutcome::Success => continue,
                other => return other,
            }
        },
    }
}

/// Decrement exit level counters at a compound command boundary.
fn unwrap_exit_level(outcome: ExecOutcome) -> ExecOutcome {
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
    frame: &mut Frame,
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
                execute(frame, code)
            } else {
                ExecOutcome::Success
            }
        }
        ExecOutcome::Failure => {
            if let Some(code) = &handler.on_failure {
                execute(frame, code)
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
    frame: &mut Frame,
    op: CmdOp,
    lead: LeadParam,
    tpar: Option<&TrailParam>,
) -> CmdResult {
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
        CmdOp::Next => frame.cmd_next(lead, tpar.unwrap()),
        CmdOp::Bridge => frame.cmd_bridge(lead, tpar.unwrap()),
        // FIXME: remove this when everything is implemented
        _ => CmdResult::Failure(CmdFailure::NotImplemented),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unwrap_exit_success_to_success() {
        assert_eq!(
            unwrap_exit_level(ExecOutcome::ExitSuccess { remaining: 1 }),
            ExecOutcome::Success
        );
    }

    #[test]
    fn test_unwrap_exit_success_decrements() {
        assert_eq!(
            unwrap_exit_level(ExecOutcome::ExitSuccess { remaining: 3 }),
            ExecOutcome::ExitSuccess { remaining: 2 }
        );
    }

    #[test]
    fn test_unwrap_exit_failure_to_failure() {
        assert_eq!(
            unwrap_exit_level(ExecOutcome::ExitFailure { remaining: 1 }),
            ExecOutcome::Failure
        );
    }

    #[test]
    fn test_unwrap_exit_failure_decrements() {
        assert_eq!(
            unwrap_exit_level(ExecOutcome::ExitFailure { remaining: 3 }),
            ExecOutcome::ExitFailure { remaining: 2 }
        );
    }

    #[test]
    fn test_unwrap_passes_through_other_outcomes() {
        assert_eq!(unwrap_exit_level(ExecOutcome::Success), ExecOutcome::Success);
        assert_eq!(unwrap_exit_level(ExecOutcome::Failure), ExecOutcome::Failure);
        assert_eq!(unwrap_exit_level(ExecOutcome::Abort), ExecOutcome::Abort);
        assert_eq!(unwrap_exit_level(ExecOutcome::ExitSuccessAll), ExecOutcome::ExitSuccessAll);
        assert_eq!(unwrap_exit_level(ExecOutcome::ExitFailureAll), ExecOutcome::ExitFailureAll);
    }
}
