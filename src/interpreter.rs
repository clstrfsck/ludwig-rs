//! Command execution engine for Ludwig compiled code.
//!
//! This module interprets compiled Ludwig commands (from [`CompiledCode`]) and executes
//! them against an [`ExecutionContext`]. It handles control flow including compound commands
//! with repetition, exit handlers, and exit level unwinding (XS/XF/XA).

use crate::code::*;
use crate::exec_context::{ExecutionContext, MAX_RECURSION_DEPTH, parse_span_name};
use crate::frame::{
    CaseMode, EditCommands, MotionCommands, PredicateCommands, SearchCommands, WordCommands,
};
use crate::{CmdFailure, CmdResult, LeadParam, TrailParam, compile};

/// Execute compiled code against an execution context. Top-level entry point.
///
/// Executes each instruction sequentially until completion or until
/// a failure/exit occurs.
pub fn execute(ctx: &mut ExecutionContext, code: &CompiledCode) -> ExecOutcome {
    for instr in code.instructions() {
        let outcome = execute_instruction(ctx, instr);
        match outcome {
            ExecOutcome::Success => continue,
            _ => return outcome,
        }
    }
    ExecOutcome::Success
}

/// Execute a single instruction.
fn execute_instruction(ctx: &mut ExecutionContext, instr: &Instruction) -> ExecOutcome {
    match instr {
        Instruction::SimpleCmd {
            op,
            lead,
            tpars,
            exit_handler,
        } => {
            let outcome = match op {
                CmdOp::SpanExecute => execute_span(ctx, *lead, tpars, true),
                CmdOp::SpanExecuteNoRecompile => execute_span(ctx, *lead, tpars, false),
                _ => {
                    let result = dispatch_cmd(ctx, *op, *lead, tpars);
                    if result.is_success() {
                        ExecOutcome::Success
                    } else {
                        ExecOutcome::Failure
                    }
                }
            };
            apply_exit_handler(ctx, outcome, exit_handler.as_ref())
        }
        Instruction::CompoundCmd {
            repeat,
            body,
            exit_handler,
        } => {
            let outcome = execute_compound(ctx, *repeat, body);
            apply_exit_handler(ctx, outcome, exit_handler.as_ref())
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
fn execute_compound(
    ctx: &mut ExecutionContext,
    repeat: RepeatCount,
    body: &CompiledCode,
) -> ExecOutcome {
    match repeat {
        RepeatCount::Once => {
            let outcome = execute(ctx, body);
            unwrap_exit_level(outcome)
        }
        RepeatCount::Times(n) => {
            for _ in 0..n {
                let outcome = execute(ctx, body);
                let outcome = unwrap_exit_level(outcome);
                match outcome {
                    ExecOutcome::Success => continue,
                    _ => return outcome,
                }
            }
            ExecOutcome::Success
        }
        RepeatCount::Indefinite => loop {
            let outcome = execute(ctx, body);
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
    ctx: &mut ExecutionContext,
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
                execute(ctx, code)
            } else {
                ExecOutcome::Success
            }
        }
        ExecOutcome::Failure => {
            if let Some(code) = &handler.on_failure {
                execute(ctx, code)
            } else {
                ExecOutcome::Success
            }
        }
        // XS/XF/XA/Abort propagate through handlers without triggering them
        _ => outcome,
    }
}

/// Execute a span procedure (EX / EN).
///
/// `recompile`:
/// - `true`  (EX) — always read and compile the span text; cache the result.
/// - `false` (EN) — use cached compiled code if present; compile and cache on
///   first call.
///
/// The lead param may be `None`/`Plus` (run once) or `Pint(n)` (run n times).
/// Returns `Failure` if the span name is invalid, the span does not exist, or
/// the recursion depth limit is exceeded.
fn execute_span(
    ctx: &mut ExecutionContext,
    lead: LeadParam,
    tpars: &[TrailParam],
    recompile: bool,
) -> ExecOutcome {
    // Validate lead and derive repeat count (None = indefinite).
    let count: Option<usize> = match lead {
        LeadParam::None | LeadParam::Plus => Some(1),
        LeadParam::Pint(n) => Some(n),
        LeadParam::Pindef => None,
        _ => return ExecOutcome::Failure,
    };

    // Parse the span name.
    let span_name = match parse_span_name(&tpars[0]) {
        Some(n) => n,
        None => return ExecOutcome::Failure,
    };

    // Recursion guard.
    if ctx.recursion_depth >= MAX_RECURSION_DEPTH {
        return ExecOutcome::Failure;
    }

    // Obtain compiled code.
    // For EX: always read + compile + cache.
    // For EN: use cache if present, else read + compile + cache.
    let compiled = if recompile {
        // Read and compile the span/frame text.
        let text = match ctx.read_span_or_frame_text(&span_name) {
            Some(t) => t,
            None => return ExecOutcome::Failure,
        };
        let code = match compile(&text) {
            Ok(c) => c,
            Err(_) => return ExecOutcome::Failure,
        };
        // Cache it.
        if let Some(span) = ctx.frame_set.get_span_mut(&span_name) {
            span.set_code(code.clone());
        } else if let Some(frame) = ctx.frame_set.get_frame_mut(&span_name) {
            frame.set_code(code.clone());
        }
        code
    } else {
        // Try cache first.
        let cached = if let Some(span) = ctx.frame_set.get_span(&span_name) {
            span.get_code().cloned()
        } else if let Some(frame) = ctx.frame_set.get_frame(&span_name) {
            frame.get_code().cloned()
        } else {
            return ExecOutcome::Failure;
        };

        if let Some(code) = cached {
            code
        } else {
            // No cache — compile and store.
            let text = match ctx.read_span_or_frame_text(&span_name) {
                Some(t) => t,
                None => return ExecOutcome::Failure,
            };
            let code = match compile(&text) {
                Ok(c) => c,
                Err(_) => return ExecOutcome::Failure,
            };
            if let Some(span) = ctx.frame_set.get_span_mut(&span_name) {
                span.set_code(code.clone());
            } else if let Some(frame) = ctx.frame_set.get_frame_mut(&span_name) {
                frame.set_code(code.clone());
            }
            code
        }
    };

    // Execute the compiled code, respecting the repeat count.
    ctx.recursion_depth += 1;
    let outcome = match count {
        Some(n) => {
            let mut outcome = ExecOutcome::Success;
            for _ in 0..n {
                outcome = execute(ctx, &compiled);
                outcome = unwrap_exit_level(outcome);
                match outcome {
                    ExecOutcome::Success => continue,
                    _ => break,
                }
            }
            outcome
        }
        None => loop {
            let outcome = execute(ctx, &compiled);
            let outcome = unwrap_exit_level(outcome);
            match outcome {
                ExecOutcome::Success => continue,
                other => break other,
            }
        },
    };
    ctx.recursion_depth -= 1;
    outcome
}

/// Dispatch a CmdOp to the appropriate handler.
fn dispatch_cmd(
    ctx: &mut ExecutionContext,
    op: CmdOp,
    lead: LeadParam,
    tpars: &[TrailParam],
) -> CmdResult {
    match op {
        CmdOp::Advance => ctx.current_frame_mut().cmd_advance(lead),
        CmdOp::Jump => ctx.current_frame_mut().cmd_jump(lead),
        CmdOp::DeleteChar => ctx.current_frame_mut().cmd_delete_char(lead),
        CmdOp::InsertText => ctx.current_frame_mut().cmd_insert_text(lead, &tpars[0]),
        CmdOp::OvertypeText => ctx.current_frame_mut().cmd_overtype_text(lead, &tpars[0]),
        CmdOp::InsertChar => ctx.current_frame_mut().cmd_insert_char(lead),
        CmdOp::InsertLine => ctx.current_frame_mut().cmd_insert_line(lead),
        CmdOp::SplitLine => ctx.current_frame_mut().cmd_split_line(lead),
        CmdOp::DeleteLine => ctx.current_frame_mut().cmd_delete_line(lead),
        CmdOp::CaseUp => ctx
            .current_frame_mut()
            .cmd_case_change(lead, CaseMode::Upper),
        CmdOp::CaseLow => ctx
            .current_frame_mut()
            .cmd_case_change(lead, CaseMode::Lower),
        CmdOp::CaseEdit => ctx
            .current_frame_mut()
            .cmd_case_change(lead, CaseMode::Edit),
        CmdOp::Next => ctx.current_frame_mut().cmd_next(lead, &tpars[0]),
        CmdOp::Bridge => ctx.current_frame_mut().cmd_bridge(lead, &tpars[0]),
        CmdOp::Left => ctx.current_frame_mut().cmd_left(lead),
        CmdOp::Right => ctx.current_frame_mut().cmd_right(lead),
        CmdOp::Up => ctx.current_frame_mut().cmd_up(lead),
        CmdOp::Down => ctx.current_frame_mut().cmd_down(lead),
        CmdOp::Return => ctx.current_frame_mut().cmd_return(lead),
        CmdOp::Rubout => ctx.current_frame_mut().cmd_rubout(lead),
        CmdOp::EqualEol => ctx.current_frame_mut().cmd_eol(lead),
        CmdOp::EqualEop => ctx.current_frame_mut().cmd_eop(lead),
        CmdOp::EqualEof => ctx.current_frame_mut().cmd_eof(lead),
        CmdOp::EqualColumn => ctx.current_frame_mut().cmd_eqc(lead, &tpars[0]),
        CmdOp::EqualMark => ctx.current_frame_mut().cmd_eqm(lead, &tpars[0]),
        CmdOp::EqualString => ctx.current_frame_mut().cmd_eqs(lead, &tpars[0]),
        CmdOp::Mark => ctx.current_frame_mut().cmd_mark(lead),
        CmdOp::Replace => ctx
            .current_frame_mut()
            .cmd_replace(lead, &tpars[0], &tpars[1]),
        CmdOp::SwapLine => ctx.current_frame_mut().cmd_swap_line(lead),
        CmdOp::Get => ctx.current_frame_mut().cmd_get(lead, &tpars[0]),
        CmdOp::WordAdvance => ctx.current_frame_mut().cmd_word_advance(lead),
        CmdOp::WordDelete => ctx.current_frame_mut().cmd_word_delete(lead),
        CmdOp::LineSquash => ctx.current_frame_mut().cmd_line_squeeze(lead),
        CmdOp::DittoUp => ctx.current_frame_mut().cmd_ditto_up(lead),
        CmdOp::DittoDown => ctx.current_frame_mut().cmd_ditto_down(lead),
        // Window commands are no-ops in batch mode; handled by App in interactive mode.
        CmdOp::WindowForward
        | CmdOp::WindowBackward
        | CmdOp::WindowLeft
        | CmdOp::WindowRight
        | CmdOp::WindowTop
        | CmdOp::WindowEnd
        | CmdOp::WindowNew
        | CmdOp::WindowMiddle => CmdResult::Success,
        // Span commands
        CmdOp::SpanDefine => ctx.cmd_span_define(lead, tpars),
        CmdOp::SpanCopy => ctx.cmd_span_copy(lead, &tpars[0]),
        CmdOp::SpanTransfer => ctx.cmd_span_transfer(lead, tpars),
        CmdOp::SpanJump => ctx.cmd_span_jump(lead, tpars),
        CmdOp::SpanAssign => ctx.cmd_span_assign(lead, tpars),
        CmdOp::SpanIndex => ctx.cmd_span_index(),
        CmdOp::SpanCompile => ctx.cmd_span_compile(lead, tpars),
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
        assert_eq!(
            unwrap_exit_level(ExecOutcome::Success),
            ExecOutcome::Success
        );
        assert_eq!(
            unwrap_exit_level(ExecOutcome::Failure),
            ExecOutcome::Failure
        );
        assert_eq!(unwrap_exit_level(ExecOutcome::Abort), ExecOutcome::Abort);
        assert_eq!(
            unwrap_exit_level(ExecOutcome::ExitSuccessAll),
            ExecOutcome::ExitSuccessAll
        );
        assert_eq!(
            unwrap_exit_level(ExecOutcome::ExitFailureAll),
            ExecOutcome::ExitFailureAll
        );
    }
}
