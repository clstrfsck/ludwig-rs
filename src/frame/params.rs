//! EP parameter parser — implements the `setparam` logic from the C++ reference.
//!
//! Parses strings of the form `K=I`, `M=(5,75)`, `O=(I,W)`, `T=D`, etc.
//! and returns a list of [`EpDirective`]s for the caller to apply.

/// The maximum valid column number (1-based user input), matching MAX_STRLEN.
pub(crate) const MAX_COL: usize = 400;

/// A single parsed EP directive, ready to apply to a frame + context.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EpDirective {
    /// `K=I|O|C` — set keyboard mode
    KeyboardMode {
        mode: EpKeyboardMode,
        set_initial: bool,
    },
    /// `O=...` — set/clear option flags
    Options {
        ops: Vec<EpOptionOp>,
        set_initial: bool,
    },
    /// `M=(left,right)` — horizontal margins (0-based Rust values after conversion)
    LrMargins {
        left_margin: usize,
        right_margin: usize,
        set_initial: bool,
    },
    /// `V=(top,bottom)` — vertical scroll margins
    TbMargins {
        margin_top: usize,
        margin_bottom: usize,
        set_initial: bool,
    },
    /// `T=...` — tab stop operation
    Tabs { op: EpTabOp, set_initial: bool },
    /// `H=n` — screen height (stored but not yet used by viewport)
    ScreenHeight { n: usize, set_initial: bool },
    /// `W=n` — screen width
    ScreenWidth { n: usize, set_initial: bool },
    /// `S=n` — space limit (no-op in our implementation)
    SpaceLimit { n: usize, set_initial: bool },
    /// `C=...` — command introducer (interactive only, no-op in batch)
    CmdIntroducer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EpKeyboardMode {
    Insert,
    Overtype,
    Command,
}

/// One option flag operation: set or clear a specific option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EpOptionOp {
    pub(crate) flag: EpOptionFlag,
    pub set: bool, // true = set, false = clear
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EpOptionFlag {
    AutoIndent, // I
    AutoWrap,   // W
    Newline,    // N
    Show,       // S — display current options (no-op in batch)
}

/// A tab-stop operation.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EpTabOp {
    /// `T=D` — reset to default (every 8 columns)
    Default,
    /// `T=S` — set tab at current dot column
    SetAtDot,
    /// `T=C` — clear tab at current dot column
    ClearAtDot,
    /// `T=T` — derive tabs from current line (template)
    Template,
    /// `T=I` — insert a ruler line above dot
    InsertRuler,
    /// `T=R` — read ruler from current line and delete it
    ReadRuler,
    /// `T=W(n)` — uniform tabs every n columns
    Uniform { n: usize },
    /// `T=(c1,c2,...)` — explicit tab columns (already converted to 0-based)
    Explicit { cols: Vec<usize> },
}

// ─── Parser ──────────────────────────────────────────────────────────────────

struct Parser {
    chars: Vec<char>,
    pos: usize,
}

impl Parser {
    fn new(s: &str) -> Self {
        // Normalise to uppercase for case-insensitive matching.
        Self {
            chars: s.chars().map(|c| c.to_ascii_uppercase()).collect(),
            pos: 0,
        }
    }

    /// Peek at the current character without advancing (returns '\0' at end).
    fn peek(&self) -> char {
        self.chars.get(self.pos).copied().unwrap_or('\0')
    }

    /// Advance and return the current character ('\0' at end).
    fn next(&mut self) -> char {
        let ch = self.chars.get(self.pos).copied().unwrap_or('\0');
        if ch != '\0' {
            self.pos += 1;
        }
        ch
    }

    /// Require the next character to equal `expected`.
    fn expect(&mut self, expected: char) -> Result<(), ()> {
        if self.next() == expected {
            Ok(())
        } else {
            Err(())
        }
    }

    /// Parse a run of ASCII digits as a `usize`.  Fails if no digits found.
    fn parse_uint(&mut self) -> Result<usize, ()> {
        let mut s = String::new();
        while self.peek().is_ascii_digit() {
            s.push(self.next());
        }
        if s.is_empty() {
            return Err(());
        }
        s.parse::<usize>().map_err(|_| ())
    }

    /// Parse an optional int (used for M= and V= margin pairs).
    ///
    /// Returns `Ok(Some(n))` if an int was found, `Ok(None)` if not (keep existing),
    /// or `Err(())` on `.` (current dot position — handled by caller).
    fn try_parse_uint(&mut self) -> Result<Option<usize>, ()> {
        if self.peek().is_ascii_digit() {
            Ok(Some(self.parse_uint()?))
        } else {
            Ok(None)
        }
    }
}

// ─── Public parse entry point ─────────────────────────────────────────────────

/// Parse an EP parameter string and return a list of directives.
///
/// The `current_left` and `current_right` values are used as defaults when the
/// `M=` form omits one margin value.  Similarly `current_top`/`current_bottom`
/// for `V=`.  The dot column is passed for `T=S`/`T=C` (stored in the directive
/// itself via `SetAtDot`/`ClearAtDot` — the actual column is resolved at
/// application time).
pub(crate) fn parse_ep(
    s: &str,
    current_left: usize,
    current_right: usize,
    current_top: usize,
    current_bottom: usize,
) -> Result<Vec<EpDirective>, ()> {
    let mut p = Parser::new(s);
    let mut directives = Vec::new();

    loop {
        // Optional '$' prefix = set initial defaults too.
        let set_initial = if p.peek() == '$' {
            p.next();
            true
        } else {
            false
        };

        // Key letter.
        let key = p.next();
        if key == '\0' {
            break;
        }

        // '='
        p.expect('=')?;

        let dir = match key {
            'K' => parse_keyboard_mode(&mut p, set_initial)?,
            'O' => parse_options(&mut p, set_initial)?,
            'M' => parse_lr_margins(&mut p, set_initial, current_left, current_right)?,
            'V' => parse_tb_margins(&mut p, set_initial, current_top, current_bottom)?,
            'T' => parse_tabs(&mut p, set_initial)?,
            'H' => {
                let n = p.parse_uint()?;
                if n == 0 {
                    return Err(());
                }
                EpDirective::ScreenHeight { n, set_initial }
            }
            'W' => {
                let n = p.parse_uint()?;
                if n < 10 {
                    return Err(());
                }
                EpDirective::ScreenWidth { n, set_initial }
            }
            'S' => {
                let n = p.parse_uint()?;
                EpDirective::SpaceLimit { n, set_initial }
            }
            'C' => {
                // Consume until ',' or end (interactive-only, no-op in batch).
                while p.peek() != ',' && p.peek() != '\0' {
                    p.next();
                }
                EpDirective::CmdIntroducer
            }
            _ => return Err(()),
        };
        directives.push(dir);

        match p.peek() {
            '\0' => break,
            ',' => {
                p.next();
            }
            _ => return Err(()),
        }
    }

    Ok(directives)
}

// ─── Sub-parsers ──────────────────────────────────────────────────────────────

fn parse_keyboard_mode(p: &mut Parser, set_initial: bool) -> Result<EpDirective, ()> {
    let mode = match p.next() {
        'I' => EpKeyboardMode::Insert,
        'O' => EpKeyboardMode::Overtype,
        'C' => EpKeyboardMode::Command,
        _ => return Err(()),
    };
    Ok(EpDirective::KeyboardMode { mode, set_initial })
}

fn parse_options(p: &mut Parser, set_initial: bool) -> Result<EpDirective, ()> {
    let mut ops = Vec::new();
    if p.peek() == '(' {
        p.next(); // consume '('
        loop {
            let set = if p.peek() == '-' {
                p.next();
                false
            } else {
                true
            };
            let flag = parse_option_flag(p.next())?;
            ops.push(EpOptionOp { flag, set });
            match p.next() {
                ')' => break,
                ',' => continue,
                _ => return Err(()),
            }
        }
    } else {
        // Single option (toggle: set=true to set the flag; use explicit '-' form to clear).
        let set = if p.peek() == '-' {
            p.next();
            false
        } else {
            true
        };
        let flag = parse_option_flag(p.next())?;
        ops.push(EpOptionOp { flag, set });
    }
    Ok(EpDirective::Options { ops, set_initial })
}

fn parse_option_flag(ch: char) -> Result<EpOptionFlag, ()> {
    match ch {
        'I' => Ok(EpOptionFlag::AutoIndent),
        'W' => Ok(EpOptionFlag::AutoWrap),
        'N' => Ok(EpOptionFlag::Newline),
        'S' => Ok(EpOptionFlag::Show),
        _ => Err(()),
    }
}

/// Parse `M=(left,right)` — values are 1-based user input; we convert here.
fn parse_lr_margins(
    p: &mut Parser,
    set_initial: bool,
    current_left: usize,  // current left_margin (0-based)
    current_right: usize, // current right_margin
) -> Result<EpDirective, ()> {
    p.expect('(')?;

    // Current values (keep if the user omits a side).
    // current_left is 0-based; for the parser we work in 1-based user values.
    let mut left_user = current_left + 1; // convert 0-based → 1-based
    let mut right_user = current_right; // right_margin is already the width value

    let ch = p.peek();
    if ch == '.' {
        p.next();
        // '.' means "current dot column + 1" — we can't resolve this here;
        // signal it by a sentinel 0 which the caller must substitute.
        left_user = 0; // sentinel: use dot.column + 1
    } else if let Some(n) = p.try_parse_uint()? {
        // 1-based; 0 is not valid (use '.' for dot column).
        if !(1..=MAX_COL).contains(&n) {
            return Err(());
        }
        left_user = n;
    }

    if p.peek() == ',' {
        p.next();
        let ch = p.peek();
        if ch == '.' {
            p.next();
            right_user = 0; // sentinel: use dot.column + 1
        } else if let Some(n) = p.try_parse_uint()? {
            if !(1..=MAX_COL).contains(&n) {
                return Err(());
            }
            right_user = n;
        }
    }

    p.expect(')')?;

    // Convert 1-based user values to internal representation.
    // Sentinel 0 means "resolve to dot column at apply time".
    let left_margin = if left_user == 0 { 0 } else { left_user - 1 };
    let right_margin = right_user; // right stays as-is (it's a width cap)

    // Deferred constraint check (when both are resolved): left < right.
    // If either is a sentinel (0) the caller must re-check after substitution.
    if left_margin != 0 && right_margin != 0 && left_margin >= right_margin {
        return Err(());
    }

    Ok(EpDirective::LrMargins {
        left_margin,
        right_margin,
        set_initial,
    })
}

fn parse_tb_margins(
    p: &mut Parser,
    set_initial: bool,
    current_top: usize,
    current_bottom: usize,
) -> Result<EpDirective, ()> {
    p.expect('(')?;

    let mut top = current_top;
    let mut bottom = current_bottom;

    if let Some(n) = p.try_parse_uint()? {
        top = n;
    }
    if p.peek() == ',' {
        p.next();
        if let Some(n) = p.try_parse_uint()? {
            bottom = n;
        }
    }

    p.expect(')')?;

    Ok(EpDirective::TbMargins {
        margin_top: top,
        margin_bottom: bottom,
        set_initial,
    })
}

fn parse_tabs(p: &mut Parser, set_initial: bool) -> Result<EpDirective, ()> {
    let sub = p.next();
    let op = match sub {
        'D' => EpTabOp::Default,
        'S' => EpTabOp::SetAtDot,
        'C' => EpTabOp::ClearAtDot,
        'T' => EpTabOp::Template,
        'I' => EpTabOp::InsertRuler,
        'R' => EpTabOp::ReadRuler,
        'W' => {
            p.expect('(')?;
            let n = p.parse_uint()?;
            if n == 0 || n > MAX_COL {
                return Err(());
            }
            p.expect(')')?;
            EpTabOp::Uniform { n }
        }
        '(' => {
            // Explicit column list (1-based input).
            let mut cols = Vec::new();
            loop {
                let col_1based = p.parse_uint()?;
                if !(1..=MAX_COL).contains(&col_1based) {
                    return Err(());
                }
                cols.push(col_1based - 1); // convert to 0-based
                match p.next() {
                    ')' => break,
                    ',' => continue,
                    _ => return Err(()),
                }
            }
            EpTabOp::Explicit { cols }
        }
        _ => return Err(()),
    };
    Ok(EpDirective::Tabs { op, set_initial })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Vec<EpDirective>, ()> {
        parse_ep(s, 0, 79, 0, 0)
    }

    #[test]
    fn test_keyboard_mode_insert() {
        assert_eq!(
            parse("K=I").unwrap(),
            vec![EpDirective::KeyboardMode {
                mode: EpKeyboardMode::Insert,
                set_initial: false
            }]
        );
    }

    #[test]
    fn test_keyboard_mode_overtype() {
        assert_eq!(
            parse("K=O").unwrap(),
            vec![EpDirective::KeyboardMode {
                mode: EpKeyboardMode::Overtype,
                set_initial: false
            }]
        );
    }

    #[test]
    fn test_keyboard_mode_command() {
        assert_eq!(
            parse("K=C").unwrap(),
            vec![EpDirective::KeyboardMode {
                mode: EpKeyboardMode::Command,
                set_initial: false
            }]
        );
    }

    #[test]
    fn test_option_single_set() {
        let dirs = parse("O=I").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::Options {
                ops: vec![EpOptionOp {
                    flag: EpOptionFlag::AutoIndent,
                    set: true
                }],
                set_initial: false,
            }]
        );
    }

    #[test]
    fn test_option_single_clear() {
        let dirs = parse("O=-I").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::Options {
                ops: vec![EpOptionOp {
                    flag: EpOptionFlag::AutoIndent,
                    set: false
                }],
                set_initial: false,
            }]
        );
    }

    #[test]
    fn test_option_parens_multiple() {
        let dirs = parse("O=(I,W)").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::Options {
                ops: vec![
                    EpOptionOp {
                        flag: EpOptionFlag::AutoIndent,
                        set: true
                    },
                    EpOptionOp {
                        flag: EpOptionFlag::AutoWrap,
                        set: true
                    },
                ],
                set_initial: false,
            }]
        );
    }

    #[test]
    fn test_option_parens_clear_one() {
        let dirs = parse("O=(-I,W)").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::Options {
                ops: vec![
                    EpOptionOp {
                        flag: EpOptionFlag::AutoIndent,
                        set: false
                    },
                    EpOptionOp {
                        flag: EpOptionFlag::AutoWrap,
                        set: true
                    },
                ],
                set_initial: false,
            }]
        );
    }

    #[test]
    fn test_lr_margins() {
        let dirs = parse("M=(5,75)").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::LrMargins {
                left_margin: 4, // 5-1 = 4 (0-based)
                right_margin: 75,
                set_initial: false,
            }]
        );
    }

    #[test]
    fn test_lr_margins_default_values() {
        // M=(1,79) → left_margin=0, right_margin=79
        let dirs = parse("M=(1,79)").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::LrMargins {
                left_margin: 0,
                right_margin: 79,
                set_initial: false,
            }]
        );
    }

    #[test]
    fn test_lr_margins_fails_when_left_ge_right() {
        assert!(parse("M=(75,5)").is_err());
    }

    #[test]
    fn test_lr_margins_fails_out_of_range() {
        assert!(parse("M=(0,79)").is_err()); // 0 is below 1 (1-based)
    }

    #[test]
    fn test_tb_margins() {
        let dirs = parse("V=(2,3)").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::TbMargins {
                margin_top: 2,
                margin_bottom: 3,
                set_initial: false,
            }]
        );
    }

    #[test]
    fn test_tab_default() {
        let dirs = parse("T=D").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::Tabs {
                op: EpTabOp::Default,
                set_initial: false
            }]
        );
    }

    #[test]
    fn test_tab_set_at_dot() {
        let dirs = parse("T=S").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::Tabs {
                op: EpTabOp::SetAtDot,
                set_initial: false
            }]
        );
    }

    #[test]
    fn test_tab_clear_at_dot() {
        let dirs = parse("T=C").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::Tabs {
                op: EpTabOp::ClearAtDot,
                set_initial: false
            }]
        );
    }

    #[test]
    fn test_tab_uniform() {
        let dirs = parse("T=W(4)").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::Tabs {
                op: EpTabOp::Uniform { n: 4 },
                set_initial: false
            }]
        );
    }

    #[test]
    fn test_tab_explicit() {
        let dirs = parse("T=(9,17,25)").unwrap();
        // 9→8, 17→16, 25→24 (convert 1-based to 0-based)
        assert_eq!(
            dirs,
            vec![EpDirective::Tabs {
                op: EpTabOp::Explicit {
                    cols: vec![8, 16, 24]
                },
                set_initial: false,
            }]
        );
    }

    #[test]
    fn test_set_initial_prefix() {
        let dirs = parse("$K=I").unwrap();
        assert_eq!(
            dirs,
            vec![EpDirective::KeyboardMode {
                mode: EpKeyboardMode::Insert,
                set_initial: true,
            }]
        );
    }

    #[test]
    fn test_multi_param() {
        let dirs = parse("M=(1,79),K=I").unwrap();
        assert_eq!(dirs.len(), 2);
        assert_eq!(
            dirs[0],
            EpDirective::LrMargins {
                left_margin: 0,
                right_margin: 79,
                set_initial: false
            }
        );
        assert_eq!(
            dirs[1],
            EpDirective::KeyboardMode {
                mode: EpKeyboardMode::Insert,
                set_initial: false
            }
        );
    }

    #[test]
    fn test_invalid_key_fails() {
        assert!(parse("Z=1").is_err());
    }

    #[test]
    fn test_missing_equals_fails() {
        assert!(parse("KI").is_err());
    }
}
