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
use crossterm::event::KeyEvent;

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

            // Check user-defined key bindings (UK command) first.
            if self.dispatch_user_key(key, terminal) {
                continue;
            }

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
                self.execute_compiled_code(&code, terminal);
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

    /// Check whether the key event matches a user-defined binding (UK).
    /// If a binding is found, execute it and return `true`; otherwise return `false`.
    fn dispatch_user_key(&mut self, key: KeyEvent, terminal: &mut dyn Terminal) -> bool {
        let name = match keybind::key_event_to_name(key) {
            Some(n) => n,
            None => return false,
        };
        // Clone the code out to avoid holding a borrow on frame_set.
        let code = match self.frame_set.user_key_bindings.get(&name) {
            Some(c) => c.clone(),
            None => return false,
        };
        self.screen
            .clear_message(self.frame_set.current_frame(), terminal);
        self.execute_compiled_code(&code, terminal);
        self.screen.fixup(self.frame_set.current_frame(), terminal);
        true
    }

    /// Execute already-compiled code through the interpreter.
    fn execute_compiled_code(
        &mut self,
        code: &crate::code::CompiledCode,
        terminal: &mut dyn Terminal,
    ) {
        let mut backend = InteractiveScreenBackend::new(&mut self.screen);
        let outcome = self.frame_set.execute_with_screen(code, &mut backend);

        for msg in backend.drain_messages() {
            self.screen.show_message(terminal, &msg);
        }

        if self.frame_set.quit_requested {
            self.frame_set.quit_requested = false;
            self.running = false;
        }

        if self.frame_set.suspend_requested {
            self.frame_set.suspend_requested = false;
            self.handle_suspend(terminal);
        }

        if self.frame_set.subprocess_requested {
            self.frame_set.subprocess_requested = false;
            self.handle_subprocess(terminal);
        }

        if !outcome.is_success() {
            terminal.beep();
        }
    }

    /// Handle UP — suspend the editor process and return control to the parent shell.
    ///
    /// Cleans up the terminal first, sends SIGTSTP, then reinitialises and redraws
    /// after the process is resumed.
    fn handle_suspend(&mut self, terminal: &mut dyn Terminal) {
        terminal.cleanup().ok();
        // Send SIGTSTP to the current process using the `kill` shell command.
        // This avoids requiring a libc dependency for a rarely-used feature.
        let pid = std::process::id();
        std::process::Command::new("kill")
            .args(["-TSTP", &pid.to_string()])
            .status()
            .ok();
        // When resumed (fg), reinitialise and redraw.
        terminal.init().ok();
        self.screen.invalidate();
        self.screen.redraw(self.frame_set.current_frame(), terminal);
    }

    /// Handle US — spawn a subprocess shell.
    ///
    /// Cleans up the terminal, runs the user's shell, then reinitialises and
    /// redraws after the shell exits.
    fn handle_subprocess(&mut self, terminal: &mut dyn Terminal) {
        terminal.cleanup().ok();
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        std::process::Command::new(&shell).status().ok();
        terminal.init().ok();
        self.screen.invalidate();
        self.screen.redraw(self.frame_set.current_frame(), terminal);
    }
}
