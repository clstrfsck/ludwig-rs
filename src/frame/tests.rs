use super::*;


#[test]
fn test_calculate_insert_effect() {
    assert_eq!(calculate_insert_effect(""), (0, 0));
    assert_eq!(calculate_insert_effect("hello"), (0, 5));
    assert_eq!(calculate_insert_effect("hello\nworld"), (1, 5));
    assert_eq!(calculate_insert_effect("line1\nline2\n"), (2, 0));
}

#[test]
fn test_new_frame() {
    let frame = Frame::new();
    assert_eq!(frame.to_string(), "");
    assert_eq!(frame.dot(), Position::zero());
}

#[test]
fn test_insert_at_beginning() {
    let mut frame = Frame::new();
    frame.insert("hello");
    assert_eq!(frame.to_string(), "hello");
    assert_eq!(frame.dot(), Position::new(0, 5));
}

#[test]
fn test_insert_with_newlines() {
    let mut frame = Frame::new();
    frame.insert("hello\nworld");
    assert_eq!(frame.to_string(), "hello\nworld");
    assert_eq!(frame.dot(), Position::new(1, 5));
}

#[test]
fn test_insert_in_middle() {
    let mut frame: Frame = Frame::from_str("helloworld");
    frame.set_dot(Position::new(0, 5));
    frame.insert(" ");
    assert_eq!(frame.to_string(), "hello world");
    assert_eq!(frame.dot(), Position::new(0, 6));
}

#[test]
fn test_virtual_space_insert() {
    let mut frame: Frame = Frame::from_str("hello\n");

    // Move dot to virtual space (column 10 on a 5-char line)
    frame.set_dot(Position::new(0, 10));
    assert!(frame.dot().column > line_length_excluding_newline(&frame.rope, 0));

    // Insert text - should pad with spaces first
    frame.insert("world");

    assert_eq!(frame.to_string(), "hello     world\n");
    // dot started at column 10 (virtual), then "world" (5 chars) was inserted
    // So dot ends up at column 15
    assert_eq!(frame.dot(), Position::new(0, 15));
}

#[test]
fn test_dot_clamped_to_lines() {
    let mut frame: Frame = Frame::from_str("hello\n");

    // (Try to) move to a line that doesn't exist yet
    frame.set_dot(Position::new(5, 3));
    assert_eq!(frame.dot().line, 1);

    frame.insert("x");

    // Should have padded with spaces
    let content = frame.to_string();
    assert!(content.contains("   x"));
}

#[test]
fn test_delete() {
    let mut frame: Frame = Frame::from_str("hello world");
    frame.delete(Position::new(0, 5), Position::new(0, 11));
    assert_eq!(frame.to_string(), "hello");
}

#[test]
fn test_delete_across_lines() {
    let mut frame: Frame = Frame::from_str("hello\nworld\n");
    frame.delete(Position::new(0, 3), Position::new(1, 2));
    assert_eq!(frame.to_string(), "helrld\n");
}

#[test]
fn test_overtype() {
    let mut frame: Frame = Frame::from_str("hello world");
    frame.set_dot(Position::new(0, 6));
    frame.overtype("there");
    assert_eq!(frame.to_string(), "hello there");
}

#[test]
fn test_overtype_extends_line() {
    let mut frame: Frame = Frame::from_str("hello");
    frame.set_dot(Position::new(0, 3));
    frame.overtype("ping world");
    assert_eq!(frame.to_string(), "helping world");
}

#[test]
fn test_marks_update_on_insert() {
    let mut frame: Frame = Frame::from_str("hello world");

    // Create a mark after "hello"
    frame.set_dot(Position::new(0, 11)); // End of text
    let end_mark = MarkId::Numbered(1);
    frame.set_mark(end_mark);

    // Insert at position 5
    frame.insert_at(Position::new(0, 5), " beautiful");

    // Mark should have moved
    assert_eq!(frame.mark_position(end_mark), Some(Position::new(0, 21)));

    assert_eq!(frame.to_string(), "hello beautiful world");
}

#[test]
fn test_marks_update_on_delete() {
    let mut frame: Frame = Frame::from_str("hello beautiful world");

    // Create a mark at the end
    let end_mark = MarkId::Numbered(1);
    frame.set_mark_at(end_mark, Position::new(0, 21));
    assert_eq!(frame.mark_position(end_mark), Some(Position::new(0, 21)));

    // Delete " beautiful"
    frame.delete(Position::new(0, 5), Position::new(0, 15));

    // Mark should have moved back
    assert_eq!(frame.mark_position(end_mark), Some(Position::new(0, 11)));
    assert_eq!(frame.to_string(), "hello world");
}

// Insert Char (C command)

#[test]
fn insert_char_default_inserts_one_space() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_char(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hel lo");
    // Positive: dot stays at original column
    assert_eq!(f.dot(), Position::new(0, 3));
}

#[test]
fn insert_char_positive_n_inserts_n_spaces() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_char(LeadParam::Pint(4));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hel    lo");
    assert_eq!(f.dot(), Position::new(0, 3));
}

#[test]
fn insert_char_negative_inserts_one_space_dot_follows() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_char(LeadParam::Minus);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hel lo");
    // Negative: dot stays on original character (moves right by 1)
    assert_eq!(f.dot(), Position::new(0, 4));
}

#[test]
fn insert_char_negative_n_inserts_n_spaces_dot_follows() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_char(LeadParam::Nint(4));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hel    lo");
    // Negative: dot stays on original character (moves right by 4)
    assert_eq!(f.dot(), Position::new(0, 7));
}

#[test]
fn insert_char_zero_is_noop() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_char(LeadParam::Pint(0));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello");
    assert_eq!(f.dot(), Position::new(0, 3));
}

#[test]
fn insert_char_sets_marks() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    f.cmd_insert_char(LeadParam::Pint(2));
    // Modified mark set to dot after insert (col 5), then dot reset to 3
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 5));
    // Last mark set to original dot position
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(0, 3));
}

#[test]
fn insert_char_updates_marks_after_insertion() {
    let mut f = Frame::from_str("hello world");
    f.set_dot(Position::new(0, 5));
    f.marks.set(MarkId::Numbered(1), Position::new(0, 3));  // before insert point
    f.marks.set(MarkId::Numbered(2), Position::new(0, 8));  // after insert point
    f.cmd_insert_char(LeadParam::Pint(3));
    assert_eq!(f.to_string(), "hello    world");
    // Mark before insert point: unchanged
    assert_eq!(f.marks.get(MarkId::Numbered(1)).unwrap(), Position::new(0, 3));
    // Mark after insert point: shifted right by 3
    assert_eq!(f.marks.get(MarkId::Numbered(2)).unwrap(), Position::new(0, 11));
}

#[test]
fn insert_char_in_virtual_space() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 8));
    let result = f.cmd_insert_char(LeadParam::Pint(3));
    assert!(result.is_success());
    // Virtual space materialized, then 3 spaces inserted
    assert_eq!(f.to_string(), "hello      ");
    assert_eq!(f.dot(), Position::new(0, 8));
}

#[test]
fn insert_char_at_beginning_of_line() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_insert_char(LeadParam::Pint(2));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "  hello");
    assert_eq!(f.dot(), Position::new(0, 0));
}

#[test]
fn insert_char_rejects_invalid_lead_param() {
    let mut f = Frame::from_str("hello");
    let result = f.cmd_insert_char(LeadParam::Pindef);
    assert!(result.is_failure());
}

const M1: MarkId = MarkId::Numbered(1);
const M2: MarkId = MarkId::Numbered(2);
const M3: MarkId = MarkId::Numbered(3);
const M4: MarkId = MarkId::Numbered(4);

#[test]
fn delete_forward_updates_marks_and_text() {
    let mut f = Frame::from_str("hello world!");
    f.marks.set(MarkId::Dot, Position::new(0, 5));
    f.marks.set(M1, Position::new(0, 3)); // Mark won't move
    f.marks.set(M2, Position::new(0, 5)); // Mark won't move (at dot)
    f.marks.set(M3, Position::new(0, 7)); // Mark is in deleted region— moves to dot
    f.marks.set(M4, Position::new(0, 12)); // Mark shifts back by 6
    f.cmd_delete_char(LeadParam::Pint(6));
    assert_eq!(f.rope.to_string(), "hello!");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Last), None);
    assert_eq!(f.marks.get(M1).unwrap(), Position::new(0, 3));
    assert_eq!(f.marks.get(M2).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(M3).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(M4).unwrap(), Position::new(0, 6));
}

#[test]
fn delete_backward_updates_marks_and_text() {
    let mut f = Frame::from_str("hello world!");
    f.marks.set(MarkId::Dot, Position::new(0, 11));
    f.marks.set(M1, Position::new(0, 3)); // Mark won't move
    f.marks.set(M2, Position::new(0, 5)); // Mark won't move
    f.marks.set(M3, Position::new(0, 7)); // Mark is in deleted region
    f.marks.set(M4, Position::new(0, 12)); // Mark shifts back by 6
    f.cmd_delete_char(LeadParam::Nint(6));
    assert_eq!(f.rope.to_string(), "hello!");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Last), None);
    assert_eq!(f.marks.get(M1).unwrap(), Position::new(0, 3));
    assert_eq!(f.marks.get(M2).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(M3).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(M4).unwrap(), Position::new(0, 6));
}

#[test]
fn delete_forward_past_end_of_line_noop() {
    let mut f = Frame::from_str("hello");
    f.marks.set(MarkId::Dot, Position::new(0, 5));
    let result = f.cmd_delete_char(LeadParam::Plus);
    assert!(result.is_success());
    assert_eq!(f.rope.to_string(), "hello");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Modified), None);
}

#[test]
fn delete_backward_past_beginning_of_line_fails() {
    let mut f = Frame::from_str("hello");
    f.marks.set(MarkId::Dot, Position::new(0, 0));
    let result = f.cmd_delete_char(LeadParam::Minus);
    assert!(result.is_failure());
    assert_eq!(f.rope.to_string(), "hello");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 0));
    assert_eq!(f.marks.get(MarkId::Modified), None);
}

#[test]
fn delete_backward_in_virtual_space() {
    let mut f = Frame::from_str("hello");
    f.marks.set(MarkId::Dot, Position::new(0, 8));
    let result = f.cmd_delete_char(LeadParam::Nint(2));
    assert!(result.is_success());
    assert_eq!(f.rope.to_string(), "hello");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 6));
    assert_eq!(f.marks.get(MarkId::Modified), None);
}

#[test]
fn delete_backward_in_virtual_space_plus_text() {
    let mut f = Frame::from_str("hello");
    f.marks.set(MarkId::Dot, Position::new(0, 8));
    let result = f.cmd_delete_char(LeadParam::Nint(5));
    assert!(result.is_success());
    assert_eq!(f.rope.to_string(), "hel");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 3));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 3));
}

#[test]
fn delete_to_mark() {
    let mut f = Frame::from_str("hello\n world!");
    f.marks.set(MarkId::Dot, Position::new(0, 5));
    f.marks.set(MarkId::Numbered(1), Position::new(1, 0));
    let result = f.cmd_delete_char(LeadParam::Marker(MarkId::Numbered(1)));
    assert!(result.is_success());
    assert_eq!(f.rope.to_string(), "hello world!");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 5));
}

#[test]
fn delete_to_mark_vspace() {
    let mut f = Frame::from_str("hello\n world!");
    f.marks.set(MarkId::Dot, Position::new(0, 8));
    f.marks.set(MarkId::Numbered(1), Position::new(1, 0));
    let result = f.cmd_delete_char(LeadParam::Marker(MarkId::Numbered(1)));
    assert!(result.is_success());
    assert_eq!(f.rope.to_string(), "hello    world!");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 8));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 8));
}

#[test]
fn insert_text_at_end_updates_marks_and_text() {
    let mut f = Frame::from_str("hello");
    f.marks.set(MarkId::Dot, Position::new(0, 5));
    f.marks.set(MarkId::Numbered(1), Position::new(0, 5));
    f.marks.set(MarkId::Numbered(2), Position::new(0, 4));
    f.cmd_insert_text(LeadParam::None, &TrailParam::from_str(" world"));
    assert_eq!(f.rope.to_string(), "hello world");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 11));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 11));
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Numbered(1)).unwrap(), Position::new(0, 11));
    assert_eq!(f.marks.get(MarkId::Numbered(2)).unwrap(), Position::new(0, 4));
}

#[test]
fn insert_text_in_middle_updates_marks_and_text() {
    let mut f = Frame::from_str("helloworld");
    f.marks.set(MarkId::Dot, Position::new(0, 5));
    f.marks.set(MarkId::Numbered(1), Position::new(0, 5));
    f.marks.set(MarkId::Numbered(2), Position::new(0, 4));
    f.cmd_insert_text(LeadParam::None, &TrailParam::from_str(" "));
    assert_eq!(f.rope.to_string(), "hello world");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 6));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 6));
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Numbered(1)).unwrap(), Position::new(0, 6));
    assert_eq!(f.marks.get(MarkId::Numbered(2)).unwrap(), Position::new(0, 4));
}

#[test]
fn overwrite_text_updates_marks_and_text_when_inserting() {
    let mut f = Frame::from_str("hello world\nline 2");
    f.marks.set(MarkId::Dot, Position::new(0, 6));
    f.marks.set(MarkId::Numbered(1), Position::new(0, 5));
    f.marks.set(MarkId::Numbered(2), Position::new(0, 6));
    f.marks.set(MarkId::Numbered(3), Position::new(0, 7));
    f.marks.set(MarkId::Numbered(4), Position::new(1, 0));
    f.cmd_overtype_text(LeadParam::None, &TrailParam::from_str("universe"));
    assert_eq!(f.rope.to_string(), "hello universe\nline 2");
    assert_eq!(f.marks.get(MarkId::Dot).unwrap(), Position::new(0, 14));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 14));
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(0, 6));
    assert_eq!(f.marks.get(MarkId::Numbered(1)).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Numbered(2)).unwrap(), Position::new(0, 6));
    assert_eq!(f.marks.get(MarkId::Numbered(3)).unwrap(), Position::new(0, 7));
    assert_eq!(f.marks.get(MarkId::Numbered(4)).unwrap(), Position::new(1, 0));
}

#[test]
fn overwrite_text_updates_marks_and_text_when_overwriting() {
    let mut f = Frame::from_str("hello universe\nline 2");
    f.marks.set(MarkId::Dot, Position::new(0, 6));
    f.marks.set(MarkId::Numbered(1), Position::new(0, 5));
    f.marks.set(MarkId::Numbered(2), Position::new(0, 6));
    f.marks.set(MarkId::Numbered(3), Position::new(0, 7));
    f.marks.set(MarkId::Numbered(4), Position::new(0, 12));
    f.cmd_overtype_text(LeadParam::None, &TrailParam::from_str("world!!!"));
    assert_eq!(f.rope.to_string(), "hello world!!!\nline 2");
    assert_eq!(f.dot(), Position::new(0, 14));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 14));
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(0, 6));
    assert_eq!(f.marks.get(MarkId::Numbered(1)).unwrap(), Position::new(0, 5));
    assert_eq!(f.marks.get(MarkId::Numbered(2)).unwrap(), Position::new(0, 6));
    assert_eq!(f.marks.get(MarkId::Numbered(3)).unwrap(), Position::new(0, 7));
    assert_eq!(f.marks.get(MarkId::Numbered(4)).unwrap(), Position::new(0, 12));
}

#[test]
fn overwrite_text_extends_line() {
    let mut f = Frame::from_str("\nline 2");
    f.marks.set(MarkId::Dot, Position::new(0, 5));
    f.cmd_overtype_text(LeadParam::Pint(3), &TrailParam::from_str("0123456789"));
    assert_eq!(f.rope.to_string(), "     012345678901234567890123456789\nline 2");
    assert_eq!(f.dot(), Position::new(0, 35));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 35));
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(0, 5));
}
