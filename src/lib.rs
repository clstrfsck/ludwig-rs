//! A Rope-based text editor frame with virtual space support and marks.
//!
//! # Example
//!
//! ```rust
//! use ludwig::{EditCommands, Frame, LeadParam, MotionCommands, Position, TrailParam};
//!
//! // Note that a final newline is automatically added if missing
//! let mut frame: Frame = Frame::from_str("hello world");
//!
//! // Move cursor to virtual space (beyond line end)
//! frame.set_dot(Position::new(0, 20));
//!
//! // Insert text - line is automatically padded
//! frame.cmd_insert_text(LeadParam::None, &TrailParam::from_str("!"));
//!
//! assert_eq!(frame.to_string(), "hello world         !\n");
//!
//! // Overtype text
//! frame.set_dot(Position::new(0, 6));
//! frame.cmd_overtype_text(LeadParam::None, &TrailParam::from_str("universe|"));
//!
//! assert_eq!(frame.to_string(), "hello universe|     !\n");
//! ```

pub mod app;
mod cmd_result;
pub mod code;
pub mod compiler;
pub mod edit_mode;
mod editor;
mod frame;
mod interpreter;
pub mod keybind;
mod lead_param;
mod marks;
mod position;
pub mod screen;
pub mod terminal;
mod trail_param;
pub mod viewport;

pub use cmd_result::{CmdFailure, CmdResult};
pub use code::{CompiledCode, ExecOutcome};
pub use compiler::compile;
pub use editor::Editor;
pub use frame::{CaseMode, EditCommands, Frame, MotionCommands, PredicateCommands, SearchCommands, WordCommands};
pub use lead_param::LeadParam;
pub use marks::{MarkId, MarkSet};
pub use position::{Position, line_length_excluding_newline};
pub use trail_param::TrailParam;
