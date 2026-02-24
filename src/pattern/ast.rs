//! AST types for Ludwig patterns.

use crate::marks::MarkId;

/// A fully parsed Ludwig pattern with optional left/middle/right contexts.
///
/// A pattern like `A,N,S` has left=Alpha, middle=Numeric, right=Space.
/// A simple pattern like `"hello"` has left=empty, middle=Literal, right=empty.
#[derive(Debug)]
pub struct PatternDef {
    pub left: Compound,
    pub middle: Compound,
    pub right: Compound,
}

/// An alternation of one or more sequences (`|`-separated).
///
/// An empty `alternatives` vec is the canonical empty compound; it matches
/// the empty string at any position.
#[derive(Debug)]
pub struct Compound {
    pub alternatives: Vec<Sequence>,
}

impl Compound {
    pub fn empty() -> Self {
        Self {
            alternatives: vec![],
        }
    }

    pub fn is_empty_pattern(&self) -> bool {
        self.alternatives.is_empty()
    }
}

/// A concatenation of items.
#[derive(Debug)]
pub struct Sequence {
    pub items: Vec<Item>,
}

/// One quantified element.
#[derive(Debug)]
pub struct Item {
    pub quantifier: Quantifier,
    pub element: Element,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Quantifier {
    Once,
    ZeroOrMore,
    OneOrMore,
    Exactly(usize),
    AtLeast(usize),
    Between(usize, usize),
}

#[derive(Debug)]
pub enum Element {
    CharSet(CharSet),
    Group(Box<Compound>),
    /// `case_fold = true` for single-quote strings (case-insensitive).
    Literal {
        text: String,
        case_fold: bool,
    },
    Positional(Positional),
    MarkCheck(MarkId),
}

#[derive(Debug)]
pub struct CharSet {
    pub negated: bool,
    pub kind: CharSetKind,
}

#[derive(Debug)]
pub enum CharSetKind {
    Alpha,     // A — alphabetic
    Upper,     // U — uppercase
    Lower,     // L — lowercase
    Numeric,   // N — ASCII digit
    Space,     // S — ASCII space only
    Punct,     // P — (),.;:"'!?-`
    Printable, // C — 0x20..0x7E
    Custom(Vec<CharClass>),
}

#[derive(Debug)]
pub enum CharClass {
    Single(char),
    Range(char, char),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Positional {
    BegLine,     // < — pos == 0
    EndLine,     // > — pos == line length
    LeftMargin,  // { — pos == ctx.left_margin
    RightMargin, // } — pos == ctx.right_margin
    DotCol,      // ^ — pos == ctx.dot_col
}
