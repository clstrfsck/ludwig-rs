//! Application event loop for interactive mode.
//!
//! The `App` struct ties together the FrameSet, Screen, Terminal, and key bindings
//! into a main event loop.  Command execution always goes through the interpreter
//! via [`FrameSet::execute_with_screen`]; there is no parallel instruction loop here.

use anyhow::Result;

use crate::TrailParam;
use crate::compiler;
use crate::frame::{EditCommands, KeyboardMode};
use crate::frame_set::FrameSet;
use crate::keybind::{self, KeyAction, PromptAction};
use crate::lead_param::LeadParam;
use crate::screen::{InteractiveScreenBackend, Screen};
use crate::terminal::Terminal;

/// The interactive application state.
pub struct App {
    pub frame_set: FrameSet,
    pub screen: Screen,
    pub file_path: Option<String>,
    pub running: bool,
}

impl App {
    pub fn new(frame_set: FrameSet, screen: Screen, file_path: Option<String>) -> Self {
        Self {
            frame_set,
            screen,
            file_path,
            running: true,
        }
    }

    /// Run the main event loop.
    pub fn run(&mut self, terminal: &mut dyn Terminal) -> Result<()> {
        terminal.init()?;

        // Initial full redraw
        self.screen.invalidate();
        self.screen.redraw(self.frame_set.current_frame(), terminal);
        self.screen.fixup(self.frame_set.current_frame(), terminal);

        while self.running {
            self.screen.redraw(self.frame_set.current_frame(), terminal);
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
            .clear_message(self.frame_set.current_frame(), terminal);

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
                self.frame_set.keyboard_mode = match self.frame_set.keyboard_mode {
                    KeyboardMode::Insert => KeyboardMode::Overtype,
                    KeyboardMode::Overtype => KeyboardMode::Insert,
                    KeyboardMode::Command => KeyboardMode::Insert,
                };
            }
            KeyAction::Resize => {
                let size = terminal.size();
                self.screen.resize(size);
                self.screen.redraw(self.frame_set.current_frame(), terminal);
            }
            KeyAction::Ignore => {}
        }

        self.screen.fixup(self.frame_set.current_frame(), terminal);
    }

    /// Handle inserting a character in insert or overtype mode.
    fn handle_insert_char(&mut self, ch: char) {
        let keyboard_mode = self.frame_set.keyboard_mode;
        let frame = self.frame_set.current_frame_mut();
        let tpar = TrailParam::from_str(&ch.to_string());
        match keyboard_mode {
            KeyboardMode::Insert => {
                frame.cmd_insert_text(LeadParam::None, &tpar);
            }
            KeyboardMode::Overtype => {
                frame.cmd_overtype_text(LeadParam::None, &tpar);
            }
            KeyboardMode::Command => {
                // In command mode, chars are not inserted
            }
        }
    }

    /// Compile and execute a Ludwig command string through the interpreter.
    ///
    /// Window commands are dispatched via [`InteractiveScreenBackend`]; the
    /// viewport is updated in place and the normal `fixup` call at the end of
    /// `handle_action` handles the subsequent terminal render.
    fn execute_command_string(&mut self, cmd_str: &str, terminal: &mut dyn Terminal) {
        match compiler::compile(cmd_str) {
            Ok(code) => {
                // Rust allows disjoint field borrows: frame_set and screen are
                // separate fields, so both can be borrowed mutably here.
                let mut backend = InteractiveScreenBackend::new(&mut self.screen);
                let outcome = self.frame_set.execute_with_screen(&code, &mut backend);

                // Flush any output lines buffered during execution (e.g. from SI).
                for msg in backend.drain_messages() {
                    self.screen.show_message(terminal, &msg);
                }

                if !outcome.is_success() {
                    terminal.beep();
                }
            }
            Err(e) => {
                self.screen.show_message(terminal, &format!("Error: {}", e));
                terminal.beep();
            }
        }
    }

    /// Handle command input mode (after pressing Escape).
    fn command_input(&mut self, terminal: &mut dyn Terminal) {
        const PROMPT: &str = "Command: ";
        let prompt_len = PROMPT.len();

        // Show prompt via buffered screen
        self.screen.msg_rows = 1;
        self.screen.update_message_row(terminal, PROMPT, prompt_len);

        // Read command line using the keybind prompt abstraction
        let mut input = String::new();

        loop {
            let key = match terminal.read_key() {
                Ok(key) => key,
                Err(_) => continue,
            };

            match keybind::resolve_prompt_key(key) {
                PromptAction::Accept => {
                    break;
                }
                PromptAction::Cancel => {
                    self.screen
                        .clear_message(self.frame_set.current_frame(), terminal);
                    return;
                }
                PromptAction::Backspace => {
                    if !input.is_empty() {
                        input.pop();
                        let line = format!("{}{}", PROMPT, input);
                        self.screen
                            .update_message_row(terminal, &line, prompt_len + input.len());
                    }
                }
                PromptAction::Char(ch) => {
                    input.push(ch);
                    let line = format!("{}{}", PROMPT, input);
                    self.screen
                        .update_message_row(terminal, &line, prompt_len + input.len());
                }
                PromptAction::Ignore => {}
            }
        }

        // Clear prompt
        self.screen
            .clear_message(self.frame_set.current_frame(), terminal);

        if !input.is_empty() {
            self.execute_command_string(&input, terminal);
        }
    }

    /// Handle quit.
    fn handle_quit(&mut self, terminal: &mut dyn Terminal) {
        if self.frame_set.modified() {
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
                            .clear_message(self.frame_set.current_frame(), terminal);
                    }
                }
            }
        } else {
            self.running = false;
        }
    }

    /// Handle save.
    fn handle_save(&mut self, terminal: &mut dyn Terminal) {
        if let Some(path) = &self.file_path.clone() {
            match crate::save::write_with_backup(&self.frame_set.to_string(), path, 1) {
                Ok(line_count) => {
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
