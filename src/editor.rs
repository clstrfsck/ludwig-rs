use std::fmt;

use crate::parse_cmd::ExecuteCommand;
use crate::{CmdResult, Frame};

pub struct Editor {
    frame: Frame,
}

impl Editor {
    pub fn new() -> Self {
        Editor {
            frame: Frame::new(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        Editor {
            frame: Frame::from_str(s),
        }
    }

    pub fn current_frame(&self) -> &Frame {
        &self.frame
    }

    pub fn current_frame_mut(&mut self) -> &mut Frame {
        &mut self.frame
    }

    pub fn execute_commands(&mut self, cmds: &[Box<dyn ExecuteCommand>]) -> CmdResult {
        for cmd in cmds {
            let result = cmd.execute(self);
            if result.is_failure() {
                return result;
            }
        }
        CmdResult::Success
    }
}

impl fmt::Display for Editor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_cmd::parse_commands;

    #[test]
    fn test_execute_insert_command() {
        let mut editor = Editor::from_str("hello");
        // Dot starts at (0,0), so insert goes before "hello"
        let cmds = parse_commands("i/world /").unwrap();
        let result = editor.execute_commands(&cmds);
        assert!(result.is_success());
        assert_eq!(editor.to_string(), "world hello");
    }

    #[test]
    fn test_execute_multiple_commands() {
        let mut editor = Editor::from_str("hello world");
        // Jump 5 columns, then insert "!"
        let cmds = parse_commands("5ji/!/").unwrap();
        let result = editor.execute_commands(&cmds);
        assert!(result.is_success());
        assert_eq!(editor.to_string(), "hello! world");
    }

    #[test]
    fn test_execute_commands_failure_stops() {
        let mut editor = Editor::from_str("hello");
        // Advance 2 lines on single-line content should fail
        let cmds = parse_commands("2ai/!/").unwrap();
        let result = editor.execute_commands(&cmds);
        assert!(result.is_failure());
        // The insert should not have executed
        assert_eq!(editor.to_string(), "hello");
    }

    #[test]
    fn test_display_delegates_to_frame() {
        let editor = Editor::from_str("test content");
        assert_eq!(editor.to_string(), "test content");
    }

    #[test]
    fn test_new_empty_editor() {
        let editor = Editor::new();
        assert_eq!(editor.to_string(), "");
    }
}
