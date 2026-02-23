use crate::marks::MarkId;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum LeadParam {
    /// No leading parameter.
    None,
    /// + without integer (e.g. `+CMD`)
    Plus,
    /// - without integer (e.g. `-CMD`)
    Minus,
    /// Positive integer (e.g. `3` or `+3`)
    Pint(usize),
    /// Negative integer (e.g. `-3`)
    Nint(usize),
    /// Indefinite positive (`>`), typically to end of line or file
    Pindef,
    /// Indefinite negative (`<`), typically to start of line or file
    Nindef,
    /// Marker (e.g. `@n`, `=`, or `%`)
    Marker(MarkId),
}
