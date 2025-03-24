use lsp_types::Position;
use ruby_prism::{Location as PrismLocation, Node};

use crate::indexer::types::fully_qualified_name::FullyQualifiedName;

pub fn lsp_pos_to_prism_loc<'a>(pos: Position, content: &str) -> PrismLocation<'a> {
    todo!()
}

pub fn prism_loc_to_lsp_pos(loc: PrismLocation, content: &str) -> Position {
    todo!()
}

pub fn node_to_fqn(node: Node) -> FullyQualifiedName {
    todo!()
}
