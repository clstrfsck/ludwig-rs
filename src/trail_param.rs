#[derive(Debug, Clone)]
pub struct TrailParam {
    pub delim: char,
    pub content: String,
}

impl TrailParam {
    pub fn new(delim: char, content: String) -> Self {
        Self { delim, content }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self {
            delim: '"',
            content: s.to_string(),
        }
    }
}
