//! Screen rendering for interactive mode.
//!
//! The Screen reads Frame state through public accessors and uses a Terminal
//! to render the visible portion of text. It owns a Viewport for tracking
//! which part of the frame is currently shown.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::frame::Frame;
use crate::terminal::{TermSize, Terminal};
use crate::viewport::{FixupAction, Viewport, ViewportParams};

/// Line cache entry for diff-based rendering.
#[derive(Debug, Clone)]
struct CachedLine {
    hash: u64,
}

/// Manages screen rendering.
pub struct Screen {
    /// The viewport tracking visible region.
    pub viewport: Viewport,
    /// Cached line hashes for each screen row, for diff-based updates.
    line_cache: Vec<Option<CachedLine>>,
    /// Number of bottom rows currently used for messages.
    pub msg_rows: usize,
}

/// Marker text for end of file.
const EOF_MARKER: &str = "<End of File>";

impl Screen {
    pub fn new(term_size: TermSize) -> Self {
        let height = term_size.height as usize;
        let width = term_size.width as usize;
        Self {
            viewport: Viewport::new(ViewportParams::new(height, width)),
            line_cache: vec![None; height],
            msg_rows: 0,
        }
    }

    /// Resize the screen (e.g. on terminal resize).
    pub fn resize(&mut self, term_size: TermSize) {
        let height = term_size.height as usize;
        let width = term_size.width as usize;
        self.viewport.resize(height, width);
        self.line_cache.resize(height, None);
        self.invalidate();
    }

    /// Invalidate the entire line cache, forcing a full redraw on next fixup.
    pub fn invalidate(&mut self) {
        for entry in &mut self.line_cache {
            *entry = None;
        }
    }

    /// Number of usable text rows (total height minus message rows).
    pub fn text_height(&self) -> usize {
        self.viewport.params.height.saturating_sub(self.msg_rows)
    }

    /// Perform a fixup: ensure dot is visible and update the screen.
    pub fn fixup(&mut self, frame: &Frame, terminal: &mut dyn Terminal) {
        let dot = frame.dot();
        let line_count = frame.line_count();
        let text_height = self.text_height();

        // Temporarily adjust viewport height for fixup computation if messages visible
        let saved_height = self.viewport.params.height;
        self.viewport.params.height = text_height;
        let action = self.viewport.compute_fixup(dot.line, dot.column, line_count);
        self.viewport.params.height = saved_height;

        match &action {
            FixupAction::None => {}
            FixupAction::ScrollV(n) => {
                self.viewport.apply_fixup(&action);
                self.scroll_screen(frame, terminal, *n);
            }
            FixupAction::SlideH(_) => {
                self.viewport.apply_fixup(&action);
                // Horizontal slide requires full redraw of all lines
                self.redraw(frame, terminal);
            }
            FixupAction::ScrollAndSlide { .. } => {
                self.viewport.apply_fixup(&action);
                self.redraw(frame, terminal);
            }
            FixupAction::Redraw => {
                self.viewport.center_on(dot.line, dot.column);
                self.redraw(frame, terminal);
            }
        }

        // Position the cursor at dot
        self.position_cursor(frame, terminal);
        terminal.flush();
    }

    /// Full screen redraw.
    pub fn redraw(&mut self, frame: &Frame, terminal: &mut dyn Terminal) {
        let text_height = self.text_height();
        for row in 0..text_height {
            self.draw_line(frame, terminal, row);
        }
    }

    /// Draw a single screen row from the frame.
    fn draw_line(&mut self, frame: &Frame, terminal: &mut dyn Terminal, screen_row: usize) {
        let frame_line = self.viewport.top_line + screen_row;
        let offset = self.viewport.offset;
        let width = self.viewport.params.width;

        // Build the line content string
        let content = self.build_line_content(frame, frame_line, offset, width);

        // Check cache
        let hash = hash_str(&content);
        if let Some(cached) = &self.line_cache[screen_row] {
            if cached.hash == hash {
                return; // Already up to date
            }
        }

        // Write the line
        terminal.move_cursor(0, screen_row as u16);
        terminal.write_str(&content);
        terminal.clear_eol();

        // Update cache
        self.line_cache[screen_row] = Some(CachedLine { hash });
    }

    /// Build the visible portion of a frame line as a string.
    fn build_line_content(
        &self,
        frame: &Frame,
        frame_line: usize,
        offset: usize,
        width: usize,
    ) -> String {
        let line_count = frame.line_count();

        if frame_line >= line_count {
            // Past end of file — show EOF marker on the first line past end, blank after
            if frame_line == line_count {
                let marker = EOF_MARKER;
                if offset < marker.len() {
                    let visible = &marker[offset..];
                    if visible.len() > width {
                        return visible[..width].to_string();
                    }
                    return visible.to_string();
                }
            }
            return String::new();
        }

        // Get the line content
        let line_len = frame.line_len(frame_line);
        if offset >= line_len {
            return String::new(); // Entirely scrolled past
        }

        let slice = frame.line_content(frame_line).unwrap();
        let start_char = offset;
        let end_char = (offset + width).min(line_len);

        // Extract the visible portion
        let mut result = String::with_capacity(width);
        for ch in slice.chars().skip(start_char).take(end_char - start_char) {
            if ch == '\n' || ch == '\r' {
                break;
            }
            if ch == '\t' {
                // Expand tab to spaces (8-col tab stops)
                let col = result.len() + offset;
                let next_tab = ((col / 8) + 1) * 8;
                let spaces = next_tab - col;
                for _ in 0..spaces.min(width - result.len()) {
                    result.push(' ');
                }
            } else if ch.is_control() {
                // Show control chars as ^X
                result.push('^');
                if result.len() < width {
                    result.push((ch as u8 + b'@') as char);
                }
            } else {
                result.push(ch);
            }
            if result.len() >= width {
                break;
            }
        }

        result
    }

    /// Scroll the screen vertically and redraw newly revealed lines.
    fn scroll_screen(&mut self, frame: &Frame, terminal: &mut dyn Terminal, scroll_amount: i32) {
        let text_height = self.text_height();

        if scroll_amount.unsigned_abs() as usize >= text_height {
            // Too much scrolling — just redraw everything
            self.invalidate();
            self.redraw(frame, terminal);
            return;
        }

        if scroll_amount > 0 {
            // Scroll up: content moves up, new lines at bottom
            let n = scroll_amount as usize;

            // Set scroll region to text area only
            if self.msg_rows > 0 {
                terminal.set_scroll_region(0, (text_height - 1) as u16);
            }
            terminal.scroll_up(scroll_amount as u16);
            if self.msg_rows > 0 {
                terminal.reset_scroll_region();
            }

            // Shift cache up
            self.line_cache.drain(..n);
            self.line_cache.resize(self.viewport.params.height, None);

            // Draw newly revealed bottom lines
            for row in (text_height - n)..text_height {
                self.draw_line(frame, terminal, row);
            }
        } else {
            // Scroll down: content moves down, new lines at top
            let n = (-scroll_amount) as usize;

            if self.msg_rows > 0 {
                terminal.set_scroll_region(0, (text_height - 1) as u16);
            }
            terminal.scroll_down(n as u16);
            if self.msg_rows > 0 {
                terminal.reset_scroll_region();
            }

            // Shift cache down
            let total = self.line_cache.len();
            self.line_cache.truncate(total - n);
            for _ in 0..n {
                self.line_cache.insert(0, None);
            }

            // Draw newly revealed top lines
            for row in 0..n {
                self.draw_line(frame, terminal, row);
            }
        }
    }

    /// Position the terminal cursor at the frame's dot position.
    fn position_cursor(&self, frame: &Frame, terminal: &mut dyn Terminal) {
        let dot = frame.dot();
        let col = dot.column.saturating_sub(self.viewport.offset) as u16;
        let row = dot.line.saturating_sub(self.viewport.top_line) as u16;
        terminal.move_cursor(col, row);
    }

    /// Show a message on the bottom line(s) of the screen.
    pub fn show_message(&mut self, terminal: &mut dyn Terminal, msg: &str) {
        let height = self.viewport.params.height;
        let row = (height - 1) as u16;
        terminal.move_cursor(0, row);
        terminal.write_str(msg);
        terminal.clear_eol();
        self.msg_rows = 1;
        terminal.flush();
    }

    /// Clear the message area and restore those lines.
    pub fn clear_message(&mut self, frame: &Frame, terminal: &mut dyn Terminal) {
        if self.msg_rows > 0 {
            self.msg_rows = 0;
            // Invalidate and redraw the previously-covered rows
            let height = self.viewport.params.height;
            for row in (height - 1)..height {
                self.line_cache[row] = None;
                self.draw_line(frame, terminal, row);
            }
        }
    }
}

fn hash_str(s: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{MockTerminal, MockOp, TermSize};

    #[test]
    fn test_screen_new() {
        let screen = Screen::new(TermSize {
            width: 80,
            height: 24,
        });
        assert_eq!(screen.viewport.top_line, 0);
        assert_eq!(screen.viewport.offset, 0);
        assert_eq!(screen.line_cache.len(), 24);
    }

    #[test]
    fn test_build_line_content_normal() {
        let screen = Screen::new(TermSize {
            width: 80,
            height: 24,
        });
        let frame = Frame::from_str("hello world");
        let content = screen.build_line_content(&frame, 0, 0, 80);
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_build_line_content_with_offset() {
        let screen = Screen::new(TermSize {
            width: 80,
            height: 24,
        });
        let frame = Frame::from_str("hello world");
        let content = screen.build_line_content(&frame, 0, 6, 80);
        assert_eq!(content, "world");
    }

    #[test]
    fn test_build_line_content_eof_marker() {
        let screen = Screen::new(TermSize {
            width: 80,
            height: 24,
        });
        let frame = Frame::from_str("hello");
        let lc = frame.line_count();
        // EOF marker appears on the first line past the end
        let content = screen.build_line_content(&frame, lc, 0, 80);
        assert_eq!(content, "<End of File>");
    }

    #[test]
    fn test_build_line_content_past_eof() {
        let screen = Screen::new(TermSize {
            width: 80,
            height: 24,
        });
        let frame = Frame::from_str("hello");
        let content = screen.build_line_content(&frame, 5, 0, 80);
        assert_eq!(content, "");
    }

    #[test]
    fn test_redraw_writes_all_lines() {
        let mut screen = Screen::new(TermSize {
            width: 80,
            height: 5,
        });
        let frame = Frame::from_str("line1\nline2\nline3");
        let mut term = MockTerminal::new(80, 5);

        screen.redraw(&frame, &mut term);

        // Should have drawn 5 rows (3 content + EOF + blank)
        let move_ops: Vec<_> = term
            .ops
            .iter()
            .filter(|op| matches!(op, MockOp::MoveCursor(0, _)))
            .collect();
        assert_eq!(move_ops.len(), 5);
    }

    #[test]
    fn test_fixup_centers_on_redraw() {
        let mut screen = Screen::new(TermSize {
            width: 80,
            height: 10,
        });
        // Create a frame with many lines
        let text: String = (0..100).map(|i| format!("line{}\n", i)).collect();
        let mut frame = Frame::from_str(&text);
        let mut term = MockTerminal::new(80, 10);

        // Move dot to line 50
        frame.set_dot(crate::Position::new(50, 0));
        screen.fixup(&frame, &mut term);

        // Viewport should be centered around line 50
        assert!(screen.viewport.top_line > 0);
        assert!(screen.viewport.top_line <= 50);
    }

    #[test]
    fn test_show_message() {
        let mut screen = Screen::new(TermSize {
            width: 80,
            height: 24,
        });
        let mut term = MockTerminal::new(80, 24);

        screen.show_message(&mut term, "Test message");
        assert_eq!(screen.msg_rows, 1);

        // Check that it wrote to the bottom row
        assert!(term
            .ops
            .contains(&MockOp::MoveCursor(0, 23)));
        assert!(term
            .ops
            .contains(&MockOp::WriteStr("Test message".to_string())));
    }

    #[test]
    fn test_clear_message() {
        let mut screen = Screen::new(TermSize {
            width: 80,
            height: 5,
        });
        let frame = Frame::from_str("hello");
        let mut term = MockTerminal::new(80, 5);

        screen.msg_rows = 1;
        screen.clear_message(&frame, &mut term);
        assert_eq!(screen.msg_rows, 0);
    }
}
