//! Double-buffered cell grid for efficient screen rendering.
//!
//! Renders frame state into a "next" buffer, diffs cell-by-cell against
//! the "current" buffer, emits only changed cells via the terminal, then swaps.

use crate::terminal::Terminal;

/// A single screen cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
}

impl Default for Cell {
    fn default() -> Self {
        Cell { ch: ' ' }
    }
}

/// A 2D grid of cells representing the terminal screen.
pub struct CellBuffer {
    cells: Vec<Cell>,
    width: usize,
    height: usize,
}

impl CellBuffer {
    /// Create a new buffer filled with spaces.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            cells: vec![Cell::default(); width * height],
            width,
            height,
        }
    }

    /// Resize the buffer, filling with spaces.
    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.cells = vec![Cell::default(); width * height];
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Get the cell at (col, row).
    pub fn get(&self, col: usize, row: usize) -> Cell {
        if col < self.width && row < self.height {
            self.cells[row * self.width + col]
        } else {
            Cell::default()
        }
    }

    /// Set the cell at (col, row).
    pub fn set(&mut self, col: usize, row: usize, cell: Cell) {
        if col < self.width && row < self.height {
            self.cells[row * self.width + col] = cell;
        }
    }

    /// Write a string starting at (col, row), clamped to width.
    pub fn write_str(&mut self, col: usize, row: usize, s: &str) {
        if row >= self.height {
            return;
        }
        let mut c = col;
        for ch in s.chars() {
            if c >= self.width {
                break;
            }
            self.cells[row * self.width + c] = Cell { ch };
            c += 1;
        }
    }

    /// Fill a row with spaces.
    pub fn clear_row(&mut self, row: usize) {
        if row >= self.height {
            return;
        }
        let start = row * self.width;
        for i in start..start + self.width {
            self.cells[i] = Cell::default();
        }
    }

    /// Fill the entire buffer with spaces.
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
    }

    /// Shift rows within a range up (positive) or down (negative) by `amount`.
    /// Newly revealed rows are cleared to spaces.
    /// Only rows in `start_row..end_row` are affected.
    pub fn shift_rows(&mut self, start_row: usize, end_row: usize, amount: i32) {
        let end_row = end_row.min(self.height);
        if start_row >= end_row {
            return;
        }
        let range_height = end_row - start_row;
        let abs = amount.unsigned_abs() as usize;
        if abs >= range_height {
            // Shift larger than range — clear everything in range
            for row in start_row..end_row {
                self.clear_row(row);
            }
            return;
        }

        if amount > 0 {
            // Shift up: row[start] = row[start+n], row[start+1] = row[start+n+1], ...
            // Bottom n rows in range are cleared.
            for row in start_row..end_row - abs {
                let dst = row * self.width;
                let src = (row + abs) * self.width;
                self.cells.copy_within(src..src + self.width, dst);
            }
            for row in end_row - abs..end_row {
                self.clear_row(row);
            }
        } else {
            // Shift down: row[end-1] = row[end-1-n], row[end-2] = row[end-2-n], ...
            // Top n rows in range are cleared.
            for row in (start_row + abs..end_row).rev() {
                let dst = row * self.width;
                let src = (row - abs) * self.width;
                self.cells.copy_within(src..src + self.width, dst);
            }
            for row in start_row..start_row + abs {
                self.clear_row(row);
            }
        }
    }

    /// Copy a row from another buffer into this buffer.
    pub fn copy_row_from(&mut self, row: usize, src: &CellBuffer, src_row: usize) {
        if row >= self.height || src_row >= src.height {
            return;
        }
        let dst_start = row * self.width;
        let src_start = src_row * src.width;
        let copy_width = self.width.min(src.width);
        for i in 0..copy_width {
            self.cells[dst_start + i] = src.cells[src_start + i];
        }
        // Clear remaining columns if dst is wider
        for i in copy_width..self.width {
            self.cells[dst_start + i] = Cell::default();
        }
    }

    /// Diff two buffers and emit only changed cells via the terminal.
    /// Consecutive changes on the same row are coalesced into single write_str calls.
    /// Short gaps of matching cells (up to 4) are included in runs to avoid extra
    /// cursor moves, since a cursor move costs ~6 bytes vs 1 byte per character.
    const GAP_THRESHOLD: usize = 4;

    pub fn diff(current: &CellBuffer, next: &CellBuffer, terminal: &mut dyn Terminal) {
        let height = current.height.min(next.height);
        let width = current.width.min(next.width);

        for row in 0..height {
            let row_base_cur = row * current.width;
            let row_base_next = row * next.width;

            let mut col = 0;
            while col < width {
                // Scan for first differing cell
                if current.cells[row_base_cur + col] == next.cells[row_base_next + col] {
                    col += 1;
                    continue;
                }

                // Found a diff — collect the run, coalescing short gaps of matching cells
                let start_col = col;
                let mut run = String::new();
                loop {
                    // Add differing cells to the run
                    while col < width
                        && current.cells[row_base_cur + col] != next.cells[row_base_next + col]
                    {
                        run.push(next.cells[row_base_next + col].ch);
                        col += 1;
                    }

                    // Check for a short gap of matching cells followed by more diffs
                    let gap_start = col;
                    while col < width
                        && col - gap_start < Self::GAP_THRESHOLD
                        && current.cells[row_base_cur + col] == next.cells[row_base_next + col]
                    {
                        col += 1;
                    }

                    if col < width
                        && col - gap_start < Self::GAP_THRESHOLD
                        && current.cells[row_base_cur + col] != next.cells[row_base_next + col]
                    {
                        // Short gap followed by more diffs — include gap in the run
                        for g in gap_start..col {
                            run.push(next.cells[row_base_next + g].ch);
                        }
                    } else {
                        // Gap is too long or we hit end of row — end the run
                        col = gap_start;
                        break;
                    }
                }

                // Emit
                terminal.move_cursor(start_col as u16, row as u16);
                terminal.write_str(&run);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{MockOp, MockTerminal};

    #[test]
    fn test_new_filled_with_spaces() {
        let buf = CellBuffer::new(10, 5);
        assert_eq!(buf.width(), 10);
        assert_eq!(buf.height(), 5);
        for row in 0..5 {
            for col in 0..10 {
                assert_eq!(buf.get(col, row), Cell { ch: ' ' });
            }
        }
    }

    #[test]
    fn test_write_str() {
        let mut buf = CellBuffer::new(10, 3);
        buf.write_str(2, 1, "hello");
        assert_eq!(buf.get(0, 1), Cell { ch: ' ' });
        assert_eq!(buf.get(1, 1), Cell { ch: ' ' });
        assert_eq!(buf.get(2, 1), Cell { ch: 'h' });
        assert_eq!(buf.get(3, 1), Cell { ch: 'e' });
        assert_eq!(buf.get(4, 1), Cell { ch: 'l' });
        assert_eq!(buf.get(5, 1), Cell { ch: 'l' });
        assert_eq!(buf.get(6, 1), Cell { ch: 'o' });
        assert_eq!(buf.get(7, 1), Cell { ch: ' ' });
    }

    #[test]
    fn test_write_str_truncated() {
        let mut buf = CellBuffer::new(5, 1);
        buf.write_str(3, 0, "hello");
        // Only 'he' fits (cols 3 and 4)
        assert_eq!(buf.get(3, 0), Cell { ch: 'h' });
        assert_eq!(buf.get(4, 0), Cell { ch: 'e' });
    }

    #[test]
    fn test_clear_row() {
        let mut buf = CellBuffer::new(5, 2);
        buf.write_str(0, 0, "hello");
        buf.clear_row(0);
        for col in 0..5 {
            assert_eq!(buf.get(col, 0), Cell { ch: ' ' });
        }
    }

    #[test]
    fn test_diff_no_changes() {
        let a = CellBuffer::new(10, 5);
        let b = CellBuffer::new(10, 5);
        let mut term = MockTerminal::new(10, 5);
        CellBuffer::diff(&a, &b, &mut term);
        // No move_cursor or write_str ops should be emitted
        assert!(
            term.ops.is_empty(),
            "Expected no ops, got: {:?}",
            term.ops
        );
    }

    #[test]
    fn test_diff_single_cell_change() {
        let current = CellBuffer::new(10, 5);
        let mut next = CellBuffer::new(10, 5);
        next.set(3, 2, Cell { ch: 'X' });

        let mut term = MockTerminal::new(10, 5);
        CellBuffer::diff(&current, &next, &mut term);

        assert_eq!(term.ops.len(), 2);
        assert_eq!(term.ops[0], MockOp::MoveCursor(3, 2));
        assert_eq!(term.ops[1], MockOp::WriteStr("X".to_string()));
    }

    #[test]
    fn test_diff_coalesces_adjacent() {
        let current = CellBuffer::new(10, 1);
        let mut next = CellBuffer::new(10, 1);
        next.write_str(2, 0, "abc");

        let mut term = MockTerminal::new(10, 1);
        CellBuffer::diff(&current, &next, &mut term);

        // Should be one move + one write, not three separate writes
        assert_eq!(term.ops.len(), 2);
        assert_eq!(term.ops[0], MockOp::MoveCursor(2, 0));
        assert_eq!(term.ops[1], MockOp::WriteStr("abc".to_string()));
    }

    #[test]
    fn test_diff_multiple_rows() {
        let current = CellBuffer::new(10, 3);
        let mut next = CellBuffer::new(10, 3);
        next.set(0, 0, Cell { ch: 'A' });
        next.set(5, 2, Cell { ch: 'B' });

        let mut term = MockTerminal::new(10, 3);
        CellBuffer::diff(&current, &next, &mut term);

        assert_eq!(term.ops.len(), 4);
        assert_eq!(term.ops[0], MockOp::MoveCursor(0, 0));
        assert_eq!(term.ops[1], MockOp::WriteStr("A".to_string()));
        assert_eq!(term.ops[2], MockOp::MoveCursor(5, 2));
        assert_eq!(term.ops[3], MockOp::WriteStr("B".to_string()));
    }

    #[test]
    fn test_diff_coalesces_short_gap() {
        let current = CellBuffer::new(10, 1);
        let mut next = CellBuffer::new(10, 1);
        next.set(1, 0, Cell { ch: 'A' });
        next.set(2, 0, Cell { ch: 'B' });
        // Gap of 2 at cols 3-4 (within GAP_THRESHOLD of 4)
        next.set(5, 0, Cell { ch: 'C' });

        let mut term = MockTerminal::new(10, 1);
        CellBuffer::diff(&current, &next, &mut term);

        // Short gap is coalesced: one run "AB  C" at col 1
        assert_eq!(term.ops.len(), 2);
        assert_eq!(term.ops[0], MockOp::MoveCursor(1, 0));
        assert_eq!(term.ops[1], MockOp::WriteStr("AB  C".to_string()));
    }

    #[test]
    fn test_diff_separate_runs_on_same_row() {
        let current = CellBuffer::new(20, 1);
        let mut next = CellBuffer::new(20, 1);
        next.set(1, 0, Cell { ch: 'A' });
        // Gap of 8 at cols 2-9 (exceeds GAP_THRESHOLD)
        next.set(10, 0, Cell { ch: 'B' });

        let mut term = MockTerminal::new(20, 1);
        CellBuffer::diff(&current, &next, &mut term);

        // Two separate runs due to long gap
        assert_eq!(term.ops.len(), 4);
        assert_eq!(term.ops[0], MockOp::MoveCursor(1, 0));
        assert_eq!(term.ops[1], MockOp::WriteStr("A".to_string()));
        assert_eq!(term.ops[2], MockOp::MoveCursor(10, 0));
        assert_eq!(term.ops[3], MockOp::WriteStr("B".to_string()));
    }

    #[test]
    fn test_copy_row_from() {
        let mut src = CellBuffer::new(10, 2);
        src.write_str(0, 0, "hello");

        let mut dst = CellBuffer::new(10, 2);
        dst.copy_row_from(1, &src, 0);

        assert_eq!(dst.get(0, 1), Cell { ch: 'h' });
        assert_eq!(dst.get(4, 1), Cell { ch: 'o' });
        assert_eq!(dst.get(5, 1), Cell { ch: ' ' });
    }

    #[test]
    fn test_shift_rows_up() {
        let mut buf = CellBuffer::new(5, 4);
        buf.write_str(0, 0, "aaaa");
        buf.write_str(0, 1, "bbbb");
        buf.write_str(0, 2, "cccc");
        buf.write_str(0, 3, "dddd");

        buf.shift_rows(0, 4, 1);

        // Row 0 = old row 1, row 1 = old row 2, row 2 = old row 3, row 3 = cleared
        assert_eq!(buf.get(0, 0), Cell { ch: 'b' });
        assert_eq!(buf.get(0, 1), Cell { ch: 'c' });
        assert_eq!(buf.get(0, 2), Cell { ch: 'd' });
        assert_eq!(buf.get(0, 3), Cell { ch: ' ' });
    }

    #[test]
    fn test_shift_rows_down() {
        let mut buf = CellBuffer::new(5, 4);
        buf.write_str(0, 0, "aaaa");
        buf.write_str(0, 1, "bbbb");
        buf.write_str(0, 2, "cccc");
        buf.write_str(0, 3, "dddd");

        buf.shift_rows(0, 4, -1);

        // Row 0 = cleared, row 1 = old row 0, row 2 = old row 1, row 3 = old row 2
        assert_eq!(buf.get(0, 0), Cell { ch: ' ' });
        assert_eq!(buf.get(0, 1), Cell { ch: 'a' });
        assert_eq!(buf.get(0, 2), Cell { ch: 'b' });
        assert_eq!(buf.get(0, 3), Cell { ch: 'c' });
    }

    #[test]
    fn test_shift_rows_partial_range() {
        let mut buf = CellBuffer::new(5, 5);
        buf.write_str(0, 0, "aaaa");
        buf.write_str(0, 1, "bbbb");
        buf.write_str(0, 2, "cccc");
        buf.write_str(0, 3, "dddd");
        buf.write_str(0, 4, "eeee");

        // Shift only rows 1..4 up by 1
        buf.shift_rows(1, 4, 1);

        // Row 0 unchanged, row 1 = old row 2, row 2 = old row 3, row 3 = cleared, row 4 unchanged
        assert_eq!(buf.get(0, 0), Cell { ch: 'a' });
        assert_eq!(buf.get(0, 1), Cell { ch: 'c' });
        assert_eq!(buf.get(0, 2), Cell { ch: 'd' });
        assert_eq!(buf.get(0, 3), Cell { ch: ' ' });
        assert_eq!(buf.get(0, 4), Cell { ch: 'e' });
    }

    #[test]
    fn test_shift_rows_exceeds_range() {
        let mut buf = CellBuffer::new(5, 3);
        buf.write_str(0, 0, "aaaa");
        buf.write_str(0, 1, "bbbb");
        buf.write_str(0, 2, "cccc");

        // Shift by more than range height — clears all
        buf.shift_rows(0, 3, 5);

        assert_eq!(buf.get(0, 0), Cell { ch: ' ' });
        assert_eq!(buf.get(0, 1), Cell { ch: ' ' });
        assert_eq!(buf.get(0, 2), Cell { ch: ' ' });
    }
}
