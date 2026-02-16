//! Tree-based intermediate representation for compiled Ludwig commands.

use crate::lead_param::LeadParam;
use crate::trail_param::TrailParam;

/// A compiled sequence of Ludwig commands.
#[derive(Debug, Clone)]
pub struct CompiledCode {
    pub instructions: Vec<Instruction>,
}

/// A single compiled instruction.
#[derive(Debug, Clone)]
pub enum Instruction {
    /// A primitive command (A, J, D, I, O, C, L, SL, etc.)
    SimpleCmd {
        op: CmdOp,
        lead: LeadParam,
        tpar: Option<TrailParam>,
        exit_handler: Option<ExitHandler>,
    },
    /// A parenthesized group: (cmds), N(cmds), >(cmds)
    CompoundCmd {
        repeat: RepeatCount,
        body: CompiledCode,
        exit_handler: Option<ExitHandler>,
    },
    /// XS / NXS — exit N nesting levels with success
    ExitSuccess(ExitLevels),
    /// XF / NXF — exit N nesting levels with failure
    ExitFailure(ExitLevels),
    /// XA — abort all execution
    ExitAbort,
}

/// Exit handler: code to run on success and/or failure of a command.
#[derive(Debug, Clone)]
pub struct ExitHandler {
    pub on_success: Option<CompiledCode>,
    pub on_failure: Option<CompiledCode>,
}

/// How many times to repeat a compound command body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatCount {
    /// (cmds) — execute once
    Once,
    /// N(cmds) — execute N times, fail if body fails
    Times(usize),
    /// >(cmds) — repeat until body fails
    Indefinite,
}

/// How many nesting levels to exit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitLevels {
    /// NXS / NXF (1 for plain XS/XF)
    Count(usize),
    /// >XS / >XF — exit all levels
    All,
}

/// Opcode identifying a primitive command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdOp {
    Noop,

    // Cursor movement
    Up,
    Down,
    Left,
    Right,
    Home,
    Return,
    Tab,
    Backtab,

    Rubout,
    Jump,
    Advance,
    PositionColumn,
    PositionLine,
    OpSysCommand,

    // Window control
    WindowForward,
    WindowBackward,
    WindowLeft,
    WindowRight,
    WindowScroll,
    WindowTop,
    WindowEnd,
    WindowNew,
    WindowMiddle,
    WindowSetHeight,
    WindowUpdate,

    // Search and comparison
    Get,
    Next,
    Bridge,
    Replace,
    EqualString,
    EqualColumn,
    EqualMark,
    EqualEol,
    EqualEop,
    EqualEof,

    OvertypeMode,
    InsertMode,

    // Text insertion/deletion
    OvertypeText,
    InsertText,
    TypeText,
    InsertLine,
    InsertChar,
    InsertInvisible,
    DeleteLine,
    DeleteChar,

    // Text manipulation
    SwapLine,
    SplitLine,
    DittoUp,
    DittoDown,
    CaseUp,
    CaseLow,
    CaseEdit,
    SetMarginLeft,
    SetMarginRight,

    // Word processing
    LineFill,
    LineJustify,
    LineSquash,
    LineCentre,
    LineLeft,
    LineRight,
    WordAdvance,
    WordDelete,
    AdvanceParagraph,
    DeleteParagraph,

    // Span commands
    SpanDefine,
    SpanTransfer,
    SpanCopy,
    SpanCompile,
    SpanJump,
    SpanIndex,
    SpanAssign,

    // Block commands
    BlockDefine,
    BlockTransfer,
    BlockCopy,

    // Frame commands
    FrameKill,
    FrameEdit,
    FrameReturn,
    SpanExecute,
    SpanExecuteNoRecompile,
    FrameParameters,

    // File commands
    FileInput,
    FileOutput,
    FileEdit,
    FileRead,
    FileWrite,
    FileClose,
    FileRewind,
    FileKill,
    FileExecute,
    FileSave,
    FileTable,
    FileGlobalInput,
    FileGlobalOutput,
    FileGlobalRewind,
    FileGlobalKill,

    UserCommandIntroducer,
    UserKey,
    UserParent,
    UserSubprocess,
    UserUndo,
    UserLearn,
    UserRecall,

    ResizeWindow,

    // Miscellaneous
    Help,
    Verify,
    Command,
    Mark,
    Page,
    Quit,
    Dump,
    Validate,
    ExecuteString,
    DoLastCommand,

    Extended,

    ExitAbort,
    ExitFailure,
    ExitSuccess,

    // End of user commands

    // Prefix commands
    PrefixAst,
    PrefixA,
    PrefixB,
    PrefixC,
    PrefixD,
    PrefixE,
    PrefixEo,
    PrefixEq,
    PrefixF,
    PrefixFg,
    PrefixI,
    PrefixK,
    PrefixL,
    PrefixO,
    PrefixP,
    PrefixS,
    PrefixT,
    PrefixTc,
    PrefixTf,
    PrefixU,
    PrefixW,
    PrefixX,
    PrefixY,
    PrefixZ,
    PrefixTilde,

    NoSuch,
}

/// Outcome of executing compiled code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecOutcome {
    Success,
    Failure,
    ExitSuccess { remaining: usize },
    ExitFailure { remaining: usize },
    ExitSuccessAll,
    ExitFailureAll,
    Abort,
}

impl ExecOutcome {
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            ExecOutcome::Success | ExecOutcome::ExitSuccess { .. } | ExecOutcome::ExitSuccessAll
        )
    }

    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            ExecOutcome::Failure | ExecOutcome::ExitFailure { .. } | ExecOutcome::ExitFailureAll
        )
    }
}
