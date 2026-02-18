//! Key bindings for interactive mode.
//!
//! Maps crossterm KeyEvents to Ludwig actions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// An action resulting from a key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    /// Insert a character at the cursor.
    InsertChar(char),
    /// Execute a Ludwig command string.
    Command(String),
    /// Enter command input mode (command introducer).
    CommandIntroducer,
    /// Quit the editor.
    Quit,
    /// Save the file.
    Save,
    /// Toggle insert/overtype mode.
    ToggleMode,
    /// Terminal was resized.
    Resize,
    /// No action (ignore the key).
    Ignore,
}

/// Resolve a KeyEvent to a KeyAction.
pub fn resolve_key(key: KeyEvent) -> KeyAction {
    // F63 is our resize sentinel from CrosstermTerminal
    if key.code == KeyCode::F(63) && key.modifiers == KeyModifiers::NONE {
        return KeyAction::Resize;
    }

    // Ctrl combinations
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('q') => KeyAction::Quit,
            KeyCode::Char('s') => KeyAction::Save,
            _ => KeyAction::Ignore,
        };
    }

    match key.code {
        // Arrow keys -> cursor movement
        KeyCode::Up => KeyAction::Command("ZU".to_string()),
        KeyCode::Down => KeyAction::Command("ZD".to_string()),
        KeyCode::Left => KeyAction::Command("ZL".to_string()),
        KeyCode::Right => KeyAction::Command("ZR".to_string()),

        // Editing keys
        KeyCode::Backspace => KeyAction::Command("ZZ".to_string()),
        KeyCode::Delete => KeyAction::Command("D".to_string()),
        KeyCode::Enter => KeyAction::Command("ZC".to_string()),
        KeyCode::Tab => KeyAction::Command("ZR".to_string()), // TODO: proper tab handling
        KeyCode::Home => KeyAction::Command(">ZL".to_string()),
        KeyCode::End => KeyAction::Command(">ZR".to_string()),
        KeyCode::PageUp => KeyAction::Command("WB".to_string()),
        KeyCode::PageDown => KeyAction::Command("WF".to_string()),

        // Insert key toggles insert/overtype
        KeyCode::Insert => KeyAction::ToggleMode,

        // Escape enters command introducer
        KeyCode::Esc => KeyAction::CommandIntroducer,

        // Printable characters
        KeyCode::Char(ch) => KeyAction::InsertChar(ch),

        _ => KeyAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }

    #[test]
    fn test_arrow_keys() {
        assert_eq!(resolve_key(key(KeyCode::Up)), KeyAction::Command("ZU".to_string()));
        assert_eq!(resolve_key(key(KeyCode::Down)), KeyAction::Command("ZD".to_string()));
        assert_eq!(resolve_key(key(KeyCode::Left)), KeyAction::Command("ZL".to_string()));
        assert_eq!(resolve_key(key(KeyCode::Right)), KeyAction::Command("ZR".to_string()));
    }

    #[test]
    fn test_printable_char() {
        assert_eq!(resolve_key(key(KeyCode::Char('a'))), KeyAction::InsertChar('a'));
    }

    #[test]
    fn test_ctrl_q_quit() {
        assert_eq!(resolve_key(ctrl_key('q')), KeyAction::Quit);
    }

    #[test]
    fn test_escape_command_introducer() {
        assert_eq!(resolve_key(key(KeyCode::Esc)), KeyAction::CommandIntroducer);
    }

    #[test]
    fn test_backspace() {
        assert_eq!(resolve_key(key(KeyCode::Backspace)), KeyAction::Command("ZZ".to_string()));
    }

    #[test]
    fn test_enter() {
        assert_eq!(resolve_key(key(KeyCode::Enter)), KeyAction::Command("ZC".to_string()));
    }
}
