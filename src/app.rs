//! Application event loop for interactive mode.
//!
//! The `App` struct ties together the Editor, Screen, Terminal, and key bindings
//! into a main event loop.

use anyhow::Result;

use crate::TrailParam;
use crate::code::{CmdOp, CompiledCode, Instruction};
use crate::compiler;
use crate::edit_mode::EditMode;
use crate::editor::Editor;
use crate::frame::EditCommands;
use crate::keybind::{self, KeyAction};
use crate::lead_param::LeadParam;
use crate::screen::Screen;
use crate::terminal::Terminal;

/// The interactive application state.
pub struct App {
    pub editor: Editor,
    pub screen: Screen,
    pub mode: EditMode,
    pub file_path: Option<String>,
    pub running: bool,
}

impl App {
    pub fn new(editor: Editor, screen: Screen, file_path: Option<String>) -> Self {
        Self {
            editor,
            screen,
            mode: EditMode::Insert,
            file_path,
            running: true,
        }
    }

    /// Run the main event loop.
    pub fn run(&mut self, terminal: &mut dyn Terminal) -> Result<()> {
        terminal.init()?;

        // Initial full redraw
        self.screen.invalidate();
        self.screen.redraw(self.editor.current_frame(), terminal);
        self.screen.fixup(self.editor.current_frame(), terminal);

        while self.running {
            self.screen.redraw(self.editor.current_frame(), terminal);
            let key = match terminal.read_key() {
                Ok(key) => key,
                Err(_) => continue,
            };

            let action = keybind::resolve_key(key);
            self.handle_action(action, terminal);
        }

        terminal.cleanup()?;
        Ok(())
    }

    /// Handle a resolved key action.
    fn handle_action(&mut self, action: KeyAction, terminal: &mut dyn Terminal) {
        // Clear any message before processing
        self.screen
            .clear_message(self.editor.current_frame(), terminal);

        match action {
            KeyAction::InsertChar(ch) => {
                self.handle_insert_char(ch);
            }
            KeyAction::Command(cmd_str) => {
                self.execute_command_string(&cmd_str, terminal);
            }
            KeyAction::CommandIntroducer => {
                self.command_input(terminal);
            }
            KeyAction::Quit => {
                self.handle_quit(terminal);
            }
            KeyAction::Save => {
                self.handle_save(terminal);
            }
            KeyAction::ToggleMode => {
                self.mode = match self.mode {
                    EditMode::Insert => EditMode::Overtype,
                    EditMode::Overtype => EditMode::Insert,
                    EditMode::Command => EditMode::Insert,
                };
            }
            KeyAction::Resize => {
                let size = terminal.size();
                self.screen.resize(size);
                self.screen.invalidate();
                self.screen.redraw(self.editor.current_frame(), terminal);
            }
            KeyAction::Ignore => {}
        }

        self.screen.fixup(self.editor.current_frame(), terminal);
    }

    /// Handle inserting a character in insert or overtype mode.
    fn handle_insert_char(&mut self, ch: char) {
        let frame = self.editor.current_frame_mut();
        let tpar = TrailParam::from_str(&ch.to_string());
        match self.mode {
            EditMode::Insert => {
                frame.cmd_insert_text(LeadParam::None, &tpar);
            }
            EditMode::Overtype => {
                frame.cmd_overtype_text(LeadParam::None, &tpar);
            }
            EditMode::Command => {
                // In command mode, chars are not inserted
            }
        }
    }

    /// Compile and execute a Ludwig command string.
    /// Window commands are intercepted and handled at the App level.
    fn execute_command_string(&mut self, cmd_str: &str, terminal: &mut dyn Terminal) {
        match compiler::compile(cmd_str) {
            Ok(code) => {
                self.execute_code(&code, terminal);
            }
            Err(e) => {
                self.screen.show_message(terminal, &format!("Error: {}", e));
                terminal.beep();
            }
        }
    }

    /// Execute compiled code, intercepting window commands.
    fn execute_code(&mut self, code: &CompiledCode, terminal: &mut dyn Terminal) {
        for instr in code.instructions() {
            if let Instruction::SimpleCmd { op, lead, .. } = instr
                && self.try_handle_window_cmd(*op, *lead, terminal)
            {
                continue;
            }
            // Not a window command — pass single instruction to interpreter
            let single = CompiledCode::new(vec![instr.clone()]);
            let outcome = self.editor.execute(&single);
            if !outcome.is_success() {
                terminal.beep();
                return;
            }
        }
    }

    /// Try to handle a window command. Returns true if handled.
    fn try_handle_window_cmd(
        &mut self,
        op: CmdOp,
        lead: LeadParam,
        terminal: &mut dyn Terminal,
    ) -> bool {
        let height = self.screen.text_height();

        let count = match lead {
            LeadParam::None | LeadParam::Plus => 1,
            LeadParam::Pint(n) => n,
            LeadParam::Pindef => usize::MAX,
            _ => return false,
        };

        match op {
            CmdOp::WindowForward => {
                // Move dot forward by text_height * count lines (like the C reference).
                // Fixup will scroll the viewport to follow dot.
                let frame = self.editor.current_frame_mut();
                let dot = frame.dot();
                let new_line = dot.line.saturating_add(count * height);
                frame.set_dot(crate::Position::new(new_line, dot.column));
                true
            }
            CmdOp::WindowBackward => {
                // Move dot backward by text_height * count lines (like the C reference).
                // Fixup will scroll the viewport to follow dot.
                let frame = self.editor.current_frame_mut();
                let dot = frame.dot();
                let new_line = dot.line.saturating_sub(count * height);
                frame.set_dot(crate::Position::new(new_line, dot.column));
                true
            }
            CmdOp::WindowTop => {
                // Position dot's line at top of window
                let dot_line = self.editor.current_frame().dot().line;
                self.screen.viewport.top_line = dot_line;
                self.screen.invalidate();
                self.screen.redraw(self.editor.current_frame(), terminal);
                true
            }
            CmdOp::WindowEnd => {
                // Position dot's line at bottom of window
                let dot_line = self.editor.current_frame().dot().line;
                self.screen.viewport.top_line = dot_line.saturating_sub(height - 1);
                self.screen.invalidate();
                self.screen.redraw(self.editor.current_frame(), terminal);
                true
            }
            CmdOp::WindowMiddle => {
                // Position dot's line at middle of window
                let dot_line = self.editor.current_frame().dot().line;
                self.screen.viewport.top_line = dot_line.saturating_sub(height / 2);
                self.screen.invalidate();
                self.screen.redraw(self.editor.current_frame(), terminal);
                true
            }
            CmdOp::WindowNew => {
                // Full screen redraw
                self.screen.invalidate();
                self.screen.redraw(self.editor.current_frame(), terminal);
                true
            }
            CmdOp::WindowLeft => {
                let scroll = count.min(self.screen.viewport.offset);
                self.screen.viewport.offset -= scroll;
                self.screen.invalidate();
                self.screen.redraw(self.editor.current_frame(), terminal);
                true
            }
            CmdOp::WindowRight => {
                self.screen.viewport.offset += count;
                self.screen.invalidate();
                self.screen.redraw(self.editor.current_frame(), terminal);
                true
            }
            _ => false,
        }
    }

    /// Handle command input mode (after pressing Escape).
    fn command_input(&mut self, terminal: &mut dyn Terminal) {
        const PROMPT: &str = "Command: ";
        let prompt_len = PROMPT.len();

        // Show prompt via buffered screen
        self.screen.msg_rows = 1;
        self.screen.update_message_row(terminal, PROMPT, prompt_len);

        // Read command line
        let mut input = String::new();

        loop {
            let key = match terminal.read_key() {
                Ok(key) => key,
                Err(_) => continue,
            };

            match key.code {
                crossterm::event::KeyCode::Enter => {
                    break;
                }
                crossterm::event::KeyCode::Esc => {
                    // Cancel command input
                    self.screen
                        .clear_message(self.editor.current_frame(), terminal);
                    return;
                }
                crossterm::event::KeyCode::Backspace => {
                    if !input.is_empty() {
                        input.pop();
                        let line = format!("{}{}", PROMPT, input);
                        self.screen
                            .update_message_row(terminal, &line, prompt_len + input.len());
                    }
                }
                crossterm::event::KeyCode::Char(ch) => {
                    input.push(ch);
                    let line = format!("{}{}", PROMPT, input);
                    self.screen
                        .update_message_row(terminal, &line, prompt_len + input.len());
                }
                _ => {}
            }
        }

        // Clear prompt
        self.screen
            .clear_message(self.editor.current_frame(), terminal);

        if !input.is_empty() {
            self.execute_command_string(&input, terminal);
        }
    }

    /// Handle quit.
    fn handle_quit(&mut self, terminal: &mut dyn Terminal) {
        if self.editor.modified() {
            self.screen.show_message(
                terminal,
                "Unsaved changes. Ctrl-Q again to quit, or Ctrl-S to save.",
            );
            terminal.flush();

            // Wait for another key
            if let Ok(key) = terminal.read_key() {
                let action = keybind::resolve_key(key);
                match action {
                    KeyAction::Quit => {
                        self.running = false;
                    }
                    KeyAction::Save => {
                        self.handle_save(terminal);
                        self.running = false;
                    }
                    _ => {
                        self.screen
                            .clear_message(self.editor.current_frame(), terminal);
                    }
                }
            }
        } else {
            self.running = false;
        }
    }

    /// Handle save.
    fn handle_save(&mut self, terminal: &mut dyn Terminal) {
        if let Some(path) = &self.file_path {
            let mut contents = self.editor.to_string();
            if !contents.is_empty() && !contents.ends_with('\n') {
                contents.push('\n');
            }
            let line_count = contents.lines().count();

            // Create backup
            let backup = format!("{}~1", path);
            if std::path::Path::new(path).exists() {
                let _ = std::fs::rename(path, &backup);
            }

            match std::fs::write(path, &contents) {
                Ok(()) => {
                    self.screen.show_message(
                        terminal,
                        &format!(
                            "{} saved ({} line{}).",
                            path,
                            line_count,
                            if line_count == 1 { "" } else { "s" }
                        ),
                    );
                }
                Err(e) => {
                    self.screen
                        .show_message(terminal, &format!("Save failed: {}", e));
                    terminal.beep();
                }
            }
        } else {
            self.screen
                .show_message(terminal, "No file path specified.");
            terminal.beep();
        }
    }
}
