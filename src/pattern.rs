//! Ludwig pattern matching engine.
//!
//! A backtick-delimited trailing parameter triggers pattern mode.
//! Patterns have three context regions: `left,middle,right`.
//!
//! # Pattern syntax
//!
//! | Token         | Meaning                                        |
//! | ------------- | ---------------------------------------------- |
//! | `A`           | One alphabetic character                       |
//! | `U`           | One uppercase letter                           |
//! | `L`           | One lowercase letter                           |
//! | `N`           | One digit                                      |
//! | `S`           | One whitespace character                       |
//! | `P`           | One punctuation character                      |
//! | `C`           | One printable character (0x20–0x7E)            |
//! | `D/…/`        | Custom character set                           |
//! | `-X`          | Negated character set                          |
//! | `"text"`      | Literal (exact case)                           |
//! | `'text'`      | Literal (case-folded)                          |
//! | `(…)`         | Grouping                                       |
//! | `X\|Y`        | Alternation                                    |
//! | `*X`          | Zero or more                                   |
//! | `+X`          | One or more                                    |
//! | `nX`          | Exactly n                                      |
//! | `[n]X`        | Exactly n                                      |
//! | `[n,]X`       | At least n                                     |
//! | `[,m]X`       | At most m                                      |
//! | `[n,m]X`      | Between n and m                                |
//! | `<`           | Beginning of line positional                   |
//! | `>`           | End of line positional                         |
//! | `{`           | Left-margin positional                         |
//! | `}`           | Right-margin positional                        |
//! | `^`           | Dot-column positional                          |
//! | `@n`          | Mark-check positional                          |
//! | `=`           | Equals (last position) check                   |
//! | `%`           | Last-modified position check                   |
//! | `A,B`         | Context separator (left=A, middle=B)           |
//! | `A,B,C`       | Full context (left=A, middle=B, right=C)       |

pub mod ast;
pub mod char_class;
pub mod matcher;
pub mod parser;

pub use matcher::{MatchCtx, find_backward, find_forward, match_at};
pub use parser::parse;
