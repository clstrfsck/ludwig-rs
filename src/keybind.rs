//! Key bindings for interactive mode.
//!
//! Maps crossterm `KeyEvents` to Ludwig actions.

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

/// Resolve a `KeyEvent` to a `KeyAction`.
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
        KeyCode::Tab => KeyAction::Command("ZT".to_string()), // TODO: proper tab handling
        KeyCode::Home => KeyAction::Command(">ZL".to_string()),
        KeyCode::End => KeyAction::Command(">ZR".to_string()),
        KeyCode::PageUp => KeyAction::Command("WB".to_string()),
        KeyCode::PageDown => KeyAction::Command("WF".to_string()),

        // Insert key toggles insert/overtype
        KeyCode::Insert => KeyAction::ToggleMode,

        // Escape enters command introducer
        KeyCode::Esc | KeyCode::Char('\\') => KeyAction::CommandIntroducer,

        // Printable characters
        KeyCode::Char(ch) => KeyAction::InsertChar(ch),

        _ => KeyAction::Ignore,
    }
}

/// An action from a key press inside the command input prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAction {
    /// Confirm the current input and execute it.
    Accept,
    /// Discard the input and exit prompt mode.
    Cancel,
    /// Delete the last character.
    Backspace,
    /// Append a printable character to the input.
    Char(char),
    /// No action (ignore the key).
    Ignore,
}

/// Convert a `KeyEvent` to a canonical key name string for UK user-key bindings.
///
/// Returns `None` for events that cannot be named (e.g. unknown modifiers).
/// Single printable characters map to themselves (as a one-char string); all
/// other keys use human-readable names like `"UP-ARROW"`, `"F1"`, etc.
pub fn key_event_to_name(key: KeyEvent) -> Option<String> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        if let KeyCode::Char(ch) = key.code {
            return Some(format!("CTRL-{}", ch.to_ascii_uppercase()));
        }
        return None;
    }
    match key.code {
        KeyCode::Char(ch) => Some(ch.to_string()),
        KeyCode::Up => Some("UP-ARROW".to_string()),
        KeyCode::Down => Some("DOWN-ARROW".to_string()),
        KeyCode::Left => Some("LEFT-ARROW".to_string()),
        KeyCode::Right => Some("RIGHT-ARROW".to_string()),
        KeyCode::Home => Some("HOME".to_string()),
        KeyCode::End => Some("END".to_string()),
        KeyCode::PageUp => Some("PAGE-UP".to_string()),
        KeyCode::PageDown => Some("PAGE-DOWN".to_string()),
        KeyCode::Backspace => Some("BACKSPACE".to_string()),
        KeyCode::Delete => Some("DELETE".to_string()),
        KeyCode::Insert => Some("INSERT".to_string()),
        KeyCode::Tab => Some("TAB".to_string()),
        KeyCode::BackTab => Some("BACK-TAB".to_string()),
        KeyCode::Enter => Some("RETURN".to_string()),
        KeyCode::Esc => Some("ESCAPE".to_string()),
        KeyCode::F(n) => Some(format!("F{n}")),
        _ => None,
    }
}

/// Resolve a `KeyEvent` to a [`PromptAction`] for use inside the command-input prompt.
pub fn resolve_prompt_key(key: KeyEvent) -> PromptAction {
    match key.code {
        KeyCode::Enter => PromptAction::Accept,
        KeyCode::Esc => PromptAction::Cancel,
        KeyCode::Backspace => PromptAction::Backspace,
        KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            PromptAction::Char(ch)
        }
        _ => PromptAction::Ignore,
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
        assert_eq!(
            resolve_key(key(KeyCode::Up)),
            KeyAction::Command("ZU".to_string())
        );
        assert_eq!(
            resolve_key(key(KeyCode::Down)),
            KeyAction::Command("ZD".to_string())
        );
        assert_eq!(
            resolve_key(key(KeyCode::Left)),
            KeyAction::Command("ZL".to_string())
        );
        assert_eq!(
            resolve_key(key(KeyCode::Right)),
            KeyAction::Command("ZR".to_string())
        );
    }

    #[test]
    fn test_printable_char() {
        assert_eq!(
            resolve_key(key(KeyCode::Char('a'))),
            KeyAction::InsertChar('a')
        );
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
        assert_eq!(
            resolve_key(key(KeyCode::Backspace)),
            KeyAction::Command("ZZ".to_string())
        );
    }

    #[test]
    fn test_enter() {
        assert_eq!(
            resolve_key(key(KeyCode::Enter)),
            KeyAction::Command("ZC".to_string())
        );
    }
}
