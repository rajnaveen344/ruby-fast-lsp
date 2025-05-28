use std::sync::{Arc, Mutex};

use log::debug;
use lsp_types::Url;
use ruby_prism::{parse, Visit};

pub mod entry;
pub mod events;
mod index;
mod traverser;
pub mod types;

use index::RubyIndex;
use traverser::Visitor;

pub struct RubyIndexer {
    index: Arc<Mutex<RubyIndex>>,
}

impl RubyIndexer {
    pub fn new() -> Result<Self, String> {
        Ok(RubyIndexer {
            index: Arc::new(Mutex::new(RubyIndex::new())),
        })
    }

    pub fn index(&self) -> Arc<Mutex<RubyIndex>> {
        self.index.clone()
    }

    pub fn process_file(&mut self, uri: Url, content: &str) -> Result<(), String> {
        self.index.lock().unwrap().remove_entries_for_uri(&uri);

        let parse_result = parse(content.as_bytes());
        let node = parse_result.node();
        let mut visitor = Visitor::new(self.index.clone(), uri.clone(), content.to_string());

        visitor.visit(&node);

        debug!("Processed file: {}", uri);
        Ok(())
    }
}
