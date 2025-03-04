#[derive(Debug, Clone)]
pub struct RubyDocument {
    pub content: String,
    pub version: i32,
}

impl RubyDocument {
    pub fn new(content: String, version: i32) -> Self {
        Self { content, version }
    }

    pub fn get_content(&self) -> &str {
        &self.content
    }

    pub fn get_version(&self) -> i32 {
        self.version
    }

    pub fn update_content(&mut self, content: String, version: i32) {
        self.content = content;
        self.version = version;
    }
}
