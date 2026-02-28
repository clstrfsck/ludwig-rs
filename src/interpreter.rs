//! Command execution engine for Ludwig compiled code.
//!
//! This module interprets compiled Ludwig commands (from [`CompiledCode`]) and executes
//! them against an [`ExecutionContext`]. It handles control flow including compound commands
//! with repetition, exit handlers, and exit level unwinding (XS/XF/XA).

use crate::Position;
use crate::code::*;
use crate::exec_context::{ExecutionContext, MAX_RECURSION_DEPTH, parse_span_name};
use crate::frame::{
    CaseMode, EditCommands, MotionCommands, PredicateCommands, SearchCommands, WordCommands,
};
use crate::marks::MarkId;
use crate::frame_set::COMMAND_FRAME_NAME;
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
                CmdOp::FileExecute => execute_file_execute(ctx, *lead, tpars),
                CmdOp::ExecuteString => execute_cmd_string(ctx, *lead, tpars),
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
        CmdOp::LineFill => ctx.current_frame_mut().cmd_line_fill(lead),
        CmdOp::LineJustify => ctx.current_frame_mut().cmd_line_justify(lead),
        CmdOp::LineCentre => ctx.current_frame_mut().cmd_line_centre(lead),
        CmdOp::LineLeft => ctx.current_frame_mut().cmd_line_left(lead),
        CmdOp::LineRight => ctx.current_frame_mut().cmd_line_right(lead),
        CmdOp::DittoUp => ctx.current_frame_mut().cmd_ditto_up(lead),
        CmdOp::DittoDown => ctx.current_frame_mut().cmd_ditto_down(lead),
        CmdOp::Tab => ctx.current_frame_mut().cmd_tab(lead),
        CmdOp::Backtab => ctx.current_frame_mut().cmd_backtab(lead),
        // ZH: move to home position.  The screen backend resolves the target
        // position (top-left of viewport in interactive mode; (0,0) in batch).
        CmdOp::Home => {
            let dot = ctx.current_frame().dot();
            let line_count = ctx.current_frame().line_count();
            let new_dot = ctx
                .screen
                .handle_window_cmd(op, lead, dot, line_count)
                .unwrap_or(Position::new(0, 0));
            ctx.current_frame_mut().set_mark_at(MarkId::Equals, dot);
            ctx.current_frame_mut().set_dot(new_dot);
            CmdResult::Success
        }
        // Window commands are dispatched to the screen backend.
        // In batch mode the backend is a no-op; in interactive mode it updates
        // the viewport (and may return a new dot position for WF/WB).
        CmdOp::WindowForward
        | CmdOp::WindowBackward
        | CmdOp::WindowLeft
        | CmdOp::WindowRight
        | CmdOp::WindowTop
        | CmdOp::WindowEnd
        | CmdOp::WindowNew
        | CmdOp::WindowMiddle
        | CmdOp::WindowScroll
        | CmdOp::WindowSetHeight
        | CmdOp::WindowUpdate => {
            let dot = ctx.current_frame().dot();
            let line_count = ctx.current_frame().line_count();
            if let Some(new_dot) = ctx.screen.handle_window_cmd(op, lead, dot, line_count) {
                ctx.current_frame_mut().set_dot(new_dot);
            }
            CmdResult::Success
        }
        // V — Verify: always succeed in batch mode.
        // In interactive mode a prompt + Y/N/A/Q dialog would be shown.
        CmdOp::Verify => CmdResult::Success,
        // ? — InsertInvisible: not meaningful in batch mode.
        CmdOp::InsertInvisible => CmdResult::Failure(CmdFailure::NotImplemented),
        // UC — insert the command introducer character (\) as literal text.
        CmdOp::UserCommandIntroducer => {
            let tpar = TrailParam::from_str("\\");
            ctx.current_frame_mut().cmd_insert_text(LeadParam::None, &tpar)
        }
        // Q — Quit: signal the application to exit after this execution.
        CmdOp::Quit => {
            ctx.frame_set.quit_requested = true;
            CmdResult::Success
        }
        // UK — bind a key name to a procedure.
        //
        // Single-character key names are stored case-sensitively so that 'a'
        // and 'A' (Shift+A) can have independent bindings.  Multi-character
        // named keys (e.g. "UP-ARROW", "F1") are normalised to UPPERCASE so
        // that `UK|up-arrow|…|` matches the "UP-ARROW" name returned by
        // `key_event_to_name`.
        CmdOp::UserKey => {
            let raw = tpars[0].content.trim();
            let key_name = if raw.chars().count() == 1 {
                raw.to_string()
            } else {
                raw.to_uppercase()
            };
            if key_name.is_empty() {
                return CmdResult::Failure(CmdFailure::SyntaxError);
            }
            let procedure = tpars[1].content.trim();
            match compile(procedure) {
                Ok(code) => {
                    ctx.frame_set.user_key_bindings.insert(key_name, code);
                    CmdResult::Success
                }
                Err(_) => CmdResult::Failure(CmdFailure::SyntaxError),
            }
        }
        // UP — suspend to parent shell (interactive-only).
        CmdOp::UserParent => {
            ctx.frame_set.suspend_requested = true;
            CmdResult::Success
        }
        // US — spawn a subprocess shell (interactive-only).
        CmdOp::UserSubprocess => {
            ctx.frame_set.subprocess_requested = true;
            CmdResult::Success
        }
        // Span commands
        CmdOp::SpanDefine => ctx.cmd_span_define(lead, tpars),
        CmdOp::SpanCopy => ctx.cmd_span_copy(lead, &tpars[0]),
        CmdOp::SpanTransfer => ctx.cmd_span_transfer(lead, tpars),
        CmdOp::SpanJump => ctx.cmd_span_jump(lead, tpars),
        CmdOp::SpanAssign => ctx.cmd_span_assign(lead, tpars),
        CmdOp::SpanIndex => ctx.cmd_span_index(),
        CmdOp::SpanCompile => ctx.cmd_span_compile(lead, tpars),
        // Frame commands
        CmdOp::FrameEdit => ctx.cmd_frame_edit(lead, tpars),
        CmdOp::FrameKill => ctx.cmd_frame_kill(lead, tpars),
        CmdOp::FrameReturn => ctx.cmd_frame_return(lead),
        CmdOp::FrameParameters => ctx.cmd_frame_parameters(tpars),
        CmdOp::SetMarginLeft => ctx.cmd_set_margin_left(lead),
        CmdOp::SetMarginRight => ctx.cmd_set_margin_right(lead),
        // File commands (FX is handled in execute_instruction, not here)
        CmdOp::FileInput => ctx.cmd_file_input(lead, tpars),
        CmdOp::FileOutput => ctx.cmd_file_output(lead, tpars),
        CmdOp::FileEdit => ctx.cmd_file_edit(lead, tpars),
        CmdOp::FileRewind => ctx.cmd_file_rewind(lead),
        CmdOp::FileKill => ctx.cmd_file_kill(lead),
        CmdOp::FileSave => ctx.cmd_file_save(lead),
        CmdOp::FileTable => ctx.cmd_file_table(lead),
        CmdOp::FileRead => ctx.cmd_file_read(lead),
        CmdOp::FileWrite => ctx.cmd_file_write(lead),
        CmdOp::FileGlobalInput => ctx.cmd_fglobal_input(lead, tpars),
        CmdOp::FileGlobalOutput => ctx.cmd_fglobal_output(lead, tpars),
        CmdOp::FileGlobalRewind => ctx.cmd_fglobal_rewind(lead),
        CmdOp::FileGlobalKill => ctx.cmd_fglobal_kill(lead),
        // FIXME: remove this when everything is implemented
        _ => CmdResult::Failure(CmdFailure::NotImplemented),
    }
}

/// FX — File Execute
///
/// Reads a file, compiles it as Ludwig commands, loads it into the COMMAND frame,
/// then executes it.  Returns Failure if we are already in the COMMAND frame,
/// if the file cannot be read, or if it does not compile.
fn execute_file_execute(
    ctx: &mut ExecutionContext,
    lead: LeadParam,
    tpars: &[TrailParam],
) -> ExecOutcome {
    if !matches!(lead, LeadParam::None | LeadParam::Plus) {
        return ExecOutcome::Failure;
    }

    // Cannot execute FX from within the COMMAND frame.
    if ctx.frame_set.current_name() == COMMAND_FRAME_NAME {
        return ExecOutcome::Failure;
    }

    let path = tpars[0].content.trim().to_string();

    // Read file content.
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return ExecOutcome::Failure,
    };

    // Compile the content.
    let code = match compile(&content) {
        Ok(c) => c,
        Err(_) => return ExecOutcome::Failure,
    };

    // Load content into the COMMAND frame (for inspection; execution stays on
    // the current data frame, mirroring how EX/EN work).
    if let Some(cmd_frame) = ctx.frame_set.get_frame_mut(COMMAND_FRAME_NAME) {
        cmd_frame.clear_content();
        if !content.is_empty() {
            cmd_frame.insert_at(Position::zero(), &content);
        }
    }

    // Execute compiled code against the current (data) frame — no frame switch.
    ctx.recursion_depth += 1;
    let outcome = execute(ctx, &code);
    let outcome = unwrap_exit_level(outcome);
    ctx.recursion_depth -= 1;

    outcome
}

/// ^ — Execute String
///
/// Compiles the trailing-param text as Ludwig commands and executes it against
/// the current frame.  Acts as a compound-command boundary for exit-level
/// propagation (same as EX/EN).  Returns `Failure` if the text does not
/// compile or the recursion limit is reached.
fn execute_cmd_string(
    ctx: &mut ExecutionContext,
    lead: LeadParam,
    tpars: &[TrailParam],
) -> ExecOutcome {
    if !matches!(lead, LeadParam::None | LeadParam::Plus) {
        return ExecOutcome::Failure;
    }

    // Guard recursion depth.
    if ctx.recursion_depth >= MAX_RECURSION_DEPTH {
        return ExecOutcome::Failure;
    }

    let text = tpars[0].content.trim().to_string();
    if text.is_empty() {
        return ExecOutcome::Success;
    }

    let code = match compile(&text) {
        Ok(c) => c,
        Err(_) => return ExecOutcome::Failure,
    };

    ctx.recursion_depth += 1;
    let outcome = execute(ctx, &code);
    let outcome = unwrap_exit_level(outcome);
    ctx.recursion_depth -= 1;

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MarkId;
    use crate::Position;
    use crate::compiler::compile;
    use crate::frame_set::FrameSet;

    fn frame_set_from_str(s: &str) -> FrameSet {
        FrameSet::from_str(s)
    }

    fn exec(content: &str, commands: &str) -> (FrameSet, ExecOutcome) {
        let mut frame_set = frame_set_from_str(content);
        let code = compile(commands).unwrap();
        let outcome = frame_set.execute(&code);
        (frame_set, outcome)
    }

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

    #[test]
    fn test_execute_insert_command() {
        let (frame_set, outcome) = exec("hello\n", "i/world /");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "world hello\n");
    }

    #[test]
    fn test_execute_multiple_commands() {
        let (frame_set, outcome) = exec("hello world\n", "5ji/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "hello! world\n");
    }

    #[test]
    fn test_execute_commands_failure_stops() {
        let (frame_set, outcome) = exec("hello\n", "2ai/!/");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(frame_set.to_string(), "hello\n");
    }

    #[test]
    fn test_exit_handler_success_branch() {
        let (frame_set, outcome) = exec("line1\nline2\n", "A[I/ok/]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "line1\nokline2\n");
    }

    #[test]
    fn test_exit_handler_failure_branch() {
        let (frame_set, outcome) = exec("hello\n", "2A[:I/fail/]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "failhello\n");
    }

    #[test]
    fn test_exit_handler_no_matching_branch() {
        let (_, outcome) = exec("hello\n", "A[:I/fail/]");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_compound_times() {
        let (frame_set, outcome) = exec("hello world\n", "3(J)I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "hel!lo world\n");
    }

    #[test]
    fn test_compound_indefinite() {
        let (frame_set, outcome) = exec("line1\nline2\nline3\n", ">(A)[i/yes/:i/no/]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "line1\nline2\nnoline3\n");
    }

    #[test]
    fn test_compound_fails() {
        let (frame_set, outcome) = exec("line1\nline2\nline3\n", ">(A)");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(frame_set.to_string(), "line1\nline2\nline3\n");
    }

    #[test]
    fn test_compound_succeeds_with_empty_exit_handler_1() {
        let (frame_set, outcome) = exec("line1\nline2\nline3\n", ">(A)[:]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "line1\nline2\nline3\n");
    }

    #[test]
    fn test_compound_succeeds_with_empty_exit_handler_2() {
        let (frame_set, outcome) = exec("line1\nline2\nline3\n", ">(A)[]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "line1\nline2\nline3\n");
    }

    #[test]
    fn test_compound_once() {
        let (frame_set, outcome) = exec("hello\n", "(5J)I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "hello!\n");
    }

    #[test]
    fn test_compound_times_failure() {
        let (_, outcome) = exec("line1\nline2", "10(A)");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_exit_success_in_compound() {
        let (frame_set, outcome) = exec("line1\nline2", "(A XS 5J)I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(1, 1));
    }

    #[test]
    fn test_exit_failure_in_compound() {
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
        let (_, outcome) = exec("hello", "(((XA)))");
        assert_eq!(outcome, ExecOutcome::Abort);
    }

    #[test]
    fn test_xs_multi_level() {
        let (frame_set, outcome) = exec("line1\nline2\nline3", "((A 2XS 5J))I/!/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(1, 1));
    }

    #[test]
    fn test_xs_all_levels() {
        let (frame_set, outcome) = exec("line1\nline2\n", "(((A >XS 5J)))I/!/");
        assert_eq!(outcome, ExecOutcome::ExitSuccessAll);
        assert_eq!(frame_set.to_string(), "line1\nline2\n");
        assert_eq!(frame_set.current_frame().dot(), Position::new(1, 0));
    }

    #[test]
    fn test_failure_stops_sequence() {
        let (frame_set, outcome) = exec("hello\n", "99AI/!/");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(frame_set.to_string(), "hello\n");
    }

    #[test]
    fn test_compound_exit_handler() {
        let (frame_set, outcome) = exec("line1\nline2\n", ">(A)[:I/done/]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "line1\ndoneline2\n");
    }

    #[test]
    fn test_xs_propagates_through_handler() {
        let (_, outcome) = exec("hello", "XS[I/no/:I/no/]");
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
        let (_, outcome) = exec("hello\nworld\n", "M A EQM'1'");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_mark_set_and_eqm_equal() {
        let (_, outcome) = exec("hello\n", "M EQM'1'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_mark_set_numbered() {
        let (_, outcome) = exec("hello\n", "3M EQM'3'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_mark_unset() {
        let (_, outcome) = exec("hello\n", "M -M EQM'1'");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_eqm_inverted() {
        let (_, outcome) = exec("hello\nworld\n", "M A -EQM'1'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqm_greater_or_equal() {
        let (_, outcome) = exec("hello\nworld\n", "M A >EQM'1'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_eqm_less_or_equal() {
        let (_, outcome) = exec("hello\nworld\n", "M <EQM'1'");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_predicate_in_loop() {
        let (frame_set, outcome) = exec("hello\n", ">(-EOL J)");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 5));
    }

    #[test]
    fn test_replace_simple() {
        let (frame_set, outcome) = exec("hello world\n", "R/world/earth/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "hello earth\n");
    }

    #[test]
    fn test_replace_not_found() {
        let (_, outcome) = exec("hello world\n", "R/xyz/abc/");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_replace_case_insensitive() {
        let (frame_set, outcome) = exec("Hello World\n", "R/hello/goodbye/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "goodbye World\n");
    }

    #[test]
    fn test_replace_case_sensitive() {
        let (_, outcome) = exec("Hello World\n", r#"R"hello"goodbye""#);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_replace_case_sensitive_match() {
        let (frame_set, outcome) = exec("Hello World\n", r#"R"Hello"Goodbye""#);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "Goodbye World\n");
    }

    #[test]
    fn test_replace_multiple() {
        let (frame_set, outcome) = exec("aaa bbb aaa\n", "2R/aaa/ccc/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "ccc bbb ccc\n");
    }

    #[test]
    fn test_replace_all() {
        let (frame_set, outcome) = exec("aa bb aa bb aa\n", ">R/aa/cc/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "cc bb cc bb cc\n");
    }

    #[test]
    fn test_replace_with_empty() {
        let (frame_set, outcome) = exec("hello world\n", "R/world//");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "hello \n");
    }

    #[test]
    fn test_replace_with_longer() {
        let (frame_set, outcome) = exec("hi\n", "R/hi/hello world/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "hello world\n");
    }

    #[test]
    fn test_get_forward() {
        let (frame_set, outcome) = exec("hello world\n", "G/world/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 11));
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals),
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
        let (frame_set, outcome) = exec("Hello World\n", "G/hello/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 5));
    }

    #[test]
    fn test_get_case_sensitive() {
        let (_, outcome) = exec("Hello World\n", r#"G"hello""#);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_get_nth_occurrence() {
        let (frame_set, outcome) = exec("aa bb aa bb\n", "2G/aa/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 8));
    }

    #[test]
    fn test_get_backward() {
        let (frame_set, outcome) = exec("hello world hello\n", ">J-G/hello/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 12));
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals),
            Some(Position::new(0, 17))
        );
    }

    #[test]
    fn test_pattern_g_forward_charset() {
        let (frame_set, outcome) = exec("hello 42 world\n", "G`N`");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 7));
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals).unwrap(),
            Position::new(0, 6)
        );
    }

    #[test]
    fn test_pattern_g_forward_literal() {
        let (frame_set, outcome) = exec("hello world\n", "G`\"world\"`");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 11));
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals).unwrap(),
            Position::new(0, 6)
        );
    }

    #[test]
    fn test_pattern_g_forward_no_match() {
        let (_, outcome) = exec("hello\n", "G`N`");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_pattern_g_multiline() {
        let (frame_set, outcome) = exec("ab\ncd\n", "G`\"cd\"`");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(1, 2));
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals).unwrap(),
            Position::new(1, 0)
        );
    }

    #[test]
    fn test_pattern_g_backward() {
        let (frame_set, outcome) = exec("ab\ncd\n", "A -G`\"ab\"`");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 2));
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals).unwrap(),
            Position::new(0, 0)
        );
    }

    #[test]
    fn test_pattern_g_count() {
        let (frame_set, outcome) = exec("abcdef\n", "3G`A`");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 3));
    }

    #[test]
    fn test_pattern_g_with_quantifier() {
        let (frame_set, outcome) = exec("abc 123 def\n", "G`+N`");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals).unwrap(),
            Position::new(0, 4)
        );
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 7));
    }

    #[test]
    fn test_pattern_r_replaces_match() {
        let (frame_set, outcome) = exec("abc 123 def\n", "R`+N`NUM`");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "abc NUM def\n");
    }

    #[test]
    fn test_pattern_r_no_match_fails() {
        let (_, outcome) = exec("hello\n", "R`N`X`");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_pattern_r_replace_all() {
        let (frame_set, outcome) = exec("a1b2c3\n", ">R`N`X`");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "aXbXcX\n");
    }

    #[test]
    fn test_pattern_eqs_matches() {
        let (_, outcome) = exec("hello\n", "EQS`A`");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_pattern_eqs_no_match() {
        let (_, outcome) = exec("hello\n", "EQS`N`");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_pattern_eqs_inverted() {
        let (_, outcome) = exec("hello\n", "-EQS`N`");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_pattern_eqs_with_context() {
        let (_, outcome) = exec("hello world\n", "4J EQS`,A,\" \"`");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_pattern_syntax_error() {
        let (_, outcome) = exec("hello\n", "G`(A`");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    // ─── Phase 10 tests ───────────────────────────────────────────────────────

    #[test]
    fn test_zh_home_in_batch() {
        // In batch mode ZH moves to (0, 0).
        let (frame_set, outcome) = exec("hello\n", "5J ZH");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 0));
    }

    #[test]
    fn test_zh_sets_equals_mark() {
        let (frame_set, outcome) = exec("hello\n", "3J ZH");
        assert_eq!(outcome, ExecOutcome::Success);
        // Equals mark should be the position before ZH
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals),
            Some(Position::new(0, 3))
        );
    }

    #[test]
    fn test_zt_tab_forward() {
        // Default tab stops at every 8 columns: 0, 8, 16, …
        let (frame_set, outcome) = exec("hello\n", "ZT");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 8));
    }

    #[test]
    fn test_zt_tab_forward_from_middle() {
        let (frame_set, outcome) = exec("hello\n", "3J ZT");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 8));
    }

    #[test]
    fn test_zt_tab_multiple() {
        let (frame_set, outcome) = exec("hello\n", "2ZT");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 16));
    }

    #[test]
    fn test_zt_sets_equals_mark() {
        let (frame_set, outcome) = exec("hello\n", "3J ZT");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals),
            Some(Position::new(0, 3))
        );
    }

    #[test]
    fn test_zb_backtab_backward() {
        // From column 8, backtab goes to column 0.
        let (frame_set, outcome) = exec("hello\n", "8J ZB");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 0));
    }

    #[test]
    fn test_zb_backtab_from_middle() {
        // From column 5, backtab goes to column 0 (first tab stop).
        let (frame_set, outcome) = exec("hello\n", "5J ZB");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 0));
    }

    #[test]
    fn test_zb_fails_at_column_zero() {
        let (_, outcome) = exec("hello\n", "ZB");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_zb_sets_equals_mark() {
        let (frame_set, outcome) = exec("hello\n", "8J ZB");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(
            frame_set.current_frame().get_mark(MarkId::Equals),
            Some(Position::new(0, 8))
        );
    }

    #[test]
    fn test_zt_zb_roundtrip() {
        // Tab from a tab stop then backtab should return to that stop.
        // Default tab stops: 0, 8, 16, …  From col 8: ZT → 16, ZB → 8.
        let (frame_set, outcome) = exec("hello\n", "8J ZT ZB");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 8));
    }

    #[test]
    fn test_tab_hits_right_margin() {
        // EP sets right margin to 10; ZT from col 5 stops at col 10.
        let (frame_set, outcome) = exec("hello world\n", "EP'M=(1,11)' 5J ZT");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 8));
    }

    #[test]
    fn test_v_succeeds_in_batch() {
        let (_, outcome) = exec("hello\n", "V/prompt/");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_q_sets_quit_flag() {
        let mut frame_set = FrameSet::from_str("hello\n");
        let code = compile("Q").unwrap();
        frame_set.execute(&code);
        assert!(frame_set.quit_requested);
    }

    #[test]
    fn test_uc_inserts_backslash() {
        let (frame_set, outcome) = exec("hello\n", "UC");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "\\hello\n");
    }

    #[test]
    fn test_execute_string_runs_commands() {
        // ^ compiles and runs its tpar text.
        let (frame_set, outcome) = exec("hello\n", "^/5J I|!|/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "hello!\n");
    }

    #[test]
    fn test_execute_string_empty_succeeds() {
        let (_, outcome) = exec("hello\n", "^//");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_execute_string_bad_code_fails() {
        let (_, outcome) = exec("hello\n", "^/ZZZZZ/");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_execute_string_respects_exit_handler() {
        // ^ treats XS/XF exit levels like a compound boundary.
        let (frame_set, outcome) = exec("hello\n", "^/XS/ [I/yes/: I/no/]");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.to_string(), "yeshello\n");
    }

    #[test]
    fn test_ws_noop_in_batch() {
        // WS (window scroll) is a no-op in batch mode.
        let (frame_set, outcome) = exec("hello\n", "WS");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot(), Position::new(0, 0));
    }

    #[test]
    fn test_wh_noop_in_batch() {
        let (_, outcome) = exec("hello\n", "WH");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    #[test]
    fn test_wu_noop_in_batch() {
        let (_, outcome) = exec("hello\n", "WU");
        assert_eq!(outcome, ExecOutcome::Success);
    }

    // ─── Phase 11 tests ───────────────────────────────────────────────────────

    #[test]
    fn test_uk_binds_key_and_executes() {
        // UK binds a key name to a procedure; verify the binding is stored.
        let mut frame_set = FrameSet::from_str("hello\n");
        let code = compile("UK|UP-ARROW|ZU|").unwrap();
        frame_set.execute(&code);
        assert!(frame_set.user_key_bindings.contains_key("UP-ARROW"));
    }

    #[test]
    fn test_uk_binding_is_compiled() {
        // After UK, the stored code is the compiled procedure.
        let mut frame_set = FrameSet::from_str("hello\n");
        let code = compile("UK|a|3J|").unwrap();
        frame_set.execute(&code);
        // Execute the stored binding directly.
        let binding = frame_set.user_key_bindings["a"].clone();
        let outcome = frame_set.execute(&binding);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot().column, 3);
    }

    #[test]
    fn test_uk_rebinds_key() {
        // UK can replace an existing binding.
        let mut frame_set = FrameSet::from_str("hello\n");
        let code = compile("UK|a|3J| UK|a|5J|").unwrap();
        frame_set.execute(&code);
        let binding = frame_set.user_key_bindings["a"].clone();
        let outcome = frame_set.execute(&binding);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(frame_set.current_frame().dot().column, 5);
    }

    #[test]
    fn test_uk_empty_key_name_fails() {
        let (_, outcome) = exec("hello\n", "UK||proc|");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_uk_bad_procedure_fails() {
        // Invalid procedure text → compile error → Failure.
        let (_, outcome) = exec("hello\n", "UK|a|ZZZZZ|");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_up_sets_suspend_flag() {
        let mut frame_set = FrameSet::from_str("hello\n");
        let code = compile("UP").unwrap();
        frame_set.execute(&code);
        assert!(frame_set.suspend_requested);
    }

    #[test]
    fn test_us_sets_subprocess_flag() {
        let mut frame_set = FrameSet::from_str("hello\n");
        let code = compile("US").unwrap();
        frame_set.execute(&code);
        assert!(frame_set.subprocess_requested);
    }
}
