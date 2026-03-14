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
    /// A frame with the given name already exists.
    FrameExists,
    /// ED: a span with the target name already exists.
    SpanOfThatNameExists,
    /// EK: frame is the current frame, a special frame, or has files attached.
    CantKillFrame,
    /// ER: the return-frame chain is empty.
    NoReturnFrame,
    /// FI/FO/FE/FGI/FGO: a file is already open for this frame/global slot.
    FileAlreadyOpen,
    /// FI/FO/FE/FGI/FGO: the specified file does not exist or cannot be opened.
    FileOpenError,
    /// FK/FB/FS/FGR/FGW/FGB/FGK: no file is open for the requested slot.
    FileNotOpen,
    /// FS: frame has no output file open.
    NoOutputFile,
}

impl CmdResult {
    pub fn is_success(&self) -> bool {
        matches!(self, CmdResult::Success)
    }
}
