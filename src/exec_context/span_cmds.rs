//! Span commands: SD, SC, ST, SJ, SA, SI, SR.

use crate::compiler::compile;
use crate::marks::NUMBERED_MARK_RANGE;
use crate::span::Span;
use crate::{
    cmd_result::CmdFailure, cmd_result::CmdResult, lead_param::LeadParam, marks::MarkId,
    position::Position, trail_param::TrailParam,
};

use super::ExecutionContext;

pub(crate) const MAX_SPAN_NAME_LEN: usize = 31;

/// Validate and normalise a span name from a trailing param.
pub(crate) fn parse_span_name(tpar: &TrailParam) -> Option<String> {
    let name = tpar.content.trim();
    if name.is_empty() || name.len() > MAX_SPAN_NAME_LEN {
        return None;
    }
    Some(name.to_uppercase())
}

impl ExecutionContext<'_> {
    /// SD — Span Define
    ///
    /// `[lead]SD/name/`
    /// Defines a span bounded by dot and another mark.
    /// Lead: None/Plus → mark 1; Pint(n) → mark n; Marker(Equals) → Equals mark.
    pub(crate) fn cmd_span_define(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        let Some(span_name) = parse_span_name(&tpars[0]) else {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        };
        if self.frame_set.contains_frame(&span_name) {
            return CmdResult::Failure(CmdFailure::FrameExists);
        }

        // Resolve the second boundary mark ID.
        let second_mark = match lead {
            LeadParam::None | LeadParam::Plus => MarkId::Numbered(1),
            LeadParam::Pint(n) => {
                let n = match u8::try_from(n) {
                    Ok(v) if NUMBERED_MARK_RANGE.contains(&v) => v,
                    _ => return CmdResult::Failure(CmdFailure::SyntaxError),
                };
                MarkId::Numbered(n)
            }
            LeadParam::Marker(id) => id,
            LeadParam::Minus => {
                if let Some(old_span) = self.frame_set.remove_span(&span_name)
                    && let Some(old_frame) = self.frame_set.get_frame_mut(&old_span.frame_name)
                {
                    old_frame.unset_mark(old_span.mark_start);
                    old_frame.unset_mark(old_span.mark_end);
                    return CmdResult::Success;
                }
                return CmdResult::Failure(CmdFailure::OutOfRange);
            }
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Get both positions from the current frame.
        let dot_pos = self.current_frame().dot();
        let Some(other_pos) = self.current_frame().get_mark(second_mark) else {
            return CmdResult::Failure(CmdFailure::MarkNotDefined);
        };

        let (pos_start, pos_end) = if dot_pos <= other_pos {
            (dot_pos, other_pos)
        } else {
            (other_pos, dot_pos)
        };

        let current_frame_name = self.frame_set.current_name().to_string();

        // If a span with this name already exists, release its old bounds.
        if let Some(old_span) = self.frame_set.remove_span(&span_name)
            && let Some(old_frame) = self.frame_set.get_frame_mut(&old_span.frame_name)
        {
            old_frame.unset_mark(old_span.mark_start);
            old_frame.unset_mark(old_span.mark_end);
        }

        // Allocate new bounds in the current frame.
        let (id_start, id_end) = self.frame_set.alloc_span_bounds();
        self.frame_set
            .current_frame_mut()
            .set_mark_at(id_start, pos_start);
        self.frame_set
            .current_frame_mut()
            .set_mark_at(id_end, pos_end);

        self.frame_set
            .insert_span(&span_name, Span::new(current_frame_name, id_start, id_end));

        CmdResult::Success
    }

    /// SC — Span Copy
    ///
    /// `nSC/name/`
    /// Copies the span's text into the current frame at dot, N times (default 1).
    pub(crate) fn cmd_span_copy(&mut self, lead: LeadParam, tpar: &TrailParam) -> CmdResult {
        let Some(span_or_frame_name) = parse_span_name(tpar) else {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        };
        let count = match lead {
            LeadParam::None | LeadParam::Plus => 1usize,
            LeadParam::Pint(n) => n,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        // Extract span text (ends the borrow on ctx.frame_set before the insert).
        if let Some(text) = self.read_span_or_frame_text(&span_or_frame_name) {
            for _ in 0..count {
                let dot = self.current_frame().dot();
                self.current_frame_mut().insert_at(dot, &text);
            }
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::SyntaxError)
        }
    }

    /// ST — Span Transfer
    ///
    /// `ST/name/`
    /// Moves the span's text from its source frame into the current frame at dot.
    /// The span's source region is deleted; the span marks collapse to the same point.
    pub(crate) fn cmd_span_transfer(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        let Some(span_name) = parse_span_name(&tpars[0]) else {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        };

        // Collect all needed info before any mutation.
        let (text, src_frame_name, src_mark_start, src_mark_end, src_from, src_to) = {
            let Some(span) = self.frame_set.get_span(&span_name) else {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            };
            let frame_name = span.frame_name.clone();
            let mark_start = span.mark_start;
            let mark_end = span.mark_end;
            let Some(frame) = self.frame_set.get_frame(&frame_name) else {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            };
            let Some(from) = frame.get_mark(mark_start) else {
                return CmdResult::Failure(CmdFailure::MarkNotDefined);
            };
            let Some(to) = frame.get_mark(mark_end) else {
                return CmdResult::Failure(CmdFailure::MarkNotDefined);
            };
            let txt = {
                let start_idx = frame.to_char_index(&from);
                let end_idx = frame.to_char_index(&to);
                if start_idx < end_idx {
                    frame.slice(start_idx..end_idx)
                } else {
                    String::new()
                }
            };
            (txt, frame_name, mark_start, mark_end, from, to)
        };

        // Delete from source frame.
        {
            let Some(src_frame) = self.frame_set.get_frame_mut(&src_frame_name) else {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            };
            src_frame.delete(src_from, src_to);
            // After delete, both src_mark_start and src_mark_end are now at src_from.
            // Reset them explicitly to be sure.
            src_frame.set_mark_at(src_mark_start, src_from);
            src_frame.set_mark_at(src_mark_end, src_from);
        }

        // Insert at current frame dot.
        let dot = self.current_frame().dot();
        self.current_frame_mut().insert_at(dot, &text);

        CmdResult::Success
    }

    /// SJ — Span Jump
    ///
    /// `[±]SJ/name/`
    /// Moves dot to the end (None/Plus) or start (Minus) of the span.
    /// Fails if the span is in a different frame.
    pub(crate) fn cmd_span_jump(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        let Some(span_name) = parse_span_name(&tpars[0]) else {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        };

        let goto_end = match lead {
            LeadParam::None | LeadParam::Plus => true,
            LeadParam::Minus => false,
            _ => return CmdResult::Failure(CmdFailure::SyntaxError),
        };

        let (target_pos, span_frame_name) = {
            let Some(span) = self.frame_set.get_span(&span_name) else {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            };
            let Some(frame) = self.frame_set.get_frame(&span.frame_name) else {
                return CmdResult::Failure(CmdFailure::OutOfRange);
            };
            let mark = if goto_end {
                span.mark_end
            } else {
                span.mark_start
            };
            let Some(pos) = frame.get_mark(mark) else {
                return CmdResult::Failure(CmdFailure::MarkNotDefined);
            };
            (pos, span.frame_name.clone())
        };

        // SJ only works when the span is in the current frame.
        if span_frame_name != self.frame_set.current_name() {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        }

        let old_dot = self.current_frame().dot();
        self.current_frame_mut()
            .set_mark_at(MarkId::Equals, old_dot);
        self.current_frame_mut().set_dot(target_pos);

        CmdResult::Success
    }

    /// SA — Span Assign
    ///
    /// `SA/name/value/`   — assign literal text to span
    /// `SA$name$refspan$` — assign contents of another span to this span
    ///
    /// If the span does not exist, it is created in the HEAP frame.
    /// If it exists, its content is replaced in-place.
    pub(crate) fn cmd_span_assign(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        let Some(span_name) = parse_span_name(&tpars[0]) else {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        };

        // Resolve value: span dereference if delim == '$', otherwise literal.
        let value = if tpars[1].delim == '$' {
            let ref_name = tpars[1].content.trim().to_uppercase();
            match self.read_span_or_frame_text(&ref_name) {
                Some(t) => t,
                None => return CmdResult::Failure(CmdFailure::MarkNotDefined),
            }
        } else {
            tpars[1].content.clone()
        };

        // Check whether the target span already exists.
        if self.frame_set.contains_span(&span_name) {
            // Update existing span in-place.
            let (frame_name, mark_start, mark_end, from, _to) = {
                let span = self.frame_set.get_span(&span_name).unwrap();
                let frame_name = span.frame_name.clone();
                let ms = span.mark_start;
                let me = span.mark_end;
                let frame = self.frame_set.get_frame(&frame_name).unwrap();
                let Some(from) = frame.get_mark(ms) else {
                    return CmdResult::Failure(CmdFailure::MarkNotDefined);
                };
                let Some(to) = frame.get_mark(me) else {
                    return CmdResult::Failure(CmdFailure::MarkNotDefined);
                };
                (frame_name, ms, me, from, to)
            };

            {
                let frame = self.frame_set.get_frame_mut(&frame_name).unwrap();
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
            let heap_name = self.frame_set.heap_name().to_string();
            let id_start;
            let id_end;

            {
                (id_start, id_end) = self.frame_set.alloc_span_bounds();
                let hf = self.frame_set.heap_frame_mut();
                let line_count = hf.line_count();
                let last_line = if line_count == 0 {
                    0
                } else {
                    line_count.saturating_sub(1)
                };
                let last_col = hf.line_length_including_newline(last_line);
                let insert_pos = Position::new(last_line, last_col);
                // Insert span text followed by a newline separator.
                hf.insert_at(insert_pos, &format!("{value}\n"));
                // mark_end points just before the separator newline.
                let mark_end_pos = insert_pos.after_text(&value);
                hf.set_mark_at(id_start, insert_pos);
                hf.set_mark_at(id_end, mark_end_pos);
            }

            self.frame_set
                .insert_span(&span_name, Span::new(heap_name, id_start, id_end));
        }

        CmdResult::Success
    }

    /// SI — Span Index
    ///
    /// Prints a listing of all spans and frames to stdout.
    pub(crate) fn cmd_span_index(&mut self) -> CmdResult {
        let names = self.frame_set.sorted_span_names();
        for name in names {
            let preview = match self.read_span_or_frame_text(name) {
                Some(t) => {
                    let first_line = t.lines().next().unwrap_or("").to_string();
                    if first_line.len() > 31 || t.contains('\n') {
                        format!("{}...", &first_line[..first_line.len().min(28)])
                    } else {
                        first_line
                    }
                }
                None => String::from("<undefined>"),
            };
            self.screen.output_line(&format!("{name:<32} {preview}"));
        }
        CmdResult::Success
    }

    /// SR — Span Recompile
    ///
    /// `SR/name/`
    /// Compiles the span's text as a Ludwig command string and caches the result.
    pub(crate) fn cmd_span_compile(&mut self, lead: LeadParam, tpars: &[TrailParam]) -> CmdResult {
        if !matches!(lead, LeadParam::None | LeadParam::Plus) {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        }

        let Some(span_name) = parse_span_name(&tpars[0]) else {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        };

        // Read the span's text.
        let Some(text) = self.read_span_or_frame_text(&span_name) else {
            return CmdResult::Failure(CmdFailure::OutOfRange);
        };

        // Compile it.
        let Ok(compiled) = compile(&text) else {
            return CmdResult::Failure(CmdFailure::SyntaxError);
        };

        // Store the compiled code on the span.
        if let Some(span) = self.frame_set.get_span_mut(&span_name) {
            span.set_code(compiled);
            CmdResult::Success
        } else if let Some(frame) = self.frame_set.get_frame_mut(&span_name) {
            frame.set_code(compiled);
            CmdResult::Success
        } else {
            CmdResult::Failure(CmdFailure::OutOfRange)
        }
    }

    /// Extracts the text of a span or frame by name.
    ///
    /// Returns `None` if neither a frame nor a span with that name exists,
    /// or if the span's boundary marks are not set in its frame.
    pub(crate) fn read_span_or_frame_text(&self, name: &str) -> Option<String> {
        if let Some(frame) = self.frame_set.get_frame(name) {
            Some(frame.text())
        } else if let Some(span) = self.frame_set.get_span(name) {
            let frame = self.frame_set.get_frame(&span.frame_name)?;
            let start = frame.get_mark(span.mark_start)?;
            let end = frame.get_mark(span.mark_end)?;
            let start_idx = frame.to_char_index(&start);
            let end_idx = frame.to_char_index(&end);
            if start_idx >= end_idx {
                Some(String::new())
            } else {
                Some(frame.slice(start_idx..end_idx))
            }
        } else {
            None
        }
    }
}
