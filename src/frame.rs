use ropey::Rope;
use std::collections::HashMap;
use crate::lead_param::LeadParam;
use crate::mark::{Mark, MarkId};

pub struct FrameId(String);

pub struct Frame {
    id: FrameId,
    text: Rope,
    dot: Mark,
    marks: HashMap<MarkId, Option<Mark>>,
    return_frame: Option<FrameId>,
}

fn make_marks() ->  HashMap<MarkId, Option<Mark>> {
    let mut marks = HashMap::new();
    marks.insert(MarkId::Last, None);
    marks.insert(MarkId::Modified, None);
    for n in 1..=9u8 {
        marks.insert(MarkId::Numbered(n), None);
    }
    marks
}

// Constructors
impl Frame {
    pub fn new(id: FrameId) -> Self {
        Frame {
            id,
            text: Rope::new(),
            dot: Mark::new(0),
            marks: make_marks(),
            return_frame: None,
        }
    }

    pub fn new_with_text(id: FrameId, text: Rope) -> Self {
        Frame {
            id,
            text,
            dot: Mark::new(0),
            marks: make_marks(),
            return_frame: None,
        }
    }
}

// Commands
impl Frame {

    pub fn advance(&mut self, lead_param: LeadParam) -> bool {
        let line = self.mark_line(&self.dot);
        let new_position = match lead_param {
            LeadParam::None | LeadParam::Plus => {
                if line + 1 < self.text.len_lines() {
                    self.text.line_to_char(line + 1)
                } else {
                    return false;
                }
            },
            LeadParam::Minus => {
                if line > 0 {
                    self.text.line_to_char(line - 1)
                } else {
                    return false;
                }
            },
            LeadParam::Pint(n) => {
                if line + n < self.text.len_lines() {
                    self.text.line_to_char(line + n)
                } else {
                    return false;
                }
            },
            LeadParam::Nint(n) => {
                if line >= n {
                    self.text.line_to_char(line - n)
                } else {
                    return false;
                }
            },
            LeadParam::Pindef => {
                self.text.line_to_char(self.text.len_lines().saturating_sub(1))
            }
            LeadParam::Nindef => {
                0
            },
            LeadParam::Marker(id) => {
                if let Some(mark) = self.get_mark(&id) {
                    self.mark_line(mark)
                } else {
                    return false;
                }
            }
        };
        self.set_mark(MarkId::Last, self.dot);
        self.dot = Mark::new(new_position);
        true
    }

    pub fn delete(&mut self, lead_param: LeadParam) -> bool {
        return match lead_param {
            LeadParam::None | LeadParam::Plus => self.delete_forward(1),
            LeadParam::Pint(n) => self.delete_forward(n),
            LeadParam::Pindef => self.delete_forward(usize::MAX),
            LeadParam::Minus => self.delete_backward(1),
            LeadParam::Nint(n) => self.delete_backward(n),
            LeadParam::Nindef => self.delete_backward(usize::MAX),
            LeadParam::Marker(id) => self.delete_to_mark(&id)
        };
    }

    pub fn insert(&mut self, lead_param: LeadParam, text: &str) -> bool {
        return match lead_param {
            LeadParam::None | LeadParam::Plus => self.insert_text(1, text),
            LeadParam::Pint(n) => self.insert_text(n, text),
            _ => false
        };
    }

    pub fn overwrite(&mut self, lead_param: LeadParam, text: &str) -> bool {
        return match lead_param {
            LeadParam::None | LeadParam::Plus => self.overwrite_text(1, text),
            LeadParam::Pint(n) => self.overwrite_text(n, text),
            _ => false
        };
    }

    fn overwrite_text(&mut self, n: usize, text: &str) -> bool {
        let overwrite_len = if let Some(pos) = text.chars().position(|c| c == '\n') {
            pos
        } else {
            text.chars().count()
        };
        if overwrite_len == 0 {
            return true;
        }
        if self.dot.vspace() > 0 {
            // Realise virtual space first
            self.realise_space(self.dot.position(), self.dot.vspace());
            self.dot = Mark::new(self.dot.position() + self.dot.vspace());
        }

        let all_text = text[..overwrite_len].repeat(n);
        let total_len = all_text.chars().count();

        let line_length = self.line_length(&self.dot);
        let dot_col = self.mark_column(&self.dot);
        let actual_overwrite_len = total_len.min(line_length.saturating_sub(dot_col));
        self.text.remove(self.dot.position()..self.dot.position() + actual_overwrite_len);
        self.text.insert(self.dot.position(), text[..actual_overwrite_len].as_ref());
        if actual_overwrite_len < total_len {
            // Need to insert the rest
            let insert_position = self.dot.position() + actual_overwrite_len;
            let remaining_text = &all_text[actual_overwrite_len..];
            self.text.insert(insert_position, remaining_text);
            self.adjust_marks_after_insert(insert_position, total_len - actual_overwrite_len);
        }
        self.set_mark(MarkId::Last, self.dot);
        self.dot = Mark::new(self.dot.position() + total_len);
        self.set_mark(MarkId::Modified, self.dot);
        true
    }

}

// Helper methods
impl Frame {
    fn set_mark(&mut self, mark_id: MarkId, value: Mark) {
        self.marks.insert(mark_id, Some(value));
    }

    fn unset_mark(&mut self, mark_id: MarkId) {
        self.marks.insert(mark_id, None);
    }

    fn get_mark(&self, mark_id: &MarkId) -> Option<&Mark> {
        self.marks.get(mark_id).unwrap().as_ref()
    }

    fn delete_forward(&mut self, n: usize) -> bool {
        if self.dot.vspace() > 0 {
            // Deletion in virtual space is a no-op
            return true;
        }
        let dot_pos = self.dot.position();
        let dot_line = self.text.char_to_line(dot_pos);
        let dot_line_start = self.text.line_to_char(dot_line);
        let dot_line_text = self.text.line(dot_line);
        let dot_line_len = dot_line_text.len_chars();

        let to_delete = n.min(dot_line_len.saturating_sub(dot_pos - dot_line_start));
        if to_delete > 0 {
            self.text.remove(dot_pos..dot_pos + to_delete);
            self.adjust_marks_after_delete(dot_pos, dot_pos + to_delete);
            self.set_mark(MarkId::Modified, self.dot);
            self.unset_mark(MarkId::Last);
        }
        true
    }

    fn delete_backward(&mut self, n: usize) -> bool {
        let col = self.mark_column(&self.dot);
        if col < n {
            return false;
        }
        let mut to_delete = n;
        if self.dot.vspace() > 0 {
            let vspace_delete = n.min(self.dot.vspace());
            self.dot = Mark::new_with_vspace(self.dot.position(), self.dot.vspace() - vspace_delete);
            to_delete -= vspace_delete;
        }
        if to_delete > 0 {
            // If we get here, vspace is zero
            self.dot = Mark::new(self.dot.position().saturating_sub(to_delete));
            return self.delete_forward(to_delete);
        }
        true
    }

    fn mark_line(&self, mark: &Mark) -> usize {
        let pos = mark.position();
        self.text.char_to_line(pos)
    }

    fn line_length(&self, mark: &Mark) -> usize {
        let line_text = self.text.line(self.mark_line(mark));
        let total_chars = line_text.len_chars();
        if line_text.chars().last() == Some('\n') {
            total_chars - 1
        } else {
            total_chars
        }
    }

    fn mark_column(&self, mark: &Mark) -> usize {
        let pos = mark.position();
        let line = self.text.char_to_line(pos);
        let line_start = self.text.line_to_char(line);
        pos - line_start + mark.vspace()
    }

    fn delete_to_mark(&mut self, mark_id: &MarkId) -> bool {
        if let Some(target_mark) = self.get_mark(mark_id) {
            if self.dot.position() == target_mark.position() {
                // Nothing to do
                return true;
            }
            let (mut first, mut last) = if self.dot.position() <= target_mark.position() {
                (self.dot.clone(), target_mark.clone())
            } else {
                (target_mark.clone(), self.dot.clone())
            };
            if first.vspace() > 0 {
                let extra_space = first.vspace();
                self.realise_space(first.position(), extra_space);
                first = Mark::new(first.position() + extra_space);
                last = Mark::new_with_vspace(last.position() + extra_space, last.vspace());
            }
            self.text.remove(first.position()..last.position());
            self.dot = Mark::new(first.position());
            self.adjust_marks_after_delete(first.position(), last.position());
            self.set_mark(MarkId::Modified, self.dot);
            self.unset_mark(MarkId::Last);
            true
        } else {
            false
        }
    }

    fn insert_text(&mut self, n: usize, text: &str) -> bool {
        if text.is_empty() {
            return true;
        }
        self.realise_space(self.dot.position(), self.dot.vspace());
        let text_len = text.chars().count();
        for _ in 0..n {
            self.text.insert(self.dot.position(), text);
        }
        self.adjust_marks_after_insert(self.dot.position(), text_len * n);
        self.set_mark(MarkId::Last, self.dot);
        self.dot = Mark::new(self.dot.position() + text_len * n);
        self.set_mark(MarkId::Modified, self.dot);
        true
    }

    fn realise_space(&mut self, position: usize, vspace: usize) {
        if vspace > 0 {
            let spaces = " ".repeat(vspace);
            self.text.insert(position, &spaces);
            self.adjust_marks_after_insert(position, position + vspace);
        }
    }

    fn adjust_marks_after_delete(&mut self, del_start: usize, del_end: usize) {
        let deleted_len = del_end - del_start;
        for mark_opt in self.marks.values_mut() {
            if let Some(mark) = mark_opt {
                let mark_pos = mark.position();
                if mark_pos >= del_end {
                    // Mark is after the cut. Shift it left.
                    let new_position = mark_pos - deleted_len;

                    // Adjust virtual space if necessary.
                    // If we deleted a newline, then vspace must be zero.
                    let mut new_vspace = mark.vspace();
                    if mark.vspace() > 0 {
                        if self.text.char(mark_pos) != '\n' {
                            // The void is no longer at a line ending.
                            // Collapse the virtual offset.
                            new_vspace = 0;
                        }
                    }
                    *mark_opt = Some(Mark::new_with_vspace(new_position, new_vspace));
                } else if mark_pos > del_start {
                    // Mark was inside the cut.
                    // Collapse it to the start of the cut.
                    *mark_opt = Some(Mark::new_with_vspace(del_start, 0));
                }
            }
        }
    }

    fn adjust_marks_after_insert(&mut self, ins_start: usize, ins_len: usize) {
        for mark_opt in self.marks.values_mut() {
            if let Some(mark) = mark_opt {
                if mark.position() >= ins_start {
                    // Mark is at or after the insertion point.
                    // Mark shifts right by the length of the new text.
                    let new_mark = Mark::new_with_vspace(mark.position() + ins_len, mark.vspace());
                    *mark_opt = Some(new_mark);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ropey::Rope;

    const M1 : MarkId = MarkId::Numbered(1);
    const M2 : MarkId = MarkId::Numbered(2);
    const M3 : MarkId = MarkId::Numbered(3);
    const M4 : MarkId = MarkId::Numbered(4);

    #[test]
    fn delete_forward_updates_marks_and_text() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello world!"));
        f.dot = Mark::new(5);
        f.marks.insert(M1, Some(Mark::new(3)));  // Mark won't move
        f.marks.insert(M2, Some(Mark::new(5)));  // Mark won't move (at dot)
        f.marks.insert(M3, Some(Mark::new(7)));  // Mark is in deleted region— moves to dot
        f.marks.insert(M4, Some(Mark::new(12))); // Mark shifts back by 6
        f.delete(LeadParam::Pint(6));
        assert_eq!(f.text.to_string(), "hello!");
        assert_eq!(f.dot.position(), 5);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&MarkId::Last).unwrap(), &None);
        assert_eq!(f.marks.get(&M1).unwrap().unwrap().position(), 3);
        assert_eq!(f.marks.get(&M2).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&M3).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&M4).unwrap().unwrap().position(), 6);
    }

    #[test]
    fn delete_backward_updates_marks_and_text() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello world!"));
        f.dot = Mark::new(11);
        f.marks.insert(M1, Some(Mark::new(3)));  // Mark won't move
        f.marks.insert(M2, Some(Mark::new(5)));  // Mark won't move
        f.marks.insert(M3, Some(Mark::new(7)));  // Mark is in deleted region
        f.marks.insert(M4, Some(Mark::new(12))); // Mark shifts back by 6
        f.delete(LeadParam::Nint(6));
        assert_eq!(f.text.to_string(), "hello!");
        assert_eq!(f.dot.position(), 5);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&MarkId::Last).unwrap(), &None);
        assert_eq!(f.marks.get(&M1).unwrap().unwrap().position(), 3);
        assert_eq!(f.marks.get(&M2).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&M3).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&M4).unwrap().unwrap().position(), 6);
    }

    #[test]
    fn delete_forward_past_end_of_line_noop() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello"));
        f.dot = Mark::new(5);
        let result = f.delete(LeadParam::Plus);
        assert!(result);
        assert_eq!(f.text.to_string(), "hello");
        assert_eq!(f.dot.position(), 5);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap(), &None);
    }

    #[test]
    fn delete_backward_past_beginning_of_line_fails() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello"));
        f.dot = Mark::new(0);
        let result = f.delete(LeadParam::Minus);
        assert!(!result);
        assert_eq!(f.text.to_string(), "hello");
        assert_eq!(f.dot.position(), 0);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap(), &None);
    }

    #[test]
    fn delete_backward_in_virtual_space() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello"));
        f.dot = Mark::new_with_vspace(5, 3);
        let result = f.delete(LeadParam::Nint(2));
        assert!(result);
        assert_eq!(f.text.to_string(), "hello");
        assert_eq!(f.dot.position(), 5);
        assert_eq!(f.dot.vspace(), 1);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap(), &None);
    }

    #[test]
    fn delete_backward_in_virtual_space_plus_text() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello"));
        f.dot = Mark::new_with_vspace(5, 3);
        let result = f.delete(LeadParam::Nint(5));
        assert!(result);
        assert_eq!(f.text.to_string(), "hel");
        assert_eq!(f.dot.position(), 3);
        assert_eq!(f.dot.vspace(), 0);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap(), Mark::new(3));
    }

    #[test]
    fn delete_to_mark() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello\n world!"));
        f.dot = Mark::new_with_vspace(5, 0);
        f.set_mark(MarkId::Numbered(1), Mark::new(6));
        let result = f.delete(LeadParam::Marker(MarkId::Numbered(1)));
        assert!(result);
        assert_eq!(f.text.to_string(), "hello world!");
        assert_eq!(f.dot.position(), 5);
        assert_eq!(f.dot.vspace(), 0);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap(), Mark::new(5));
    }

    #[test]
    fn delete_to_mark_vspace() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello\n world!"));
        f.dot = Mark::new_with_vspace(5, 3);
        f.set_mark(MarkId::Numbered(1), Mark::new(6));
        let result = f.delete(LeadParam::Marker(MarkId::Numbered(1)));
        assert!(result);
        assert_eq!(f.text.to_string(), "hello    world!");
        assert_eq!(f.dot.position(), 8);
        assert_eq!(f.dot.vspace(), 0);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap(), Mark::new(8));
    }

    #[test]
    fn insert_text_at_end_updates_marks_and_text() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello"));
        f.dot = Mark::new(5);
        f.set_mark(MarkId::Numbered(1), Mark::new(5));
        f.set_mark(MarkId::Numbered(2), Mark::new(4));
        f.insert(LeadParam::None, " world");
        assert_eq!(f.text.to_string(), "hello world");
        assert_eq!(f.dot.position(), 11);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap().position(), 11);
        assert_eq!(f.marks.get(&MarkId::Last).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&MarkId::Numbered(1)).unwrap().unwrap().position(), 11);
        assert_eq!(f.marks.get(&MarkId::Numbered(2)).unwrap().unwrap().position(), 4);
    }

    #[test]
    fn insert_text_in_middle_updates_marks_and_text() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("helloworld"));
        f.dot = Mark::new(5);
        f.set_mark(MarkId::Numbered(1), Mark::new(5));
        f.set_mark(MarkId::Numbered(2), Mark::new(4));
        f.insert(LeadParam::None, " ");
        assert_eq!(f.text.to_string(), "hello world");
        assert_eq!(f.dot.position(), 6);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap().position(), 6);
        assert_eq!(f.marks.get(&MarkId::Last).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&MarkId::Numbered(1)).unwrap().unwrap().position(), 6);
        assert_eq!(f.marks.get(&MarkId::Numbered(2)).unwrap().unwrap().position(), 4);
    }

    #[test]
    fn overwrite_text_updates_marks_and_text_when_inserting() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello world\nline 2"));
        f.dot = Mark::new(6);
        f.set_mark(MarkId::Numbered(1), Mark::new(5));
        f.set_mark(MarkId::Numbered(2), Mark::new(6));
        f.set_mark(MarkId::Numbered(3), Mark::new(7));
        f.set_mark(MarkId::Numbered(4), Mark::new(12));
        f.overwrite(LeadParam::None, "universe");
        assert_eq!(f.text.to_string(), "hello universe\nline 2");
        assert_eq!(f.dot.position(), 14);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap().position(), 14);
        assert_eq!(f.marks.get(&MarkId::Last).unwrap().unwrap().position(), 6);
        assert_eq!(f.marks.get(&MarkId::Numbered(1)).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&MarkId::Numbered(2)).unwrap().unwrap().position(), 6);
        assert_eq!(f.marks.get(&MarkId::Numbered(3)).unwrap().unwrap().position(), 7);
        assert_eq!(f.marks.get(&MarkId::Numbered(4)).unwrap().unwrap().position(), 15);
    }

    #[test]
    fn overwrite_text_updates_marks_and_text_when_overwriting() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("hello universe\nline 2"));
        f.dot = Mark::new(6);
        f.set_mark(MarkId::Numbered(1), Mark::new(5));
        f.set_mark(MarkId::Numbered(2), Mark::new(6));
        f.set_mark(MarkId::Numbered(3), Mark::new(7));
        f.set_mark(MarkId::Numbered(4), Mark::new(12));
        f.overwrite(LeadParam::None, "world!!!");
        assert_eq!(f.text.to_string(), "hello world!!!\nline 2");
        assert_eq!(f.dot.position(), 14);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap().position(), 14);
        assert_eq!(f.marks.get(&MarkId::Last).unwrap().unwrap().position(), 6);
        assert_eq!(f.marks.get(&MarkId::Numbered(1)).unwrap().unwrap().position(), 5);
        assert_eq!(f.marks.get(&MarkId::Numbered(2)).unwrap().unwrap().position(), 6);
        assert_eq!(f.marks.get(&MarkId::Numbered(3)).unwrap().unwrap().position(), 7);
        assert_eq!(f.marks.get(&MarkId::Numbered(4)).unwrap().unwrap().position(), 12);
    }

    #[test]
    fn overwrite_text_extends_line() {
        let mut f = Frame::new_with_text(FrameId("frame".into()), Rope::from_str("\nline 2"));
        f.dot = Mark::new_with_vspace(0, 5);
        f.overwrite(LeadParam::Pint(3), "0123456789");
        assert_eq!(f.text.to_string(), "     012345678901234567890123456789\nline 2");
        assert_eq!(f.dot.position(), 35);
        assert_eq!(f.marks.get(&MarkId::Modified).unwrap().unwrap().position(), 35);
        assert_eq!(f.marks.get(&MarkId::Last).unwrap().unwrap().position(), 5);
    }
}
