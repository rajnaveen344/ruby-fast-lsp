//! `EngineQuery` definition lookup for the identifier at a cursor position.

use crate::invariant::ExpectInvariant;
use log::info;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::{FullyQualifiedName, SymbolKind};
use ruby_analysis::engine::AnalysisQuery;
use ruby_analysis::indexer::yard::parser::YardParser;
use ruby_analysis::indexer::{Identifier, RubyPrismAnalyzer};
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::features::cursor::analysis_location::{locations_for_ranges, non_empty_locations};
use crate::features::cursor::EngineQuery;
use crate::utils::lsp::{lsp_text_location, source_position};
use crate::utils::parser::position_to_offset;

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
        if let Some(yard_type) =
            YardParser::find_type_at_position(content, source_position(position))
        {
            info!("Found YARD type at position: {}", yard_type.type_name);
            // Get the enclosing namespace context for proper resolution
            let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
            let byte_offset = u32::try_from(position_to_offset(content, position))
                .expect_invariant(
                    "definition position exceeded u32 byte offsets",
                    "analysis TextRange offsets are u32",
                    "widen domain offsets before accepting larger source files",
                );
            let ancestors = analyzer.get_namespace_at_offset(byte_offset);
            info!("YARD type namespace context: {:?}", ancestors);
            return self.find_yard_type_definitions(&yard_type.type_name, &ancestors);
        }

        let analyzer = self.analyzer_at_position(uri, content, position);
        let byte_offset = u32::try_from(position_to_offset(content, position)).expect_invariant(
            "definition position exceeded u32 byte offsets",
            "analysis TextRange offsets are u32",
            "widen domain offsets before accepting larger source files",
        );
        let (identifier, _, ancestors, _scope_stack, namespace_kind) =
            analyzer.get_identifier(byte_offset);

        // Lexical bindings own their tokens even at the trailing cursor boundary
        // of an adjacent method reference, such as the `[` in table[key].
        // Neither a resolved enclosing call nor an Unknown dispatch barrier may
        // replace that binding with method navigation.
        if let Some(Identifier::RubyLocalVariable { name, .. }) = &identifier {
            return self.find_local_variable_definitions_at_position(name, position);
        }

        if let Some(locations) = self.resolved_reference_definition_locations(position) {
            return Some(locations);
        }

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

        self.find_definitions_for_identifier(
            &identifier,
            &ancestors,
            namespace_kind,
            position,
            content,
        )
    }

    /// An empty result records an engine dispatch barrier; lexical local-variable
    /// lookup remains valid even when the binding's value type is unknown.
    fn resolved_reference_definition_locations(&self, position: Position) -> Option<Vec<Location>> {
        let document = self.doc()?.read();
        let file_id = document.analysis_file_id();
        let byte_offset = document.position_to_analysis_offset(source_position(position));
        let engine = self.analysis_engine()?.read();
        let query = AnalysisQuery::new(&engine);
        let resolved = query.resolved_reference_definition_ranges_at(file_id, byte_offset);
        if query.navigation_must_fail_closed_at(file_id, byte_offset, !resolved.is_empty()) {
            return Some(Vec::new());
        }
        let locations = locations_for_ranges(&engine.view(), resolved);
        if !locations.is_empty() {
            return Some(locations);
        }
        None
    }

    /// Find definitions for a local variable using VariableScopes (position-based lookup)
    fn find_local_variable_definitions_at_position(
        &self,
        name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let doc_arc = self.doc()?;
        let document = doc_arc.read();

        let byte_offset = document.position_to_analysis_offset(source_position(position));
        document
            .local_variable_definition_range_before(name, byte_offset)
            .map(|range| vec![lsp_text_location(&document, range)])
            .or_else(|| {
                self.local_variable_definition_locations_from_analysis(
                    name,
                    document.analysis_file_id(),
                    byte_offset,
                )
            })
    }

    /// Find definitions for a global variable.
    fn find_global_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        self.global_variable_definition_locations_from_analysis(name)
    }

    /// Find definitions for a constant (class or module) by path.
    fn find_constant_definitions_by_path(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let fqn = self.resolve_constant_fqn(constant_path, ancestors);
        info!("Resolved constant FQN: {}", fqn);
        self.constant_definition_locations_from_analysis(constant_path, ancestors)
    }
}

fn method_receiver_allows_private(
    receiver: &ruby_analysis::core::MethodReceiver,
    content: &str,
    position: Position,
) -> bool {
    matches!(
        receiver,
        ruby_analysis::core::MethodReceiver::None | ruby_analysis::core::MethodReceiver::Super
    ) || static_send_symbol_at_position(content, position)
}

fn static_send_symbol_at_position(content: &str, position: Position) -> bool {
    let Some(line) = content.lines().nth(position.line as usize) else {
        return false;
    };
    line.contains(".send(:")
        || line.contains(".__send__(:")
        || line.contains(".send(\"")
        || line.contains(".__send__(\"")
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
        content: &str,
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
            } => {
                if method_receiver_allows_private(receiver, content, position) {
                    self.find_method_definitions(
                        receiver,
                        iden,
                        namespace,
                        namespace_kind,
                        position,
                    )
                } else {
                    let caller_namespace_fqn =
                        FullyQualifiedName::namespace_with_kind(ancestors.to_vec(), namespace_kind);
                    self.find_protected_method_definitions(
                        receiver,
                        iden,
                        namespace,
                        namespace_kind,
                        position,
                        &caller_namespace_fqn,
                    )
                }
            }
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
        self.yard_type_definition_locations_from_analysis(type_name, ancestors)
    }

    /// Find instance variable definitions.
    fn find_instance_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        self.instance_variable_definition_locations_from_analysis(name)
    }

    /// Find class variable definitions.
    fn find_class_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        self.class_variable_definition_locations_from_analysis(name)
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

        FullyQualifiedName::constant(constant_path.to_vec())
    }

    fn resolve_constant_fqn_from_analysis(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        AnalysisQuery::new(&engine).resolve_constant_in_context(constant_path, ancestors)
    }

    fn constant_definition_locations_from_analysis(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine.view(),
            query.constant_definition_ranges(constant_path, ancestors),
        ))
    }

    fn yard_type_definition_locations_from_analysis(
        &self,
        type_name: &str,
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine.view(),
            query.yard_type_definition_ranges(type_name, ancestors),
        ))
    }

    fn instance_variable_definition_locations_from_analysis(
        &self,
        name: &str,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine.view(),
            query.instance_variable_definition_ranges(name),
        ))
    }

    fn class_variable_definition_locations_from_analysis(
        &self,
        name: &str,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine.view(),
            query.class_variable_definition_ranges(name),
        ))
    }

    fn global_variable_definition_locations_from_analysis(
        &self,
        name: &str,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine.view(),
            query.global_variable_definition_ranges(name),
        ))
    }

    fn local_variable_definition_locations_from_analysis(
        &self,
        name: &str,
        file_id: ruby_analysis::core::SourceFileId,
        byte_offset: u32,
    ) -> Option<Vec<Location>> {
        let fqn = FullyQualifiedName::local_variable(name.to_string()).ok()?;
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let range = engine
            .view()
            .symbol_facts_for(&fqn)
            .into_iter()
            .filter(|fact| fact.kind == SymbolKind::LocalVariable)
            .filter(|fact| fact.range.file_id == file_id)
            .filter(|fact| fact.range.start_byte < byte_offset)
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.range)?;
        non_empty_locations(locations_for_ranges(&engine.view(), vec![range]))
    }
}
