//! Definition Query - Find where symbols are defined
//!
//! Consolidates definition logic from `capabilities/definitions/`.

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use crate::yard::YardParser;
use log::info;
use ruby_analysis_core::NamespaceKind;
use ruby_analysis_core::SymbolKind as AnalysisSymbolKind;
use ruby_analysis_engine::AnalysisQuery;
use tower_lsp::lsp_types::{Location, Position, Url};

use super::analysis_location::location_for_range;
use super::EngineQuery;

impl EngineQuery {
    /// Find definitions for an identifier at the given position.
    ///
    /// This handles all identifier types:
    /// - Constants (classes, modules)
    /// - Methods (instance and class methods)
    /// - Variables (local, instance, class, global)
    /// - YARD type references
    pub fn find_definitions_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        // First check if we're in a YARD comment type reference
        if let Some(yard_type) = YardParser::find_type_at_position(content, position) {
            info!("Found YARD type at position: {}", yard_type.type_name);
            // Get the enclosing namespace context for proper resolution
            let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
            let ancestors = analyzer.get_namespace_at_position(position);
            info!("YARD type namespace context: {:?}", ancestors);
            return self.find_yard_type_definitions(&yard_type.type_name, &ancestors);
        }

        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier, _, ancestors, _scope_stack, namespace_kind) =
            analyzer.get_identifier(position);

        let identifier = match identifier {
            Some(id) => id,
            None => {
                info!("No identifier found at position {:?}", position);
                return None;
            }
        };

        info!(
            "Looking for definition of: {}->{}",
            FullyQualifiedName::from(ancestors.clone()),
            identifier,
        );

        self.find_definitions_for_identifier(&identifier, &ancestors, namespace_kind, position)
    }

    /// Find definitions for a local variable using VariableScopes (position-based lookup)
    fn find_local_variable_definitions_at_position(
        &self,
        name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let doc_arc = self.doc.as_ref()?;
        let document = doc_arc.read();

        // Use position-based scope lookup in the VariableScopes tree
        let tree_scope_id = document
            .find_scope_for_variable_at(name, position)
            .or_else(|| document.scope_at_position(position))?;

        if let Some((_sid, var)) = document
            .variable_scopes()
            .find_variable(name, tree_scope_id)
        {
            if var.definition_location.start_byte < document.position_to_analysis_offset(position) {
                return Some(vec![
                    document.text_range_to_lsp_location(var.definition_location)
                ]);
            }
        }

        None
    }

    /// Find definitions for a global variable.
    fn find_global_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        if let Ok(fqn) = FullyQualifiedName::global_variable(name.to_string()) {
            return self.find_variable_definitions(&fqn);
        }
        None
    }

    /// Find definitions for a constant (class or module) by path.
    fn find_constant_definitions_by_path(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let fqn = self.resolve_constant_fqn(constant_path, ancestors);
        info!("Resolved constant FQN: {}", fqn);

        if let Some(locations) = self.symbol_locations_from_analysis(
            &fqn,
            &[
                AnalysisSymbolKind::Class,
                AnalysisSymbolKind::Module,
                AnalysisSymbolKind::Constant,
            ],
        ) {
            return Some(locations);
        }

        None
    }
}

// Private helpers
impl EngineQuery {
    /// Find definitions for a given identifier.
    fn find_definitions_for_identifier(
        &self,
        identifier: &Identifier,
        ancestors: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
    ) -> Option<Vec<Location>> {
        match identifier {
            Identifier::RubyConstant { namespace: _, iden } => {
                // iden is Vec<RubyConstant> - the full constant path being referenced
                self.find_constant_definitions_by_path(iden, ancestors)
            }
            Identifier::RubyMethod {
                namespace,
                receiver,
                iden,
            } => self.find_method_definitions(receiver, iden, namespace, namespace_kind, position),
            Identifier::RubyInstanceVariable { name, .. } => {
                self.find_instance_variable_definitions(name)
            }
            Identifier::RubyClassVariable { name, .. } => {
                self.find_class_variable_definitions(name)
            }
            Identifier::RubyGlobalVariable { name, .. } => {
                self.find_global_variable_definitions(name)
            }
            Identifier::RubyLocalVariable { name, .. } => {
                self.find_local_variable_definitions_at_position(name, position)
            }
            Identifier::YardType { type_name, .. } => {
                // YardType identifier doesn't have namespace context, use empty ancestors
                // The main YARD type path (detected via YardParser) handles namespace resolution
                self.find_yard_type_definitions(type_name, &[])
            }
        }
    }

    /// Find definitions for a YARD type reference string (e.g., "String", "Foo::Bar").
    /// Uses namespace resolution to find types relative to the enclosing scope.
    fn find_yard_type_definitions(
        &self,
        type_name: &str,
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        // Handle built-in types
        let builtins = ["nil", "true", "false", "void", "Boolean", "bool"];
        if builtins.iter().any(|b| b.eq_ignore_ascii_case(type_name)) {
            return None;
        }

        // Check if it's a root constant (starts with ::)
        let is_root_constant = type_name.starts_with("::");
        let type_to_parse = if is_root_constant {
            &type_name[2..] // Strip leading ::
        } else {
            type_name
        };

        // Parse path into constant parts
        let parts: Vec<&str> = type_to_parse.split("::").collect();
        let mut constant_path = Vec::new();
        for part in parts {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(constant) = RubyConstant::try_from(trimmed) {
                constant_path.push(constant);
            } else {
                return None;
            }
        }

        if constant_path.is_empty() {
            return None;
        }

        // Use namespace resolution (same as code constant resolution)
        // For root constants (::Foo), use empty ancestors
        let effective_ancestors = if is_root_constant { &[][..] } else { ancestors };

        // Reuse the same resolution logic as code constants
        self.find_constant_definitions_by_path(&constant_path, effective_ancestors)
    }

    /// Find instance variable definitions.
    fn find_instance_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        // Instance variables are stored with just their name (e.g., "@foo")
        if let Ok(fqn) = FullyQualifiedName::instance_variable(name.to_string()) {
            if let Some(locations) =
                self.symbol_locations_from_analysis(&fqn, &[AnalysisSymbolKind::InstanceVariable])
            {
                return Some(locations);
            }
        }
        None
    }

    /// Find class variable definitions.
    fn find_class_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        // Class variables are stored with just their name (e.g., "@@foo")
        if let Ok(fqn) = FullyQualifiedName::class_variable(name.to_string()) {
            if let Some(locations) =
                self.symbol_locations_from_analysis(&fqn, &[AnalysisSymbolKind::ClassVariable])
            {
                return Some(locations);
            }
        }
        None
    }

    /// Resolve constant FQN from path.
    pub(crate) fn resolve_constant_fqn(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> FullyQualifiedName {
        if let Some(fqn) = self.resolve_constant_fqn_from_analysis(constant_path, ancestors) {
            return fqn;
        }

        FullyQualifiedName::Constant(constant_path.to_vec())
    }

    /// Find variable definitions by FQN.
    fn find_variable_definitions(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        if let Some(locations) = self.symbol_locations_from_analysis(
            fqn,
            &[
                AnalysisSymbolKind::LocalVariable,
                AnalysisSymbolKind::InstanceVariable,
                AnalysisSymbolKind::ClassVariable,
                AnalysisSymbolKind::GlobalVariable,
            ],
        ) {
            return Some(locations);
        }

        None
    }

    fn resolve_constant_fqn_from_analysis(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        AnalysisQuery::new(&engine).resolve_constant_in_context(constant_path, ancestors)
    }

    fn symbol_locations_from_analysis(
        &self,
        fqn: &FullyQualifiedName,
        allowed_kinds: &[AnalysisSymbolKind],
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.lock();
        let query = AnalysisQuery::new(&engine);
        let locations = query
            .symbol_definition_ranges(fqn, allowed_kinds)
            .into_iter()
            .filter_map(|range| location_for_range(&engine, range))
            .collect::<Vec<_>>();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }
}
