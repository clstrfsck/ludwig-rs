//! Command execution engine for Ludwig compiled code.
//!
//! This module interprets compiled Ludwig commands (from [`CompiledCode`]) and executes
//! them against an [`ExecutionContext`]. It handles control flow including compound commands
//! with repetition, exit handlers, and exit level unwinding (XS/XF/XA).

use ropey::Rope;

use crate::code::*;
use crate::compiler::compile;
use crate::exec_context::ExecutionContext;
use crate::frame::{CaseMode, EditCommands, MotionCommands, PredicateCommands, SearchCommands, WordCommands};
use crate::position::{Position, line_length_excluding_newline};
use crate::span::Span;
use crate::{CmdFailure, CmdResult, LeadParam, MarkId, TrailParam};

/// Execute compiled code against an execution context. Top-level entry point.
///
/// Executes each instruction sequentially until completion or until
/// a failure/exit occurs.
pub fn execute(ctx: &mut ExecutionContext, code: &CompiledCode) -> ExecOutcome {
    for instr in &code.instructions {
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
            let result = dispatch_cmd(ctx, *op, *lead, tpars);
            let outcome = if result.is_success() {
                ExecOutcome::Success
            } else {
                ExecOutcome::Failure
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
fn execute_compound(ctx: &mut ExecutionContext, repeat: RepeatCount, body: &CompiledCode) -> ExecOutcome {
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

/// Dispatch a CmdOp to the appropriate handler.
fn dispatch_cmd(ctx: &mut ExecutionContext, op: CmdOp, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
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
        CmdOp::CaseUp => ctx.current_frame_mut().cmd_case_change(lead, CaseMode::Upper),
        CmdOp::CaseLow => ctx.current_frame_mut().cmd_case_change(lead, CaseMode::Lower),
        CmdOp::CaseEdit => ctx.current_frame_mut().cmd_case_change(lead, CaseMode::Edit),
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
        CmdOp::Replace => ctx.current_frame_mut().cmd_replace(lead, &tpars[0], &tpars[1]),
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
        CmdOp::SpanDefine => dispatch_span_define(ctx, lead, tpars),
        CmdOp::SpanCopy => dispatch_span_copy(ctx, lead, tpars),
        CmdOp::SpanTransfer => dispatch_span_transfer(ctx, lead, tpars),
        CmdOp::SpanJump => dispatch_span_jump(ctx, lead, tpars),
        CmdOp::SpanAssign => dispatch_span_assign(ctx, lead, tpars),
        CmdOp::SpanIndex => dispatch_span_index(ctx),
        CmdOp::SpanCompile => dispatch_span_compile(ctx, lead, tpars),
        // FIXME: remove this when everything is implemented
        _ => CmdResult::Failure(CmdFailure::NotImplemented),
    }
}

// ---------------------------------------------------------------------------
// Span command implementations
// ---------------------------------------------------------------------------

const MAX_SPAN_NAME_LEN: usize = 31;

/// Validate and normalise a span name from a trailing param.
fn parse_span_name(tpar: &TrailParam) -> Option<String> {
    let name = tpar.str.trim();
    if name.is_empty() || name.len() > MAX_SPAN_NAME_LEN {
        return None;
    }
    Some(name.to_uppercase())
}

/// Extract the text of a span from its owning frame.
/// Returns `None` if the span's marks are not set in its frame.
fn read_span_text(ctx: &ExecutionContext, span: &Span) -> Option<String> {
    let frame = ctx.frame_set.get_frame(&span.frame_name)?;
    let start = frame.get_mark(span.mark_start)?;
    let end = frame.get_mark(span.mark_end)?;
    let start_idx = start.to_char_index(frame.rope());
    let end_idx = end.to_char_index(frame.rope());
    if start_idx >= end_idx {
        Some(String::new())
    } else {
        Some(frame.rope().slice(start_idx..end_idx).to_string())
    }
}

/// Compute the position that is `text.len()` chars after `from` (no virtual space).
fn position_after_text(from: Position, text: &str) -> Position {
    if text.is_empty() {
        return from;
    }
    let r = Rope::from_str(text);
    let lines_added = r.len_lines() - 1;
    let last_line_col = line_length_excluding_newline(&r, r.len_lines() - 1);
    if lines_added == 0 {
        Position::new(from.line, from.column + last_line_col)
    } else {
        Position::new(from.line + lines_added, last_line_col)
    }
}

/// SD — Span Define
///
/// `[lead]SD/name/`
/// Defines a span bounded by dot and another mark.
/// Lead: None/Plus → mark 1; Pint(n) → mark n; Marker(Equals) → Equals mark.
fn dispatch_span_define(ctx: &mut ExecutionContext, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
    let span_name = match parse_span_name(&tpars[0]) {
        Some(n) => n,
        None => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    // Resolve the second boundary mark ID.
    let second_mark = match lead {
        LeadParam::None | LeadParam::Plus => MarkId::Numbered(1),
        LeadParam::Pint(n) => {
            if n < 1 || n > 9 {
                return CmdResult::Failure(CmdFailure::SyntaxError);
            }
            MarkId::Numbered(n as u8)
        }
        LeadParam::Marker(id) => id,
        _ => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    // Get both positions from the current frame.
    let dot_pos = ctx.current_frame().dot();
    let other_pos = match ctx.current_frame().get_mark(second_mark) {
        Some(p) => p,
        None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
    };

    let (pos_start, pos_end) = if dot_pos <= other_pos {
        (dot_pos, other_pos)
    } else {
        (other_pos, dot_pos)
    };

    let current_frame_name = ctx.frame_set.current_name().to_string();

    // If a span with this name already exists, release its old bounds.
    if let Some(old_span) = ctx.frame_set.spans.remove(&span_name) {
        if let Some(old_frame) = ctx.frame_set.get_frame_mut(&old_span.frame_name) {
            old_frame.unset_mark(old_span.mark_start);
            old_frame.unset_mark(old_span.mark_end);
        }
    }

    // Allocate new bounds in the current frame.
    let (id_start, id_end) = ctx.frame_set.current_frame_mut().alloc_span_bounds();
    ctx.frame_set.current_frame_mut().set_mark_at(id_start, pos_start);
    ctx.frame_set.current_frame_mut().set_mark_at(id_end, pos_end);

    ctx.frame_set.spans.insert(&span_name, Span {
        frame_name: current_frame_name,
        mark_start: id_start,
        mark_end: id_end,
        code: None,
    });

    CmdResult::Success
}

/// SC — Span Copy
///
/// `[N]SC/name/`
/// Copies the span's text into the current frame at dot, N times (default 1).
fn dispatch_span_copy(ctx: &mut ExecutionContext, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
    let span_name = match parse_span_name(&tpars[0]) {
        Some(n) => n,
        None => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    let count = match lead {
        LeadParam::None | LeadParam::Plus => 1usize,
        LeadParam::Pint(n) => n,
        _ => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    // Extract span text (ends the borrow on ctx.frame_set before the insert).
    let text = {
        let span = match ctx.frame_set.spans.get(&span_name) {
            Some(s) => s,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };
        match read_span_text(ctx, span) {
            Some(t) => t,
            None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
        }
    };

    for _ in 0..count {
        let dot = ctx.current_frame().dot();
        ctx.current_frame_mut().insert_at(dot, &text);
    }

    CmdResult::Success
}

/// ST — Span Transfer
///
/// `ST/name/`
/// Moves the span's text from its source frame into the current frame at dot.
/// The span's source region is deleted; the span marks collapse to the same point.
fn dispatch_span_transfer(ctx: &mut ExecutionContext, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
    if !matches!(lead, LeadParam::None | LeadParam::Plus) {
        return CmdResult::Failure(CmdFailure::SyntaxError);
    }

    let span_name = match parse_span_name(&tpars[0]) {
        Some(n) => n,
        None => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    // Collect all needed info before any mutation.
    let (text, src_frame_name, src_mark_start, src_mark_end, src_from, src_to) = {
        let span = match ctx.frame_set.spans.get(&span_name) {
            Some(s) => s,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };
        let frame_name = span.frame_name.clone();
        let mark_start = span.mark_start;
        let mark_end = span.mark_end;
        let frame = match ctx.frame_set.get_frame(&frame_name) {
            Some(f) => f,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };
        let from = match frame.get_mark(mark_start) {
            Some(p) => p,
            None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
        };
        let to = match frame.get_mark(mark_end) {
            Some(p) => p,
            None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
        };
        let txt = {
            let start_idx = from.to_char_index(frame.rope());
            let end_idx = to.to_char_index(frame.rope());
            if start_idx < end_idx {
                frame.rope().slice(start_idx..end_idx).to_string()
            } else {
                String::new()
            }
        };
        (txt, frame_name, mark_start, mark_end, from, to)
    };

    // Delete from source frame.
    {
        let src_frame = match ctx.frame_set.get_frame_mut(&src_frame_name) {
            Some(f) => f,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };
        src_frame.delete(src_from, src_to);
        // After delete, both src_mark_start and src_mark_end are now at src_from.
        // Reset them explicitly to be sure.
        src_frame.set_mark_at(src_mark_start, src_from);
        src_frame.set_mark_at(src_mark_end, src_from);
    }

    // Insert at current frame dot.
    let dot = ctx.current_frame().dot();
    ctx.current_frame_mut().insert_at(dot, &text);

    CmdResult::Success
}

/// SJ — Span Jump
///
/// `[±]SJ/name/`
/// Moves dot to the end (None/Plus) or start (Minus) of the span.
/// Fails if the span is in a different frame.
fn dispatch_span_jump(ctx: &mut ExecutionContext, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
    let span_name = match parse_span_name(&tpars[0]) {
        Some(n) => n,
        None => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    let goto_end = match lead {
        LeadParam::None | LeadParam::Plus => true,
        LeadParam::Minus => false,
        _ => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    let (target_pos, span_frame_name) = {
        let span = match ctx.frame_set.spans.get(&span_name) {
            Some(s) => s,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };
        let frame = match ctx.frame_set.get_frame(&span.frame_name) {
            Some(f) => f,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };
        let mark = if goto_end { span.mark_end } else { span.mark_start };
        let pos = match frame.get_mark(mark) {
            Some(p) => p,
            None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
        };
        (pos, span.frame_name.clone())
    };

    // SJ only works when the span is in the current frame.
    if span_frame_name != ctx.frame_set.current_name() {
        return CmdResult::Failure(CmdFailure::OutOfRange);
    }

    let old_dot = ctx.current_frame().dot();
    ctx.current_frame_mut().set_mark_at(MarkId::Equals, old_dot);
    ctx.current_frame_mut().set_dot(target_pos);

    CmdResult::Success
}

/// SA — Span Assign
///
/// `SA/name/value/`   — assign literal text to span
/// `SA$name$refspan$` — assign contents of another span to this span
///
/// If the span does not exist, it is created in the HEAP frame.
/// If it exists, its content is replaced in-place.
fn dispatch_span_assign(ctx: &mut ExecutionContext, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
    if !matches!(lead, LeadParam::None | LeadParam::Plus) {
        return CmdResult::Failure(CmdFailure::SyntaxError);
    }

    let span_name = match parse_span_name(&tpars[0]) {
        Some(n) => n,
        None => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    // Resolve value: span dereference if dlm == '$', otherwise literal.
    let value: String = if tpars[1].dlm == '$' {
        let ref_name = tpars[1].str.trim().to_uppercase();
        let ref_span = match ctx.frame_set.spans.get(&ref_name) {
            Some(s) => s,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };
        match read_span_text(ctx, ref_span) {
            Some(t) => t,
            None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
        }
    } else {
        tpars[1].str.clone()
    };

    // Check whether the target span already exists.
    if ctx.frame_set.spans.contains(&span_name) {
        // Update existing span in-place.
        let (frame_name, mark_start, mark_end, from, _to) = {
            let span = ctx.frame_set.spans.get(&span_name).unwrap();
            let fname = span.frame_name.clone();
            let ms = span.mark_start;
            let me = span.mark_end;
            let frame = ctx.frame_set.get_frame(&fname).unwrap();
            let from = match frame.get_mark(ms) {
                Some(p) => p,
                None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
            };
            let to = match frame.get_mark(me) {
                Some(p) => p,
                None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
            };
            (fname, ms, me, from, to)
        };

        {
            let frame = ctx.frame_set.get_frame_mut(&frame_name).unwrap();
            // Get fresh to after resolving above
            let to = frame.get_mark(mark_end).unwrap();
            frame.delete(from, to);
            frame.insert_at(from, &value);
            // After delete+insert both marks end up past the new text.
            // Reset mark_start back to 'from'.
            frame.set_mark_at(mark_start, from);
            // mark_end is already correct (it's at from + text_length).
        }
    } else {
        // Create new span in HEAP.
        let heap_name = ctx.frame_set.heap_name().to_string();
        let insert_pos;
        let id_start;
        let id_end;
        let mark_end_pos;

        {
            let hf = ctx.frame_set.heap_frame_mut();
            let line_count = hf.line_count();
            let last_line = if line_count == 0 { 0 } else { line_count.saturating_sub(1) };
            let last_col = hf.line_len(last_line);
            insert_pos = Position::new(last_line, last_col);
            let bounds = hf.alloc_span_bounds();
            id_start = bounds.0;
            id_end = bounds.1;
            // Insert span text followed by a newline separator.
            hf.insert_at(insert_pos, &format!("{}\n", value));
            // mark_end points just before the separator newline.
            mark_end_pos = position_after_text(insert_pos, &value);
            hf.set_mark_at(id_start, insert_pos);
            hf.set_mark_at(id_end, mark_end_pos);
        }

        ctx.frame_set.spans.insert(&span_name, Span {
            frame_name: heap_name,
            mark_start: id_start,
            mark_end: id_end,
            code: None,
        });
    }

    CmdResult::Success
}

/// SI — Span Index
///
/// Prints a listing of all spans and frames to stdout.
/// (Interactive display deferred to Phase 10.)
fn dispatch_span_index(ctx: &mut ExecutionContext) -> CmdResult {
    let names = ctx.frame_set.spans.sorted_names();
    for name in names {
        if let Some(span) = ctx.frame_set.spans.get(name) {
            let preview = match read_span_text(ctx, span) {
                Some(t) => {
                    let first_line = t.lines().next().unwrap_or("").to_string();
                    if first_line.len() > 31 || t.contains('\n') {
                        format!("{}…", &first_line[..first_line.len().min(31)])
                    } else {
                        first_line
                    }
                }
                None => String::from("<undefined>"),
            };
            println!("{:<32} {}", name, preview);
        }
    }
    CmdResult::Success
}

/// SR — Span Recompile
///
/// `SR/name/`
/// Compiles the span's text as a Ludwig command string and caches the result.
fn dispatch_span_compile(ctx: &mut ExecutionContext, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
    if !matches!(lead, LeadParam::None | LeadParam::Plus) {
        return CmdResult::Failure(CmdFailure::SyntaxError);
    }

    let span_name = match parse_span_name(&tpars[0]) {
        Some(n) => n,
        None => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    // Read the span's text.
    let text = {
        let span = match ctx.frame_set.spans.get(&span_name) {
            Some(s) => s,
            None => return CmdResult::Failure(CmdFailure::OutOfRange),
        };
        match read_span_text(ctx, span) {
            Some(t) => t,
            None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
        }
    };

    // Compile it.
    let compiled = match compile(&text) {
        Ok(c) => c,
        Err(_) => return CmdResult::Failure(CmdFailure::SyntaxError),
    };

    // Store the compiled code on the span.
    if let Some(span) = ctx.frame_set.spans.get_mut(&span_name) {
        span.code = Some(compiled);
    }

    CmdResult::Success
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
