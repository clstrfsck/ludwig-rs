//! Edit mode tracking for interactive mode.

/// The current editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditMode {
    /// Characters are inserted at the cursor position.
    Insert,
    /// Characters overtype (replace) at the cursor position.
    Overtype,
    /// Command mode — keys map to Ludwig commands.
    Command,
}

impl Default for EditMode {
    fn default() -> Self {
        EditMode::Insert
    }
}

impl EditMode {
    /// Return the display name for the mode (for status messages).
    pub fn name(&self) -> &'static str {
        match self {
            EditMode::Insert => "Insert",
            EditMode::Overtype => "Overtype",
            EditMode::Command => "Command",
        }
    }

    /// Whether this is a text-entry mode (Insert or Overtype).
    pub fn is_text_entry(&self) -> bool {
        matches!(self, EditMode::Insert | EditMode::Overtype)
    }
}
