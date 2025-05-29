use lsp_types::Url;

pub struct RubyDocument {
    pub uri: Url,
    pub content: String,
}

impl RubyDocument {
    pub fn new(uri: Url, content: String) -> Self {
        Self { uri, content }
    }
}
