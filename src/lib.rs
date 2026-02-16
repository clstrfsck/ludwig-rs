//! A Rope-based text editor frame with virtual space support and marks.
//!
//! # Example
//!
//! ```rust
//! use ludwig::{EditCommands, Frame, LeadParam, MotionCommands, Position, TrailParam};
//!
//! let mut frame: Frame = Frame::from_str("hello world");
//!
//! // Move cursor to virtual space (beyond line end)
//! frame.set_dot(Position::new(0, 20));
//!
//! // Insert text - line is automatically padded
//! frame.cmd_insert_text(LeadParam::None, &TrailParam::from_str("!"));
//!
//! assert_eq!(frame.to_string(), "hello world         !");
//!
//! // Overtype text
//! frame.set_dot(Position::new(0, 6));
//! frame.cmd_overtype_text(LeadParam::None, &TrailParam::from_str("universe|"));
//!
//! assert_eq!(frame.to_string(), "hello universe|     !");
//! ```

mod cmd_result;
pub mod code;
pub mod compiler;
mod editor;
mod frame;
mod interpreter;
mod lead_param;
mod marks;
mod position;
mod trail_param;

pub use cmd_result::{CmdFailure, CmdResult};
pub use code::{CompiledCode, ExecOutcome};
pub use compiler::compile;
pub use editor::Editor;
pub use frame::{CaseMode, EditCommands, Frame, MotionCommands};
pub use lead_param::LeadParam;
pub use marks::{MarkId, MarkSet};
pub use position::{Position, line_length_excluding_newline};
pub use trail_param::TrailParam;
