//! Viewport tracking for screen mode.
//!
//! The Viewport maps frame coordinates to screen coordinates. It tracks which
//! portion of the frame is visible and computes what needs to happen when the
//! cursor (dot) moves outside the visible area.

/// Parameters that define the viewport geometry.
#[derive(Debug, Clone, Copy)]
pub struct ViewportParams {
    /// Number of text rows available on screen.
    pub height: usize,
    /// Number of columns available on screen.
    pub width: usize,
    /// Vertical scroll margin: when dot is within this many rows of the top or
    /// bottom, the viewport scrolls to keep it visible with some context.
    pub v_margin: usize,
    /// Horizontal scroll margin.
    pub h_margin: usize,
}

impl ViewportParams {
    pub fn new(height: usize, width: usize) -> Self {
        let v_margin = (height / 4).clamp(1, 5);
        let h_margin = 8usize.min(width / 4);
        Self {
            height,
            width,
            v_margin,
            h_margin,
        }
    }
}

/// The viewport state: which portion of the frame is currently shown.
#[derive(Debug)]
pub struct Viewport {
    /// First visible frame line (0-based).
    pub top_line: usize,
    /// Horizontal scroll offset (0-based column of left edge).
    pub offset: usize,
    /// Geometry parameters.
    pub params: ViewportParams,
}

/// What action is needed to bring dot into view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixupAction {
    /// No change needed — dot is already visible.
    None,
    /// Scroll vertically by n lines (positive = up, negative = down).
    ScrollV(i32),
    /// Scroll horizontally — new offset value.
    SlideH(usize),
    /// Both vertical scroll and horizontal slide needed.
    ScrollAndSlide { scroll_v: i32, new_offset: usize },
    /// Full redraw needed (dot too far from current view).
    Redraw,
}

impl Viewport {
    pub fn new(params: ViewportParams) -> Self {
        Self {
            top_line: 0,
            offset: 0,
            params,
        }
    }

    /// Resize the viewport (e.g. on terminal resize).
    pub fn resize(&mut self, height: usize, width: usize) {
        self.params = ViewportParams::new(height, width);
    }

    /// The last visible line index (exclusive): top_line + height.
    pub fn bottom_line(&self) -> usize {
        self.top_line + self.params.height
    }

    /// Convert a frame line to a screen row (0-based).
    /// Returns None if the line is not visible.
    pub fn frame_to_screen_row(&self, frame_line: usize) -> Option<usize> {
        if frame_line >= self.top_line && frame_line < self.bottom_line() {
            Some(frame_line - self.top_line)
        } else {
            Option::None
        }
    }

    /// Convert a frame column to a screen column, accounting for horizontal offset.
    /// Returns None if the column is not visible.
    pub fn frame_to_screen_col(&self, frame_col: usize) -> Option<usize> {
        if frame_col >= self.offset && frame_col < self.offset + self.params.width {
            Some(frame_col - self.offset)
        } else {
            Option::None
        }
    }

    /// Compute the fixup action needed to make the given dot position visible.
    pub fn compute_fixup(&self, dot_line: usize, dot_col: usize, line_count: usize) -> FixupAction {
        let v_action = self.compute_v_fixup(dot_line, line_count);
        let h_action = self.compute_h_fixup(dot_col);

        match (v_action, h_action) {
            (FixupAction::None, FixupAction::None) => FixupAction::None,
            (FixupAction::None, FixupAction::SlideH(off)) => FixupAction::SlideH(off),
            (FixupAction::ScrollV(n), FixupAction::None) => FixupAction::ScrollV(n),
            (FixupAction::ScrollV(n), FixupAction::SlideH(off)) => {
                FixupAction::ScrollAndSlide {
                    scroll_v: n,
                    new_offset: off,
                }
            }
            (FixupAction::Redraw, _) | (_, FixupAction::Redraw) => FixupAction::Redraw,
            _ => FixupAction::Redraw,
        }
    }

    /// Compute vertical fixup only.
    fn compute_v_fixup(&self, dot_line: usize, line_count: usize) -> FixupAction {
        let top = self.top_line;
        let height = self.params.height;
        let margin = self.params.v_margin;
        let bottom = top + height; // exclusive

        // Maximum upward scroll (positive) before EOF floats above the bottom row.
        // EOF is at line_count; we want top + scroll + height - 1 <= line_count.
        let max_up_scroll = if line_count >= top + height {
            line_count - top - height + 1
        } else {
            0
        };

        if dot_line >= top && dot_line < bottom {
            // Dot is within the visible area — check margins
            let screen_row = dot_line - top;
            if screen_row < margin && top > 0 {
                // Too close to top — scroll down (reveal lines above)
                let scroll = (top.min(margin - screen_row)) as i32;
                FixupAction::ScrollV(-scroll)
            } else if screen_row >= height - margin && screen_row < height {
                // Too close to bottom — scroll up (reveal lines below)
                let scroll = (screen_row - (height - margin) + 1).min(max_up_scroll);
                if scroll > 0 {
                    FixupAction::ScrollV(scroll as i32)
                } else {
                    FixupAction::None
                }
            } else {
                FixupAction::None
            }
        } else if dot_line < top {
            // Dot is above the visible area — scroll to place dot at the margin
            let delta = top - dot_line;
            if delta <= height {
                let scroll = delta + margin.min(top.saturating_sub(delta));
                FixupAction::ScrollV(-(scroll as i32))
            } else {
                FixupAction::Redraw
            }
        } else {
            // Dot is below the visible area — scroll to place dot at (height - 1 - margin)
            let delta = dot_line - (bottom - 1);
            if delta <= height {
                let scroll = (delta + margin).min(max_up_scroll);
                if scroll > 0 {
                    FixupAction::ScrollV(scroll as i32)
                } else {
                    FixupAction::None
                }
            } else {
                FixupAction::Redraw
            }
        }
    }

    /// Compute horizontal fixup only.
    fn compute_h_fixup(&self, dot_col: usize) -> FixupAction {
        let offset = self.offset;
        let width = self.params.width;
        let margin = self.params.h_margin;

        if dot_col >= offset && dot_col < offset + width {
            // Visible — check margins
            let screen_col = dot_col - offset;
            if screen_col < margin && offset > 0 {
                let new_offset = offset.saturating_sub(margin - screen_col);
                FixupAction::SlideH(new_offset)
            } else if screen_col >= width - margin {
                let new_offset = offset + (screen_col - (width - margin) + 1);
                FixupAction::SlideH(new_offset)
            } else {
                FixupAction::None
            }
        } else if dot_col < offset {
            // Left of visible area
            let new_offset = dot_col.saturating_sub(margin);
            FixupAction::SlideH(new_offset)
        } else {
            // Right of visible area
            let new_offset = dot_col.saturating_sub(width - margin - 1);
            FixupAction::SlideH(new_offset)
        }
    }

    /// Apply a fixup action, updating top_line and offset.
    pub fn apply_fixup(&mut self, action: &FixupAction) {
        match action {
            FixupAction::None => {}
            FixupAction::ScrollV(n) => {
                if *n > 0 {
                    self.top_line += *n as usize;
                } else {
                    self.top_line = self.top_line.saturating_sub((-n) as usize);
                }
            }
            FixupAction::SlideH(new_offset) => {
                self.offset = *new_offset;
            }
            FixupAction::ScrollAndSlide {
                scroll_v,
                new_offset,
            } => {
                if *scroll_v > 0 {
                    self.top_line += *scroll_v as usize;
                } else {
                    self.top_line = self.top_line.saturating_sub((-scroll_v) as usize);
                }
                self.offset = *new_offset;
            }
            FixupAction::Redraw => {
                // Redraw will be handled by the caller — we just center the viewport
                // on dot. The caller passes dot_line to apply_redraw separately.
            }
        }
    }

    /// Center the viewport on a given dot position (for Redraw fixup).
    pub fn center_on(&mut self, dot_line: usize, dot_col: usize) {
        self.top_line = dot_line.saturating_sub(self.params.height / 2);
        if dot_col >= self.params.width {
            self.offset = dot_col.saturating_sub(self.params.width / 2);
        } else {
            self.offset = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dot_visible_no_fixup() {
        let vp = Viewport::new(ViewportParams::new(24, 80));
        let action = vp.compute_fixup(10, 20, 100);
        assert_eq!(action, FixupAction::None);
    }

    #[test]
    fn test_dot_below_visible_scroll_up() {
        let vp = Viewport::new(ViewportParams::new(24, 80));
        // dot at line 25 is just past the bottom (top=0, bottom=24)
        let action = vp.compute_fixup(25, 0, 100);
        match action {
            FixupAction::ScrollV(n) => assert!(n > 0),
            _ => panic!("Expected ScrollV, got {:?}", action),
        }
    }

    #[test]
    fn test_dot_above_visible_scroll_down() {
        let mut vp = Viewport::new(ViewportParams::new(24, 80));
        vp.top_line = 20;
        let action = vp.compute_fixup(10, 0, 100);
        match action {
            FixupAction::ScrollV(n) => assert!(n < 0),
            _ => panic!("Expected ScrollV negative, got {:?}", action),
        }
    }

    #[test]
    fn test_dot_far_below_redraw() {
        let vp = Viewport::new(ViewportParams::new(24, 80));
        let action = vp.compute_fixup(100, 0, 200);
        assert_eq!(action, FixupAction::Redraw);
    }

    #[test]
    fn test_dot_right_of_visible_slide() {
        let vp = Viewport::new(ViewportParams::new(24, 80));
        let action = vp.compute_fixup(10, 85, 100);
        match action {
            FixupAction::SlideH(off) => assert!(off > 0),
            _ => panic!("Expected SlideH, got {:?}", action),
        }
    }

    #[test]
    fn test_apply_scroll_v_positive() {
        let mut vp = Viewport::new(ViewportParams::new(24, 80));
        vp.apply_fixup(&FixupAction::ScrollV(5));
        assert_eq!(vp.top_line, 5);
    }

    #[test]
    fn test_apply_scroll_v_negative() {
        let mut vp = Viewport::new(ViewportParams::new(24, 80));
        vp.top_line = 10;
        vp.apply_fixup(&FixupAction::ScrollV(-3));
        assert_eq!(vp.top_line, 7);
    }

    #[test]
    fn test_center_on() {
        let mut vp = Viewport::new(ViewportParams::new(24, 80));
        vp.center_on(50, 10);
        assert_eq!(vp.top_line, 38); // 50 - 12
        assert_eq!(vp.offset, 0);
    }

    #[test]
    fn test_frame_to_screen_row() {
        let mut vp = Viewport::new(ViewportParams::new(24, 80));
        vp.top_line = 10;
        assert_eq!(vp.frame_to_screen_row(10), Some(0));
        assert_eq!(vp.frame_to_screen_row(33), Some(23));
        assert_eq!(vp.frame_to_screen_row(34), None);
        assert_eq!(vp.frame_to_screen_row(9), None);
    }

    #[test]
    fn test_frame_to_screen_col() {
        let mut vp = Viewport::new(ViewportParams::new(24, 80));
        vp.offset = 10;
        assert_eq!(vp.frame_to_screen_col(10), Some(0));
        assert_eq!(vp.frame_to_screen_col(89), Some(79));
        assert_eq!(vp.frame_to_screen_col(90), None);
        assert_eq!(vp.frame_to_screen_col(9), None);
    }

    #[test]
    fn test_bottom_margin_no_scroll_at_eof() {
        // height=24, v_margin=5, 30 lines (EOF at line 30)
        // top_line=7: EOF on last row (7+24-1=30). Dot at line 25 = screen row 18.
        // screen row 18 < height-margin (19), so no scroll needed.
        let vp = Viewport::new(ViewportParams::new(24, 80));
        assert_eq!(vp.params.v_margin, 5);
        let mut vp2 = Viewport::new(ViewportParams::new(24, 80));
        vp2.top_line = 7;
        // dot at line 25, screen row 18 = height-margin-1, no fixup
        let action = vp2.compute_fixup(25, 0, 30);
        assert_eq!(action, FixupAction::None);
        // dot at line 26, screen row 19 = height-margin, would want to scroll
        // but max_up_scroll = 30 - 7 - 24 + 1 = 0, so no scroll
        let action = vp2.compute_fixup(26, 0, 30);
        assert_eq!(action, FixupAction::None);
    }

    #[test]
    fn test_bottom_margin_limited_scroll_near_eof() {
        // height=24, v_margin=5, 35 lines (EOF at line 35)
        // top_line=7: bottom=31. Dot at line 26 = screen row 19 = height-margin.
        // Want to scroll 1, max_up_scroll = 35 - 7 - 24 + 1 = 5, so scroll 1 is fine.
        let mut vp = Viewport::new(ViewportParams::new(24, 80));
        vp.top_line = 7;
        let action = vp.compute_fixup(26, 0, 35);
        assert_eq!(action, FixupAction::ScrollV(1));
    }
}
