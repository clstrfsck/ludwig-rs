
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MarkNumber(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkId {
    Last,
    Modified,
    Numbered(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    position: usize,
    vspace: usize,
}

impl Mark {
    pub fn new(position: usize) -> Self {
        Self::new_with_vspace(position, 0)
    }

    pub fn new_with_vspace(position: usize, vspace: usize) -> Self {
        Mark { position, vspace }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn vspace(&self) -> usize {
        self.vspace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undefined_and_new() {
        assert_eq!(Mark::new_with_vspace(5, 10).position(), 5);
        assert_eq!(Mark::new_with_vspace(5, 10).vspace(), 10);
        assert_eq!(Mark::new(5).position(), 5);
        assert_eq!(Mark::new(5).vspace(), 0);
    }
}
