use crate::mark::MarkId;

pub enum LeadParam {
    None,           // no leading parameter
    Plus,           // + without integer
    Minus,          // - without integer
    Pint(usize),    // +ve integer
    Nint(usize),    // -ve integer
    Pindef,         // >
    Nindef,         // <
    Marker(MarkId), // @ or = or %
}
