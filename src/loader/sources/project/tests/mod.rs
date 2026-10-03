//! Project source collection tests, split by behaviour.

use super::collection::map_owned_project_inputs;
use super::navigation::{prioritize_project_files, select_navigation_demand_files};
use super::*;
use crate::environment::config::IndexingConfig;
use crate::environment::runtime::jruby::imports::JrubyImportProvider;
use crate::environment::runtime::jruby::java_catalog::{JavaClassDeclaration, ProjectJavaCatalog};
use crate::server::Server;
use ruby_analysis::core::SourceKind;
use ruby_fast_lsp_jvm_metadata::ClassFile;
use std::collections::BTreeMap;
use tempfile::TempDir;
use tower_lsp::lsp_types::{
    DidOpenTextDocumentParams, InlayHintParams, Position, Range, TextDocumentIdentifier,
    TextDocumentItem, Url,
};

mod batching;
mod cross_file_constants;
mod jruby_replay;
mod navigation_demand;
mod open_documents;
