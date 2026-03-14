use super::*;
use crate::compiler::compile;
use crate::frame_set::FrameSet;
use crate::marks::MarkId;
use crate::position::Position;

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
