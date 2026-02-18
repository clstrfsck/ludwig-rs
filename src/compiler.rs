//! Recursive descent compiler for Ludwig command strings.
//!
//! Transforms a textual command string into a tree-structured [`CompiledCode`].

use itertools::Itertools;
use phf::{Map, phf_map};
use std::iter::Peekable;
use std::str::Chars;

use anyhow::{Result, bail};

use crate::code::*;
use crate::lead_param::LeadParam;
use crate::marks::{MarkId, NUMBERED_MARK_RANGE};
use crate::trail_param::TrailParam;

/// Compile a Ludwig command string into a [`CompiledCode`] tree.
pub fn compile(input: &str) -> Result<CompiledCode> {
    let mut compiler = Compiler {
        chars: input.chars().peekable(),
    };
    let code = compiler.compile_sequence()?;
    compiler.skip_whitespace_and_comments();
    if compiler.chars.peek().is_some() {
        bail!("Syntax error: unexpected characters after commands.");
    }
    Ok(code)
}

struct Compiler<'a> {
    chars: Peekable<Chars<'a>>,
}

impl Compiler<'_> {
    /// Parse a sequence of instructions until a terminator (EOF, `)`, `]`, `:`).
    fn compile_sequence(&mut self) -> Result<CompiledCode> {
        let mut instructions = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match self.chars.peek() {
                None | Some(')') | Some(']') | Some(':') => break,
                _ => {
                    let instr = self.compile_command()?;
                    instructions.push(instr);
                }
            }
        }
        Ok(CompiledCode { instructions })
    }

    /// Parse one command: leading param, then `(` for compound or command name for simple.
    fn compile_command(&mut self) -> Result<Instruction> {
        self.skip_whitespace_and_comments();
        let lead = self.parse_leading_param()?;

        self.skip_whitespace_and_comments();
        match self.chars.peek() {
            Some(&'(') => self.compile_compound(lead),
            _ => self.compile_simple(lead),
        }
    }

    /// Parse a compound command: `(body)` with optional exit handler.
    fn compile_compound(&mut self, lead: LeadParam) -> Result<Instruction> {
        // Consume '('
        self.chars.next();

        let repeat = match lead {
            LeadParam::None | LeadParam::Plus => RepeatCount::Once,
            LeadParam::Pint(n) => RepeatCount::Times(n),
            LeadParam::Pindef => RepeatCount::Indefinite,
            _ => bail!("Syntax error: invalid leading parameter for compound command."),
        };

        let body = self.compile_sequence()?;

        // Expect ')'
        match self.chars.next() {
            Some(')') => {}
            _ => bail!("Syntax error: unclosed parenthesis."),
        }

        let exit_handler = self.parse_exit_handler()?;

        Ok(Instruction::CompoundCmd {
            repeat,
            body,
            exit_handler,
        })
    }

    /// Parse a simple command (or exit command) with optional trailing param and exit handler.
    fn compile_simple(&mut self, lead: LeadParam) -> Result<Instruction> {
        let cmd = self.parse_command()?;

        // Handle exit commands
        match cmd.op {
            CmdOp::ExitSuccess => {
                let levels = match lead {
                    LeadParam::None | LeadParam::Plus => ExitLevels::Count(1),
                    LeadParam::Pint(n) => ExitLevels::Count(n),
                    LeadParam::Pindef => ExitLevels::All,
                    _ => bail!("Syntax error: invalid leading parameter for XS."),
                };
                let _ = self.parse_exit_handler()?;
                return Ok(Instruction::ExitSuccess(levels));
            }
            CmdOp::ExitFailure => {
                let levels = match lead {
                    LeadParam::None | LeadParam::Plus => ExitLevels::Count(1),
                    LeadParam::Pint(n) => ExitLevels::Count(n),
                    LeadParam::Pindef => ExitLevels::All,
                    _ => bail!("Syntax error: invalid leading parameter for XF."),
                };
                let _ = self.parse_exit_handler()?;
                return Ok(Instruction::ExitFailure(levels));
            }
            CmdOp::ExitAbort => {
                if lead != LeadParam::None && lead != LeadParam::Plus {
                    bail!("Syntax error: XA does not accept a leading parameter.");
                }
                let _ = self.parse_exit_handler()?;
                return Ok(Instruction::ExitAbort);
            }
            _ => {}
        }

        // Validate leading parameter
        let kind = lead_param_kind(&lead);
        if !cmd.allows_lead(&kind) {
            bail!("Syntax error.");
        }

        // Parse trailing parameters if needed.
        // For multi-tpar commands (like R/search/replace/), all tpars share the
        // same delimiter: delim text1 delim text2 delim ...
        let mut tpars = Vec::new();
        if cmd.tpar_count > 0 {
            let first = self.parse_trailing_param()?;
            let delim = first.dlm;
            tpars.push(first);
            for _ in 1..cmd.tpar_count {
                tpars.push(self.parse_trailing_param_with_delim(delim)?);
            }
        }

        let exit_handler = self.parse_exit_handler()?;

        Ok(Instruction::SimpleCmd {
            op: cmd.op,
            lead,
            tpars,
            exit_handler,
        })
    }

    /// Parse an optional exit handler: `[success_code : fail_code]`.
    fn parse_exit_handler(&mut self) -> Result<Option<ExitHandler>> {
        self.skip_whitespace_and_comments();
        if self.chars.peek() != Some(&'[') {
            return Ok(None);
        }
        self.chars.next(); // consume '['

        let on_success = {
            let code = self.compile_sequence()?;
            if code.instructions.is_empty() {
                None
            } else {
                Some(code)
            }
        };

        // Check for ':' separator or ']' end
        let on_failure = match self.chars.peek() {
            Some(&':') => {
                self.chars.next(); // consume ':'
                let code = self.compile_sequence()?;
                if code.instructions.is_empty() {
                    None
                } else {
                    Some(code)
                }
            }
            _ => None,
        };

        match self.chars.next() {
            Some(']') => {}
            _ => bail!("Syntax error: unclosed exit handler bracket."),
        }

        Ok(Some(ExitHandler {
            on_success,
            on_failure,
        }))
    }

    /// Parse leading parameter (digits, +, -, >, <, @, =, %).
    fn parse_leading_param(&mut self) -> Result<LeadParam> {
        let buf: String = self
            .chars
            .peeking_take_while(|&ch| {
                ch.is_ascii_digit()
                    || matches!(ch, '-' | '+' | '=' | '%' | '@' | '<' | '>' | ',' | '.')
            })
            .collect();
        if buf.is_empty() {
            return Ok(LeadParam::None);
        }
        match buf.as_str() {
            "+" => Ok(LeadParam::Plus),
            "-" => Ok(LeadParam::Minus),
            ">" | "." => Ok(LeadParam::Pindef),
            "<" | "," => Ok(LeadParam::Nindef),
            "@" => Ok(LeadParam::Marker(MarkId::Numbered(1))),
            s if s.starts_with('@') => {
                let id = s[1..].parse::<u8>()?;
                if !NUMBERED_MARK_RANGE.contains(&id) {
                    bail!("Syntax error: marker ID must be between 1 and 9.");
                }
                Ok(LeadParam::Marker(MarkId::Numbered(id)))
            }
            s if s.starts_with('=') => Ok(LeadParam::Marker(MarkId::Equals)),
            s if s.starts_with('%') => Ok(LeadParam::Marker(MarkId::Modified)),
            s if s.starts_with('+') => {
                let num = s[1..].parse::<usize>()?;
                Ok(LeadParam::Pint(num))
            }
            s if s.starts_with('-') => {
                let num = s[1..].parse::<usize>()?;
                Ok(LeadParam::Nint(num))
            }
            s if s.chars().all(|ch| ch.is_ascii_digit()) => {
                let num = s.parse::<usize>()?;
                Ok(LeadParam::Pint(num))
            }
            _ => bail!("Invalid leading parameter: {}", buf),
        }
    }

    /// Parse a command name (1-3 chars, may start with `*` for prefix commands).
    fn parse_command(&mut self) -> Result<&'static CmdInfo> {
        let mut name = String::new();
        // Collect up to 3 alphabetic chars
        while let Some(&ch) = self.chars.peek() {
            if is_command_char(ch) && name.len() < 3 {
                name.push(ch.to_ascii_lowercase());
                self.chars.next();
                // Check if this is a known command name
                if let Ok(info) = name_to_info(&name) {
                    // If it's known, we can return it immediately
                    return Ok(info);
                }
            } else {
                break;
            }
        }
        if name.is_empty() {
            bail!("Syntax error: expected command name.");
        }
        bail!("Syntax error: unknown command '{}'.", name.to_uppercase());
    }

    /// Parse a trailing parameter: delimiter-bounded string.
    fn parse_trailing_param(&mut self) -> Result<TrailParam> {
        let delim = match self.chars.next() {
            Some(c) if c.is_ascii_punctuation() => c,
            _ => bail!("Syntax error: expected trailing parameter delimiter."),
        };
        self.parse_trailing_param_with_delim(delim)
    }

    /// Parse a trailing parameter using a known delimiter.
    fn parse_trailing_param_with_delim(&mut self, delim: char) -> Result<TrailParam> {
        let mut content = String::new();
        loop {
            match self.chars.next() {
                Some(c) if c == delim => return Ok(TrailParam::new(delim, content)),
                Some(c) => content.push(c),
                None => bail!("Syntax error: unclosed trailing parameter."),
            }
        }
    }

    /// Skip whitespace and `!`-to-end-of-line comments.
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.chars.peek() {
                Some(&ch) if ch.is_ascii_whitespace() => {
                    self.chars.next();
                }
                Some(&'!') => {
                    // Skip to end of line
                    self.chars.next();
                    while let Some(&ch) = self.chars.peek() {
                        if ch == '\n' {
                            self.chars.next();
                            break;
                        }
                        self.chars.next();
                    }
                }
                _ => break,
            }
        }
    }
}

macro_rules! lead_param_mask {
    ($($kind:ident),* $(,)?) => {
        {
            0u8 $(| (1u8 << (LeadParamKind::$kind as u8)))*
        }
    };
}


/// Which kinds of leading parameter are accepted (used for validation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeadParamKind {
    None = 0,
    Plus = 1,
    Minus = 2,
    Pint = 3,
    Nint = 4,
    Pindef = 5,
    Nindef = 6,
    Marker = 7,
}

fn lead_param_kind(lp: &LeadParam) -> LeadParamKind {
    match lp {
        LeadParam::None => LeadParamKind::None,
        LeadParam::Plus => LeadParamKind::Plus,
        LeadParam::Minus => LeadParamKind::Minus,
        LeadParam::Pint(_) => LeadParamKind::Pint,
        LeadParam::Nint(_) => LeadParamKind::Nint,
        LeadParam::Pindef => LeadParamKind::Pindef,
        LeadParam::Nindef => LeadParamKind::Nindef,
        LeadParam::Marker(_) => LeadParamKind::Marker,
    }
}

struct CmdInfo {
    op: CmdOp,
    allowed_leads: u8,
    tpar_count: u8,
}

impl CmdInfo {
    fn allows_lead(&self, kind: &LeadParamKind) -> bool {
        (self.allowed_leads & (1u8 << (*kind as u8))) != 0
    }
}

/// Map of command names to their CmdOp and parameter requirements.
/// This table will be quite large.  Please keep names sorted alphabetically
/// for readability.
const NAME_TO_OP_MAP: Map<&'static str, CmdInfo> = phf_map! {
    "a" => CmdInfo {
        op: CmdOp::Advance,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "c" => CmdInfo {
        op: CmdOp::InsertChar,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 0
    },
    "d" => CmdInfo {
        op: CmdOp::DeleteChar,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "g" => CmdInfo {
        op: CmdOp::Get,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 1
    },
    "i" => CmdInfo {
        op: CmdOp::InsertText,
        allowed_leads: lead_param_mask!(None, Plus, Pint),
        tpar_count: 1
    },
    "j" => CmdInfo {
        op: CmdOp::Jump,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "l" => CmdInfo {
        op: CmdOp::InsertLine,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 0
    },
    "br" => CmdInfo {
        op: CmdOp::Bridge,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 1
    },
    "eol" => CmdInfo {
        op: CmdOp::EqualEol,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pindef, Nindef),
        tpar_count: 0
    },
    "eop" => CmdInfo {
        op: CmdOp::EqualEop,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 0
    },
    "eof" => CmdInfo {
        op: CmdOp::EqualEof,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 0
    },
    "eqc" => CmdInfo {
        op: CmdOp::EqualColumn,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pindef, Nindef),
        tpar_count: 1
    },
    "eqm" => CmdInfo {
        op: CmdOp::EqualMark,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pindef, Nindef),
        tpar_count: 1
    },
    "eqs" => CmdInfo {
        op: CmdOp::EqualString,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pindef, Nindef),
        tpar_count: 1
    },
    "k" => CmdInfo {
        op: CmdOp::DeleteLine,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "m" => CmdInfo {
        op: CmdOp::Mark,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 0
    },
    "n" => CmdInfo {
        op: CmdOp::Next,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 1
    },
    "o" => CmdInfo {
        op: CmdOp::OvertypeText,
        allowed_leads: lead_param_mask!(None, Plus, Pint),
        tpar_count: 1
    },
    "r" => CmdInfo {
        op: CmdOp::Replace,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 2
    },
    "sw" => CmdInfo {
        op: CmdOp::SwapLine,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "wb" => CmdInfo {
        op: CmdOp::WindowBackward,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "we" => CmdInfo {
        op: CmdOp::WindowEnd,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "wf" => CmdInfo {
        op: CmdOp::WindowForward,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "wl" => CmdInfo {
        op: CmdOp::WindowLeft,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "wm" => CmdInfo {
        op: CmdOp::WindowMiddle,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "wn" => CmdInfo {
        op: CmdOp::WindowNew,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "wr" => CmdInfo {
        op: CmdOp::WindowRight,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "wt" => CmdInfo {
        op: CmdOp::WindowTop,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "sl" => CmdInfo {
        op: CmdOp::SplitLine,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "ya" => CmdInfo {
        op: CmdOp::WordAdvance,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "xa" => CmdInfo {
        op: CmdOp::ExitAbort,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "xf" => CmdInfo {
        op: CmdOp::ExitFailure,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "xs" => CmdInfo {
        op: CmdOp::ExitSuccess,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "zd" => CmdInfo {
        op: CmdOp::Down,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "zl" => CmdInfo {
        op: CmdOp::Left,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "zr" => CmdInfo {
        op: CmdOp::Right,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "zc" => CmdInfo {
        op: CmdOp::Return,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "zu" => CmdInfo {
        op: CmdOp::Up,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "zz" => CmdInfo {
        op: CmdOp::Rubout,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "\"" => CmdInfo {
        op: CmdOp::DittoUp,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "'" => CmdInfo {
        op: CmdOp::DittoDown,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "*e" => CmdInfo {
        op: CmdOp::CaseEdit,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "*l" => CmdInfo {
        op: CmdOp::CaseLow,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "*u" => CmdInfo {
        op: CmdOp::CaseUp,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
};

/// Map a command name string to its CmdInfo.
fn name_to_info(name: &str) -> Result<&'static CmdInfo> {
    NAME_TO_OP_MAP.get(name)
        .ok_or_else(|| anyhow::anyhow!("Syntax error: unknown command '{}'.", name.to_uppercase()))
}

/// Check if a character is valid in a command name.
fn is_command_char(ch: char) -> bool {
    matches!(ch, '\\' | '"' | '\'' | '*' | '{' | '}' | '?') || ch.is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to compile and return the instructions vec
    fn compile_ok(input: &str) -> Vec<Instruction> {
        compile(input).unwrap().instructions
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
                assert_eq!(tpars[0].dlm, '/');
                assert_eq!(tpars[0].str, "hello");
            }
            _ => panic!("expected SimpleCmd"),
        }
    }

    #[test]
    fn test_insert_with_count() {
        let instrs = compile_ok("3I'world'");
        match &instrs[0] {
            Instruction::SimpleCmd { op, lead, tpars, .. } => {
                assert_eq!(*op, CmdOp::InsertText);
                assert_eq!(*lead, LeadParam::Pint(3));
                assert_eq!(tpars[0].str, "world");
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
                assert_eq!(body.instructions.len(), 1);
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
                assert_eq!(body.instructions.len(), 2);
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
                assert_eq!(body.instructions.len(), 2);
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
                assert_eq!(body.instructions.len(), 2);
                match &body.instructions[0] {
                    Instruction::SimpleCmd { op, .. } => {
                        assert_eq!(*op, CmdOp::Advance);
                    }
                    _ => panic!("expected SimpleCmd"),
                }
                match &body.instructions[1] {
                    Instruction::CompoundCmd {
                        body: inner_body,
                        exit_handler,
                        ..
                    } => {
                        assert_eq!(inner_body.instructions.len(), 1);
                        assert!(exit_handler.is_none());
                        match &inner_body.instructions[0] {
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
            Instruction::SimpleCmd { op, exit_handler, .. } => {
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
}
