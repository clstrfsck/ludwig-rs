use super::*;
use crate::lead_param::LeadParam;
use crate::marks::MarkId;
use crate::trail_param::TrailParam;
use edit::CaseMode;
use search::SearchCommands;

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

// Insert Line (L command)

#[test]
fn insert_line_default_inserts_one_line() {
    let mut f = Frame::from_str("hello\nworld");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "\nhello\nworld");
    // Positive: dot moves to topmost inserted line, same column
    assert_eq!(f.dot(), Position::new(0, 3));
}

#[test]
fn insert_line_positive_n() {
    let mut f = Frame::from_str("hello\nworld");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_line(LeadParam::Pint(3));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "\n\n\nhello\nworld");
    // Dot on the topmost inserted line, same column (virtual space)
    assert_eq!(f.dot(), Position::new(0, 3));
}

#[test]
fn insert_line_negative_inserts_one_line_dot_stays() {
    let mut f = Frame::from_str("hello\nworld");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_line(LeadParam::Minus);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "\nhello\nworld");
    // Negative: dot stays on original character (shifted down by 1)
    assert_eq!(f.dot(), Position::new(1, 3));
}

#[test]
fn insert_line_negative_n() {
    let mut f = Frame::from_str("hello\nworld");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_line(LeadParam::Nint(3));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "\n\n\nhello\nworld");
    // Negative: dot stays on original character (shifted down by 3)
    assert_eq!(f.dot(), Position::new(3, 3));
}

#[test]
fn insert_line_zero_is_noop() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_insert_line(LeadParam::Pint(0));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello");
    assert_eq!(f.dot(), Position::new(0, 3));
}

#[test]
fn insert_line_on_second_line() {
    let mut f = Frame::from_str("hello\nworld\nfoo");
    f.set_dot(Position::new(1, 2));
    let result = f.cmd_insert_line(LeadParam::Pint(2));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello\n\n\nworld\nfoo");
    // Dot on the topmost inserted line (line 1), same column
    assert_eq!(f.dot(), Position::new(1, 2));
}

#[test]
fn insert_line_sets_marks() {
    let mut f = Frame::from_str("hello\nworld");
    f.set_dot(Position::new(1, 3));
    f.cmd_insert_line(LeadParam::Pint(2));
    // Last set to original dot position
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(1, 3));
    // Modified set after insert (dot was shifted to line 3 before reset)
    assert!(f.marks.get(MarkId::Modified).is_some());
}

#[test]
fn insert_line_shifts_marks_below() {
    let mut f = Frame::from_str("hello\nworld\nfoo");
    f.set_dot(Position::new(1, 0));
    f.marks.set(MarkId::Numbered(1), Position::new(0, 3)); // above: unchanged
    f.marks.set(MarkId::Numbered(2), Position::new(1, 2)); // at insert line: shifts down
    f.marks.set(MarkId::Numbered(3), Position::new(2, 1)); // below: shifts down
    f.cmd_insert_line(LeadParam::Pint(2));
    assert_eq!(
        f.marks.get(MarkId::Numbered(1)).unwrap(),
        Position::new(0, 3)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(2)).unwrap(),
        Position::new(3, 2)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(3)).unwrap(),
        Position::new(4, 1)
    );
}

#[test]
fn insert_line_rejects_invalid_lead_param() {
    let mut f = Frame::from_str("hello");
    let result = f.cmd_insert_line(LeadParam::Pindef);
    assert!(result.is_failure());
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
    f.marks.set(MarkId::Numbered(1), Position::new(0, 3)); // before insert point
    f.marks.set(MarkId::Numbered(2), Position::new(0, 8)); // after insert point
    f.cmd_insert_char(LeadParam::Pint(3));
    assert_eq!(f.to_string(), "hello    world");
    // Mark before insert point: unchanged
    assert_eq!(
        f.marks.get(MarkId::Numbered(1)).unwrap(),
        Position::new(0, 3)
    );
    // Mark after insert point: shifted right by 3
    assert_eq!(
        f.marks.get(MarkId::Numbered(2)).unwrap(),
        Position::new(0, 11)
    );
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
    assert_eq!(
        f.marks.get(MarkId::Numbered(1)).unwrap(),
        Position::new(0, 11)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(2)).unwrap(),
        Position::new(0, 4)
    );
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
    assert_eq!(
        f.marks.get(MarkId::Numbered(1)).unwrap(),
        Position::new(0, 6)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(2)).unwrap(),
        Position::new(0, 4)
    );
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
    assert_eq!(
        f.marks.get(MarkId::Numbered(1)).unwrap(),
        Position::new(0, 5)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(2)).unwrap(),
        Position::new(0, 6)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(3)).unwrap(),
        Position::new(0, 7)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(4)).unwrap(),
        Position::new(1, 0)
    );
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
    assert_eq!(
        f.marks.get(MarkId::Numbered(1)).unwrap(),
        Position::new(0, 5)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(2)).unwrap(),
        Position::new(0, 6)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(3)).unwrap(),
        Position::new(0, 7)
    );
    assert_eq!(
        f.marks.get(MarkId::Numbered(4)).unwrap(),
        Position::new(0, 12)
    );
}

#[test]
fn overwrite_text_extends_line() {
    let mut f = Frame::from_str("\nline 2");
    f.marks.set(MarkId::Dot, Position::new(0, 5));
    f.cmd_overtype_text(LeadParam::Pint(3), &TrailParam::from_str("0123456789"));
    assert_eq!(
        f.rope.to_string(),
        "     012345678901234567890123456789\nline 2"
    );
    assert_eq!(f.dot(), Position::new(0, 35));
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 35));
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(0, 5));
}

#[test]
fn split_line_at_bol() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_split_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "\nhello");
    assert_eq!(f.dot(), Position::new(1, 0));
}

#[test]
fn split_line_middle() {
    let mut f = Frame::from_str("hello!");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_split_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hel\nlo!");
    assert_eq!(f.dot(), Position::new(1, 0));
}

#[test]
fn split_line_eol() {
    let mut f = Frame::from_str("hello!");
    f.set_dot(Position::new(0, 6));
    let result = f.cmd_split_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello!\n");
    assert_eq!(f.dot(), Position::new(1, 0));
}

#[test]
fn split_line_vspace() {
    let mut f = Frame::from_str("hello!");
    f.set_dot(Position::new(0, 10));
    let result = f.cmd_split_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello!\n");
    assert_eq!(f.dot(), Position::new(1, 0));
}

// --- Case change tests ---

#[test]
fn case_up_single_char() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_case_change(LeadParam::None, CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "Hello");
    assert_eq!(f.dot(), Position::new(0, 1));
}

#[test]
fn case_up_multiple_chars() {
    let mut f = Frame::from_str("hello world");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_case_change(LeadParam::Pint(5), CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "HELLO world");
    assert_eq!(f.dot(), Position::new(0, 5));
}

#[test]
fn case_low_multiple_chars() {
    let mut f = Frame::from_str("HELLO WORLD");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_case_change(LeadParam::Pint(5), CaseMode::Lower);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello WORLD");
    assert_eq!(f.dot(), Position::new(0, 5));
}

#[test]
fn case_edit_capitalizes_words() {
    let mut f = Frame::from_str("hello world");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_case_change(LeadParam::Pint(11), CaseMode::Edit);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "Hello World");
    assert_eq!(f.dot(), Position::new(0, 11));
}

#[test]
fn case_edit_from_mid_word() {
    let mut f = Frame::from_str("hELLO");
    f.set_dot(Position::new(0, 1));
    let result = f.cmd_case_change(LeadParam::Pint(4), CaseMode::Edit);
    assert!(result.is_success());
    // Preceding char 'h' is a letter, so all become lowercase
    assert_eq!(f.to_string(), "hello");
}

#[test]
fn case_up_backward() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 5));
    let result = f.cmd_case_change(LeadParam::Nint(3), CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "heLLO");
    assert_eq!(f.dot(), Position::new(0, 2));
}

#[test]
fn case_up_backward_single() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_case_change(LeadParam::Minus, CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "heLlo");
    assert_eq!(f.dot(), Position::new(0, 2));
}

#[test]
fn case_up_pindef_to_eol() {
    let mut f = Frame::from_str("hello world");
    f.set_dot(Position::new(0, 6));
    let result = f.cmd_case_change(LeadParam::Pindef, CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello WORLD");
    assert_eq!(f.dot(), Position::new(0, 11));
}

#[test]
fn case_low_nindef_to_col0() {
    let mut f = Frame::from_str("HELLO WORLD");
    f.set_dot(Position::new(0, 5));
    let result = f.cmd_case_change(LeadParam::Nindef, CaseMode::Lower);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello WORLD");
    assert_eq!(f.dot(), Position::new(0, 0));
}

#[test]
fn case_change_at_eol_is_noop() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 5));
    let result = f.cmd_case_change(LeadParam::None, CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello");
    assert_eq!(f.dot(), Position::new(0, 5));
}

#[test]
fn case_change_in_virtual_space_is_noop() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 10));
    let result = f.cmd_case_change(LeadParam::Pint(3), CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello");
}

#[test]
fn case_change_clamps_to_eol() {
    // Requesting more chars than available should change what's there
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_case_change(LeadParam::Pint(100), CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "helLO");
    assert_eq!(f.dot(), Position::new(0, 5));
}

#[test]
fn case_change_sets_marks() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    f.cmd_case_change(LeadParam::Pint(3), CaseMode::Upper);
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 3));
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(0, 0));
}

#[test]
fn case_change_backward_sets_marks() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 5));
    f.cmd_case_change(LeadParam::Nint(3), CaseMode::Upper);
    assert_eq!(f.marks.get(MarkId::Modified).unwrap(), Position::new(0, 2));
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(0, 5));
}

#[test]
fn case_change_non_alpha_unchanged() {
    let mut f = Frame::from_str("h3llo!");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_case_change(LeadParam::Pint(6), CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "H3LLO!");
}

#[test]
fn case_edit_non_alpha_triggers_uppercase() {
    // *E: after a non-letter (digit), next letter becomes uppercase
    let mut f = Frame::from_str("abc1def");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_case_change(LeadParam::Pint(7), CaseMode::Edit);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "Abc1Def");
}

#[test]
fn case_change_rejects_marker_lead() {
    let mut f = Frame::from_str("hello");
    let result = f.cmd_case_change(LeadParam::Marker(MarkId::Dot), CaseMode::Upper);
    assert!(result.is_failure());
}

#[test]
fn case_up_zero_count_noop() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_case_change(LeadParam::Pint(0), CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello");
    assert_eq!(f.dot(), Position::new(0, 0));
}

#[test]
fn case_backward_at_col0_noop() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_case_change(LeadParam::Minus, CaseMode::Upper);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello");
    assert_eq!(f.dot(), Position::new(0, 0));
}

// --- Delete line (K) tests ---

#[test]
fn delete_line_single_from_first() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_delete_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line2\nline3");
    assert_eq!(f.dot(), Position::new(0, 0));
}

#[test]
fn delete_line_single_from_middle() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(1, 3));
    let result = f.cmd_delete_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line1\nline3");
    // Dot stays at line 1 col 3 (now on "line3")
    assert_eq!(f.dot(), Position::new(1, 3));
}

#[test]
fn delete_line_single_last_line() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(2, 0));
    let result = f.cmd_delete_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line1\nline2");
    assert_eq!(f.dot(), Position::new(1, 0));
}

#[test]
fn delete_line_single_only_line() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_delete_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "");
    assert_eq!(f.dot(), Position::new(0, 3));
}

#[test]
fn delete_line_multiple_forward() {
    let mut f = Frame::from_str("line1\nline2\nline3\nline4");
    f.set_dot(Position::new(1, 2));
    let result = f.cmd_delete_line(LeadParam::Pint(2));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line1\nline4");
    // Dot stays at line 1, column preserved
    assert_eq!(f.dot(), Position::new(1, 2));
}

#[test]
fn delete_line_forward_past_end_fails() {
    let mut f = Frame::from_str("line1\nline2");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_delete_line(LeadParam::Pint(3));
    assert!(result.is_failure());
    assert_eq!(f.to_string(), "line1\nline2");
}

#[test]
fn delete_line_on_empty_frame_fails() {
    let mut f = Frame::new();
    let result = f.cmd_delete_line(LeadParam::None);
    assert!(result.is_failure());
}

#[test]
fn delete_line_backward_single() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(2, 3));
    let result = f.cmd_delete_line(LeadParam::Minus);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line1\nline3");
    // Dot stays on same text (line3), now at line 1, column preserved
    assert_eq!(f.dot(), Position::new(1, 3));
}

#[test]
fn delete_line_backward_multiple() {
    let mut f = Frame::from_str("line1\nline2\nline3\nline4");
    f.set_dot(Position::new(3, 0));
    let result = f.cmd_delete_line(LeadParam::Nint(2));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line1\nline4");
    assert_eq!(f.dot(), Position::new(1, 0));
}

#[test]
fn delete_line_backward_at_first_line_fails() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_delete_line(LeadParam::Minus);
    assert!(result.is_failure());
    assert_eq!(f.to_string(), "hello");
}

#[test]
fn delete_line_backward_too_many_fails() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(1, 0));
    let result = f.cmd_delete_line(LeadParam::Nint(3));
    assert!(result.is_failure());
    assert_eq!(f.to_string(), "line1\nline2\nline3");
}

#[test]
fn delete_line_pindef_from_beginning() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_delete_line(LeadParam::Pindef);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "");
}

#[test]
fn delete_line_pindef_from_middle() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(1, 2));
    let result = f.cmd_delete_line(LeadParam::Pindef);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line1");
    assert_eq!(f.dot(), Position::new(0, 2));
}

#[test]
fn delete_line_nindef() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(2, 0));
    let result = f.cmd_delete_line(LeadParam::Nindef);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line3");
    assert_eq!(f.dot(), Position::new(0, 0));
}

#[test]
fn delete_line_nindef_at_first_fails() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_delete_line(LeadParam::Nindef);
    assert!(result.is_failure());
}

#[test]
fn delete_line_zero_count_noop() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_delete_line(LeadParam::Pint(0));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "hello");
}

#[test]
fn delete_line_preserves_column_in_virtual_space() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(0, 20)); // virtual space
    let result = f.cmd_delete_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line2\nline3");
    assert_eq!(f.dot(), Position::new(0, 20));
}

#[test]
fn delete_line_sets_marks() {
    let mut f = Frame::from_str("line1\nline2\nline3");
    f.set_dot(Position::new(1, 2));
    f.cmd_delete_line(LeadParam::None);
    assert!(f.marks.get(MarkId::Modified).is_some());
    assert_eq!(f.marks.get(MarkId::Last).unwrap(), Position::new(1, 2));
}

#[test]
fn delete_line_with_trailing_newline() {
    let mut f = Frame::from_str("line1\nline2\n");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_delete_line(LeadParam::None);
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line2\n");
}

#[test]
fn delete_line_marker() {
    let mut f = Frame::from_str("line1\nline2\nline3\nline4");
    f.set_dot(Position::new(1, 0));
    f.marks.set(MarkId::Numbered(1), Position::new(2, 3));
    let result = f.cmd_delete_line(LeadParam::Marker(MarkId::Numbered(1)));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line1\nline3\nline4");
    assert_eq!(f.dot(), Position::new(1, 0));
}

#[test]
fn delete_line_marker_before_dot() {
    let mut f = Frame::from_str("line1\nline2\nline3\nline4");
    f.set_dot(Position::new(2, 0));
    f.marks.set(MarkId::Numbered(1), Position::new(1, 0));
    let result = f.cmd_delete_line(LeadParam::Marker(MarkId::Numbered(1)));
    assert!(result.is_success());
    assert_eq!(f.to_string(), "line1\nline3\nline4");
    assert_eq!(f.dot(), Position::new(1, 0));
}

// ===== Next (N) command tests =====

#[test]
fn next_find_single_char_forward() {
    let mut f = Frame::from_str("hello world");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_next(LeadParam::None, &TrailParam::from_str("w"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 6));
    // Equals mark set to original dot
    assert_eq!(f.get_mark(MarkId::Equals), Some(Position::new(0, 0)));
}

#[test]
fn next_find_char_from_set() {
    let mut f = Frame::from_str("hello world");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_next(LeadParam::None, &TrailParam::from_str("ow"));
    assert!(result.is_success());
    // Should find 'o' at position 4 (first occurrence of 'o' or 'w')
    assert_eq!(f.dot(), Position::new(0, 4));
}

#[test]
fn next_find_with_range() {
    let mut f = Frame::from_str("abc123def");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_next(LeadParam::None, &TrailParam::from_str("0..9"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 3)); // first digit '1'
}

#[test]
fn next_find_nth_occurrence() {
    let mut f = Frame::from_str("abracadabra");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_next(LeadParam::Pint(3), &TrailParam::from_str("a"));
    assert!(result.is_success());
    // 'a' at 0 (skipped, at dot), 1->col 3, 2->col 5, 3->col 7
    assert_eq!(f.dot(), Position::new(0, 7));
}

#[test]
fn next_skips_char_at_dot() {
    let mut f = Frame::from_str("aaa");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_next(LeadParam::None, &TrailParam::from_str("a"));
    assert!(result.is_success());
    // N skips the char at dot, finds next 'a'
    assert_eq!(f.dot(), Position::new(0, 1));
}

#[test]
fn next_not_found_fails() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_next(LeadParam::None, &TrailParam::from_str("z"));
    assert!(result.is_failure());
    // Dot should not move on failure
    assert_eq!(f.dot(), Position::new(0, 0));
}

#[test]
fn next_crosses_lines() {
    let mut f = Frame::from_str("hello\nworld");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_next(LeadParam::None, &TrailParam::from_str("w"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(1, 0));
}

#[test]
fn next_backward_single() {
    let mut f = Frame::from_str("hello world");
    f.set_dot(Position::new(0, 10));
    let result = f.cmd_next(LeadParam::Minus, &TrailParam::from_str("o"));
    assert!(result.is_success());
    // Backward search finds 'o' at col 7, dot lands AFTER it at col 8
    assert_eq!(f.dot(), Position::new(0, 8));
}

#[test]
fn next_backward_skips_adjacent() {
    // N backward skips the char immediately before dot
    let mut f = Frame::from_str("abab");
    f.set_dot(Position::new(0, 3)); // at 'b'
    let result = f.cmd_next(LeadParam::Minus, &TrailParam::from_str("a"));
    assert!(result.is_success());
    // Backward: skip col 2 ('a' - but we skip one more), find 'a' at col 0
    // Dot lands at col 0 + 1 = 1
    assert_eq!(f.dot(), Position::new(0, 1));
}

#[test]
fn next_backward_not_found_fails() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 2));
    let result = f.cmd_next(LeadParam::Minus, &TrailParam::from_str("z"));
    assert!(result.is_failure());
    assert_eq!(f.dot(), Position::new(0, 2));
}

#[test]
fn next_backward_nth() {
    let mut f = Frame::from_str("abracadabra");
    f.set_dot(Position::new(0, 10)); // at last 'a'
    let result = f.cmd_next(LeadParam::Nint(2), &TrailParam::from_str("a"));
    assert!(result.is_success());
    // Backward from col 10: skip col 9 ('r'), find 'a' at col 7 (count 1),
    //   skip to find 'a' at col 5 (count 2). Dot lands at col 5+1=6
    assert_eq!(f.dot(), Position::new(0, 6));
}

#[test]
fn next_rejects_pindef() {
    let mut f = Frame::from_str("hello");
    let result = f.cmd_next(LeadParam::Pindef, &TrailParam::from_str("h"));
    assert!(result.is_failure());
}

#[test]
fn next_zero_count_sets_equals() {
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 3));
    let result = f.cmd_next(LeadParam::Pint(0), &TrailParam::from_str("h"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 3));
    assert_eq!(f.get_mark(MarkId::Equals), Some(Position::new(0, 3)));
}

// ===== Bridge (BR) command tests =====

#[test]
fn bridge_skips_matching_chars() {
    let mut f = Frame::from_str("   hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_bridge(LeadParam::None, &TrailParam::from_str(" "));
    assert!(result.is_success());
    // BR skips spaces, stops at first non-space 'h'
    assert_eq!(f.dot(), Position::new(0, 3));
    assert_eq!(f.get_mark(MarkId::Equals), Some(Position::new(0, 0)));
}

#[test]
fn bridge_no_matching_chars_succeeds_without_moving() {
    // If char at dot is NOT in the set, dot doesn't move and command succeeds
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_bridge(LeadParam::None, &TrailParam::from_str(" "));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 0));
}

#[test]
fn bridge_all_matching_stops_at_eol() {
    // All chars on line match, BR skips past them to EOL position
    let mut f = Frame::from_str("aaa");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_bridge(LeadParam::None, &TrailParam::from_str("a"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 3)); // virtual space after last 'a'
}

#[test]
fn bridge_with_range() {
    let mut f = Frame::from_str("123abc");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_bridge(LeadParam::None, &TrailParam::from_str("0..9"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 3)); // stops at 'a'
}

#[test]
fn bridge_backward() {
    let mut f = Frame::from_str("abc   ");
    f.set_dot(Position::new(0, 6)); // past end (virtual space)
    let result = f.cmd_bridge(LeadParam::Minus, &TrailParam::from_str(" "));
    assert!(result.is_success());
    // Skips spaces backward, stops after 'c'
    assert_eq!(f.dot(), Position::new(0, 3));
}

#[test]
fn bridge_backward_at_start_succeeds() {
    // Bridge backward at start of file succeeds
    let mut f = Frame::from_str("hello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_bridge(LeadParam::Minus, &TrailParam::from_str("a..z"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 0));
}

#[test]
fn bridge_rejects_pint() {
    let mut f = Frame::from_str("hello");
    let result = f.cmd_bridge(LeadParam::Pint(3), &TrailParam::from_str("h"));
    assert!(result.is_failure());
}

#[test]
fn bridge_crosses_lines() {
    let mut f = Frame::from_str("aaa\naab");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_bridge(LeadParam::None, &TrailParam::from_str("a"));
    assert!(result.is_success());
    // 'aaa' on line 0, space at EOL doesn't match 'a', so stops there
    // Actually, EOL space: char_matches(' ', {'a'}, bridge=true) means
    // we're checking if ' ' is NOT in {'a'} => true, so it's a match for BR
    // Wait, that's wrong. For BR, char_matches checks !chars.contains.
    // chars = {'a'}, bridge=true => we look for chars NOT in {'a'} => ' ' matches.
    // So the EOL space IS a match. Dot should stop at col 3 (space at EOL).
    assert_eq!(f.dot(), Position::new(0, 3));
}

// ===== Unicode tests =====

#[test]
fn next_finds_unicode_char() {
    let mut f = Frame::from_str("hello wörld");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_next(LeadParam::None, &TrailParam::from_str("ö"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 7));
}

#[test]
fn next_unicode_range() {
    // Range with unicode characters
    let mut f = Frame::from_str("abc\u{00e0}\u{00e1}\u{00e2}def");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_next(LeadParam::None, &TrailParam::from_str("\u{00e0}..\u{00e2}"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 3));
}

#[test]
fn bridge_skips_unicode_chars() {
    let mut f = Frame::from_str("ääähello");
    f.set_dot(Position::new(0, 0));
    let result = f.cmd_bridge(LeadParam::None, &TrailParam::from_str("ä"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 3));
}

// ===== N and BR as inverses =====

#[test]
fn next_then_bridge_round_trip() {
    // N finds first digit, BR skips past all digits
    let mut f = Frame::from_str("abc123def");
    f.set_dot(Position::new(0, 0));
    // Find first digit
    let result = f.cmd_next(LeadParam::None, &TrailParam::from_str("0..9"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 3));
    // Now bridge over the digits
    let result = f.cmd_bridge(LeadParam::None, &TrailParam::from_str("0..9"));
    assert!(result.is_success());
    assert_eq!(f.dot(), Position::new(0, 6)); // at 'd'
}
