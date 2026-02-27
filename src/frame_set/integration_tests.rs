//! FrameSet integration tests.

#[cfg(test)]
mod tests {
    use crate::MarkId;
    use crate::Position;
    use crate::code::ExecOutcome;
    use crate::compiler::compile;
    use crate::frame_set::FrameSet;

    fn frame_set_new() -> FrameSet {
        FrameSet::empty()
    }

    fn frame_set_from_str(s: &str) -> FrameSet {
        FrameSet::from_str(s)
    }

    // Helper: compile and execute, return outcome
    fn exec(content: &str, commands: &str) -> (FrameSet, ExecOutcome) {
        let mut editor = frame_set_from_str(content);
        let code = compile(commands).unwrap();
        let outcome = editor.execute(&code);
        (editor, outcome)
    }

    // Core execution/control-flow tests moved to `interpreter::tests`.

    // Predicate/search/replace/get tests moved to `interpreter::tests`.

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
    fn test_word_delete_forward_one() {
        // Delete the first word (and trailing space) — same line, no newline re-insertion.
        let (editor, outcome) = exec("hello world\n", "YD");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "world\n");
        assert_eq!(editor.current_frame().dot(), Position::new(0, 0));
    }

    #[test]
    fn test_word_delete_from_middle_of_word() {
        // Dot in the middle of "world"; YD should delete from word-start to next word start.
        let (editor, outcome) = exec("hello world test\n", "8J YD");
        assert_eq!(outcome, ExecOutcome::Success);
        // "world " deleted, leaving "hello test"
        assert_eq!(editor.to_string(), "hello test\n");
    }

    #[test]
    fn test_word_delete_n_words() {
        let (editor, outcome) = exec("hello world test\n", "2YD");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "test\n");
    }

    #[test]
    fn test_word_delete_cross_line() {
        // Deleting the last word on a line ("world") advances into next line.
        // The newline must be re-inserted to preserve the line boundary.
        let (editor, outcome) = exec("hello world\nnext para\n", "7J YD");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello \nnext para\n");
    }

    #[test]
    fn test_word_delete_backward_one() {
        // Dot at "world"; -YD deletes the previous word ("hello ").
        let (editor, outcome) = exec("hello world\n", "6J -YD");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "world\n");
    }

    #[test]
    fn test_line_squeeze_basic() {
        // Multiple spaces within a line get collapsed to one.
        let (editor, outcome) = exec("hello   world\n", "YS");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello world\n");
        // Dot advances to start of next line (line 1, col 0).
        assert_eq!(editor.current_frame().dot(), Position::new(1, 0));
    }

    #[test]
    fn test_line_squeeze_leading_spaces_preserved() {
        // Leading spaces are not removed, only internal multi-space runs.
        let (editor, outcome) = exec("   hello   world\n", "YS");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "   hello world\n");
    }

    #[test]
    fn test_line_squeeze_already_single_spaces() {
        // A line with only single spaces between words is unchanged.
        let (editor, outcome) = exec("hello world\n", "YS");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello world\n");
    }

    #[test]
    fn test_line_squeeze_multiple_lines() {
        // 2YS processes two lines.
        let (editor, outcome) = exec("foo   bar\nbaz   qux\n", "2YS");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "foo bar\nbaz qux\n");
    }

    #[test]
    fn test_line_squeeze_empty_line_fails() {
        // YS on an empty line fails.
        let (editor, outcome) = exec("\nhello world\n", "YS");
        assert_eq!(outcome, ExecOutcome::Failure);
        // Frame is unchanged.
        assert_eq!(editor.to_string(), "\nhello world\n");
    }

    #[test]
    fn test_line_squeeze_pint_precheck_fails_on_empty() {
        // 2YS fails if either of the two lines is empty.
        let (_, outcome) = exec("foo   bar\n\nbaz   qux\n", "2YS");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_line_squeeze_sets_modified_mark() {
        let (editor, outcome) = exec("foo   bar\n", "YS");
        assert_eq!(outcome, ExecOutcome::Success);
        // MARK_MODIFIED set to (1, 0) — the dot position after advancing to next line.
        assert_eq!(
            editor.current_frame().get_mark(MarkId::Modified),
            Some(Position::new(1, 0))
        );
    }

    #[test]
    fn test_get_then_replace_at_match() {
        // Use G to find text, then use Equals mark for delete
        let (editor, outcome) = exec("hello world\n", "G/world/ =D");
        assert_eq!(outcome, ExecOutcome::Success);
        // G finds "world", dot=11, Equals=6. =D deletes from dot to Equals mark.
        assert_eq!(editor.to_string(), "hello \n");
    }

    // --- Span command tests ---

    #[test]
    fn test_span_define_dot_to_mark1() {
        // 1M SD/myspan/ — mark 1 at dot (0,0), advance 5, define span from (0,0) to (0,5)
        let (editor, outcome) = exec("hello world\n", "1M 5J SD/myspan/");
        assert_eq!(outcome, ExecOutcome::Success);
        // Span "MYSPAN" should be in the registry pointing to current frame
        assert!(editor.get_span("myspan").is_some());
        let span = editor.get_span("myspan").unwrap();
        assert_eq!(span.frame_name, "LUDWIG");
    }

    #[test]
    fn test_span_name_case_insensitive() {
        // Define with MixedCase, find with lowercase
        let (editor, outcome) = exec("hello\n", "1M 5J SD/MySpan/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert!(editor.get_span("myspan").is_some());
        assert!(editor.get_span("MYSPAN").is_some());
    }

    #[test]
    fn test_span_assign_literal() {
        // SA/x/hello/ creates span "X" in HEAP with content "hello"
        let (editor, outcome) = exec("", "SA/x/hello/");
        assert_eq!(outcome, ExecOutcome::Success);
        let span = editor.get_span("X").unwrap();
        assert_eq!(span.frame_name, "HEAP");
        // Read span text from HEAP
        let heap = editor.get_frame("HEAP").unwrap();
        let start = heap.get_mark(span.mark_start).unwrap();
        let end = heap.get_mark(span.mark_end).unwrap();
        let start_idx = heap.to_char_index(&start);
        let end_idx = heap.to_char_index(&end);
        let text: String = heap.slice(start_idx..end_idx);
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_span_assign_updates_existing() {
        // SA/x/hello/ then SA/x/bye/ — second replaces first
        let (editor, outcome) = exec("", "SA/x/hello/ SA/x/bye/");
        assert_eq!(outcome, ExecOutcome::Success);
        let span = editor.get_span("X").unwrap();
        let heap = editor.get_frame("HEAP").unwrap();
        let start = heap.get_mark(span.mark_start).unwrap();
        let end = heap.get_mark(span.mark_end).unwrap();
        let start_idx = heap.to_char_index(&start);
        let end_idx = heap.to_char_index(&end);
        let text: String = heap.slice(start_idx..end_idx);
        assert_eq!(text, "bye");
    }

    #[test]
    fn test_span_copy_inserts_text() {
        // SA/x/world/ creates span "x" = "world" in HEAP.
        // SC/x/ inserts "world" at current dot (col 0 of "hello\n").
        let (editor, outcome) = exec("hello\n", "SA/x/world/ SC/x/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "worldhello\n");
    }

    #[test]
    fn test_span_copy_n_times() {
        // SA/x/ab/ then 2SC/x/ inserts "ab" twice
        let (editor, outcome) = exec("\n", "SA/x/ab/ 2SC/x/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "abab\n");
    }

    #[test]
    fn test_span_transfer_empties_source() {
        // SA creates span "x" = "world" in HEAP. 5J moves to end of "hello".
        // ST transfers "world" from HEAP → current frame; HEAP's span marks collapse.
        // Since the source is a different frame, the current frame's dot is unaffected by the delete.
        let (editor, outcome) = exec("hello\n", "SA/x/world/ 5J ST/x/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "helloworld\n");
    }

    #[test]
    fn test_span_jump_to_end() {
        // Define span, jump to its end
        let (editor, outcome) = exec("hello world\n", "1M 5J SD/s/ 0J SJ/s/");
        assert_eq!(outcome, ExecOutcome::Success);
        // mark_end of span "s" is at col 5 (dot when SD was called)
        assert_eq!(editor.current_frame().dot(), Position::new(0, 5));
    }

    #[test]
    fn test_span_jump_to_start() {
        // Define span, jump to its start
        let (editor, outcome) = exec("hello world\n", "1M 5J SD/s/ -SJ/s/");
        assert_eq!(outcome, ExecOutcome::Success);
        // mark_start of span "s" is at col 0 (mark 1 position when SD was called)
        assert_eq!(editor.current_frame().dot(), Position::new(0, 0));
    }

    #[test]
    fn test_span_recompile() {
        // SA stores "2A" as span text; SR compiles it
        let (editor, outcome) = exec("line1\nline2\n", "SA/cmd/2A/ SR/cmd/");
        assert_eq!(outcome, ExecOutcome::Success);
        // span "CMD" should now have compiled code
        let span = editor.get_span("CMD").unwrap();
        assert!(span.get_code().is_some());
    }

    #[test]
    fn test_span_assign_span_ref() {
        // SA/x/hello/ creates span x; SA$y$x$ sets y to the same content
        let (editor, outcome) = exec("", "SA/x/hello/ SA$y$x$");
        assert_eq!(outcome, ExecOutcome::Success);
        let span_y = editor.get_span("Y").unwrap();
        let heap = editor.get_frame("HEAP").unwrap();
        let start = heap.get_mark(span_y.mark_start).unwrap();
        let end = heap.get_mark(span_y.mark_end).unwrap();
        let start_idx = heap.to_char_index(&start);
        let end_idx = heap.to_char_index(&end);
        let text: String = heap.slice(start_idx..end_idx);
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_span_bounds_survive_insert_before() {
        // Define span, insert text before span, verify bounds shift.
        // ,J (Nindef-J) jumps to column 0. Then I/abc/ inserts at (0,0).
        // mark_start was at (0,0), mark_end at (0,5); both are AT or AFTER the
        // insert point, so both shift right by 3.
        let (editor, outcome) = exec("hello\n", "1M 5J SD/s/ ,J I/abc/");
        assert_eq!(outcome, ExecOutcome::Success);
        let span = editor.get_span("S").unwrap();
        let frame = editor.get_frame("LUDWIG").unwrap();
        let start = frame.get_mark(span.mark_start).unwrap();
        let end = frame.get_mark(span.mark_end).unwrap();
        assert_eq!(start, Position::new(0, 3)); // was 0, shifted by 3
        assert_eq!(end, Position::new(0, 8)); // was 5, shifted by 3
    }

    // --- EX / EN: span execution ---

    #[test]
    fn test_ex_executes_span_text() {
        // SA creates a span holding "I/x/"; EX compiles and runs it.
        // Use '|' as SA delimiter so '/' inside the span text is not ambiguous.
        let (editor, outcome) = exec("", "SA|cmd|I/x/| EX/cmd/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "x");
    }

    #[test]
    fn test_ex_with_count() {
        // 3EX/cmd/ runs the span three times.
        let (editor, outcome) = exec("", "SA|cmd|I/a/| 3EX/cmd/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "aaa");
    }

    #[test]
    fn test_ex_always_recompiles() {
        // After SA updates the span, EX should use the new text, not any old cache.
        let (editor, outcome) = exec("", "SA|cmd|I/old/| EX/cmd/ SA|cmd|I/new/| EX/cmd/");
        assert_eq!(outcome, ExecOutcome::Success);
        // First EX inserts "old"; second EX (with updated span) inserts "new"
        // after "old", so the buffer becomes "oldnew".
        assert_eq!(editor.to_string(), "oldnew");
    }

    #[test]
    fn test_en_executes_and_caches() {
        // EN compiles on first call and caches.
        let (editor, outcome) = exec("", "SA|cmd|I/x/| EN/cmd/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "x");
        // The span should now have cached compiled code.
        let span = editor.get_span("CMD").unwrap();
        assert!(span.get_code().is_some());
    }

    #[test]
    fn test_en_uses_cache_not_updated_text() {
        // After EN caches "I/old/", updating the span text and calling EN again
        // should still use the old compiled code (no recompile).
        let (editor, outcome) = exec("", "SA|cmd|I/old/| EN/cmd/ SA|cmd|I/new/| EN/cmd/");
        assert_eq!(outcome, ExecOutcome::Success);
        // First EN inserts "old"; second EN re-uses the cached "I/old/" code,
        // inserting "old" again at the new dot position (after "old"),
        // so result is "oldold".
        assert_eq!(editor.to_string(), "oldold");
    }

    #[test]
    fn test_ex_pindef_runs_until_failure() {
        // >EX runs the span indefinitely; stops when the span exits with failure.
        let (editor, outcome) = exec("ab\ncd\nef\ngh\nij\nkl\n", "SA|step|A| >EX/step/");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(editor.current_frame().dot(), Position::new(5, 0));
    }

    #[test]
    fn test_ex_fails_on_missing_span() {
        let (_, outcome) = exec("", "EX/nosuchspan/");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_ex_xs_exits_span() {
        // XS inside EX exits the span (one compound boundary); execution
        // continues after EX.
        let (editor, outcome) = exec("", "SA|cmd|I/a/ XS I/b/| EX/cmd/ I/c/");
        assert_eq!(outcome, ExecOutcome::Success);
        // "a" inserted, then XS exits the span, then "c" inserted: "ac"
        assert_eq!(editor.to_string(), "ac");
    }

    #[test]
    fn test_ex_xf_propagates_failure() {
        // XF inside EX exits the span as failure; without a handler the outer
        // sequence stops.
        let (editor, outcome) = exec("", "SA/cmd/XF/ EX/cmd/ I/unreachable/");
        assert_eq!(outcome, ExecOutcome::Failure);
        assert_eq!(editor.to_string(), "");
    }

    #[test]
    fn test_ex_2xs_exits_two_levels() {
        // 2XS inside EX exits through the span AND one more compound level.
        let (editor, outcome) = exec("", "SA/cmd/2XS/ (EX/cmd/ I/inner/) I/outer/");
        assert_eq!(outcome, ExecOutcome::Success);
        // 2XS: level 1 consumed by EX boundary → ExitSuccess{1}; level 2
        // consumed by the outer compound → Success.  "outer" should execute.
        assert_eq!(editor.to_string(), "outer");
    }

    // Pattern-command execution tests moved to `interpreter::tests`.

    // ─── Phase 7: Word formatting commands (YF, YJ, YC, YL, YR) ──────────────

    /// Helper: compile+execute with explicit left/right margins.
    fn exec_with_margins(
        content: &str,
        commands: &str,
        left_margin: usize,
        right_margin: usize,
    ) -> (FrameSet, ExecOutcome) {
        let mut editor = frame_set_from_str(content);
        editor.current_frame_mut().left_margin = left_margin;
        editor.current_frame_mut().right_margin = right_margin;
        let code = compile(commands).unwrap();
        let outcome = editor.execute(&code);
        (editor, outcome)
    }

    // ── YL: left-align ──────────────────────────────────────────────────────

    #[test]
    fn test_yl_removes_leading_spaces() {
        // "   hello" → "hello" after YL with left_margin=0
        let (editor, outcome) = exec_with_margins("   hello\n\n", "YL", 0, 79);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello\n\n");
    }

    #[test]
    fn test_yl_already_at_margin_noop() {
        // "hello" (no leading spaces) → unchanged
        let (editor, outcome) = exec_with_margins("hello\n\n", "YL", 0, 79);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello\n\n");
    }

    #[test]
    fn test_yl_empty_line_fails() {
        let (_, outcome) = exec_with_margins("\nhello\n", "YL", 0, 79);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_yl_multiple_lines() {
        // 2YL left-aligns two lines
        let (editor, outcome) = exec_with_margins("  foo\n  bar\n\n", "2YL", 0, 79);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "foo\nbar\n\n");
    }

    #[test]
    fn test_yl_pindef_whole_paragraph() {
        // >YL left-aligns until blank line
        let (editor, outcome) = exec_with_margins("  foo\n  bar\n\n", ">YL", 0, 79);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "foo\nbar\n\n");
    }

    #[test]
    fn test_yl_advances_dot() {
        // After YL, dot should be on next line at left_margin.
        let (editor, outcome) = exec_with_margins("  hello\n\n", "YL", 0, 79);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().dot(), Position::new(1, 0));
    }

    // ── YR: right-align ─────────────────────────────────────────────────────

    #[test]
    fn test_yr_adds_leading_spaces() {
        // "hello" with right_margin=10 → "     hello" (5 leading spaces)
        let (editor, outcome) = exec_with_margins("hello\n\n", "YR", 0, 10);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "     hello\n\n");
    }

    #[test]
    fn test_yr_already_at_margin_noop() {
        // "hello" with right_margin=5 (line_len == right_margin) → no-op
        let (editor, outcome) = exec_with_margins("hello\n\n", "YR", 0, 5);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello\n\n");
    }

    #[test]
    fn test_yr_too_long_fails() {
        // "hello world" (11 chars) with right_margin=5 → fail
        let (_, outcome) = exec_with_margins("hello world\n\n", "YR", 0, 5);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_yr_multiple_lines() {
        // 2YR right-aligns two lines
        let (editor, outcome) = exec_with_margins("hi\nbye\n\n", "2YR", 0, 5);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "   hi\n  bye\n\n");
    }

    #[test]
    fn test_yr_advances_dot() {
        let (editor, outcome) = exec_with_margins("hi\n\n", "YR", 0, 5);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().dot(), Position::new(1, 0));
    }

    // ── YC: centre ──────────────────────────────────────────────────────────

    #[test]
    fn test_yc_centres_line() {
        // "hello" (5 chars) in margin [0, 15] → target leading = (15-5)/2 = 5
        // space_to_add = (15 + 0 - 5 + 0) / 2 - (0 - 0) = 10/2 = 5
        let (editor, outcome) = exec_with_margins("hello\n\n", "YC", 0, 15);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "     hello\n\n");
    }

    #[test]
    fn test_yc_removes_excess_spaces() {
        // "          hello" (10 leading + 5 text = 15 chars, right=15):
        // space_to_add = (15 + 0 - 15 + 10) / 2 - (10 - 0) = 10/2 - 10 = 5 - 10 = -5
        // Delete 5 spaces from left_margin=0: "     hello"
        let (editor, outcome) = exec_with_margins("          hello\n\n", "YC", 0, 15);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "     hello\n\n");
    }

    #[test]
    fn test_yc_too_long_fails() {
        // "hello world" (11 chars) with right_margin=5 → line > right_margin → fail
        let (_, outcome) = exec_with_margins("hello world\n\n", "YC", 0, 5);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_yc_empty_line_fails() {
        let (_, outcome) = exec_with_margins("\nhello\n", "YC", 0, 15);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_yc_advances_dot() {
        let (editor, outcome) = exec_with_margins("hello\n\n", "YC", 0, 15);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().dot(), Position::new(1, 0));
    }

    // ── YJ: justify ─────────────────────────────────────────────────────────

    #[test]
    fn test_yj_justifies_line() {
        // "hello world" (11 chars) with right_margin=15:
        // space_to_add = 15 - 11 = 4. One hole. Insert 4 spaces between words.
        let (editor, outcome) = exec_with_margins("hello world\nnext line\n", "YJ", 0, 15);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "hello     world\nnext line\n");
    }

    #[test]
    fn test_yj_skips_last_para_line() {
        // Next line is blank → last paragraph line → skip justification, just advance dot.
        let (editor, outcome) = exec_with_margins("hello world\n\n", "YJ", 0, 15);
        assert_eq!(outcome, ExecOutcome::Success);
        // Content unchanged.
        assert_eq!(editor.to_string(), "hello world\n\n");
        assert_eq!(editor.current_frame().dot(), Position::new(1, 0));
    }

    #[test]
    fn test_yj_too_long_fails() {
        // Line longer than right_margin → fail.
        let (_, outcome) = exec_with_margins("hello world extra\nnext\n", "YJ", 0, 10);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_yj_distributes_spaces_evenly() {
        // "a b c" (5 chars) → right_margin=8 → space_to_add=3, holes=2
        // fill_ratio = 1.5. Iteration 1: debit=1.5, insert 2 spaces. Iteration 2: debit=1.0, insert 1 space.
        // Result: "a   b  c"
        let (editor, outcome) = exec_with_margins("a b c\nnext\n", "YJ", 0, 8);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "a   b  c\nnext\n");
    }

    #[test]
    fn test_yj_advances_dot() {
        let (editor, outcome) = exec_with_margins("hello world\nnext\n", "YJ", 0, 15);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().dot(), Position::new(1, 0));
    }

    // ── YF: line fill ────────────────────────────────────────────────────────

    #[test]
    fn test_yf_pulls_word_from_next_line() {
        // "hello" fits 4 more chars (right=10). "world" from next line fits (5+1+5=11 > 10? No: space_avail = 10-5-1=4, "world" is 5 chars → doesn't fit).
        // Use shorter next word: "hi" (2 chars). space_avail = 10-5-1=4, "hi" fits (2<=4).
        let (editor, outcome) = exec_with_margins("hello\nhi there\n\n", "YF", 0, 10);
        assert_eq!(outcome, ExecOutcome::Success);
        // "hello" + " hi" = "hello hi" (8 chars). "there" stays on next line.
        assert_eq!(editor.to_string(), "hello hi\nthere\n\n");
    }

    #[test]
    fn test_yf_splits_long_line() {
        // "hello world" (11 chars) with right_margin=5 — too long, split at 'o'/'w' boundary.
        // right=5 → end_col=5, str[5]=' ' (space between "hello" and "world") → split there.
        // Actually: "hello world" with right=5, end_col=5, str[5]=' '.
        // Overflow_start scans forward: already at ' ', 5=' '→6='w'. overflow_start=6.
        // kept: "hello " (end up as "hello " on line 0), new line: "world"
        let (editor, outcome) = exec_with_margins("hello world\n\n", "YF", 0, 5);
        assert_eq!(outcome, ExecOutcome::Success);
        // Line 0: "hello " (the split keeps trailing space from the space run)
        // Line 1: "world" (overflow)
        assert!(editor.to_string().contains("hello"));
        assert!(editor.to_string().contains("world"));
        // Both words should be on separate lines.
        let content = editor.to_string();
        let lines: Vec<&str> = content.lines().collect();
        assert!(lines.len() >= 2);
    }

    #[test]
    fn test_yf_empty_line_stops() {
        // YF stops at an empty line (EOP).
        let (_, outcome) = exec_with_margins("\nhello\n", "YF", 0, 79);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_yf_pulls_until_full() {
        // Line has space for multiple words from next line.
        // "a" (1 char) with right=10. Next line: "b c d" (5 chars).
        // space_avail = 10-1-1=8. "b c d" is 5 chars → fits. Pull it.
        let (editor, outcome) = exec_with_margins("a\nb c d\n\n", "YF", 0, 10);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.to_string(), "a b c d\n\n");
    }

    #[test]
    fn test_yf_advances_dot() {
        // After YF, dot is at start of next line.
        let (editor, outcome) = exec_with_margins("hello\nworld\n\n", "YF", 0, 5);
        assert_eq!(outcome, ExecOutcome::Success);
        // Dot should be on line 1 (at left_margin=0).
        assert_eq!(editor.current_frame().dot().line, 1);
        assert_eq!(editor.current_frame().dot().column, 0);
    }

    #[test]
    fn test_yf_pindef_whole_paragraph() {
        // >YF fills entire paragraph.
        let (editor, outcome) = exec_with_margins("hello\nworld\n\n", ">YF", 0, 79);
        assert_eq!(outcome, ExecOutcome::Success);
        // "hello" + " " + "world" should be on one line.
        assert_eq!(editor.to_string(), "hello world\n\n");
    }

    // ── ED: frame navigation ─────────────────────────────────────────────────

    #[test]
    fn test_ed_switch_to_new_frame() {
        let (editor, outcome) = exec("hello\n", "ED/OTHER/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_name(), "OTHER");
        assert!(editor.contains_frame("OTHER"));
        // New frame is empty.
        assert_eq!(editor.to_string(), "");
    }

    #[test]
    fn test_ed_switch_to_existing_frame() {
        // Create a second frame, switch back to first, then re-enter second.
        let (mut editor, outcome) = exec("hello\n", "ED/B/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_name(), "B");
        // Now switch back to LUDWIG.
        let code = compile("ED/LUDWIG/").unwrap();
        let outcome2 = editor.execute(&code);
        assert_eq!(outcome2, ExecOutcome::Success);
        assert_eq!(editor.current_name(), "LUDWIG");
    }

    #[test]
    fn test_ed_same_frame_is_noop() {
        let (editor, outcome) = exec("hello\n", "ED/LUDWIG/");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_name(), "LUDWIG");
        assert_eq!(editor.to_string(), "hello\n");
    }

    #[test]
    fn test_ed_empty_tpar_switches_to_ludwig() {
        // Switching to empty name = LUDWIG.
        let (editor, outcome) = exec("hello\n", "ED//");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_name(), "LUDWIG");
    }

    #[test]
    fn test_ed_sets_return_frame_name() {
        let (editor, outcome) = exec("hello\n", "ED/OTHER/");
        assert_eq!(outcome, ExecOutcome::Success);
        let other = editor.get_frame("OTHER").unwrap();
        assert_eq!(other.return_frame_name.as_deref(), Some("LUDWIG"));
    }

    #[test]
    fn test_ed_new_frame_inherits_defaults() {
        let mut editor = frame_set_from_str("hello\n");
        // Set left margin to 5, right to 60 on defaults via EP.
        let code = compile("EP'$M=(6,60)'").unwrap();
        editor.execute(&code);
        // Now create a new frame — should inherit left=5, right=60.
        let code2 = compile("ED/NEW/").unwrap();
        editor.execute(&code2);
        let new_frame = editor.get_frame("NEW").unwrap();
        assert_eq!(new_frame.left_margin, 5);
        assert_eq!(new_frame.right_margin, 60);
    }

    // ── EK: frame kill ───────────────────────────────────────────────────────

    #[test]
    fn test_ek_kills_another_frame() {
        // Create frame B, go back to LUDWIG, kill B.
        let (mut editor, _) = exec("hello\n", "ED/B/");
        let code = compile("ED/LUDWIG/").unwrap();
        editor.execute(&code);
        let code2 = compile("EK/B/").unwrap();
        let outcome = editor.execute(&code2);
        assert_eq!(outcome, ExecOutcome::Success);
        assert!(!editor.contains_frame("B"));
    }

    #[test]
    fn test_ek_fails_on_current_frame() {
        let (mut editor, _) = exec("hello\n", "");
        let code = compile("EK/LUDWIG/").unwrap();
        let outcome = editor.execute(&code);
        assert_eq!(outcome, ExecOutcome::Failure);
        assert!(editor.contains_frame("LUDWIG"));
    }

    #[test]
    fn test_ek_fails_on_special_frame() {
        let (mut editor, _) = exec("hello\n", "");
        let code = compile("EK/COMMAND/").unwrap();
        let outcome = editor.execute(&code);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_ek_fails_if_frame_not_exist() {
        let (mut editor, _) = exec("hello\n", "");
        let code = compile("EK/NOSUCHFRAME/").unwrap();
        let outcome = editor.execute(&code);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    // ── ER: frame return ─────────────────────────────────────────────────────

    #[test]
    fn test_er_returns_to_previous_frame() {
        let (mut editor, _) = exec("hello\n", "ED/B/");
        assert_eq!(editor.current_name(), "B");
        let code = compile("ER").unwrap();
        let outcome = editor.execute(&code);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_name(), "LUDWIG");
    }

    #[test]
    fn test_er_fails_if_no_return_frame() {
        let (mut editor, _) = exec("hello\n", "");
        // LUDWIG has no return_frame_name set initially.
        let code = compile("ER").unwrap();
        let outcome = editor.execute(&code);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    #[test]
    fn test_er_multi_level() {
        // ED/B/, ED/C/, then 2ER should land back on LUDWIG.
        let (mut editor, _) = exec("hello\n", "ED/B/");
        let code = compile("ED/C/").unwrap();
        editor.execute(&code);
        let code2 = compile("2ER").unwrap();
        let outcome = editor.execute(&code2);
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_name(), "LUDWIG");
    }

    // ── EP: keyboard mode ────────────────────────────────────────────────────

    #[test]
    fn test_ep_keyboard_mode_insert() {
        use crate::frame::KeyboardMode;
        let (editor, outcome) = exec("hello\n", "EP'K=I'");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.keyboard_mode, KeyboardMode::Insert);
    }

    #[test]
    fn test_ep_keyboard_mode_overtype() {
        use crate::frame::KeyboardMode;
        let (mut editor, _) = exec("hello\n", "EP'K=I'");
        let code = compile("EP'K=O'").unwrap();
        editor.execute(&code);
        assert_eq!(editor.keyboard_mode, KeyboardMode::Overtype);
    }

    #[test]
    fn test_ep_keyboard_mode_command() {
        use crate::frame::KeyboardMode;
        let (editor, outcome) = exec("hello\n", "EP'K=C'");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.keyboard_mode, KeyboardMode::Command);
    }

    // ── EP: options ───────────────────────────────────────────────────────────

    #[test]
    fn test_ep_option_auto_indent() {
        let (editor, outcome) = exec("hello\n", "EP'O=I'");
        assert_eq!(outcome, ExecOutcome::Success);
        assert!(editor.current_frame().options.auto_indent);
    }

    #[test]
    fn test_ep_option_auto_wrap() {
        let (editor, outcome) = exec("hello\n", "EP'O=W'");
        assert_eq!(outcome, ExecOutcome::Success);
        assert!(editor.current_frame().options.auto_wrap);
    }

    #[test]
    fn test_ep_option_toggle_then_clear() {
        // Toggle on then explicitly clear.
        let (mut editor, _) = exec("hello\n", "EP'O=I'");
        let code = compile("EP'O=(-I)'").unwrap();
        editor.execute(&code);
        assert!(!editor.current_frame().options.auto_indent);
    }

    #[test]
    fn test_ep_option_set_initial_updates_defaults() {
        let (editor, outcome) = exec("hello\n", "EP'$O=I'");
        assert_eq!(outcome, ExecOutcome::Success);
        assert!(editor.current_frame().options.auto_indent);
        assert!(editor.defaults.options.auto_indent);
    }

    // ── EP: LR margins ───────────────────────────────────────────────────────

    #[test]
    fn test_ep_lr_margins() {
        let (editor, outcome) = exec("hello\n", "EP'M=(5,60)'");
        assert_eq!(outcome, ExecOutcome::Success);
        // left_margin is 0-based: user 5 → internal 4
        assert_eq!(editor.current_frame().left_margin, 4);
        assert_eq!(editor.current_frame().right_margin, 60);
    }

    #[test]
    fn test_ep_lr_margins_set_initial() {
        let (editor, outcome) = exec("hello\n", "EP'$M=(2,40)'");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().left_margin, 1);
        assert_eq!(editor.defaults.left_margin, 1);
        assert_eq!(editor.defaults.right_margin, 40);
    }

    // ── EP: TB margins ───────────────────────────────────────────────────────

    #[test]
    fn test_ep_tb_margins() {
        let (editor, outcome) = exec("hello\n", "EP'V=(2,3)'");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().margin_top, 2);
        assert_eq!(editor.current_frame().margin_bottom, 3);
    }

    // ── EP: tab stops ─────────────────────────────────────────────────────────

    #[test]
    fn test_ep_tab_default() {
        let (mut editor, _) = exec("hello\n", "EP'T=W(4)'"); // set uniform 4
        // Reset to default (every 8).
        let code = compile("EP'T=D'").unwrap();
        editor.execute(&code);
        assert!(editor.current_frame().tab_stops[8]);
        assert!(editor.current_frame().tab_stops[16]);
        assert!(!editor.current_frame().tab_stops[1]);
    }

    #[test]
    fn test_ep_tab_uniform() {
        let (editor, outcome) = exec("hello\n", "EP'T=W(4)'");
        assert_eq!(outcome, ExecOutcome::Success);
        // Stops at 0, 4, 8, 12, ...
        assert!(editor.current_frame().tab_stops[0]);
        assert!(editor.current_frame().tab_stops[4]);
        assert!(!editor.current_frame().tab_stops[1]);
        assert!(!editor.current_frame().tab_stops[3]);
    }

    #[test]
    fn test_ep_tab_explicit() {
        let (editor, outcome) = exec("hello\n", "EP'T=(5,10)'");
        assert_eq!(outcome, ExecOutcome::Success);
        // User gave 1-based 5 and 10 → 0-based 4 and 9.
        assert!(editor.current_frame().tab_stops[4]);
        assert!(editor.current_frame().tab_stops[9]);
        assert!(!editor.current_frame().tab_stops[0]);
        assert!(!editor.current_frame().tab_stops[5]);
    }

    #[test]
    fn test_ep_tab_set_at_dot() {
        // Dot is at column 0 after J; set a tab stop there.
        let (editor, outcome) = exec("hello\n", "EP'T=S'");
        assert_eq!(outcome, ExecOutcome::Success);
        // Column 0 should already be a stop (default), so just confirm success.
        assert!(editor.current_frame().tab_stops[0]);
    }

    #[test]
    fn test_ep_tab_clear_at_dot() {
        // Default has stop at col 0; clear it.
        let (editor, outcome) = exec("hello\n", "EP'T=C'");
        assert_eq!(outcome, ExecOutcome::Success);
        assert!(!editor.current_frame().tab_stops[0]);
    }

    // ── EP: insert ruler (T=I) ────────────────────────────────────────────────

    #[test]
    fn test_ep_insert_ruler() {
        // margins left=0, right=10; default tab stops at 0 and 8.
        let (mut editor, _) = exec("content\n", "EP'M=(1,10)'");
        let code = compile("EP'T=I'").unwrap();
        editor.execute(&code);
        let text = editor.to_string();
        // Ruler line should be the first line (inserted before dot line).
        let first_line = text.lines().next().unwrap();
        assert_eq!(first_line.len(), 10);
        // Position 0 = left margin = 'L' (overrides 'T').
        assert_eq!(&first_line[0..1], "L");
        // Position 9 = right_margin - 1 = 'R'.
        assert_eq!(&first_line[9..10], "R");
    }

    // ── EP: syntax error ──────────────────────────────────────────────────────

    #[test]
    fn test_ep_bad_syntax_fails() {
        let (_, outcome) = exec("hello\n", "EP'Q=X'");
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    // ── {}: margin bracket commands ───────────────────────────────────────────

    #[test]
    fn test_left_bracket_sets_left_margin() {
        // Jump to column 5, then {.
        let (editor, outcome) = exec("hello world\n", "5J{");
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().left_margin, 5);
    }

    #[test]
    fn test_minus_left_bracket_resets_left_margin() {
        // Set left margin to 5, then reset.
        let (mut editor, _) = exec("hello world\n", "5J{");
        let code = compile("-{").unwrap();
        editor.execute(&code);
        // Default left_margin is 0.
        assert_eq!(editor.current_frame().left_margin, 0);
    }

    #[test]
    fn test_right_bracket_sets_right_margin() {
        // Jump to column 39, then }. right_margin becomes 40 (col + 1).
        let (editor, outcome) = exec(
            "hello world this is a long line with more content!\n",
            "39J}",
        );
        assert_eq!(outcome, ExecOutcome::Success);
        assert_eq!(editor.current_frame().right_margin, 40);
    }

    #[test]
    fn test_minus_right_bracket_resets_right_margin() {
        // Change right margin then reset.
        let (mut editor, _) = exec(
            "hello world this is a long line with more content!\n",
            "39J}",
        );
        let code = compile("-}").unwrap();
        editor.execute(&code);
        // Default right_margin is 79.
        assert_eq!(editor.current_frame().right_margin, 79);
    }

    #[test]
    fn test_left_bracket_fails_if_at_or_past_right_margin() {
        // Dot at col 79 (= right_margin) → { should fail.
        let (mut editor, _) = exec("hello world\n", "");
        editor.current_frame_mut().left_margin = 0;
        editor.current_frame_mut().right_margin = 5;
        // Jump dot to col 5 (= right_margin) — should fail
        let code = compile("5J{").unwrap();
        let outcome = editor.execute(&code);
        assert_eq!(outcome, ExecOutcome::Failure);
    }

    // -----------------------------------------------------------------------
    // Phase 9: File I/O command tests
    // -----------------------------------------------------------------------

    #[cfg(test)]
    mod file_io_tests {
        use super::*;
        use std::fs;
        use std::io::Write;
        use tempfile::NamedTempFile;

        // Helper: build a temporary file with the given content and return it.
        // The temp file stays alive as long as the returned NamedTempFile is alive.
        fn write_temp(content: &str) -> NamedTempFile {
            let mut f = NamedTempFile::new().unwrap();
            f.write_all(content.as_bytes()).unwrap();
            f
        }

        // Helper: exec commands on a fresh editor (no initial content).
        // Uses `|` as delimiter in the command string to avoid conflicts with
        // path separators in temporary file paths.
        fn exec_empty(commands: &str) -> (FrameSet, ExecOutcome) {
            let mut editor = frame_set_new();
            let code = compile(commands).unwrap();
            let outcome = editor.execute(&code);
            (editor, outcome)
        }

        // ---- FI — File Input ----

        #[test]
        fn test_fi_loads_file_into_frame() {
            let tmp = write_temp("line1\nline2\n");
            let path = tmp.path().to_str().unwrap();
            // Use `|` as delimiter so that `/` in the path is not treated as the
            // trailing-parameter delimiter.
            let cmd = format!("FI|{path}|");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            assert_eq!(editor.to_string(), "line1\nline2\n");
            // Frame should have an input handle open.
            assert!(editor.current_frame().input_file.is_some());
        }

        #[test]
        fn test_fi_fails_on_missing_file() {
            // Path does not exist — FI should return Failure.
            let (_, outcome) = exec_empty("FI|/no/such/file/here.txt|");
            assert_eq!(outcome, ExecOutcome::Failure);
        }

        #[test]
        fn test_fi_close_minus() {
            let tmp = write_temp("hello\n");
            let path = tmp.path().to_str().unwrap();
            let cmd = format!("FI|{path}|-FI||");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            // After close the handle should be gone.
            assert!(editor.current_frame().input_file.is_none());
        }

        #[test]
        fn test_fi_fails_if_already_open() {
            let tmp = write_temp("hello\n");
            let path = tmp.path().to_str().unwrap();
            // Open twice — second FI should fail.
            let cmd = format!("FI|{path}|FI|{path}|");
            let (_, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Failure);
        }

        // ---- FO — File Output ----

        #[test]
        fn test_fo_creates_temp_file_and_finalizes() {
            let tmp = write_temp("");
            let path = tmp.path().to_str().unwrap();
            let cmd = format!("I/hello world/FO|{path}|-FO||");
            let (_, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            // `I/hello world/` inserts without a trailing newline, so the file
            // should contain exactly what the frame rope holds.
            let written = fs::read_to_string(path).unwrap();
            assert_eq!(written, "hello world");
        }

        #[test]
        fn test_fo_close_minus_writes_content() {
            let tmp = write_temp("");
            let path = tmp.path().to_str().unwrap();
            let mut editor = frame_set_from_str("alpha\nbeta\n");
            let code = compile(&format!("FO|{path}|-FO||")).unwrap();
            let outcome = editor.execute(&code);
            assert_eq!(outcome, ExecOutcome::Success);
            let written = fs::read_to_string(path).unwrap();
            assert_eq!(written, "alpha\nbeta\n");
        }

        // ---- FE — File Edit ----

        #[test]
        fn test_fe_opens_both_and_loads_content() {
            let tmp = write_temp("original line\n");
            let path = tmp.path().to_str().unwrap();
            let cmd = format!("FE|{path}|");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            assert_eq!(editor.to_string(), "original line\n");
            assert!(editor.current_frame().input_file.is_some());
            assert!(editor.current_frame().output_file.is_some());
        }

        #[test]
        fn test_fe_close_writes_and_finalizes() {
            let tmp = write_temp("old content\n");
            let path = tmp.path().to_str().unwrap();
            let cmd = format!("FE|{path}|I/new /-FE||");
            let (_, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            let written = fs::read_to_string(path).unwrap();
            assert_eq!(written, "new old content\n");
        }

        // ---- FK — File Kill ----

        #[test]
        fn test_fk_deletes_temp_without_creating_real_file() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let path = tmp_dir.path().join("output.txt");
            let path_str = path.to_str().unwrap();
            let cmd = format!("I/data/FO|{path_str}|FK");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            // Real file should NOT exist.
            assert!(!path.exists());
            // Handle should be gone.
            assert!(editor.current_frame().output_file.is_none());
        }

        // ---- FB — File Back (rewind) ----

        #[test]
        fn test_fb_rewinds_and_reloads() {
            let tmp = write_temp("line1\nline2\n");
            let path = tmp.path().to_str().unwrap();
            // Open file, modify frame, then rewind — frame should reset to file content.
            let cmd = format!("FI|{path}|I/EXTRA /FB");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            // After FB, frame is reloaded from the file (original content).
            assert_eq!(editor.to_string(), "line1\nline2\n");
        }

        // ---- FS — File Save ----

        #[test]
        fn test_fs_saves_if_modified_and_reopens() {
            let tmp = write_temp("original\n");
            let path = tmp.path().to_str().unwrap();
            // FE opens input+output; insert text → modified; FS saves.
            let cmd = format!("FE|{path}|I/prefix /FS");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            // The written file should have the updated content.
            let written = fs::read_to_string(path).unwrap();
            assert_eq!(written, "prefix original\n");
            // Handles should be reopened after FS.
            assert!(editor.current_frame().input_file.is_some());
            assert!(editor.current_frame().output_file.is_some());
        }

        #[test]
        fn test_fs_noop_if_not_modified() {
            let tmp = write_temp("content\n");
            let path = tmp.path().to_str().unwrap();
            // FE then FS without any edits → file unchanged.
            let cmd = format!("FE|{path}|FS");
            let (_, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            let written = fs::read_to_string(path).unwrap();
            assert_eq!(written, "content\n");
        }

        #[test]
        fn test_fs_fails_if_no_output_file() {
            let tmp = write_temp("content\n");
            let path = tmp.path().to_str().unwrap();
            // FI only (no output) → FS should fail.
            let cmd = format!("FI|{path}|FS");
            let (_, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Failure);
        }

        // ---- FX — File Execute ----

        #[test]
        fn test_fx_executes_command_file() {
            // Create a command file that inserts "hello".
            let mut cmd_file = NamedTempFile::new().unwrap();
            cmd_file.write_all(b"I/hello/").unwrap();
            let cmd_path = cmd_file.path().to_str().unwrap();

            let cmd = format!("FX|{cmd_path}|");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            // `I/hello/` inserts without a trailing newline.
            assert_eq!(editor.to_string(), "hello");
        }

        #[test]
        fn test_fx_fails_on_missing_file() {
            let (_, outcome) = exec_empty("FX|/no/such/file.lud|");
            assert_eq!(outcome, ExecOutcome::Failure);
        }

        // ---- FGI / FGO / FGR / FGW / FGB / FGK ----

        #[test]
        fn test_fgr_reads_lines_from_global_input() {
            let tmp = write_temp("alpha\nbeta\ngamma\n");
            let path = tmp.path().to_str().unwrap();
            // Open global input, read 2 lines into (empty) frame.
            let cmd = format!("FGI|{path}|2FGR");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            // Two lines should have been inserted.
            assert_eq!(editor.to_string(), "alpha\nbeta\n");
        }

        #[test]
        fn test_fgr_at_eof_fails() {
            let tmp = write_temp("only_one\n");
            let path = tmp.path().to_str().unwrap();
            // Read 1 line (succeeds), then try to read again (EOF → failure).
            let cmd = format!("FGI|{path}|FGRFGR");
            let (_, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Failure);
        }

        #[test]
        fn test_fgw_writes_lines_to_global_output() {
            let mut editor = frame_set_from_str("line1\nline2\nline3\n");
            // Set up a global output file.
            let tmp_dir = tempfile::tempdir().unwrap();
            let out_path = tmp_dir.path().join("out.txt");
            let out_str = out_path.to_str().unwrap();

            let code = compile(&format!("FGO|{out_str}|2FGWFGW-FGO||")).unwrap();
            let outcome = editor.execute(&code);
            assert_eq!(outcome, ExecOutcome::Success);

            let written = fs::read_to_string(&out_path).unwrap();
            assert_eq!(written, "line1\nline2\nline3\n");
        }

        #[test]
        fn test_fgb_rewinds_global_input() {
            let tmp = write_temp("lineA\nlineB\n");
            let path = tmp.path().to_str().unwrap();
            // Read all, rewind, read again → same content inserted twice.
            let cmd = format!("FGI|{path}|2FGRFGB2FGR");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            // Four lines total: lineA, lineB, lineA, lineB.
            assert_eq!(editor.to_string(), "lineA\nlineB\nlineA\nlineB\n");
        }

        #[test]
        fn test_fgk_deletes_global_output_temp() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let out_path = tmp_dir.path().join("killed.txt");
            let out_str = out_path.to_str().unwrap();

            let (editor, outcome) = exec_empty(&format!("FGO|{out_str}|FGK"));
            assert_eq!(outcome, ExecOutcome::Success);
            // Real file should NOT exist.
            assert!(!out_path.exists());
            // Handle should be gone.
            assert!(editor.global_output.is_none());
        }

        #[test]
        fn test_fgi_close_minus() {
            let tmp = write_temp("data\n");
            let path = tmp.path().to_str().unwrap();
            let cmd = format!("FGI|{path}|-FGI||");
            let (editor, outcome) = exec_empty(&cmd);
            assert_eq!(outcome, ExecOutcome::Success);
            assert!(editor.global_input.is_none());
        }

        #[test]
        fn test_fgo_close_minus_creates_file() {
            let tmp_dir = tempfile::tempdir().unwrap();
            let out_path = tmp_dir.path().join("result.txt");
            let out_str = out_path.to_str().unwrap();

            let mut editor = frame_set_from_str("written content\n");
            let code = compile(&format!("FGO|{out_str}|FGW-FGO||")).unwrap();
            editor.execute(&code);

            let written = fs::read_to_string(&out_path).unwrap();
            assert_eq!(written, "written content\n");
        }
    }
}
