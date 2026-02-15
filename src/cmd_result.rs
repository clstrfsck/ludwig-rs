/// The result of executing a Ludwig command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdResult {
    Success,
    Failure(CmdFailure),
}

/// The reason a command failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdFailure {
    /// Not implemeneted yet, or doesn't exist
    NotImplemented,
    /// Movement or deletion past frame boundaries.
    OutOfRange,
    /// A mark referenced by the command is not set.
    MarkNotDefined,
    /// The leading parameter is not valid for this command.
    SyntaxError,
}

impl CmdResult {
    pub fn is_success(&self) -> bool {
        matches!(self, CmdResult::Success)
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, CmdResult::Failure(_))
    }
}
