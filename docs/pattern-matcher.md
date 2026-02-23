# Use of the Pattern Matcher

A pattern definition consists of

```text
[<left context> ,] <middle context> [, <right context>]
```

Each context being a context free pattern (COMPOUND in syntax),
defaulting to left and middle if a third  pattern is not specified.

When a pattern has been successfully matched, the marks Dot and Equals
are left wrapped around the middle context.

A context free pattern is defined with a regular expression.
`|` is used to denote OR (alternative).  Concatenation is implicit.
Parentheses are used to structure the expression.
Most elements will take a repeat count parameter.

Parameter prefixes:

- Numeric: positive integers only
- Kleene star: `*`  0 or more repetitions.
- Kleene plus: `+`  1 or more repetitions.
- Range :   `[nn , mm]`
  - Specifies a range of repetitions `nn` to `mm` inclusive.
  - The low bound defaults to 0, the high bound to indefinitely many.
  - The comma is therefore mandatory.
- Negation: `-`  Applies only to sets.

## Elements of Patterns

### The sets

- A : Alphabetic characters.
- U : Uppercase alphabetic.
- L : Lowercase alphabetic.
- P : Punctuation characters.  (),.;:"'!?-`  only
- N : Numeric characters.
- S : The space.  (non-space is therefore -S)
- C : All printable characters.
- D : Define.  To define a set.
  - `D<delimiter> { {char} | {char .. char} } <delimiter>` (the syntax is identical to NEXT and BRIDGE)
  - eg.  `D/a..z/` is the same as `L` (for ASCII at least)
  - `D/a..zA..Z$_0..9/` are all the valid characters in a VAX Pascal identifier.
  - If the delimiters are $ or &, then the standard Ludwig span dereferencing
    and prompting mechanisms are used.  The returned span is used as the
    contents of a set definition specification.

### Strings

Character strings are delimited with either single or double quotes.
Double quotes are used to indicate exact case matching, whilst
single quotes indicate inexact case matching.

Dereferencing is allowed within strings if the string contains only a
dereference, ie.`'$fred$'` will use the contents of span fred as a quoted
string.  `'&String : &'` will prompt  `String : `, and the input treated as a
quoted string with inexact case match.  Both work with `"` as well, in which
case exact case matching is used.

To get the string `$fred$` use `'$''fred$'`, so `$` is not both the first and
last char of the string.

### Marks

`@n`, where n is a mark number, is used in a pattern to test for a
mark at the present position.  Note that the presence of a mark
where not needed will not affect the normal execution of the
pattern matcher.

### Positionals

- `<` `>` are respectively the beginning and end of lines.
- `{` `}` the left and right margins.
- `^`  the column that the Dot was in when the match started.

Note : The marks and positionals are conceptually in the gaps to the
left of the character in a column.  Also note that it is possible
for more than one positional or mark to appear in exactly the same
place.  If the behaviour of the pattern is dependent upon which of
the positionals is found then the path taken will be
indeterminate.

## Dereferenced Patterns

A pattern definition may be obtained using standard Ludwig dereferencing
mechanisms.

Notes:

- Dereferenced spans are pattern definitions NOT strings, therefore if a string
  is desired use string dereferencing as described above.
- Dereferenced spans are Context free patterns and therefore may not contain
  context changes (commas).

## Syntax

Numbers are positive integers, letters are case independent except in literal
strings, delimiters are matching non-alphanumeric characters.

```bnf
PATTERN_DEFINITION ::= [COMPOUND ','] COMPOUND [',' COMPOUND]
COMPOUND ::= PATTERN [ '|' COMPOUND ]
PATTERN ::=
           { ( [ PARAMETER ]( SET | '(' COMPOUND ')' | STRING ))  |
             DEREFERENCE | ( '@' number )  } { ' ' }
PARAMETER ::=
           '*' | '+' | number | ( '[' [ number ] ',' [ number ] ']' )
SET ::=
           [ '-' ] ( 'A'|'P'|'N'|'U'|'L'|'S'|
           ('D' ( DEREFERENCE |
            (delimiter {{character} | {character ".." character}} delimiter)))
STRING ::=
           (''' {character} ''') | ('"' {character} '"') |
           (''' DEREFERENCE ''') | ('"' DEREFERENCE  '"')
DEREFERENCE ::=
           ( '$' span name '$' ) | ( '&' prompt '&' )
```
