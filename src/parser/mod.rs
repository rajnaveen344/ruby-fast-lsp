pub mod document;

use ::tree_sitter::{Parser, Tree};
use anyhow::{Ok, Result};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct RubyParser {
    parser: Arc<Mutex<Parser>>,
}

impl RubyParser {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let language = tree_sitter_ruby::LANGUAGE;
        let _ = parser
            .set_language(&language.into())
            .map_err(|_| "Failed to load Ruby grammar".to_string());

        Ok(Self {
            parser: Arc::new(Mutex::new(parser)),
        })
    }

    pub fn parse(&self, source_code: &str) -> Option<Tree> {
        self.parser.lock().unwrap().parse(source_code, None)
    }
}
