//! Symbol facts for local, instance, class, and global variable bindings.

use crate::core::{FullyQualifiedName, SymbolFact, SymbolKind};

use super::AnalysisIndexer;

impl AnalysisIndexer {
    pub(super) fn push_local_variable_fact(
        &mut self,
        name: &[u8],
        location: ruby_prism::Location<'_>,
    ) {
        let name = String::from_utf8_lossy(name).to_string();
        if let Ok(fqn) = FullyQualifiedName::local_variable(name) {
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::LocalVariable,
                self.range(&location),
            ));
        }
    }

    pub(super) fn push_instance_variable_fact(
        &mut self,
        name: &[u8],
        location: ruby_prism::Location<'_>,
    ) {
        let name = String::from_utf8_lossy(name).to_string();
        if let Ok(fqn) = FullyQualifiedName::instance_variable(name) {
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::InstanceVariable,
                self.range(&location),
            ));
        }
    }

    pub(super) fn push_class_variable_fact(
        &mut self,
        name: &[u8],
        location: ruby_prism::Location<'_>,
    ) {
        let name = String::from_utf8_lossy(name).to_string();
        if let Ok(fqn) = FullyQualifiedName::class_variable(name) {
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::ClassVariable,
                self.range(&location),
            ));
        }
    }

    pub(super) fn push_global_variable_fact(
        &mut self,
        name: &[u8],
        location: ruby_prism::Location<'_>,
    ) {
        let name = String::from_utf8_lossy(name).to_string();
        if let Ok(fqn) = FullyQualifiedName::global_variable(name) {
            self.facts.symbols.push(SymbolFact::new(
                fqn,
                SymbolKind::GlobalVariable,
                self.range(&location),
            ));
        }
    }
}
