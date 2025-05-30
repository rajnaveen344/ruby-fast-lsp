use lsp_types::Url;

#[derive(Clone)]
pub struct RubyDocument {
    pub uri: Url,
    pub content: String,
    pub version: i32,
}

impl RubyDocument {
    pub fn new(uri: Url, content: String, version: i32) -> Self {
        Self {
            uri,
            content,
            version,
        }
    }

    pub fn update_content(&mut self, content: String, version: i32) {
        self.content = content;
        self.version = version;
    }
}
