//! Reference Query - Find usages of symbols
//!
//! Consolidates reference logic from `capabilities/references.rs`.

use crate::analyzer_prism::{Identifier, MethodReceiver, RubyPrismAnalyzer};
use crate::indexer::entry::NamespaceKind;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;
use crate::types::ruby_namespace::RubyConstant;
use crate::yard::YardTypeConverter;
use log::info;
use ruby_analysis_core::{ReferenceFact, TextRange};
use ruby_analysis_engine::SourceFile;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use super::IndexQuery;

impl IndexQuery {
    /// Find all references to the symbol at the given position.
    pub fn find_references_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier_opt, _, ancestors, _scope_stack, namespace_kind) =
            analyzer.get_identifier(position);

        let identifier = identifier_opt?;

        self.find_references_for_identifier(&identifier, &ancestors, namespace_kind, position)
    }

    /// Find references to a constant by FQN.
    fn find_constant_references(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        if let Some(entries) = self.reference_locations_from_analysis(fqn) {
            info!("Found {} constant references to: {}", entries.len(), fqn);
            return Some(entries);
        }

        let index = self.index.lock();
        let entries = index.references(fqn);
        if !entries.is_empty() {
            info!("Found {} constant references to: {}", entries.len(), fqn);
            return Some(entries);
        }
        None
    }

    /// Find references to a variable (instance, class, or global).
    fn find_variable_references(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        if let Some(entries) = self.reference_locations_from_analysis(fqn) {
            info!("Found {} variable references to: {}", entries.len(), fqn);
            return Some(entries);
        }

        let index = self.index.lock();
        let entries = index.references(fqn);
        if !entries.is_empty() {
            info!("Found {} variable references to: {}", entries.len(), fqn);
            return Some(entries);
        }
        None
    }

    /// Find references to a method.
    ///
    /// Uses the same type-inference-based receiver resolution as go-to-definition
    /// to correctly resolve expression receivers. If the receiver type cannot be
    /// inferred, returns None rather than guessing (correctness over completeness).
    fn find_method_references(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        ancestors: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<Vec<Location>> {
        // `def initialize` is indexed as `new` (singleton) — map accordingly
        if method.as_str() == "initialize" {
            if let Ok(new_method) = RubyMethod::new("new") {
                let context_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
                return self.find_method_refs_with_receiver(&context_fqn, &new_method);
            }
        }

        match receiver {
            MethodReceiver::Constant(receiver_ns) => {
                let receiver_fqn = self.resolve_receiver_fqn(receiver_ns, ancestors);
                self.find_method_refs_with_receiver(&receiver_fqn, method)
            }
            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                let context_fqn = FullyQualifiedName::Constant(ancestors.to_vec());
                self.find_method_refs_without_receiver(&context_fqn, method)
            }
            // For expression receivers, use type inference to resolve the actual type.
            // This mirrors go-to-definition's `resolve_receiver_to_namespace`.
            _ => {
                let resolved_ns = self.resolve_receiver_to_namespace(
                    receiver,
                    ancestors,
                    namespace_kind,
                    position,
                )?;
                self.find_method_refs_for_resolved_namespace(&resolved_ns, method)
            }
        }
    }

    /// Find references to a local variable using VariableScopes.
    fn find_local_variable_references(
        &self,
        name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let doc_arc = self.doc.as_ref()?;
        let document = doc_arc.read();

        // Use position-based lookup to find the scope owning this variable
        let scope_id = document
            .variable_scopes()
            .find_scope_for_variable_at(name, position)?;

        // Use VariableScopes to find all references
        let targets = document
            .variable_scopes()
            .find_rename_targets(name, scope_id);

        if targets.is_empty() {
            return None;
        }

        let mut all_locations = Vec::new();
        for target in targets {
            all_locations.push(target.location);
        }

        Some(all_locations)
    }
}

// Private helpers
impl IndexQuery {
    /// Find references for a given identifier.
    fn find_references_for_identifier(
        &self,
        identifier: &Identifier,
        ancestors: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<Vec<Location>> {
        match identifier {
            Identifier::RubyConstant { namespace: _, iden } => {
                let mut combined_ns = ancestors.to_vec();
                combined_ns.extend(iden.clone());

                // Try as Namespace first (for class/module references)
                let namespace_fqn = FullyQualifiedName::namespace(combined_ns.clone());
                if let Some(refs) = self.find_constant_references(&namespace_fqn) {
                    return Some(refs);
                }

                // Then try as Constant (for value constant references like VALUE = 42)
                let constant_fqn = FullyQualifiedName::Constant(combined_ns);
                self.find_constant_references(&constant_fqn)
            }
            Identifier::RubyMethod {
                namespace: _,
                receiver,
                iden,
            } => self.find_method_references(receiver, iden, ancestors, namespace_kind, position),
            Identifier::RubyInstanceVariable { name, .. } => {
                if let Ok(fqn) = FullyQualifiedName::instance_variable(name.clone()) {
                    self.find_variable_references(&fqn)
                } else {
                    None
                }
            }
            Identifier::RubyClassVariable { name, .. } => {
                if let Ok(fqn) = FullyQualifiedName::class_variable(name.clone()) {
                    self.find_variable_references(&fqn)
                } else {
                    None
                }
            }
            Identifier::RubyGlobalVariable { name, .. } => {
                if let Ok(fqn) = FullyQualifiedName::global_variable(name.clone()) {
                    self.find_variable_references(&fqn)
                } else {
                    None
                }
            }
            Identifier::RubyLocalVariable { name, .. } => {
                self.find_local_variable_references(name, position)
            }
            Identifier::YardType { type_name, .. } => {
                if let Some(fqn) = YardTypeConverter::parse_type_name_to_fqn_public(type_name) {
                    self.find_constant_references(&fqn)
                } else {
                    None
                }
            }
        }
    }

    /// Resolve receiver FQN from namespace path.
    fn resolve_receiver_fqn(
        &self,
        receiver_ns: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> FullyQualifiedName {
        if !receiver_ns.is_empty() && !ancestors.is_empty() {
            let first_receiver_part = &receiver_ns[0];
            if let Some(pos) = ancestors.iter().position(|c| c == first_receiver_part) {
                let mut resolved_ns = ancestors[..=pos].to_vec();
                resolved_ns.extend(receiver_ns[1..].iter().cloned());
                return FullyQualifiedName::Constant(resolved_ns);
            } else {
                let mut full_ns = vec![ancestors[0].clone()];
                full_ns.extend(receiver_ns.iter().cloned());
                return FullyQualifiedName::Constant(full_ns);
            }
        }
        let mut full_ns = ancestors.to_vec();
        full_ns.extend(receiver_ns.iter().cloned());
        FullyQualifiedName::Constant(full_ns)
    }

    /// Find method references when the receiver has been resolved to a namespace FQN.
    /// Searches the namespace's ancestor chain, descendants, and including classes.
    fn find_method_refs_for_resolved_namespace(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let mut all_references = Vec::new();
        let kind = NamespaceKind::Instance;

        if let Some(refs) = self.find_method_refs_in_ancestor_chain(namespace_fqn, method, kind) {
            all_references.extend(refs);
        }

        if let Some(refs) = self.find_method_refs_in_descendants(namespace_fqn, method, kind) {
            all_references.extend(refs);
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    /// Find method references with a constant receiver (singleton namespace).
    fn find_method_refs_with_receiver(
        &self,
        receiver_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        self.find_method_refs_in_ancestor_chain(receiver_fqn, method, NamespaceKind::Singleton)
    }

    /// Find method references without a receiver (instance method in current scope).
    fn find_method_refs_without_receiver(
        &self,
        context_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let mut all_references = Vec::new();
        let method_kind = NamespaceKind::Instance;

        if let Some(refs) =
            self.find_method_refs_in_ancestor_chain(context_fqn, method, method_kind)
        {
            all_references.extend(refs);
        }

        // Also search in classes that include this module
        if let Some(refs) =
            self.find_method_refs_in_sibling_modules(context_fqn, method, method_kind)
        {
            all_references.extend(refs);
        }

        // Search descendants (subclasses) — a call to `parent_method` in Child < Parent
        // is indexed as Child#parent_method, so we need to check subclasses too
        if let Some(refs) = self.find_method_refs_in_descendants(context_fqn, method, method_kind) {
            all_references.extend(refs);
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    /// Find method references in ancestor chain.
    fn find_method_refs_in_ancestor_chain(
        &self,
        context_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        kind: NamespaceKind,
    ) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let mut all_references = Vec::new();
        let context_ns =
            FullyQualifiedName::namespace_with_kind(context_fqn.namespace_parts(), kind);
        let ancestor_chain = index.get_ancestor_chain(&context_ns);

        for ancestor_fqn in ancestor_chain {
            let method_fqn =
                FullyQualifiedName::method(ancestor_fqn.namespace_parts(), method.clone());
            let refs = self.reference_locations_for_fqn_with_index_fallback(&index, &method_fqn);
            if !refs.is_empty() {
                all_references.extend(refs);
            }

            // Also check including classes
            let including_classes = index.including_classes(&ancestor_fqn);
            for (including_class_fqn, _via_modules) in including_classes {
                let inc_method_fqn = FullyQualifiedName::method(
                    including_class_fqn.namespace_parts(),
                    method.clone(),
                );
                let inc_refs =
                    self.reference_locations_for_fqn_with_index_fallback(&index, &inc_method_fqn);
                if !inc_refs.is_empty() {
                    all_references.extend(inc_refs);
                }
            }
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    /// Find method references in sibling modules.
    fn find_method_refs_in_sibling_modules(
        &self,
        module_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        kind: NamespaceKind,
    ) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let mut all_references = Vec::new();
        let including_classes = index.including_classes(module_fqn);

        for (including_class_fqn, _via_modules) in including_classes {
            let class_ns = FullyQualifiedName::namespace_with_kind(
                including_class_fqn.namespace_parts(),
                kind,
            );
            let ancestor_chain = index.get_ancestor_chain(&class_ns);
            for ancestor_fqn in ancestor_chain {
                let method_fqn =
                    FullyQualifiedName::method(ancestor_fqn.namespace_parts(), method.clone());
                let refs =
                    self.reference_locations_for_fqn_with_index_fallback(&index, &method_fqn);
                if !refs.is_empty() {
                    all_references.extend(refs);
                }
            }
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    /// Find method references in descendant classes (subclasses, sub-subclasses, etc.)
    /// and descendants of classes that include this module.
    ///
    /// When `parent_method` is defined in Parent and called as a bare method in
    /// `Child < Parent`, the reference is indexed as `Child#parent_method`.
    /// Similarly, when a module method is called in a subclass of the including class.
    fn find_method_refs_in_descendants(
        &self,
        context_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        kind: NamespaceKind,
    ) -> Option<Vec<Location>> {
        let index = self.index.lock();
        let mut all_references = Vec::new();

        let context_ns =
            FullyQualifiedName::namespace_with_kind(context_fqn.namespace_parts(), kind);

        // Collect all FQNs to check descendants of:
        // 1. The context itself (direct subclasses)
        // 2. Classes that include this module (their subclasses too)
        let mut roots_to_search = vec![context_ns.clone()];
        let including_classes = index.including_classes(&context_ns);
        for (class_fqn, _via) in &including_classes {
            roots_to_search.push(FullyQualifiedName::namespace_with_kind(
                class_fqn.namespace_parts(),
                kind,
            ));
        }

        for root in &roots_to_search {
            let descendants = index.descendants(root);
            for descendant_fqn in descendants {
                let method_fqn =
                    FullyQualifiedName::method(descendant_fqn.namespace_parts(), method.clone());
                let refs =
                    self.reference_locations_for_fqn_with_index_fallback(&index, &method_fqn);
                if !refs.is_empty() {
                    all_references.extend(refs);
                }
            }
        }

        if all_references.is_empty() {
            None
        } else {
            Some(all_references)
        }
    }

    fn reference_locations_for_fqn_with_index_fallback(
        &self,
        index: &crate::indexer::index::RubyIndex,
        fqn: &FullyQualifiedName,
    ) -> Vec<Location> {
        self.reference_locations_from_analysis(fqn)
            .unwrap_or_else(|| index.references(fqn))
    }

    fn reference_locations_from_analysis(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let mut locations = Vec::new();
        for fact in engine.reference_facts_for(fqn) {
            if let Some(location) = reference_fact_to_location(&engine, fact) {
                locations.push(location);
            }
        }
        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }
}

fn reference_fact_to_location(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fact: &ReferenceFact,
) -> Option<Location> {
    let file = engine.file(fact.range.file_id)?;
    Some(Location {
        uri: source_file_uri(file)?,
        range: text_range_to_lsp_range(file, fact.range)?,
    })
}

fn source_file_uri(file: &SourceFile) -> Option<Url> {
    Url::from_file_path(&file.path).ok()
}

fn text_range_to_lsp_range(file: &SourceFile, range: TextRange) -> Option<Range> {
    assert!(
        file.id == range.file_id,
        "INVARIANT VIOLATED: reference range file id does not match source file id. \
         This is a bug because analysis facts must only be converted with their owning source file. \
         Fix: look up the SourceFile by fact.range.file_id before converting."
    );
    Some(Range::new(
        byte_offset_to_position(&file.source, range.start_byte)?,
        byte_offset_to_position(&file.source, range.end_byte)?,
    ))
}

fn byte_offset_to_position(source: &str, byte_offset: u32) -> Option<Position> {
    let target = usize::try_from(byte_offset).ok()?;
    if target > source.len() || !source.is_char_boundary(target) {
        return None;
    }

    let mut line = 0u32;
    let mut line_start = 0usize;
    for (idx, byte) in source.bytes().enumerate() {
        if idx >= target {
            break;
        }
        if byte == b'\n' {
            line += 1;
            line_start = idx + 1;
        }
    }
    let character = source[line_start..target].chars().count();
    let character = u32::try_from(character).expect(
        "INVARIANT VIOLATED: LSP character offset exceeded u32. \
         This is a bug because LSP positions require u32 columns. \
         Fix: reject or segment lines longer than u32::MAX characters.",
    );
    Some(Position::new(line, character))
}
