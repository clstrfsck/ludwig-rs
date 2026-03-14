use super::compile;
use crate::code::*;
use crate::lead_param::LeadParam;

// Helper to compile and return the instructions vec
fn compile_ok(input: &str) -> Vec<Instruction> {
    compile(input).unwrap().instructions().to_vec()
}

fn compile_err(input: &str) -> String {
    compile(input).unwrap_err().to_string()
}

// --- Simple commands ---

#[test]
fn test_simple_advance() {
    let instrs = compile_ok("A");
    assert_eq!(instrs.len(), 1);
    match &instrs[0] {
        Instruction::SimpleCmd {
            op,
            lead,
            tpars,
            exit_handler,
        } => {
            assert_eq!(*op, CmdOp::Advance);
            assert_eq!(*lead, LeadParam::None);
            assert!(tpars.is_empty());
            assert!(exit_handler.is_none());
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_simple_with_pint() {
    let instrs = compile_ok("5A");
    match &instrs[0] {
        Instruction::SimpleCmd { op, lead, .. } => {
            assert_eq!(*op, CmdOp::Advance);
            assert_eq!(*lead, LeadParam::Pint(5));
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_simple_with_nint() {
    let instrs = compile_ok("-3J");
    match &instrs[0] {
        Instruction::SimpleCmd { op, lead, .. } => {
            assert_eq!(*op, CmdOp::Jump);
            assert_eq!(*lead, LeadParam::Nint(3));
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_simple_pindef() {
    let instrs = compile_ok(">D");
    match &instrs[0] {
        Instruction::SimpleCmd { op, lead, .. } => {
            assert_eq!(*op, CmdOp::DeleteChar);
            assert_eq!(*lead, LeadParam::Pindef);
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_multiple_commands() {
    let instrs = compile_ok("AJ5D");
    assert_eq!(instrs.len(), 3);
}

// --- Trailing params ---

#[test]
fn test_insert_trailing_param() {
    let instrs = compile_ok("I/hello/");
    match &instrs[0] {
        Instruction::SimpleCmd { op, tpars, .. } => {
            assert_eq!(*op, CmdOp::InsertText);
            assert_eq!(tpars.len(), 1);
            assert_eq!(tpars[0].delim, '/');
            assert_eq!(tpars[0].content, "hello");
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_insert_with_count() {
    let instrs = compile_ok("3I'world'");
    match &instrs[0] {
        Instruction::SimpleCmd {
            op, lead, tpars, ..
        } => {
            assert_eq!(*op, CmdOp::InsertText);
            assert_eq!(*lead, LeadParam::Pint(3));
            assert_eq!(tpars[0].content, "world");
        }
        _ => panic!("expected SimpleCmd"),
    }
}

// --- Exit handlers ---

#[test]
fn test_exit_handler_success_only() {
    let instrs = compile_ok("A[I/ok/]");
    match &instrs[0] {
        Instruction::SimpleCmd { exit_handler, .. } => {
            let eh = exit_handler.as_ref().unwrap();
            assert!(eh.on_success.is_some());
            assert!(eh.on_failure.is_none());
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_exit_handler_both() {
    let instrs = compile_ok("A[I/ok/:I/fail/]");
    match &instrs[0] {
        Instruction::SimpleCmd { exit_handler, .. } => {
            let eh = exit_handler.as_ref().unwrap();
            assert!(eh.on_success.is_some());
            assert!(eh.on_failure.is_some());
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_exit_handler_failure_only() {
    let instrs = compile_ok("A[:I/fail/]");
    match &instrs[0] {
        Instruction::SimpleCmd { exit_handler, .. } => {
            let eh = exit_handler.as_ref().unwrap();
            assert!(eh.on_success.is_none());
            assert!(eh.on_failure.is_some());
        }
        _ => panic!("expected SimpleCmd"),
    }
}

// --- Compound commands ---

#[test]
fn test_compound_once() {
    let instrs = compile_ok("(A)");
    match &instrs[0] {
        Instruction::CompoundCmd {
            repeat,
            body,
            exit_handler,
        } => {
            assert_eq!(*repeat, RepeatCount::Once);
            assert_eq!(body.instructions().len(), 1);
            assert!(exit_handler.is_none());
        }
        _ => panic!("expected CompoundCmd"),
    }
}

#[test]
fn test_compound_times() {
    let instrs = compile_ok("3(AJ)");
    match &instrs[0] {
        Instruction::CompoundCmd { repeat, body, .. } => {
            assert_eq!(*repeat, RepeatCount::Times(3));
            assert_eq!(body.instructions().len(), 2);
        }
        _ => panic!("expected CompoundCmd"),
    }
}

#[test]
fn test_compound_indefinite() {
    let instrs = compile_ok(">(AD)");
    match &instrs[0] {
        Instruction::CompoundCmd { repeat, body, .. } => {
            assert_eq!(*repeat, RepeatCount::Indefinite);
            assert_eq!(body.instructions().len(), 2);
        }
        _ => panic!("expected CompoundCmd"),
    }
}

#[test]
fn test_compound_with_exit_handler() {
    let instrs = compile_ok(">(A)[I/done/]");
    match &instrs[0] {
        Instruction::CompoundCmd {
            repeat,
            exit_handler,
            ..
        } => {
            assert_eq!(*repeat, RepeatCount::Indefinite);
            assert!(exit_handler.is_some());
        }
        _ => panic!("expected CompoundCmd"),
    }
}

// --- Nested ---

#[test]
fn test_nested_compound() {
    let instrs = compile_ok(">(A(J[:D]))");
    assert_eq!(instrs.len(), 1);
    match &instrs[0] {
        Instruction::CompoundCmd { body, .. } => {
            // Body has A and (J[:D])
            assert_eq!(body.instructions().len(), 2);
            match &body.instructions()[0] {
                Instruction::SimpleCmd { op, .. } => {
                    assert_eq!(*op, CmdOp::Advance);
                }
                _ => panic!("expected SimpleCmd"),
            }
            match &body.instructions()[1] {
                Instruction::CompoundCmd {
                    body: inner_body,
                    exit_handler,
                    ..
                } => {
                    assert_eq!(inner_body.instructions().len(), 1);
                    assert!(exit_handler.is_none());
                    match &inner_body.instructions()[0] {
                        Instruction::SimpleCmd {
                            op, exit_handler, ..
                        } => {
                            assert_eq!(*op, CmdOp::Jump);
                            println!("Exit handler: {:?}", exit_handler);
                            let eh = exit_handler.as_ref().unwrap();
                            assert!(eh.on_success.is_none());
                            assert!(eh.on_failure.is_some());
                        }
                        _ => panic!("expected SimpleCmd"),
                    }
                }
                _ => panic!("expected inner CompoundCmd"),
            }
        }
        _ => panic!("expected CompoundCmd"),
    }
}

// --- Exit commands ---

#[test]
fn test_exit_success() {
    let instrs = compile_ok("XS");
    match &instrs[0] {
        Instruction::ExitSuccess(ExitLevels::Count(1)) => {}
        _ => panic!("expected ExitSuccess(Count(1))"),
    }
}

#[test]
fn test_exit_success_with_count() {
    let instrs = compile_ok("2XF");
    match &instrs[0] {
        Instruction::ExitFailure(ExitLevels::Count(2)) => {}
        _ => panic!("expected ExitFailure(Count(2))"),
    }
}

#[test]
fn test_exit_success_all() {
    let instrs = compile_ok(">XS");
    match &instrs[0] {
        Instruction::ExitSuccess(ExitLevels::All) => {}
        _ => panic!("expected ExitSuccess(All)"),
    }
}

#[test]
fn test_exit_abort() {
    let instrs = compile_ok("XA");
    match &instrs[0] {
        Instruction::ExitAbort => {}
        _ => panic!("expected ExitAbort"),
    }
}

// --- Comments and whitespace ---

#[test]
fn test_whitespace_between_commands() {
    let instrs = compile_ok("A J");
    assert_eq!(instrs.len(), 2);
}

#[test]
fn test_comment() {
    let instrs = compile_ok("A ! comment\nJ");
    assert_eq!(instrs.len(), 2);
}

#[test]
fn test_comment_at_end() {
    let instrs = compile_ok("A ! comment");
    assert_eq!(instrs.len(), 1);
}

// --- Error cases ---

#[test]
fn test_unclosed_paren() {
    let msg = compile_err(">(");
    assert!(msg.contains("unclosed parenthesis"), "got: {}", msg);
}

#[test]
fn test_unknown_command() {
    let msg = compile_err("Z");
    assert!(msg.contains("unknown command"), "got: {}", msg);
}

#[test]
fn test_invalid_lead_for_split_line() {
    let msg = compile_err(">SL");
    assert!(msg.contains("Syntax error."), "got: {}", msg);
}

#[test]
fn test_unclosed_trailing_param() {
    let msg = compile_err("I/hello");
    assert!(msg.contains("unclosed trailing parameter"), "got: {}", msg);
}

#[test]
fn test_unclosed_exit_handler() {
    let msg = compile_err("A[I/ok/");
    assert!(msg.contains("unclosed exit handler"), "got: {}", msg);
}

#[test]
fn test_empty_input() {
    let instrs = compile_ok("");
    assert!(instrs.is_empty());
}

#[test]
fn test_whitespace_only() {
    let instrs = compile_ok("   ");
    assert!(instrs.is_empty());
}

#[test]
fn test_invalid_lead_for_compound() {
    let msg = compile_err("-(A)");
    assert!(msg.contains("invalid leading parameter"), "got: {}", msg);
}

// --- SL command ---

#[test]
fn test_split_line() {
    let instrs = compile_ok("SL");
    match &instrs[0] {
        Instruction::SimpleCmd { op, lead, .. } => {
            assert_eq!(*op, CmdOp::SplitLine);
            assert_eq!(*lead, LeadParam::None);
        }
        _ => panic!("expected SimpleCmd"),
    }
}

// --- Case insensitivity ---

#[test]
fn test_lowercase_commands() {
    let instrs = compile_ok("a j d");
    assert_eq!(instrs.len(), 3);
}

#[test]
fn test_mixed_case() {
    let instrs = compile_ok("Sl");
    match &instrs[0] {
        Instruction::SimpleCmd { op, .. } => {
            assert_eq!(*op, CmdOp::SplitLine);
        }
        _ => panic!("expected SimpleCmd"),
    }
}

// --- Case change commands ---

#[test]
fn test_case_up() {
    let instrs = compile_ok("*U");
    match &instrs[0] {
        Instruction::SimpleCmd { op, lead, .. } => {
            assert_eq!(*op, CmdOp::CaseUp);
            assert_eq!(*lead, LeadParam::None);
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_case_low_with_count() {
    let instrs = compile_ok("5*L");
    match &instrs[0] {
        Instruction::SimpleCmd { op, lead, .. } => {
            assert_eq!(*op, CmdOp::CaseLow);
            assert_eq!(*lead, LeadParam::Pint(5));
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_case_edit_pindef() {
    let instrs = compile_ok(">*E");
    match &instrs[0] {
        Instruction::SimpleCmd { op, lead, .. } => {
            assert_eq!(*op, CmdOp::CaseEdit);
            assert_eq!(*lead, LeadParam::Pindef);
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_case_low_lowercase() {
    let instrs = compile_ok("*l");
    match &instrs[0] {
        Instruction::SimpleCmd { op, .. } => {
            assert_eq!(*op, CmdOp::CaseLow);
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_case_with_exit_handler() {
    let instrs = compile_ok("*U[I/ok/]");
    match &instrs[0] {
        Instruction::SimpleCmd {
            op, exit_handler, ..
        } => {
            assert_eq!(*op, CmdOp::CaseUp);
            assert!(exit_handler.is_some());
        }
        _ => panic!("expected SimpleCmd"),
    }
}

#[test]
fn test_case_invalid_star() {
    let msg = compile_err("*Z");
    assert!(msg.contains("unknown command"), "got: {}", msg);
}
