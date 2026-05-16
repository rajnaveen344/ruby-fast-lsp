use log::{error, trace};
use ruby_analysis_core::{TypeFact, TypeProvenance, TypeSubject};
use ruby_prism::ConstantPathWriteNode;

use crate::analyzer_prism::utils;
use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_constant_path_write_node_entry(&mut self, node: &ConstantPathWriteNode) {
        // Extract the constant path
        let constant_path = node.target();

        // Extract the constant name (the rightmost part of the path)
        let constant_name = match constant_path.name() {
            Some(name) => String::from_utf8_lossy(name.as_slice()).to_string(),
            None => {
                error!("Could not extract constant name from ConstantPathWriteNode");
                return;
            }
        };

        trace!("Visiting constant path write node: {}", constant_name);

        // Create a RubyConstant from the name
        let constant = match RubyConstant::new(&constant_name) {
            Ok(constant) => constant,
            Err(e) => {
                error!("Error creating constant: {}", e);
                return;
            }
        };

        // Extract the full constant path from the target.
        let mut namespace_parts = Vec::new();
        utils::collect_namespaces(&constant_path, &mut namespace_parts);

        // Get the current namespace and add the collected parts
        let mut fqn_parts = self.scope_tracker.get_ns_stack();
        fqn_parts.extend(namespace_parts);
        assert!(
            fqn_parts.last() == Some(&constant),
            "INVARIANT VIOLATED: constant path write target `{}` did not end with its name. \
             This is a bug because Prism target path collection must preserve the written constant. \
             Fix: inspect collect_namespaces for ConstantPathWriteNode targets.",
            constant_name
        );

        // Value constants use Constant variant, not Namespace
        let fqn = FullyQualifiedName::constant(fqn_parts);
        let inferred_type = self.infer_type_from_value(&node.value());
        self.type_store.add(TypeFact::new(
            TypeSubject::Constant(fqn.clone()),
            inferred_type.clone(),
            self.document.prism_location_to_text_range(&node.location()),
            TypeProvenance::Assignment,
        ));

        // Create an Entry with EntryKind::Constant
        let entry = {
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
        if let Ok(entry) = entry {
            self.add_entry(entry);
            trace!(
                "Added constant path write node entry: {} -> {:?}",
                constant_name,
                inferred_type
            );
        } else {
            error!("Error creating entry for constant path: {}", constant_name);
        }
    }

    pub fn process_constant_path_write_node_exit(&mut self, _node: &ConstantPathWriteNode) {
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
    fn constant_path_write_emits_type_fact() {
        let content = "Foo::VALUE = 1";
        let uri = Url::parse("file:///test.rb").unwrap();
        let index = create_test_index();
        let document =
            crate::types::ruby_document::RubyDocument::new(uri.clone(), content.to_string(), 1);
        let mut visitor = IndexVisitor::new(index, document);
        let parse_result = ruby_prism::parse(content.as_bytes());

        visitor.visit(&parse_result.node());

        let subject = TypeSubject::Constant(FullyQualifiedName::constant(vec![
            RubyConstant::new("Foo").unwrap(),
            RubyConstant::new("VALUE").unwrap(),
        ]));
        match visitor.type_store.type_at(&subject, SourceFileId(0), 13) {
            TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::integer()),
            other => panic!("expected constant path type fact, got {other:?}"),
        }
    }
}
