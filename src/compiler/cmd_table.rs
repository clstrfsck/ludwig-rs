//! Command registry: name-to-opcode table and leading-parameter validation.

use anyhow::Result;
use phf::{Map, phf_map};

use crate::code::CmdOp;

macro_rules! lead_param_mask {
    ($($kind:ident),* $(,)?) => {
        {
            0u8 $(| (1u8 << (LeadParamKind::$kind as u8)))*
        }
    };
}

/// Which kinds of leading parameter are accepted (used for validation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LeadParamKind {
    None = 0,
    Plus = 1,
    Minus = 2,
    Pint = 3,
    Nint = 4,
    Pindef = 5,
    Nindef = 6,
    Marker = 7,
}

pub(super) struct CmdInfo {
    pub(super) op: CmdOp,
    pub(super) allowed_leads: u8,
    pub(super) tpar_count: u8,
}

impl CmdInfo {
    pub(super) fn allows_lead(&self, kind: LeadParamKind) -> bool {
        (self.allowed_leads & (1u8 << (kind as u8))) != 0
    }
}

/// Map of command names to their `CmdOp` and parameter requirements.
/// Keep names sorted alphabetically for readability.
const NAME_TO_OP_MAP: Map<&'static str, CmdInfo> = phf_map! {
    "a" => CmdInfo {
        op: CmdOp::Advance,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "c" => CmdInfo {
        op: CmdOp::InsertChar,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 0
    },
    "d" => CmdInfo {
        op: CmdOp::DeleteChar,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "g" => CmdInfo {
        op: CmdOp::Get,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 1
    },
    "i" => CmdInfo {
        op: CmdOp::InsertText,
        allowed_leads: lead_param_mask!(None, Plus, Pint),
        tpar_count: 1
    },
    "j" => CmdInfo {
        op: CmdOp::Jump,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "l" => CmdInfo {
        op: CmdOp::InsertLine,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 0
    },
    "br" => CmdInfo {
        op: CmdOp::Bridge,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 1
    },
    "fb" => CmdInfo {
        op: CmdOp::FileRewind,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 0
    },
    "fe" => CmdInfo {
        op: CmdOp::FileEdit,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 1
    },
    "fgb" => CmdInfo {
        op: CmdOp::FileGlobalRewind,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 0
    },
    "fgi" => CmdInfo {
        op: CmdOp::FileGlobalInput,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 1
    },
    "fgk" => CmdInfo {
        op: CmdOp::FileGlobalKill,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 0
    },
    "fgo" => CmdInfo {
        op: CmdOp::FileGlobalOutput,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 1
    },
    "fgr" => CmdInfo {
        op: CmdOp::FileRead,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "fgw" => CmdInfo {
        op: CmdOp::FileWrite,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "fi" => CmdInfo {
        op: CmdOp::FileInput,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 1
    },
    "fk" => CmdInfo {
        op: CmdOp::FileKill,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 0
    },
    "fo" => CmdInfo {
        op: CmdOp::FileOutput,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 1
    },
    "fs" => CmdInfo {
        op: CmdOp::FileSave,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 0
    },
    "ft" => CmdInfo {
        op: CmdOp::FileTable,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 0
    },
    "fx" => CmdInfo {
        op: CmdOp::FileExecute,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 1
    },
    "ed" => CmdInfo {
        op: CmdOp::FrameEdit,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 1
    },
    "ek" => CmdInfo {
        op: CmdOp::FrameKill,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 1
    },
    "en" => CmdInfo {
        op: CmdOp::SpanExecuteNoRecompile,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 1
    },
    "eol" => CmdInfo {
        op: CmdOp::EqualEol,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pindef, Nindef),
        tpar_count: 0
    },
    "ex" => CmdInfo {
        op: CmdOp::SpanExecute,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 1
    },
    "eop" => CmdInfo {
        op: CmdOp::EqualEop,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 0
    },
    "eof" => CmdInfo {
        op: CmdOp::EqualEof,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 0
    },
    "eqc" => CmdInfo {
        op: CmdOp::EqualColumn,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pindef, Nindef),
        tpar_count: 1
    },
    "eqm" => CmdInfo {
        op: CmdOp::EqualMark,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pindef, Nindef),
        tpar_count: 1
    },
    "eqs" => CmdInfo {
        op: CmdOp::EqualString,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pindef, Nindef),
        tpar_count: 1
    },
    "ep" => CmdInfo {
        op: CmdOp::FrameParameters,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 1
    },
    "er" => CmdInfo {
        op: CmdOp::FrameReturn,
        allowed_leads: lead_param_mask!(None, Plus, Pint),
        tpar_count: 0
    },
    "k" => CmdInfo {
        op: CmdOp::DeleteLine,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "m" => CmdInfo {
        op: CmdOp::Mark,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 0
    },
    "n" => CmdInfo {
        op: CmdOp::Next,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint),
        tpar_count: 1
    },
    "o" => CmdInfo {
        op: CmdOp::OvertypeText,
        allowed_leads: lead_param_mask!(None, Plus, Pint),
        tpar_count: 1
    },
    "r" => CmdInfo {
        op: CmdOp::Replace,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 2
    },
    "sa" => CmdInfo {
        op: CmdOp::SpanAssign,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 2
    },
    "sc" => CmdInfo {
        op: CmdOp::SpanCopy,
        allowed_leads: lead_param_mask!(None, Plus, Pint),
        tpar_count: 1
    },
    "sd" => CmdInfo {
        op: CmdOp::SpanDefine,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Marker),
        tpar_count: 1
    },
    "si" => CmdInfo {
        op: CmdOp::SpanIndex,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "sj" => CmdInfo {
        op: CmdOp::SpanJump,
        allowed_leads: lead_param_mask!(None, Plus, Minus),
        tpar_count: 1
    },
    "sr" => CmdInfo {
        op: CmdOp::SpanCompile,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 1
    },
    "st" => CmdInfo {
        op: CmdOp::SpanTransfer,
        allowed_leads: lead_param_mask!(None, Plus),
        tpar_count: 1
    },
    "sw" => CmdInfo {
        op: CmdOp::SwapLine,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "wb" => CmdInfo {
        op: CmdOp::WindowBackward,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "we" => CmdInfo {
        op: CmdOp::WindowEnd,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "wf" => CmdInfo {
        op: CmdOp::WindowForward,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "wl" => CmdInfo {
        op: CmdOp::WindowLeft,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "wm" => CmdInfo {
        op: CmdOp::WindowMiddle,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "wn" => CmdInfo {
        op: CmdOp::WindowNew,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "wr" => CmdInfo {
        op: CmdOp::WindowRight,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "wt" => CmdInfo {
        op: CmdOp::WindowTop,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "sl" => CmdInfo {
        op: CmdOp::SplitLine,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "ya" => CmdInfo {
        op: CmdOp::WordAdvance,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "yd" => CmdInfo {
        op: CmdOp::WordDelete,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "yc" => CmdInfo {
        op: CmdOp::LineCentre,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "yf" => CmdInfo {
        op: CmdOp::LineFill,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "yj" => CmdInfo {
        op: CmdOp::LineJustify,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "yl" => CmdInfo {
        op: CmdOp::LineLeft,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "yr" => CmdInfo {
        op: CmdOp::LineRight,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "ys" => CmdInfo {
        op: CmdOp::LineSquash,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "xa" => CmdInfo {
        op: CmdOp::ExitAbort,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "xf" => CmdInfo {
        op: CmdOp::ExitFailure,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "xs" => CmdInfo {
        op: CmdOp::ExitSuccess,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "zd" => CmdInfo {
        op: CmdOp::Down,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "zl" => CmdInfo {
        op: CmdOp::Left,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "zr" => CmdInfo {
        op: CmdOp::Right,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "zc" => CmdInfo {
        op: CmdOp::Return,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef, Marker),
        tpar_count: 0
    },
    "zu" => CmdInfo {
        op: CmdOp::Up,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "zz" => CmdInfo {
        op: CmdOp::Rubout,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "\"" => CmdInfo {
        op: CmdOp::DittoUp,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "'" => CmdInfo {
        op: CmdOp::DittoDown,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "{" => CmdInfo {
        op: CmdOp::SetMarginLeft,
        allowed_leads: lead_param_mask!(None, Minus),
        tpar_count: 0
    },
    "}" => CmdInfo {
        op: CmdOp::SetMarginRight,
        allowed_leads: lead_param_mask!(None, Minus),
        tpar_count: 0
    },
    "*e" => CmdInfo {
        op: CmdOp::CaseEdit,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "*l" => CmdInfo {
        op: CmdOp::CaseLow,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "*u" => CmdInfo {
        op: CmdOp::CaseUp,
        allowed_leads: lead_param_mask!(None, Plus, Minus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "?" => CmdInfo {
        op: CmdOp::InsertInvisible,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Pindef),
        tpar_count: 0
    },
    "^" => CmdInfo {
        op: CmdOp::ExecuteString,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 1
    },
    "q" => CmdInfo {
        op: CmdOp::Quit,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "uc" => CmdInfo {
        op: CmdOp::UserCommandIntroducer,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "v" => CmdInfo {
        op: CmdOp::Verify,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 1
    },
    "wh" => CmdInfo {
        op: CmdOp::WindowSetHeight,
        allowed_leads: lead_param_mask!(None, Plus, Pint),
        tpar_count: 0
    },
    "ws" => CmdInfo {
        op: CmdOp::WindowScroll,
        allowed_leads: lead_param_mask!(None, Plus, Pint, Nint, Pindef, Nindef),
        tpar_count: 0
    },
    "wu" => CmdInfo {
        op: CmdOp::WindowUpdate,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "zb" => CmdInfo {
        op: CmdOp::Backtab,
        allowed_leads: lead_param_mask!(None, Plus, Pint),
        tpar_count: 0
    },
    "zh" => CmdInfo {
        op: CmdOp::Home,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "zt" => CmdInfo {
        op: CmdOp::Tab,
        allowed_leads: lead_param_mask!(None, Plus, Pint),
        tpar_count: 0
    },
    "uk" => CmdInfo {
        op: CmdOp::UserKey,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 2
    },
    "up" => CmdInfo {
        op: CmdOp::UserParent,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
    "us" => CmdInfo {
        op: CmdOp::UserSubprocess,
        allowed_leads: lead_param_mask!(None),
        tpar_count: 0
    },
};

/// Map a command name string to its [`CmdInfo`].
pub(super) fn name_to_info(name: &str) -> Result<&'static CmdInfo> {
    NAME_TO_OP_MAP
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("Syntax error: unknown command '{}'.", name.to_uppercase()))
}

/// Check if a character is valid in a command name.
pub(super) fn is_command_char(ch: char) -> bool {
    matches!(ch, '\\' | '"' | '\'' | '*' | '{' | '}' | '?' | '^') || ch.is_ascii_alphabetic()
}
