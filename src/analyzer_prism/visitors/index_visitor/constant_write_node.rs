use log::{error, trace};
use ruby_analysis_core::{TypeFact, TypeProvenance, TypeSubject};
use ruby_prism::ConstantWriteNode;

use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_constant_write_node_entry(&mut self, node: &ConstantWriteNode) {
        let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        trace!("Visiting constant write node: {}", constant_name);

        // Create a RubyConstant from the name
        let constant = match RubyConstant::new(&constant_name) {
            Ok(constant) => constant,
            Err(e) => {
                error!("Error creating constant: {}", e);
                return;
            }
        };

        // Create a FullyQualifiedName using the current namespace stack and the constant
        // First get the current flattened namespace, then add the new constant
        let mut namespace = self.scope_tracker.get_ns_stack();
        namespace.push(constant);
        // Value constants use Constant variant, not Namespace
        let fqn = FullyQualifiedName::constant(namespace);
        let inferred_type = self.infer_type_from_value(&node.value());
        self.document.type_store.add(TypeFact::new(
            TypeSubject::Constant(fqn.clone()),
            inferred_type.clone(),
            self.document.prism_location_to_text_range(&node.location()),
            TypeProvenance::Assignment,
        ));

        let entry_result = {
            let mut index = self.index.lock();
            EntryBuilder::new()
                .fqn(fqn)
                .location(
                    self.document
                        .prism_location_to_lsp_location(&node.location()),
                )
                .kind(EntryKind::new_typed_constant(
                    None,
                    None,
                    inferred_type.clone(),
                ))
                .build(&mut index)
        };

        // Add the entry to the index
        if let Ok(entry) = entry_result {
            trace!(
                "Added constant write node entry: {} -> {:?}",
                constant_name,
                inferred_type
            );
            self.add_entry(entry);
        } else {
            error!("Error creating entry for constant: {}", constant_name);
        }
    }

    pub fn process_constant_write_node_exit(&mut self, _node: &ConstantWriteNode) {
        // No-op for now
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;
    use crate::indexer::index_ref::{Index, Unlocked};
    use crate::inferrer::r#type::ruby::RubyType;
    use parking_lot::RwLock;
    use ruby_analysis_core::{SourceFileId, TypeResolution};
    use ruby_prism::Visit;
    use std::sync::Arc;
    use tower_lsp::lsp_types::Url;

    fn create_test_index() -> Index<Unlocked> {
        Index::new(Arc::new(RwLock::new(RubyIndex::new())))
    }

    #[test]
    fn constant_write_emits_type_fact() {
        let content = "VALUE = 1";
        let uri = Url::parse("file:///test.rb").unwrap();
        let index = create_test_index();
        let document =
            crate::types::ruby_document::RubyDocument::new(uri.clone(), content.to_string(), 1);
        let mut visitor = IndexVisitor::new(index, document);
        let parse_result = ruby_prism::parse(content.as_bytes());

        visitor.visit(&parse_result.node());

        let subject = TypeSubject::Constant(FullyQualifiedName::constant(vec![RubyConstant::new(
            "VALUE",
        )
        .unwrap()]));
        match visitor
            .document
            .type_store
            .type_at(&subject, SourceFileId(0), 8)
        {
            TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::integer()),
            other => panic!("expected constant type fact, got {other:?}"),
        }
    }
}
