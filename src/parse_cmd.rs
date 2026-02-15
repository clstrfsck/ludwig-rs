use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use crate::editor::Editor;
use crate::{CmdResult, Frame, LeadParam, MarkId, TrailParam};
use anyhow::Result;

#[derive(Debug)]
struct ParseableCommand {
    lpars: HashSet<LeadParam>,
    tpars: usize,
}

fn parseable_commands() -> HashMap<String, ParseableCommand> {
    HashMap::from([
        ("i".to_string(), ParseableCommand {
            lpars: HashSet::from([
                LeadParam::None,
                LeadParam::Plus,
                LeadParam::Pint(0),
            ]),
            tpars: 1,
        }),
        ("o".to_string(), ParseableCommand {
            lpars: HashSet::from([
                LeadParam::None,
                LeadParam::Plus,
                LeadParam::Pint(0),
            ]),
            tpars: 1,
        }),
        ("l".to_string(), ParseableCommand {
            lpars: HashSet::from([
                LeadParam::None,
                LeadParam::Plus,
                LeadParam::Pint(0),
                LeadParam::Minus,
                LeadParam::Nint(0),
            ]),
            tpars: 0,
        }),
        ("c".to_string(), ParseableCommand {
            lpars: HashSet::from([
                LeadParam::None,
                LeadParam::Plus,
                LeadParam::Pint(0),
                LeadParam::Minus,
                LeadParam::Nint(0),
            ]),
            tpars: 0,
        }),
        ("a".to_string(), ParseableCommand {
            lpars: HashSet::from([
                LeadParam::None,
                LeadParam::Plus,
                LeadParam::Pint(0),
                LeadParam::Pindef,
                LeadParam::Minus,
                LeadParam::Nint(0),
                LeadParam::Nindef,
                LeadParam::Marker(MarkId::Dot),
            ]),
            tpars: 0,
        }),
        ("d".to_string(), ParseableCommand {
            lpars: HashSet::from([
                LeadParam::None,
                LeadParam::Plus,
                LeadParam::Minus,
                LeadParam::Pint(0),
                LeadParam::Nint(0),
                LeadParam::Pindef,
                LeadParam::Nindef,
                LeadParam::Marker(MarkId::Dot),
            ]),
            tpars: 0,
        }),
        ("j".to_string(), ParseableCommand {
            lpars: HashSet::from([
                LeadParam::None,
                LeadParam::Plus,
                LeadParam::Minus,
                LeadParam::Pint(0),
                LeadParam::Nint(0),
                LeadParam::Pindef,
                LeadParam::Nindef,
                LeadParam::Marker(MarkId::Dot),
            ]),
            tpars: 0,
        }),
    ])
}

type TparamCmd = fn(&mut Frame, LeadParam, &TrailParam) -> CmdResult;
type NoTparamCmd = fn(&mut Frame, LeadParam) -> CmdResult;

static TPARAM_CMDS: LazyLock<HashMap<&str, TparamCmd>> = LazyLock::new(|| {
    HashMap::from([
        ("i", Frame::cmd_insert_text as TparamCmd),
        ("o", Frame::cmd_overtype_text as TparamCmd),
    ])
});

static NO_TPARAM_CMDS: LazyLock<HashMap<&str, NoTparamCmd>> = LazyLock::new(|| {
    HashMap::from([
        ("a", Frame::cmd_advance as NoTparamCmd),
        ("c", Frame::cmd_insert_char as NoTparamCmd),
        ("d", Frame::cmd_delete_char as NoTparamCmd),
        ("j", Frame::cmd_jump as NoTparamCmd),
        ("l", Frame::cmd_insert_line as NoTparamCmd),
        ("sl", Frame::cmd_split_line as NoTparamCmd),
    ])
});

pub trait ExecuteCommand {
    fn execute(&self, editor: &mut Editor) -> CmdResult;
}

pub struct NoTparamParsedCommand {
    pub leading_param: LeadParam,
    pub cmd: NoTparamCmd,
}

pub struct TparamParsedCommand {
    pub leading_param: LeadParam,
    pub trailing_param: TrailParam,
    pub cmd: TparamCmd,
}

impl ExecuteCommand for NoTparamParsedCommand {
    fn execute(&self, editor: &mut Editor) -> CmdResult {
        (self.cmd)(editor.current_frame_mut(), self.leading_param)
    }
}

impl ExecuteCommand for TparamParsedCommand {
    fn execute(&self, editor: &mut Editor) -> CmdResult {
        (self.cmd)(editor.current_frame_mut(), self.leading_param, &self.trailing_param)
    }
}

fn parse_command_name(chars: &mut std::iter::Peekable<std::str::Chars>) -> Result<String> {
    let mut command_name = String::new();
    // FIXME: This is a bit hacky, but it works for now
    for _ in 1..=3 {
        let next_char = chars.next().ok_or_else(|| anyhow::anyhow!("Syntax error."))?;
        command_name.push(next_char);
        if TPARAM_CMDS.contains_key(command_name.as_str()) || NO_TPARAM_CMDS.contains_key(command_name.as_str()) {
            return Ok(command_name);
        }
    }
    anyhow::bail!("Syntax error.");
}

pub fn parse_commands(input: &str) -> Result<Vec<Box<dyn ExecuteCommand>>> {
    let mut commands = Vec::<Box<dyn ExecuteCommand>>::new();
    let mut chars = input.trim().chars().peekable();
    let parseable_commands = parseable_commands();

    while let Some(&_) = chars.peek() {
        // 1. Parse Leading Parameter
        // Leading params can be integers or specific symbols: -, +, =, %, @, <, >
        let mut leading_param = String::new();
        while let Some(&next_char) = chars.peek() {
            if next_char.is_digit(10) || "-+=%@<>,.".contains(next_char) {
                leading_param.push(next_char);
                chars.next();
            } else {
                break;
            }
        }
        let leading_param = if leading_param.is_empty() {
            LeadParam::None
        } else {
            match leading_param.as_str() {
                "+" => LeadParam::Plus,
                "-" => LeadParam::Minus,
                ">" | "." => LeadParam::Pindef,
                "<" | "," => LeadParam::Nindef,
                s if s.starts_with('@') => {
                    let id = s[1..].parse::<u8>()?;
                    LeadParam::Marker(MarkId::Numbered(id))
                }
                s if s.starts_with('=') => LeadParam::Marker(MarkId::Last),
                s if s.starts_with('%') => LeadParam::Marker(MarkId::Modified),
                s if s.starts_with('+') => {
                    let num = s[1..].parse::<usize>()?;
                    LeadParam::Pint(num)
                }
                s if s.starts_with('-') => {
                    let num = s[1..].parse::<usize>()?;
                    LeadParam::Nint(num)
                }
                s if s.chars().all(|ch| ch.is_digit(10)) => {
                    let num = s.parse::<usize>()?;
                    LeadParam::Pint(num)
                }
                _ => anyhow::bail!("Invalid leading parameter: {}", leading_param),
            }
        };

        // 2. Parse Command Name
        // The command name is expected to be the next characters
        let command_name = parse_command_name(&mut chars)?;

        // 3. Validate Command and Leading Parameter
        let parseable_command = match parseable_commands.get(command_name.as_str()) {
            Some(cmd) => cmd,
            None => anyhow::bail!("Syntax error."),
        };
        match leading_param {
            LeadParam::Pint(_) => {
                if !parseable_command.lpars.contains(&LeadParam::Pint(0)) {
                    anyhow::bail!("Syntax error.");
                }
            }
            LeadParam::Nint(_) => {
                if !parseable_command.lpars.contains(&LeadParam::Nint(0)) {
                    anyhow::bail!("Syntax error.");
                }
            }
            LeadParam::Marker(_) => {
                if !parseable_command.lpars.contains(&LeadParam::Marker(MarkId::Dot)) {
                    anyhow::bail!("Syntax error.");
                }
            }
            _ => {
                if !parseable_command.lpars.contains(&leading_param) {
                    anyhow::bail!("Syntax error.");
                }
            }
        }


        // 5. Parse Trailing Parameter (if applicable)
        if parseable_command.tpars > 0 {
            // FIXME: Currently only supports one trailing parameter
            if let Some(delim) = chars.next() {
                if !delim.is_ascii_punctuation() {
                    anyhow::bail!("Syntax error.");
                }
                let mut complete = false;
                let mut tpar = String::new();
                while let Some(c) = chars.next() {
                    if c == delim {
                        complete = true;
                        break;
                    } else {
                        tpar.push(c);
                    }
                }
                if !complete {
                    anyhow::bail!("Syntax error.");
                }
                commands.push(Box::new(TparamParsedCommand {
                    leading_param,
                    trailing_param: TrailParam::from_str(&tpar),
                    cmd: *TPARAM_CMDS.get(command_name.as_str()).unwrap(),
                }));
            } else {
                anyhow::bail!("Syntax error.");
            }
        } else {
            commands.push(Box::new(NoTparamParsedCommand {
                leading_param,
                cmd: *NO_TPARAM_CMDS.get(command_name.as_str()).unwrap(),
            }));
        }
    }

    Ok(commands)
}
