//! Edit mode tracking for interactive mode.

/// The current editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditMode {
    /// Characters are inserted at the cursor position.
    #[default]
    Insert,
    /// Characters overtype (replace) at the cursor position.
    Overtype,
    /// Command mode — keys map to Ludwig commands.
    Command,
}
