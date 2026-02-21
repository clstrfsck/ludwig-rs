//! Terminal abstraction layer.
//!
//! Provides a `Terminal` trait for screen-mode I/O and two implementations:
//! - `CrosstermTerminal` for real terminal interaction
//! - `MockTerminal` for testing

use std::io::Write;

use anyhow::Result;
use crossterm::event::{Event, KeyEvent};

/// Terminal dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TermSize {
    pub width: u16,
    pub height: u16,
}

/// Abstraction over terminal operations.
pub trait Terminal {
    /// Enter raw mode and prepare the terminal.
    fn init(&mut self) -> Result<()>;

    /// Restore the terminal to its original state.
    fn cleanup(&mut self) -> Result<()>;

    /// Get the current terminal dimensions.
    fn size(&self) -> TermSize;

    /// Move the cursor to (col, row), both 0-based.
    fn move_cursor(&mut self, col: u16, row: u16);

    /// Write a string at the current cursor position.
    fn write_str(&mut self, s: &str);

    /// Write a single character at the current cursor position.
    fn write_char(&mut self, ch: char);

    /// Clear from cursor to end of line.
    fn clear_eol(&mut self);

    /// Clear the entire screen.
    fn clear_screen(&mut self);

    /// Scroll the screen up by n lines (content moves up, new blank lines at bottom).
    fn scroll_up(&mut self, n: u16);

    /// Scroll the screen down by n lines (content moves down, new blank lines at top).
    fn scroll_down(&mut self, n: u16);

    /// Sound the terminal bell.
    fn beep(&mut self);

    /// Flush output to the terminal.
    fn flush(&mut self);

    /// Block until a key event is received.
    fn read_key(&mut self) -> Result<KeyEvent>;

    /// Set the scroll region (top_row..=bottom_row inclusive, 0-based).
    fn set_scroll_region(&mut self, top: u16, bottom: u16);

    /// Reset the scroll region to the full terminal height.
    fn reset_scroll_region(&mut self);
}

/// Real terminal using crossterm.
pub struct CrosstermTerminal {
    size: TermSize,
    cursor_visible: bool,
}

impl Default for CrosstermTerminal {
    fn default() -> Self {
        Self::new()
    }
}

impl CrosstermTerminal {
    pub fn new() -> Self {
        let (w, h) = crossterm::terminal::size().unwrap_or((80, 24));
        Self {
            size: TermSize {
                width: w,
                height: h,
            },
            cursor_visible: true,
        }
    }

    fn cursor(&mut self, show: bool) {
        if show && !self.cursor_visible {
            crossterm::execute!(std::io::stdout(), crossterm::cursor::Show,).ok();
            self.cursor_visible = true;
        } else if !show && self.cursor_visible {
            crossterm::execute!(std::io::stdout(), crossterm::cursor::Hide,).ok();
            self.cursor_visible = false;
        }
    }
}

impl Terminal for CrosstermTerminal {
    fn init(&mut self) -> Result<()> {
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen,)?;
        self.cursor(false);
        let (w, h) = crossterm::terminal::size()?;
        self.size = TermSize {
            width: w,
            height: h,
        };
        Ok(())
    }

    fn cleanup(&mut self) -> Result<()> {
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        )?;
        crossterm::terminal::disable_raw_mode()?;
        Ok(())
    }

    fn size(&self) -> TermSize {
        self.size
    }

    fn move_cursor(&mut self, col: u16, row: u16) {
        crossterm::execute!(std::io::stdout(), crossterm::cursor::MoveTo(col, row)).ok();
    }

    fn write_str(&mut self, s: &str) {
        // We use the heuristic of hiding the cursor when writing text, but not
        // e.g. when moving the cursor.  This gives reasonable behaviour on
        // MacOS, which I'm using.  Other systems may need a different approach.
        self.cursor(false);
        crossterm::execute!(std::io::stdout(), crossterm::style::Print(s)).ok();
    }

    fn write_char(&mut self, ch: char) {
        // We use the heuristic of hiding the cursor when writing text, but not
        // e.g. when moving the cursor.  This gives reasonable behaviour on
        // MacOS, which I'm using.  Other systems may need a different approach.
        self.cursor(false);
        crossterm::execute!(std::io::stdout(), crossterm::style::Print(ch)).ok();
    }

    fn clear_eol(&mut self) {
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::UntilNewLine)
        )
        .ok();
    }

    fn clear_screen(&mut self) {
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
        )
        .ok();
    }

    fn scroll_up(&mut self, n: u16) {
        crossterm::execute!(std::io::stdout(), crossterm::terminal::ScrollUp(n)).ok();
    }

    fn scroll_down(&mut self, n: u16) {
        crossterm::execute!(std::io::stdout(), crossterm::terminal::ScrollDown(n)).ok();
    }

    fn beep(&mut self) {
        crossterm::execute!(std::io::stdout(), crossterm::style::Print('\x07')).ok();
    }

    fn flush(&mut self) {
        std::io::stdout().flush().ok();
    }

    fn read_key(&mut self) -> Result<KeyEvent> {
        self.cursor(true);
        loop {
            match crossterm::event::read()? {
                Event::Key(key) => return Ok(key),
                Event::Resize(w, h) => {
                    self.size = TermSize {
                        width: w,
                        height: h,
                    };
                    // Resize events are returned as a special key
                    return Ok(KeyEvent::new(
                        crossterm::event::KeyCode::F(63),
                        crossterm::event::KeyModifiers::NONE,
                    ));
                }
                _ => {} // Ignore mouse events etc.
            }
        }
    }

    fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        // Use CSI directly: ESC[top;bottomr (1-based)
        crossterm::execute!(
            std::io::stdout(),
            crossterm::style::Print(format!("\x1b[{};{}r", top + 1, bottom + 1))
        )
        .ok();
    }

    fn reset_scroll_region(&mut self) {
        crossterm::execute!(
            std::io::stdout(),
            crossterm::style::Print(format!("\x1b[1;{}r", self.size.height))
        )
        .ok();
    }
}

/// Mock terminal for testing — records all operations.
#[cfg(test)]
pub struct MockTerminal {
    pub size: TermSize,
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub ops: Vec<MockOp>,
    pub key_queue: Vec<KeyEvent>,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub enum MockOp {
    Init,
    Cleanup,
    MoveCursor(u16, u16),
    WriteStr(String),
    WriteChar(char),
    ClearEol,
    ClearScreen,
    ScrollUp(u16),
    ScrollDown(u16),
    Beep,
    Flush,
    SetScrollRegion(u16, u16),
    ResetScrollRegion,
}

#[cfg(test)]
impl MockTerminal {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            size: TermSize { width, height },
            cursor_col: 0,
            cursor_row: 0,
            ops: Vec::new(),
            key_queue: Vec::new(),
        }
    }

    pub fn push_key(&mut self, key: KeyEvent) {
        self.key_queue.push(key);
    }
}

#[cfg(test)]
impl Terminal for MockTerminal {
    fn init(&mut self) -> Result<()> {
        self.ops.push(MockOp::Init);
        Ok(())
    }

    fn cleanup(&mut self) -> Result<()> {
        self.ops.push(MockOp::Cleanup);
        Ok(())
    }

    fn size(&self) -> TermSize {
        self.size
    }

    fn move_cursor(&mut self, col: u16, row: u16) {
        self.cursor_col = col;
        self.cursor_row = row;
        self.ops.push(MockOp::MoveCursor(col, row));
    }

    fn write_str(&mut self, s: &str) {
        self.ops.push(MockOp::WriteStr(s.to_string()));
    }

    fn write_char(&mut self, ch: char) {
        self.ops.push(MockOp::WriteChar(ch));
    }

    fn clear_eol(&mut self) {
        self.ops.push(MockOp::ClearEol);
    }

    fn clear_screen(&mut self) {
        self.ops.push(MockOp::ClearScreen);
    }

    fn scroll_up(&mut self, n: u16) {
        self.ops.push(MockOp::ScrollUp(n));
    }

    fn scroll_down(&mut self, n: u16) {
        self.ops.push(MockOp::ScrollDown(n));
    }

    fn beep(&mut self) {
        self.ops.push(MockOp::Beep);
    }

    fn flush(&mut self) {
        self.ops.push(MockOp::Flush);
    }

    fn read_key(&mut self) -> Result<KeyEvent> {
        if self.key_queue.is_empty() {
            anyhow::bail!("No more keys in mock queue");
        }
        Ok(self.key_queue.remove(0))
    }

    fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        self.ops.push(MockOp::SetScrollRegion(top, bottom));
    }

    fn reset_scroll_region(&mut self) {
        self.ops.push(MockOp::ResetScrollRegion);
    }
}
