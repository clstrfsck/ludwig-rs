//! A Rope-based text editor frame with virtual space support and marks.

pub mod app;
pub mod frame_set;
pub mod save;
pub mod screen;
pub mod terminal;

mod cell_buffer;
mod cmd_result;
mod code;
mod compiler;
mod exec_context;
mod file_io;
mod frame;
mod interpreter;
mod keybind;
mod lead_param;
mod marks;
mod pattern;
mod position;
mod span;
mod trail_param;
mod viewport;

pub use code::ExecOutcome;
pub use compiler::compile;
