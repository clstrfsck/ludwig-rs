//! Recursive descent compiler for Ludwig command strings.
//!
//! Transforms a textual command string into a tree-structured [`CompiledCode`].

mod cmd_table;
use cmd_table::{LeadParamKind, is_command_char, name_to_info};

#[cfg(test)]
mod tests;

use itertools::Itertools;
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
        Ok(CompiledCode::new(instructions))
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
            let delim = first.delim;
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
            if code.instructions().is_empty() {
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
                if code.instructions().is_empty() {
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
    fn parse_command(&mut self) -> Result<&'static cmd_table::CmdInfo> {
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
