//! Command execution engine for Ludwig compiled code.
//!
//! This module interprets compiled Ludwig commands (from [`CompiledCode`]) and executes
//! them against an [`ExecutionContext`]. It handles control flow including compound commands
//! with repetition, exit handlers, and exit level unwinding (XS/XF/XA).

use crate::code::{
    CmdOp, CompiledCode, ExecOutcome, ExitHandler, ExitLevels, Instruction, RepeatCount,
};
use crate::compiler::compile;
use crate::exec_context::{ExecutionContext, MAX_RECURSION_DEPTH, parse_span_name};
use crate::frame::{
    CaseMode, EditCommands, LineFormatCommands, MotionCommands, PredicateCommands, SearchCommands,
    WordCommands,
};
use crate::frame_set::COMMAND_FRAME_NAME;
use crate::marks::MarkId;
use crate::position::Position;
use crate::{
    cmd_result::CmdFailure, cmd_result::CmdResult, lead_param::LeadParam, trail_param::TrailParam,
};

#[cfg(test)]
mod tests;

/// Execute compiled code against an execution context. Top-level entry point.
///
/// Executes each instruction sequentially until completion or until
/// a failure/exit occurs.
pub(crate) fn execute(ctx: &mut ExecutionContext, code: &CompiledCode) -> ExecOutcome {
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

/// Dispatch a CmdOp to the appropriate category handler.
fn dispatch_cmd(
    ctx: &mut ExecutionContext,
    op: CmdOp,
    lead: LeadParam,
    tpars: &[TrailParam],
) -> CmdResult {
    match op {
        CmdOp::Advance
        | CmdOp::Jump
        | CmdOp::DeleteChar
        | CmdOp::InsertText
        | CmdOp::OvertypeText
        | CmdOp::InsertChar
        | CmdOp::InsertLine
        | CmdOp::SplitLine
        | CmdOp::DeleteLine
        | CmdOp::CaseUp
        | CmdOp::CaseLow
        | CmdOp::CaseEdit
        | CmdOp::Next
        | CmdOp::Bridge
        | CmdOp::Left
        | CmdOp::Right
        | CmdOp::Up
        | CmdOp::Down
        | CmdOp::Return
        | CmdOp::Rubout
        | CmdOp::EqualEol
        | CmdOp::EqualEop
        | CmdOp::EqualEof
        | CmdOp::EqualColumn
        | CmdOp::EqualMark
        | CmdOp::EqualString
        | CmdOp::Mark
        | CmdOp::Replace
        | CmdOp::SwapLine
        | CmdOp::Get
        | CmdOp::WordAdvance
        | CmdOp::WordDelete
        | CmdOp::LineSquash
        | CmdOp::LineFill
        | CmdOp::LineJustify
        | CmdOp::LineCentre
        | CmdOp::LineLeft
        | CmdOp::LineRight
        | CmdOp::DittoUp
        | CmdOp::DittoDown
        | CmdOp::Tab
        | CmdOp::Backtab => dispatch_frame_cmd(ctx, op, lead, tpars),

        CmdOp::Home
        | CmdOp::WindowForward
        | CmdOp::WindowBackward
        | CmdOp::WindowLeft
        | CmdOp::WindowRight
        | CmdOp::WindowTop
        | CmdOp::WindowEnd
        | CmdOp::WindowNew
        | CmdOp::WindowMiddle
        | CmdOp::WindowScroll
        | CmdOp::WindowSetHeight
        | CmdOp::WindowUpdate => dispatch_window_cmd(ctx, op, lead),

        CmdOp::SpanDefine
        | CmdOp::SpanCopy
        | CmdOp::SpanTransfer
        | CmdOp::SpanJump
        | CmdOp::SpanAssign
        | CmdOp::SpanIndex
        | CmdOp::SpanCompile => dispatch_span_cmd(ctx, op, lead, tpars),

        CmdOp::FrameEdit
        | CmdOp::FrameKill
        | CmdOp::FrameReturn
        | CmdOp::FrameParameters
        | CmdOp::SetMarginLeft
        | CmdOp::SetMarginRight => dispatch_frame_mgmt_cmd(ctx, op, lead, tpars),

        // FX is handled in execute_instruction before reaching dispatch_cmd.
        CmdOp::FileInput
        | CmdOp::FileOutput
        | CmdOp::FileEdit
        | CmdOp::FileRewind
        | CmdOp::FileKill
        | CmdOp::FileSave
        | CmdOp::FileTable
        | CmdOp::FileRead
        | CmdOp::FileWrite
        | CmdOp::FileGlobalInput
        | CmdOp::FileGlobalOutput
        | CmdOp::FileGlobalRewind
        | CmdOp::FileGlobalKill => dispatch_file_cmd(ctx, op, lead, tpars),

        CmdOp::Verify
        | CmdOp::InsertInvisible
        | CmdOp::UserCommandIntroducer
        | CmdOp::Quit
        | CmdOp::UserKey
        | CmdOp::UserParent
        | CmdOp::UserSubprocess => dispatch_special_cmd(ctx, op, lead, tpars),

        // FIXME: remove this when everything is implemented
        _ => CmdResult::Failure(CmdFailure::NotImplemented),
    }
}

/// Frame text commands: editing, motion, search, predicates, and word operations.
fn dispatch_frame_cmd(
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
        _ => unreachable!(),
    }
}

/// Window and viewport commands: ZH, WF, WB, WT, WE, WN, WM, WL, WR, WS, WH, WU.
fn dispatch_window_cmd(ctx: &mut ExecutionContext, op: CmdOp, lead: LeadParam) -> CmdResult {
    let dot = ctx.current_frame().dot();
    let line_count = ctx.current_frame().line_count();
    match op {
        // ZH: the screen backend resolves the home position
        // (top-left of viewport in interactive mode; (0,0) in batch).
        CmdOp::Home => {
            let new_dot = ctx
                .screen
                .handle_window_cmd(op, lead, dot, line_count)
                .unwrap_or(Position::new(0, 0));
            ctx.current_frame_mut().set_mark_at(MarkId::Equals, dot);
            ctx.current_frame_mut().set_dot(new_dot);
        }
        // All other window commands: dispatch to screen backend.
        // In batch mode the backend is a no-op; in interactive mode it updates
        // the viewport (and may return a new dot position for WF/WB).
        _ => {
            if let Some(new_dot) = ctx.screen.handle_window_cmd(op, lead, dot, line_count) {
                ctx.current_frame_mut().set_dot(new_dot);
            }
        }
    }
    CmdResult::Success
}

/// Span commands: SD, SC, ST, SJ, SA, SI, SR.
fn dispatch_span_cmd(
    ctx: &mut ExecutionContext,
    op: CmdOp,
    lead: LeadParam,
    tpars: &[TrailParam],
) -> CmdResult {
    match op {
        CmdOp::SpanDefine => ctx.cmd_span_define(lead, tpars),
        CmdOp::SpanCopy => ctx.cmd_span_copy(lead, &tpars[0]),
        CmdOp::SpanTransfer => ctx.cmd_span_transfer(lead, tpars),
        CmdOp::SpanJump => ctx.cmd_span_jump(lead, tpars),
        CmdOp::SpanAssign => ctx.cmd_span_assign(lead, tpars),
        CmdOp::SpanIndex => ctx.cmd_span_index(),
        CmdOp::SpanCompile => ctx.cmd_span_compile(lead, tpars),
        _ => unreachable!(),
    }
}

/// Frame management commands: ED, EK, ER, EP, {, }.
fn dispatch_frame_mgmt_cmd(
    ctx: &mut ExecutionContext,
    op: CmdOp,
    lead: LeadParam,
    tpars: &[TrailParam],
) -> CmdResult {
    match op {
        CmdOp::FrameEdit => ctx.cmd_frame_edit(lead, tpars),
        CmdOp::FrameKill => ctx.cmd_frame_kill(lead, tpars),
        CmdOp::FrameReturn => ctx.cmd_frame_return(lead),
        CmdOp::FrameParameters => ctx.cmd_frame_parameters(tpars),
        CmdOp::SetMarginLeft => ctx.cmd_set_margin_left(lead),
        CmdOp::SetMarginRight => ctx.cmd_set_margin_right(lead),
        _ => unreachable!(),
    }
}

/// File I/O commands: FI, FO, FE, FK, FB, FS, FT, FGI, FGO, FGR, FGW, FGB, FGK.
fn dispatch_file_cmd(
    ctx: &mut ExecutionContext,
    op: CmdOp,
    lead: LeadParam,
    tpars: &[TrailParam],
) -> CmdResult {
    match op {
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
        _ => unreachable!(),
    }
}

/// Application-level commands: V, ?, UC, Q, UK, UP, US.
fn dispatch_special_cmd(
    ctx: &mut ExecutionContext,
    op: CmdOp,
    lead: LeadParam,
    tpars: &[TrailParam],
) -> CmdResult {
    let _ = lead;
    match op {
        // V — Verify: always succeed in batch mode.
        CmdOp::Verify => CmdResult::Success,
        // ? — InsertInvisible: not meaningful in batch mode.
        CmdOp::InsertInvisible => CmdResult::Failure(CmdFailure::NotImplemented),
        // UC — insert the command introducer character (\) as literal text.
        CmdOp::UserCommandIntroducer => {
            let tpar = TrailParam::from_str("\\");
            ctx.current_frame_mut()
                .cmd_insert_text(LeadParam::None, &tpar)
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
        _ => unreachable!(),
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
