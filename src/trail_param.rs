
#[derive(Debug)]
pub struct TrailParam {
    pub dlm: char,
    pub str: String,
}

impl TrailParam {
    pub fn new(dlm: char, str: String) -> Self {
        Self { dlm, str }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self {
            dlm: '"',
            str: s.to_string(),
        }
    }
}
