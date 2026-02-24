//! Pattern matcher: find matches of a [`PatternDef`] in a line of text.
//!
//! All positions are **character** (not byte) indices into the line.

use crate::marks::MarkSet;

use super::ast::*;
use super::char_class::charset_matches;

/// Context provided to the matcher for a single line.
pub struct MatchCtx {
    /// Line content as a character vector (no trailing newline).
    pub line: Vec<char>,
    /// Original dot column (used for the `^` positional).
    pub dot_col: usize,
    /// Left margin column (`{`). Currently always 0.
    pub left_margin: usize,
    /// Right margin column (`}`). Currently always `line.len()`.
    pub right_margin: usize,
    /// 0-based index of this line in the frame (for `@N` mark checks).
    pub line_idx: usize,
    /// All marks in the frame (cloned for immutable access during matching).
    pub marks: MarkSet,
}

/// The result of a successful pattern match.
pub struct MatchResult {
    /// Column of the start of the middle context. Becomes the new Equals mark.
    pub middle_start: usize,
    /// Column of the end of the middle context. Becomes the new Dot.
    pub middle_end: usize,
}

/// Maximum number of backtracking steps before giving up.
const MAX_STEPS: usize = 100_000;

// ─── Public API ─────────────────────────────────────────────────────────────

/// Find the leftmost match at or after `start_col`.
///
/// Used by G (forward search).
pub fn find_forward(pattern: &PatternDef, ctx: &MatchCtx, start_col: usize) -> Option<MatchResult> {
    let len = ctx.line.len();
    let mut steps = 0usize;

    if pattern.left.is_empty_pattern() {
        // Optimisation: iterate mid_start directly from start_col
        for mid_start in start_col..=len {
            if let Some(mid_end) = match_compound(&pattern.middle, ctx, mid_start, &mut steps)
                && match_compound(&pattern.right, ctx, mid_end, &mut steps).is_some()
            {
                return Some(MatchResult {
                    middle_start: mid_start,
                    middle_end: mid_end,
                });
            }
            if steps > MAX_STEPS {
                return None;
            }
        }
    } else {
        for outer_start in 0..=len {
            if let Some(mid_start) = match_compound(&pattern.left, ctx, outer_start, &mut steps)
                && mid_start >= start_col
                && let Some(mid_end) = match_compound(&pattern.middle, ctx, mid_start, &mut steps)
                && match_compound(&pattern.right, ctx, mid_end, &mut steps).is_some()
            {
                return Some(MatchResult {
                    middle_start: mid_start,
                    middle_end: mid_end,
                });
            }
            if steps > MAX_STEPS {
                return None;
            }
        }
    }
    None
}

/// Find the rightmost match where mid_start ≤ `start_col`.
///
/// Used by G (backward search).
pub fn find_backward(
    pattern: &PatternDef,
    ctx: &MatchCtx,
    start_col: usize,
) -> Option<MatchResult> {
    let len = ctx.line.len();
    let upper = start_col.min(len);
    let mut last_match = None;
    let mut steps = 0usize;

    if pattern.left.is_empty_pattern() {
        for mid_start in 0..=upper {
            if let Some(mid_end) = match_compound(&pattern.middle, ctx, mid_start, &mut steps)
                && match_compound(&pattern.right, ctx, mid_end, &mut steps).is_some()
            {
                last_match = Some(MatchResult {
                    middle_start: mid_start,
                    middle_end: mid_end,
                });
            }
            if steps > MAX_STEPS {
                break;
            }
        }
    } else {
        for outer_start in 0..=upper {
            if let Some(mid_start) = match_compound(&pattern.left, ctx, outer_start, &mut steps)
                && mid_start <= upper
                && let Some(mid_end) = match_compound(&pattern.middle, ctx, mid_start, &mut steps)
                && match_compound(&pattern.right, ctx, mid_end, &mut steps).is_some()
            {
                last_match = Some(MatchResult {
                    middle_start: mid_start,
                    middle_end: mid_end,
                });
            }
            if steps > MAX_STEPS {
                break;
            }
        }
    }
    last_match
}

/// Test whether a match begins exactly at `col`.
///
/// Used by EQS (equality predicate).
pub fn match_at(pattern: &PatternDef, ctx: &MatchCtx, col: usize) -> Option<MatchResult> {
    let mut steps = 0usize;

    // Left context must end at col
    if !pattern.left.is_empty_pattern() {
        let left_ok = (0..=col).any(|start| {
            match_compound(&pattern.left, ctx, start, &mut steps)
                .map(|end| end == col)
                .unwrap_or(false)
        });
        if !left_ok {
            return None;
        }
    }

    let mid_end = match_compound(&pattern.middle, ctx, col, &mut steps)?;
    match_compound(&pattern.right, ctx, mid_end, &mut steps)?;
    Some(MatchResult {
        middle_start: col,
        middle_end: mid_end,
    })
}

// ─── Core matching functions ─────────────────────────────────────────────────

/// Match a compound (alternation) starting at `pos`.  Returns the end position
/// of the first successful alternative, or `None`.
///
/// An empty compound (no alternatives) always succeeds, returning `Some(pos)`.
fn match_compound(
    compound: &Compound,
    ctx: &MatchCtx,
    pos: usize,
    steps: &mut usize,
) -> Option<usize> {
    if compound.alternatives.is_empty() {
        return Some(pos); // empty compound = always match
    }
    for seq in &compound.alternatives {
        if let Some(end) = match_sequence(seq, ctx, pos, steps) {
            return Some(end);
        }
    }
    None
}

fn match_sequence(seq: &Sequence, ctx: &MatchCtx, pos: usize, steps: &mut usize) -> Option<usize> {
    match_items_backtrack(&seq.items, ctx, pos, steps)
}

/// Backtracking sequence matcher.
///
/// Tries each item in order, using greedy-first positions for quantified items.
/// Backtracks to earlier items when a later item fails to match.
fn match_items_backtrack(
    items: &[Item],
    ctx: &MatchCtx,
    pos: usize,
    steps: &mut usize,
) -> Option<usize> {
    *steps += 1;
    if *steps > MAX_STEPS {
        return None;
    }
    if items.is_empty() {
        return Some(pos);
    }
    let item = &items[0];
    let rest = &items[1..];
    for end in item_positions(item, ctx, pos, steps) {
        if let Some(result) = match_items_backtrack(rest, ctx, end, steps) {
            return Some(result);
        }
    }
    None
}

/// Return candidate end positions for `item` starting at `pos`, in greedy-first
/// order (most characters consumed first).
fn item_positions(item: &Item, ctx: &MatchCtx, pos: usize, steps: &mut usize) -> Vec<usize> {
    match &item.quantifier {
        Quantifier::Once => match match_element_once(&item.element, ctx, pos, steps) {
            Some(p) => vec![p],
            None => vec![],
        },
        Quantifier::ZeroOrMore => {
            greedy_positions(&item.element, ctx, pos, 0, ctx.line.len() + 2, steps)
        }
        Quantifier::OneOrMore => {
            greedy_positions(&item.element, ctx, pos, 1, ctx.line.len() + 2, steps)
        }
        Quantifier::Exactly(n) => match exact_matches(&item.element, ctx, pos, *n, steps) {
            Some(p) => vec![p],
            None => vec![],
        },
        Quantifier::AtLeast(n) => {
            greedy_positions(&item.element, ctx, pos, *n, ctx.line.len() + 2, steps)
        }
        Quantifier::Between(lo, hi) => greedy_positions(&item.element, ctx, pos, *lo, *hi, steps),
    }
}

/// Collect end positions for `min..max` repetitions of `element` starting at
/// `pos`, then reverse to yield the greediest (most chars) first.
fn greedy_positions(
    element: &Element,
    ctx: &MatchCtx,
    start: usize,
    min: usize,
    max: usize,
    steps: &mut usize,
) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut cur = start;
    let mut count = 0usize;

    loop {
        if count >= min {
            positions.push(cur);
        }
        if count >= max {
            break;
        }
        match match_element_once(element, ctx, cur, steps) {
            Some(next) if next != cur => {
                cur = next;
                count += 1;
            }
            _ => break, // no progress (zero-width infinite loop guard) or no match
        }
    }
    positions.reverse(); // greedy-first = most-first
    positions
}

fn exact_matches(
    element: &Element,
    ctx: &MatchCtx,
    start: usize,
    n: usize,
    steps: &mut usize,
) -> Option<usize> {
    let mut cur = start;
    for _ in 0..n {
        cur = match_element_once(element, ctx, cur, steps)?;
    }
    Some(cur)
}

// ─── Element matching ────────────────────────────────────────────────────────

/// Try to match `element` at position `pos` (in chars).
/// Returns the new position if successful, or `None`.
fn match_element_once(
    element: &Element,
    ctx: &MatchCtx,
    pos: usize,
    steps: &mut usize,
) -> Option<usize> {
    match element {
        Element::CharSet(cs) => {
            let ch = char_at(&ctx.line, pos);
            if charset_matches(cs, ch) {
                Some(pos + 1)
            } else {
                None
            }
        }
        Element::Literal { text, case_fold } => {
            let mut cur = pos;
            for pat_ch in text.chars() {
                let line_ch = char_at(&ctx.line, cur);
                let matched = if *case_fold {
                    line_ch.eq_ignore_ascii_case(&pat_ch)
                } else {
                    line_ch == pat_ch
                };
                if !matched {
                    return None;
                }
                cur += 1;
            }
            Some(cur)
        }
        Element::Group(compound) => match_compound(compound, ctx, pos, steps),
        Element::Positional(p) => {
            let ok = match p {
                Positional::BegLine => pos == 0,
                Positional::EndLine => pos == ctx.line.len(),
                Positional::LeftMargin => pos == ctx.left_margin,
                Positional::RightMargin => pos == ctx.right_margin,
                Positional::DotCol => pos == ctx.dot_col,
            };
            if ok { Some(pos) } else { None }
        }
        Element::MarkCheck(id) => {
            let mark_pos = ctx.marks.get(*id)?;
            if mark_pos.line == ctx.line_idx && mark_pos.column == pos {
                Some(pos)
            } else {
                None
            }
        }
    }
}

/// Get the character at position `pos`, returning virtual space `' '` at EOL.
fn char_at(line: &[char], pos: usize) -> char {
    line.get(pos).copied().unwrap_or(' ')
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marks::MarkSet;
    use crate::pattern::parser::parse;

    fn ctx(line: &str) -> MatchCtx {
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        MatchCtx {
            line: chars,
            dot_col: 0,
            left_margin: 0,
            right_margin: len,
            line_idx: 0,
            marks: MarkSet::new(),
        }
    }

    fn fwd(pattern_str: &str, line: &str, start: usize) -> Option<(usize, usize)> {
        let p = parse(pattern_str).unwrap();
        let c = ctx(line);
        find_forward(&p, &c, start).map(|m| (m.middle_start, m.middle_end))
    }

    fn bwd(pattern_str: &str, line: &str, start: usize) -> Option<(usize, usize)> {
        let p = parse(pattern_str).unwrap();
        let c = ctx(line);
        find_backward(&p, &c, start).map(|m| (m.middle_start, m.middle_end))
    }

    fn mat(pattern_str: &str, line: &str, col: usize) -> Option<(usize, usize)> {
        let p = parse(pattern_str).unwrap();
        let c = ctx(line);
        match_at(&p, &c, col).map(|m| (m.middle_start, m.middle_end))
    }

    // --- Literals ---

    #[test]
    fn literal_double_quote_forward() {
        assert_eq!(fwd(r#""hello""#, "hello world", 0), Some((0, 5)));
    }

    #[test]
    fn literal_single_quote_case_fold() {
        assert_eq!(fwd("'HELLO'", "hello world", 0), Some((0, 5)));
    }

    #[test]
    fn literal_not_at_start() {
        assert_eq!(fwd(r#""world""#, "hello world", 0), Some((6, 11)));
    }

    #[test]
    fn literal_no_match() {
        assert_eq!(fwd(r#""xyz""#, "hello world", 0), None);
    }

    // --- Named charsets ---

    #[test]
    fn alpha_once() {
        assert_eq!(fwd("A", "abc", 0), Some((0, 1)));
    }

    #[test]
    fn numeric_once() {
        assert_eq!(fwd("N", "abc123", 0), Some((3, 4)));
    }

    #[test]
    fn space_once() {
        assert_eq!(fwd("S", "hello world", 0), Some((5, 6)));
    }

    #[test]
    fn negated_space() {
        assert_eq!(fwd("-S", "hello", 0), Some((0, 1)));
    }

    // --- Quantifiers ---

    #[test]
    fn exactly_n() {
        assert_eq!(fwd("3A", "abcdef", 0), Some((0, 3)));
    }

    #[test]
    fn zero_or_more_greedy() {
        // *A"." — greedily match as many alpha as possible, then "."
        assert_eq!(fwd(r#"*A".""#, "abc.", 0), Some((0, 4)));
    }

    #[test]
    fn one_or_more_spaces() {
        assert_eq!(fwd("+S", "  hello", 0), Some((0, 2)));
    }

    #[test]
    fn zero_or_more_zero_matches() {
        // *N at a non-digit position matches 0 digits (returns start..start)
        assert_eq!(fwd("*N", "abc", 0), Some((0, 0)));
    }

    // --- Alternation ---

    #[test]
    fn alternation_first_wins() {
        // A|N — first char is digit; first alternative (Alpha) fails, second (Numeric) wins
        assert_eq!(fwd("A|N", "5abc", 0), Some((0, 1)));
    }

    #[test]
    fn alternation_second_wins() {
        assert_eq!(fwd("N|A", "abc", 0), Some((0, 1)));
    }

    // --- Positionals ---

    #[test]
    fn beg_line_matches_at_0() {
        assert_eq!(fwd("<A", "abc", 0), Some((0, 1)));
    }

    #[test]
    fn beg_line_fails_not_at_0() {
        // <A where first char is a space: BegLine OK at pos 0, but A fails on ' '
        assert_eq!(fwd("<A", "  abc", 0), None);
    }

    #[test]
    fn end_line_positional() {
        // >  matches only at end of line
        let p = parse(">").unwrap();
        let c = ctx("ab");
        assert_eq!(find_forward(&p, &c, 0).map(|m| m.middle_start), Some(2));
    }

    // --- Context patterns ---

    #[test]
    fn left_right_context() {
        // "he",'lo'  — left="he", middle='lo'
        // "helo" = h(0) e(1) l(2) o(3): "he" at [0,2), 'lo' immediately after at [2,4)
        assert_eq!(fwd(r#""he",'lo'"#, "helo", 0), Some((2, 4)));
    }

    #[test]
    fn right_context_restricts() {
        // ,A," " — left=empty, middle=A, right=" "
        // Finds the alpha immediately before a space; returns the alpha's span.
        assert_eq!(fwd(r#",A," ""#, "hello world", 0), Some((4, 5)));
    }

    // --- match_at ---

    #[test]
    fn match_at_success() {
        assert_eq!(mat("A", "abc", 0), Some((0, 1)));
    }

    #[test]
    fn match_at_wrong_pos() {
        // N at pos 0 in "abc" — no digit there
        assert_eq!(mat("N", "abc", 0), None);
    }

    #[test]
    fn match_at_correct_pos() {
        assert_eq!(mat("N", "abc123", 3), Some((3, 4)));
    }

    // --- Backward search ---

    #[test]
    fn backward_finds_last() {
        // Find rightmost 'A' at or before col 4 in "abcde"
        // 'a'=0,'b'=1,'c'=2,'d'=3,'e'=4  — last at col<=4 is 'd' (pos 3..4)
        assert_eq!(bwd("A", "abcde", 4), Some((4, 5)));
    }

    #[test]
    fn backward_respects_start_col() {
        // Only look at positions 0..=2 in "abcde"
        assert_eq!(bwd("A", "abcde", 2), Some((2, 3)));
    }

    // --- Custom set ---

    #[test]
    fn custom_set_range() {
        assert_eq!(fwd("D/a..z/", "ABC", 0), None);
        assert_eq!(fwd("D/a..z/", "abc", 0), Some((0, 1)));
    }

    // --- Virtual space ---

    #[test]
    fn virtual_space_at_eol() {
        // S should match the virtual space at end of "abc"
        let p = parse("S").unwrap();
        let c = ctx("abc");
        // Position 3 is end-of-line (virtual space)
        let result = find_forward(&p, &c, 3).map(|m| (m.middle_start, m.middle_end));
        assert_eq!(result, Some((3, 4)));
    }
}
