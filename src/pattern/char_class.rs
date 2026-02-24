//! Character class membership tests for Ludwig pattern matching.

use super::ast::{CharClass, CharSet, CharSetKind};

/// Test whether `ch` is a member of `cs`.
///
/// At end-of-line positions, callers pass the virtual space character `' '`.
pub fn charset_matches(cs: &CharSet, ch: char) -> bool {
    let base = kind_matches(&cs.kind, ch);
    if cs.negated { !base } else { base }
}

fn kind_matches(kind: &CharSetKind, ch: char) -> bool {
    match kind {
        CharSetKind::Alpha => ch.is_alphabetic(),
        CharSetKind::Upper => ch.is_uppercase(),
        CharSetKind::Lower => ch.is_lowercase(),
        CharSetKind::Numeric => ch.is_ascii_digit(),
        CharSetKind::Space => ch == ' ',
        CharSetKind::Punct => "(),.;:\"'!?-`".contains(ch),
        CharSetKind::Printable => ('\x20'..='\x7e').contains(&ch),
        CharSetKind::Custom(classes) => classes.iter().any(|c| char_in_class(c, ch)),
    }
}

/// Test whether `ch` falls within a single `CharClass`.
pub fn char_in_class(class: &CharClass, ch: char) -> bool {
    match class {
        CharClass::Single(c) => *c == ch,
        CharClass::Range(lo, hi) => *lo <= ch && ch <= *hi,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::ast::{CharClass, CharSet, CharSetKind};

    fn cs(kind: CharSetKind) -> CharSet {
        CharSet {
            negated: false,
            kind,
        }
    }
    fn neg(kind: CharSetKind) -> CharSet {
        CharSet {
            negated: true,
            kind,
        }
    }

    #[test]
    fn alpha_matches_letters() {
        assert!(charset_matches(&cs(CharSetKind::Alpha), 'a'));
        assert!(charset_matches(&cs(CharSetKind::Alpha), 'Z'));
        assert!(!charset_matches(&cs(CharSetKind::Alpha), '5'));
    }

    #[test]
    fn negated_alpha_matches_non_letters() {
        assert!(charset_matches(&neg(CharSetKind::Alpha), '5'));
        assert!(!charset_matches(&neg(CharSetKind::Alpha), 'a'));
    }

    #[test]
    fn space_matches_only_ascii_space() {
        assert!(charset_matches(&cs(CharSetKind::Space), ' '));
        assert!(!charset_matches(&cs(CharSetKind::Space), '\t'));
    }

    #[test]
    fn punct_set() {
        for ch in ['(', ')', ',', '.', ';', ':', '"', '\'', '!', '?', '-', '`'] {
            assert!(
                charset_matches(&cs(CharSetKind::Punct), ch),
                "expected punct: {ch}"
            );
        }
        assert!(!charset_matches(&cs(CharSetKind::Punct), 'a'));
    }

    #[test]
    fn printable_range() {
        assert!(charset_matches(&cs(CharSetKind::Printable), ' '));
        assert!(charset_matches(&cs(CharSetKind::Printable), '~'));
        assert!(!charset_matches(&cs(CharSetKind::Printable), '\x01'));
        assert!(!charset_matches(&cs(CharSetKind::Printable), '\x7f'));
    }

    #[test]
    fn custom_range() {
        let kind = CharSetKind::Custom(vec![CharClass::Range('a', 'z')]);
        let cs = CharSet {
            negated: false,
            kind,
        };
        assert!(charset_matches(&cs, 'a'));
        assert!(charset_matches(&cs, 'm'));
        assert!(charset_matches(&cs, 'z'));
        assert!(!charset_matches(&cs, 'A'));
        assert!(!charset_matches(&cs, '0'));
    }

    #[test]
    fn custom_single() {
        let kind = CharSetKind::Custom(vec![CharClass::Single('x')]);
        let cs = CharSet {
            negated: false,
            kind,
        };
        assert!(charset_matches(&cs, 'x'));
        assert!(!charset_matches(&cs, 'y'));
    }
}
