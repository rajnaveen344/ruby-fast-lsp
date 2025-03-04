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
        let lang = tree_sitter_ruby::language();

        parser.set_language(lang)?;

        Ok(Self {
            parser: Arc::new(Mutex::new(parser)),
        })
    }

    pub fn parse(&self, source_code: &str) -> Option<Tree> {
        self.parser.lock().unwrap().parse(source_code, None)
    }
}
