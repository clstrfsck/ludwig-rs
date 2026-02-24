//! Recursive descent parser for Ludwig pattern strings.

use std::iter::Peekable;
use std::str::Chars;

use crate::marks::MarkId;

use super::ast::*;

/// Errors that can occur while parsing a Ludwig pattern.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternError {
    UnexpectedChar(char),
    UnexpectedEnd,
    InvalidNumber,
    UnclosedGroup,
    UnclosedString,
    UnclosedCustomSet,
    InvalidQuantifierRange,
    DereferenceNotSupported,
}

impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedChar(c) => write!(f, "Unexpected character in pattern: {c:?}"),
            Self::UnexpectedEnd => write!(f, "Unexpected end of pattern"),
            Self::InvalidNumber => write!(f, "Invalid number in pattern"),
            Self::UnclosedGroup => write!(f, "Unclosed group '(' in pattern"),
            Self::UnclosedString => write!(f, "Unclosed string literal in pattern"),
            Self::UnclosedCustomSet => write!(f, "Unclosed custom character set"),
            Self::InvalidQuantifierRange => write!(f, "Invalid quantifier range [n,m]"),
            Self::DereferenceNotSupported => {
                write!(f, "Dereference ($...$, &...&) not yet supported")
            }
        }
    }
}

/// Parse a Ludwig pattern string into a [`PatternDef`].
pub fn parse(input: &str) -> Result<PatternDef, PatternError> {
    Parser {
        chars: input.chars().peekable(),
    }
    .parse_pattern_def()
}

struct Parser<'a> {
    chars: Peekable<Chars<'a>>,
}

impl Parser<'_> {
    fn parse_pattern_def(&mut self) -> Result<PatternDef, PatternError> {
        let first = self.parse_compound()?;

        self.skip_space();
        if self.chars.peek() == Some(&',') {
            self.chars.next(); // consume first ','
            let second = self.parse_compound()?;

            self.skip_space();
            if self.chars.peek() == Some(&',') {
                self.chars.next(); // consume second ','
                let third = self.parse_compound()?;
                Ok(PatternDef {
                    left: first,
                    middle: second,
                    right: third,
                })
            } else {
                // two parts: left,middle; right is empty
                Ok(PatternDef {
                    left: first,
                    middle: second,
                    right: Compound::empty(),
                })
            }
        } else {
            // single compound — it is the middle context
            Ok(PatternDef {
                left: Compound::empty(),
                middle: first,
                right: Compound::empty(),
            })
        }
    }

    fn parse_compound(&mut self) -> Result<Compound, PatternError> {
        let mut alternatives = Vec::new();
        loop {
            let seq = self.parse_sequence()?;
            alternatives.push(seq);
            self.skip_space();
            if self.chars.peek() == Some(&'|') {
                self.chars.next(); // consume '|'
            } else {
                break;
            }
        }
        // Normalise: a single empty-sequence alternative is the canonical empty compound.
        // This ensures patterns like ",A" produce Compound::empty() for the left part.
        if alternatives.len() == 1 && alternatives[0].items.is_empty() {
            return Ok(Compound::empty());
        }
        Ok(Compound { alternatives })
    }

    fn parse_sequence(&mut self) -> Result<Sequence, PatternError> {
        let mut items = Vec::new();
        loop {
            self.skip_space();
            match self.parse_item()? {
                Some(item) => items.push(item),
                None => break,
            }
        }
        Ok(Sequence { items })
    }

    /// Parse one item (optional quantifier + element).
    ///
    /// Returns `None` at sequence terminators: `|`, `,`, `)`, end-of-input.
    fn parse_item(&mut self) -> Result<Option<Item>, PatternError> {
        self.skip_space();

        // Peek for terminators before consuming
        match self.chars.peek() {
            None | Some('|') | Some(')') | Some(',') => return Ok(None),
            _ => {}
        }

        let quantifier = self.parse_quantifier()?;

        // After a quantifier, skip spaces and re-check for terminators
        self.skip_space();
        match self.chars.peek() {
            None | Some('|') | Some(')') | Some(',') => {
                if quantifier.is_some() {
                    // A quantifier with no following element is an error
                    return Err(PatternError::UnexpectedEnd);
                }
                return Ok(None);
            }
            _ => {}
        }

        let element = self.parse_element()?;
        Ok(Some(Item {
            quantifier: quantifier.unwrap_or(Quantifier::Once),
            element,
        }))
    }

    /// Try to parse a quantifier prefix.  Returns `None` if the next token is
    /// not a quantifier.
    fn parse_quantifier(&mut self) -> Result<Option<Quantifier>, PatternError> {
        match self.chars.peek() {
            Some('*') => {
                self.chars.next();
                Ok(Some(Quantifier::ZeroOrMore))
            }
            Some('+') => {
                self.chars.next();
                Ok(Some(Quantifier::OneOrMore))
            }
            Some('[') => {
                self.chars.next();
                Ok(Some(self.parse_bracket_quantifier()?))
            }
            Some(&c) if c.is_ascii_digit() => Ok(Some(Quantifier::Exactly(self.parse_number()?))),
            _ => Ok(None),
        }
    }

    /// Parse a `[n,m]`-style quantifier (the opening `[` has already been consumed).
    fn parse_bracket_quantifier(&mut self) -> Result<Quantifier, PatternError> {
        let first = if matches!(self.chars.peek(), Some(',') | Some(']')) {
            None
        } else {
            Some(self.parse_number()?)
        };

        // Single-number form: [n] = Exactly(n)
        if self.chars.peek() == Some(&']') {
            self.chars.next();
            return Ok(Quantifier::Exactly(
                first.ok_or(PatternError::InvalidQuantifierRange)?,
            ));
        }

        // Expect ','
        match self.chars.next() {
            Some(',') => {}
            _ => return Err(PatternError::InvalidQuantifierRange),
        }

        let second = if self.chars.peek() == Some(&']') {
            None
        } else {
            Some(self.parse_number()?)
        };

        match self.chars.next() {
            Some(']') => {}
            _ => return Err(PatternError::UnclosedGroup),
        }

        match (first, second) {
            (None, None) => Ok(Quantifier::ZeroOrMore),       // [,]
            (Some(n), None) => Ok(Quantifier::AtLeast(n)),    // [n,]
            (None, Some(m)) => Ok(Quantifier::Between(0, m)), // [,m]
            (Some(n), Some(m)) if n <= m => Ok(Quantifier::Between(n, m)),
            _ => Err(PatternError::InvalidQuantifierRange),
        }
    }

    fn parse_number(&mut self) -> Result<usize, PatternError> {
        let mut s = String::new();
        while let Some(&c) = self.chars.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.chars.next();
            } else {
                break;
            }
        }
        if s.is_empty() {
            return Err(PatternError::InvalidNumber);
        }
        s.parse::<usize>().map_err(|_| PatternError::InvalidNumber)
    }

    fn parse_element(&mut self) -> Result<Element, PatternError> {
        match self.chars.peek() {
            None => Err(PatternError::UnexpectedEnd),
            Some(&'(') => {
                self.chars.next();
                let compound = self.parse_compound()?;
                match self.chars.next() {
                    Some(')') => {}
                    _ => return Err(PatternError::UnclosedGroup),
                }
                Ok(Element::Group(Box::new(compound)))
            }
            Some(&'\'') | Some(&'"') => {
                let (text, case_fold) = self.parse_string()?;
                Ok(Element::Literal { text, case_fold })
            }
            Some(&'<') => {
                self.chars.next();
                Ok(Element::Positional(Positional::BegLine))
            }
            Some(&'>') => {
                self.chars.next();
                Ok(Element::Positional(Positional::EndLine))
            }
            Some(&'{') => {
                self.chars.next();
                Ok(Element::Positional(Positional::LeftMargin))
            }
            Some(&'}') => {
                self.chars.next();
                Ok(Element::Positional(Positional::RightMargin))
            }
            Some(&'^') => {
                self.chars.next();
                Ok(Element::Positional(Positional::DotCol))
            }
            Some(&'@') => {
                self.chars.next();
                let digit = match self.chars.next() {
                    Some(c) if c.is_ascii_digit() && c != '0' => c as u8 - b'0',
                    Some(c) => return Err(PatternError::UnexpectedChar(c)),
                    None => return Err(PatternError::UnexpectedEnd),
                };
                Ok(Element::MarkCheck(MarkId::Numbered(digit)))
            }
            Some(&'$') | Some(&'&') => Err(PatternError::DereferenceNotSupported),
            Some(&'-') => {
                self.chars.next();
                let kind = self.parse_charset_kind()?;
                Ok(Element::CharSet(CharSet {
                    negated: true,
                    kind,
                }))
            }
            Some(&c) if is_charset_letter(c) => {
                let kind = self.parse_charset_kind()?;
                Ok(Element::CharSet(CharSet {
                    negated: false,
                    kind,
                }))
            }
            Some(&c) => Err(PatternError::UnexpectedChar(c)),
        }
    }

    fn parse_charset_kind(&mut self) -> Result<CharSetKind, PatternError> {
        match self.chars.next() {
            None => Err(PatternError::UnexpectedEnd),
            Some(c) => match c.to_ascii_uppercase() {
                'A' => Ok(CharSetKind::Alpha),
                'U' => Ok(CharSetKind::Upper),
                'L' => Ok(CharSetKind::Lower),
                'N' => Ok(CharSetKind::Numeric),
                'S' => Ok(CharSetKind::Space),
                'P' => Ok(CharSetKind::Punct),
                'C' => Ok(CharSetKind::Printable),
                'D' => Ok(CharSetKind::Custom(self.parse_custom_set()?)),
                other => Err(PatternError::UnexpectedChar(other)),
            },
        }
    }

    /// Parse a custom set `D<dlm>...<dlm>` (the `D` has already been consumed).
    fn parse_custom_set(&mut self) -> Result<Vec<CharClass>, PatternError> {
        let dlm = self.chars.next().ok_or(PatternError::UnexpectedEnd)?;
        let mut classes = Vec::new();
        let mut pending: Option<char> = None;

        loop {
            match self.chars.next() {
                None => return Err(PatternError::UnclosedCustomSet),
                Some(c) if c == dlm => {
                    if let Some(p) = pending.take() {
                        classes.push(CharClass::Single(p));
                    }
                    break;
                }
                Some('.') => {
                    if self.chars.peek() == Some(&'.') {
                        // '..' range notation
                        self.chars.next(); // consume second '.'
                        match self.chars.next() {
                            Some(c) if c != dlm => {
                                if let Some(lo) = pending.take() {
                                    classes.push(CharClass::Range(lo, c));
                                } else {
                                    // No preceding char — treat all three as singles
                                    classes.push(CharClass::Single('.'));
                                    classes.push(CharClass::Single('.'));
                                    classes.push(CharClass::Single(c));
                                }
                            }
                            _ => return Err(PatternError::UnclosedCustomSet),
                        }
                    } else {
                        // single '.'
                        if let Some(p) = pending.take() {
                            classes.push(CharClass::Single(p));
                        }
                        pending = Some('.');
                    }
                }
                Some(c) => {
                    if let Some(p) = pending.take() {
                        classes.push(CharClass::Single(p));
                    }
                    pending = Some(c);
                }
            }
        }
        Ok(classes)
    }

    /// Parse a `'...'` or `"..."` string literal.
    ///
    /// Returns `(text, case_fold)` where `case_fold = true` for single-quote.
    fn parse_string(&mut self) -> Result<(String, bool), PatternError> {
        let delim = self.chars.next().ok_or(PatternError::UnexpectedEnd)?;
        let case_fold = delim == '\'';
        let mut text = String::new();
        loop {
            match self.chars.next() {
                Some(c) if c == delim => break,
                Some(c) => text.push(c),
                None => return Err(PatternError::UnclosedString),
            }
        }
        Ok((text, case_fold))
    }

    fn skip_space(&mut self) {
        while matches!(self.chars.peek(), Some(' ') | Some('\t')) {
            self.chars.next();
        }
    }
}

/// True for characters that can begin a named charset (`A`, `U`, `L`, `N`, `S`, `P`, `C`, `D`).
fn is_charset_letter(c: char) -> bool {
    matches!(
        c.to_ascii_uppercase(),
        'A' | 'U' | 'L' | 'N' | 'S' | 'P' | 'C' | 'D'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(s: &str) -> PatternDef {
        parse(s).expect("parse should succeed")
    }
    fn parse_err(s: &str) -> PatternError {
        parse(s).expect_err("parse should fail")
    }

    fn middle_items(p: PatternDef) -> Vec<Item> {
        assert_eq!(
            p.middle.alternatives.len(),
            1,
            "expected single alternative"
        );
        p.middle.alternatives.into_iter().next().unwrap().items
    }

    // --- Literals ---

    #[test]
    fn test_double_quote_literal() {
        let p = parse_ok(r#""hello""#);
        let items = middle_items(p);
        assert_eq!(items.len(), 1);
        match &items[0].element {
            Element::Literal { text, case_fold } => {
                assert_eq!(text, "hello");
                assert!(!case_fold, "double-quote should be case-sensitive");
            }
            _ => panic!("expected Literal"),
        }
    }

    #[test]
    fn test_single_quote_literal_case_fold() {
        let p = parse_ok("'hello'");
        let items = middle_items(p);
        match &items[0].element {
            Element::Literal { text, case_fold } => {
                assert_eq!(text, "hello");
                assert!(*case_fold, "single-quote should be case-insensitive");
            }
            _ => panic!("expected Literal"),
        }
    }

    // --- Named charsets ---

    #[test]
    fn test_alpha_charset() {
        let p = parse_ok("A");
        let items = middle_items(p);
        match &items[0].element {
            Element::CharSet(cs) => {
                assert!(!cs.negated);
                assert!(matches!(cs.kind, CharSetKind::Alpha));
            }
            _ => panic!("expected CharSet"),
        }
    }

    #[test]
    fn test_charset_lowercase() {
        let p = parse_ok("n");
        let items = middle_items(p);
        match &items[0].element {
            Element::CharSet(cs) => assert!(matches!(cs.kind, CharSetKind::Numeric)),
            _ => panic!("expected CharSet"),
        }
    }

    #[test]
    fn test_negated_charset() {
        let p = parse_ok("-S");
        let items = middle_items(p);
        match &items[0].element {
            Element::CharSet(cs) => {
                assert!(cs.negated);
                assert!(matches!(cs.kind, CharSetKind::Space));
            }
            _ => panic!("expected CharSet"),
        }
    }

    // --- Custom set ---

    #[test]
    fn test_custom_set_range() {
        let p = parse_ok("D/a..z/");
        let items = middle_items(p);
        match &items[0].element {
            Element::CharSet(cs) => match &cs.kind {
                CharSetKind::Custom(classes) => {
                    assert_eq!(classes.len(), 1);
                    assert!(matches!(&classes[0], CharClass::Range('a', 'z')));
                }
                _ => panic!("expected Custom"),
            },
            _ => panic!("expected CharSet"),
        }
    }

    #[test]
    fn test_custom_set_singles() {
        let p = parse_ok("D/abc/");
        let items = middle_items(p);
        match &items[0].element {
            Element::CharSet(cs) => match &cs.kind {
                CharSetKind::Custom(classes) => {
                    assert_eq!(classes.len(), 3);
                }
                _ => panic!("expected Custom"),
            },
            _ => panic!("expected CharSet"),
        }
    }

    // --- Quantifiers ---

    #[test]
    fn test_zero_or_more() {
        let p = parse_ok("*A");
        let items = middle_items(p);
        assert!(matches!(items[0].quantifier, Quantifier::ZeroOrMore));
    }

    #[test]
    fn test_one_or_more() {
        let p = parse_ok("+A");
        let items = middle_items(p);
        assert!(matches!(items[0].quantifier, Quantifier::OneOrMore));
    }

    #[test]
    fn test_exactly_n() {
        let p = parse_ok("3A");
        let items = middle_items(p);
        assert_eq!(items[0].quantifier, Quantifier::Exactly(3));
    }

    #[test]
    fn test_bracket_exactly() {
        let p = parse_ok("[3]A");
        let items = middle_items(p);
        assert_eq!(items[0].quantifier, Quantifier::Exactly(3));
    }

    #[test]
    fn test_bracket_at_least() {
        let p = parse_ok("[3,]A");
        let items = middle_items(p);
        assert_eq!(items[0].quantifier, Quantifier::AtLeast(3));
    }

    #[test]
    fn test_bracket_between() {
        let p = parse_ok("[2,5]N");
        let items = middle_items(p);
        assert_eq!(items[0].quantifier, Quantifier::Between(2, 5));
    }

    #[test]
    fn test_bracket_zero_or_more() {
        let p = parse_ok("[,]A");
        let items = middle_items(p);
        assert!(matches!(items[0].quantifier, Quantifier::ZeroOrMore));
    }

    #[test]
    fn test_bracket_zero_to_m() {
        let p = parse_ok("[,5]A");
        let items = middle_items(p);
        assert_eq!(items[0].quantifier, Quantifier::Between(0, 5));
    }

    // --- Alternation ---

    #[test]
    fn test_alternation() {
        let p = parse_ok("A|N");
        assert_eq!(p.middle.alternatives.len(), 2);
    }

    #[test]
    fn test_group() {
        let p = parse_ok("(A|N)");
        let items = middle_items(p);
        assert_eq!(items.len(), 1);
        match &items[0].element {
            Element::Group(c) => assert_eq!(c.alternatives.len(), 2),
            _ => panic!("expected Group"),
        }
    }

    // --- Positionals ---

    #[test]
    fn test_beg_line() {
        let p = parse_ok("<");
        let items = middle_items(p);
        assert!(matches!(
            &items[0].element,
            Element::Positional(Positional::BegLine)
        ));
    }

    #[test]
    fn test_end_line() {
        let p = parse_ok(">");
        let items = middle_items(p);
        assert!(matches!(
            &items[0].element,
            Element::Positional(Positional::EndLine)
        ));
    }

    #[test]
    fn test_dot_col() {
        let p = parse_ok("^");
        let items = middle_items(p);
        assert!(matches!(
            &items[0].element,
            Element::Positional(Positional::DotCol)
        ));
    }

    // --- Mark checks ---

    #[test]
    fn test_mark_check() {
        let p = parse_ok("@3");
        let items = middle_items(p);
        match &items[0].element {
            Element::MarkCheck(MarkId::Numbered(3)) => {}
            _ => panic!("expected MarkCheck(3)"),
        }
    }

    // --- Context separation ---

    #[test]
    fn test_three_part_context() {
        let p = parse_ok("A,N,S");
        assert!(!p.left.is_empty_pattern());
        assert!(!p.middle.is_empty_pattern());
        assert!(!p.right.is_empty_pattern());
        // left = Alpha
        match &p.left.alternatives[0].items[0].element {
            Element::CharSet(cs) => assert!(matches!(cs.kind, CharSetKind::Alpha)),
            _ => panic!("left should be Alpha"),
        }
        // middle = Numeric
        match &p.middle.alternatives[0].items[0].element {
            Element::CharSet(cs) => assert!(matches!(cs.kind, CharSetKind::Numeric)),
            _ => panic!("middle should be Numeric"),
        }
    }

    #[test]
    fn test_two_part_context_left_empty() {
        let p = parse_ok(",'hello'");
        assert!(p.left.is_empty_pattern(), "left should be empty");
        assert!(!p.middle.is_empty_pattern());
    }

    #[test]
    fn test_single_compound_is_middle() {
        let p = parse_ok("A");
        assert!(p.left.is_empty_pattern());
        assert!(p.right.is_empty_pattern());
        assert!(!p.middle.is_empty_pattern());
    }

    // --- Whitespace ---

    #[test]
    fn test_whitespace_between_items() {
        let p = parse_ok("A N");
        let items = middle_items(p);
        assert_eq!(items.len(), 2);
    }

    // --- Errors ---

    #[test]
    fn test_unclosed_group() {
        assert!(matches!(parse_err("(A"), PatternError::UnclosedGroup));
    }

    #[test]
    fn test_unclosed_string() {
        assert!(matches!(parse_err("'hello"), PatternError::UnclosedString));
    }

    #[test]
    fn test_dereference_not_supported() {
        assert!(matches!(
            parse_err("$name$"),
            PatternError::DereferenceNotSupported
        ));
    }

    #[test]
    fn test_invalid_bracket_range() {
        assert!(matches!(
            parse_err("[5,2]A"),
            PatternError::InvalidQuantifierRange
        ));
    }
}
