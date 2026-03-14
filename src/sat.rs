pub fn isize_to_usize(n: isize) -> usize {
    usize::try_from(n).unwrap_or(if n < 0 { 0 } else { usize::MAX })
}

pub fn usize_to_isize(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}

pub fn i32_to_usize(n: i32) -> usize {
    usize::try_from(n).unwrap_or(if n < 0 { 0 } else { usize::MAX })
}

pub fn usize_to_i32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}
