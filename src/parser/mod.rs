pub mod document;
pub mod tree_sitter;

use anyhow::{anyhow, Result};
use log::{info, warn};
use ::tree_sitter::{Parser, Tree};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct RubyParser {
    parser: Arc<Mutex<Parser>>,
    has_ruby_grammar: bool,
}

impl RubyParser {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        
        // Try to load Ruby grammar
        match tree_sitter::ruby_language() {
            Some(language) => {
                parser.set_language(language)?;
                info!("Ruby grammar loaded successfully");
                Ok(Self {
                    parser: Arc::new(Mutex::new(parser)),
                    has_ruby_grammar: true,
                })
            }
            None => {
                warn!("Ruby grammar not available");
                Err(anyhow!("Ruby grammar not available"))
            }
        }
    }
    
    pub fn parse(&self, source_code: &str) -> Option<Tree> {
        if !self.has_ruby_grammar {
            warn!("Cannot parse Ruby code: grammar not available");
            return None;
        }
        
        match self.parser.lock().unwrap().parse(source_code, None) {
            Some(tree) => {
                info!("Ruby code parsed successfully");
                Some(tree)
            }
            None => {
                warn!("Failed to parse Ruby code");
                None
            }
        }
    }
    
    pub fn has_grammar(&self) -> bool {
        self.has_ruby_grammar
    }
}
