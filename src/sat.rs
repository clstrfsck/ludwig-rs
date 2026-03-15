pub trait SignedInt: Copy + Ord + From<i8> {}

pub trait SignedMax: SignedInt {
    const MAX: Self;
}

macro_rules! impl_signed_int {
    ($($t:ty),* $(,)?) => {
        $(impl SignedInt for $t {})*
    };
}

macro_rules! impl_signed_max {
    ($($t:ty),* $(,)?) => {
        $(impl SignedMax for $t {
            const MAX: Self = <$t>::MAX;
        })*
    };
}

impl_signed_int!(i8, i16, i32, i64, i128, isize);
impl_signed_max!(i8, i16, i32, i64, i128, isize);

pub fn signed_to_usize<T>(n: T) -> usize
where
    T: SignedInt,
    usize: TryFrom<T>,
{
    match usize::try_from(n) {
        Ok(v) => v,
        Err(_) => {
            if n < T::from(0) {
                0
            } else {
                usize::MAX
            }
        }
    }
}

pub fn usize_to_signed<T>(n: usize) -> T
where
    T: SignedMax + TryFrom<usize>,
{
    T::try_from(n).unwrap_or(T::MAX)
}
