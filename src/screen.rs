//! Screen rendering for interactive mode.
//!
//! The Screen reads Frame state through public accessors and uses a Terminal
//! to render the visible portion of text. It uses a double-buffered cell grid:
//! render into `next`, diff against `current`, emit only changed cells, then swap.

use crate::cell_buffer::CellBuffer;
use crate::frame::Frame;
use crate::terminal::{TermSize, Terminal};
use crate::viewport::{FixupAction, Viewport, ViewportParams};

/// Manages screen rendering.
pub struct Screen {
    /// The viewport tracking visible region.
    pub viewport: Viewport,
    /// What is currently on the terminal screen.
    current: CellBuffer,
    /// What we are rendering into before diffing.
    next: CellBuffer,
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
            current: CellBuffer::new(width, height),
            next: CellBuffer::new(width, height),
            msg_rows: 0,
        }
    }

    /// Resize the screen (e.g. on terminal resize).
    pub fn resize(&mut self, term_size: TermSize) {
        let height = term_size.height as usize;
        let width = term_size.width as usize;
        self.viewport.resize(height, width);
        self.current.resize(width, height);
        self.next.resize(width, height);
        self.invalidate();
    }

    /// Invalidate the screen, forcing a full redraw on next diff.
    pub fn invalidate(&mut self) {
        self.current.clear();
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
        let action = self
            .viewport
            .compute_fixup(dot.line, dot.column, line_count);
        self.viewport.params.height = saved_height;

        match &action {
            FixupAction::None => {}
            FixupAction::ScrollV(n) => {
                self.viewport.apply_fixup(&action);
                self.scroll_terminal(terminal, *n);
            }
            FixupAction::SlideH(_) => {
                self.viewport.apply_fixup(&action);
            }
            FixupAction::ScrollAndSlide { .. } => {
                self.viewport.apply_fixup(&action);
            }
            FixupAction::Redraw => {
                self.viewport.center_on(dot.line, dot.column, line_count);
            }
        }

        // Render into next buffer, diff, and swap
        self.render_and_flush(frame, terminal);

        // Position the cursor at dot
        self.position_cursor(frame, terminal);
        terminal.flush();
    }

    /// Full screen redraw: invalidate, render, diff, and flush.
    pub fn redraw(&mut self, frame: &Frame, terminal: &mut dyn Terminal) {
        self.render_and_flush(frame, terminal);
    }

    /// Render all visible lines into `next`, diff against `current`, emit changes, swap.
    fn render_and_flush(&mut self, frame: &Frame, terminal: &mut dyn Terminal) {
        let text_height = self.text_height();
        self.next.clear();
        for row in 0..text_height {
            self.render_line(frame, row);
        }
        // Preserve message rows in next buffer from current (messages are managed separately)
        let height = self.viewport.params.height;
        for row in text_height..height {
            self.next.copy_row_from(row, &self.current, row);
        }
        CellBuffer::diff(&self.current, &self.next, terminal);
        std::mem::swap(&mut self.current, &mut self.next);
    }

    /// Render a single screen row from the frame into the `next` buffer.
    fn render_line(&mut self, frame: &Frame, screen_row: usize) {
        let frame_line = self.viewport.top_line + screen_row;
        let offset = self.viewport.offset;
        let width = self.viewport.params.width;

        let content = self.build_line_content(frame, frame_line, offset, width);
        self.next.write_str(0, screen_row, &content);
    }

    /// Scroll the physical terminal and shift `current` to match.
    /// After this, `current` accurately reflects what's on the terminal,
    /// so the subsequent `render_and_flush` diff only emits newly revealed rows.
    fn scroll_terminal(&mut self, terminal: &mut dyn Terminal, amount: i32) {
        let text_height = self.text_height();

        if amount.unsigned_abs() as usize >= text_height {
            // Scroll exceeds screen — no point in terminal scroll, full diff will handle it
            return;
        }

        // Constrain scroll region to the text area if messages are visible
        if self.msg_rows > 0 {
            terminal.set_scroll_region(0, (text_height - 1) as u16);
        }

        if amount > 0 {
            terminal.scroll_up(amount as u16);
        } else {
            terminal.scroll_down((-amount) as u16);
        }

        if self.msg_rows > 0 {
            terminal.reset_scroll_region();
        }

        // Shift `current` the same way so it matches the terminal
        self.current.shift_rows(0, text_height, amount);
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
        let width = self.viewport.params.width;
        let row = height - 1;
        self.msg_rows = 1;

        // Write message into next buffer, diff, swap
        self.next.clear_row(row);
        self.next.write_str(0, row, &msg[..msg.len().min(width)]);
        // Copy all other rows from current
        for r in 0..row {
            self.next.copy_row_from(r, &self.current, r);
        }
        CellBuffer::diff(&self.current, &self.next, terminal);
        std::mem::swap(&mut self.current, &mut self.next);
        terminal.flush();
    }

    /// Update the message row content and position cursor at a given column.
    /// Used by command_input to keep the prompt line in sync with the cell buffer.
    pub fn update_message_row(
        &mut self,
        terminal: &mut dyn Terminal,
        content: &str,
        cursor_col: usize,
    ) {
        let height = self.viewport.params.height;
        let width = self.viewport.params.width;
        let row = height - 1;

        self.next.clear_row(row);
        self.next
            .write_str(0, row, &content[..content.len().min(width)]);
        // Copy all other rows from current
        for r in 0..row {
            self.next.copy_row_from(r, &self.current, r);
        }
        CellBuffer::diff(&self.current, &self.next, terminal);
        std::mem::swap(&mut self.current, &mut self.next);
        terminal.move_cursor(cursor_col as u16, row as u16);
        terminal.flush();
    }

    /// Return the screen row used for messages (bottom row).
    pub fn message_row(&self) -> u16 {
        (self.viewport.params.height - 1) as u16
    }

    /// Clear the message area. The next render_and_flush will overwrite the
    /// prompt on screen because `current` still holds the prompt text — the
    /// diff against the freshly rendered frame content emits every differing
    /// cell, including trailing spaces that erase leftover prompt characters.
    pub fn clear_message(&mut self, _frame: &Frame, _terminal: &mut dyn Terminal) {
        self.msg_rows = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{MockOp, MockTerminal, TermSize};

    #[test]
    fn test_screen_new() {
        let screen = Screen::new(TermSize {
            width: 80,
            height: 24,
        });
        assert_eq!(screen.viewport.top_line, 0);
        assert_eq!(screen.viewport.offset, 0);
        assert_eq!(screen.current.width(), 80);
        assert_eq!(screen.current.height(), 24);
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

        // Invalidate so everything differs
        screen.invalidate();
        screen.redraw(&frame, &mut term);

        // Should have written content for 3 lines + EOF marker line
        // (line 4 is blank past EOF, same as cleared buffer)
        let write_ops: Vec<_> = term
            .ops
            .iter()
            .filter(|op| matches!(op, MockOp::WriteStr(_)))
            .collect();
        // At least the 3 content lines + EOF marker should have been written
        assert!(
            write_ops.len() >= 4,
            "Expected at least 4 writes, got: {:?}",
            write_ops
        );
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

        // Check that terminal ops include the message content
        assert!(
            term.ops
                .iter()
                .any(|op| matches!(op, MockOp::WriteStr(s) if s.contains("Test message")))
        );
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

    #[test]
    fn test_redraw_no_change_second_time() {
        let mut screen = Screen::new(TermSize {
            width: 80,
            height: 5,
        });
        let frame = Frame::from_str("line1\nline2\nline3");
        let mut term = MockTerminal::new(80, 5);

        // First redraw after invalidate
        screen.invalidate();
        screen.redraw(&frame, &mut term);

        // Second redraw without changes — should produce no terminal ops
        term.ops.clear();
        screen.redraw(&frame, &mut term);
        let write_ops: Vec<_> = term
            .ops
            .iter()
            .filter(|op| matches!(op, MockOp::WriteStr(_)))
            .collect();
        assert_eq!(
            write_ops.len(),
            0,
            "Expected no writes on unchanged redraw, got: {:?}",
            write_ops
        );
    }

    #[test]
    fn test_scroll_up_uses_terminal_scroll() {
        let mut screen = Screen::new(TermSize {
            width: 20,
            height: 5,
        });
        // 10 lines so we can scroll
        let text: String = (0..10).map(|i| format!("line{}\n", i)).collect();
        let mut frame = Frame::from_str(&text);
        let mut term = MockTerminal::new(20, 5);

        // Initial render to populate current buffer
        screen.invalidate();
        screen.fixup(&frame, &mut term);
        term.ops.clear();

        // Move dot below visible area to trigger scroll
        frame.set_dot(crate::Position::new(5, 0));
        screen.fixup(&frame, &mut term);

        // Should have used terminal scroll_up
        assert!(
            term.ops.iter().any(|op| matches!(op, MockOp::ScrollUp(_))),
            "Expected ScrollUp in ops: {:?}",
            term.ops
        );
    }

    #[test]
    fn test_scroll_down_uses_terminal_scroll() {
        let mut screen = Screen::new(TermSize {
            width: 20,
            height: 5,
        });
        let text: String = (0..10).map(|i| format!("line{}\n", i)).collect();
        let mut frame = Frame::from_str(&text);
        let mut term = MockTerminal::new(20, 5);

        // Position viewport in the middle
        frame.set_dot(crate::Position::new(5, 0));
        screen.fixup(&frame, &mut term);
        term.ops.clear();

        // Move dot above visible area to trigger scroll down
        frame.set_dot(crate::Position::new(0, 0));
        screen.fixup(&frame, &mut term);

        assert!(
            term.ops
                .iter()
                .any(|op| matches!(op, MockOp::ScrollDown(_))),
            "Expected ScrollDown in ops: {:?}",
            term.ops
        );
    }

    #[test]
    fn test_scroll_only_writes_new_rows() {
        let mut screen = Screen::new(TermSize {
            width: 20,
            height: 5,
        });
        // Use distinct line content so we can identify what was written
        let text: String = (0..10).map(|i| format!("LINE-{:02}\n", i)).collect();
        let mut frame = Frame::from_str(&text);
        let mut term = MockTerminal::new(20, 5);

        // Initial render: shows lines 0-4
        screen.invalidate();
        screen.fixup(&frame, &mut term);
        term.ops.clear();

        // Move dot to line 5 (just past bottom). With v_margin=1 (height=5),
        // the fixup scrolls by delta+margin = 1+1 = 2 lines.
        // Viewport moves from lines 0-4 to lines 2-6.
        // Terminal scroll shifts 2 rows, revealing 2 new rows at the bottom.
        frame.set_dot(crate::Position::new(5, 0));
        screen.fixup(&frame, &mut term);

        // Only the newly revealed lines should be written, not the
        // lines which were already on screen (just shifted by terminal scroll)
        let write_ops: Vec<_> = term
            .ops
            .iter()
            .filter_map(|op| match op {
                MockOp::WriteStr(s) => Some(s.clone()),
                _ => None,
            })
            .collect();

        // 2 newly revealed lines: LINE-05 and LINE-06 (due to margin)
        assert_eq!(
            write_ops.len(),
            2,
            "Expected 2 writes for new rows, got: {:?}",
            write_ops
        );
        assert!(
            write_ops[0].contains("LINE-05"),
            "Expected newly revealed line, got: {:?}",
            write_ops
        );
        assert!(
            write_ops[1].contains("LINE-06"),
            "Expected newly revealed line, got: {:?}",
            write_ops
        );
    }
}
