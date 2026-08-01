//! Definition Query - Find where symbols are defined
//!
//! Consolidates definition logic from `capabilities/definitions/`.

use log::info;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::{FullyQualifiedName, SymbolKind};
use ruby_analysis::engine::AnalysisQuery;
use ruby_analysis::indexer::yard::YardParser;
use ruby_analysis::indexer::{Identifier, MethodReceiver, RubyPrismAnalyzer};
use tower_lsp::lsp_types::{Location, Position, Url};

use super::analysis_location::{locations_for_ranges, non_empty_locations};
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

        if let Some(locations) = self.resolved_reference_definition_locations(position) {
            return Some(locations);
        }

        let analyzer = self.analyzer_at_position(uri, content, position);
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

        self.find_definitions_for_identifier(
            &identifier,
            &ancestors,
            namespace_kind,
            position,
            content,
        )
    }

    fn resolved_reference_definition_locations(&self, position: Position) -> Option<Vec<Location>> {
        let document = self.doc.as_ref()?.read();
        let file_id = document.analysis_file_id();
        let byte_offset = document.position_to_analysis_offset(position);
        let engine = self.analysis_engine()?.read();
        let ranges = AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(file_id, byte_offset);
        non_empty_locations(locations_for_ranges(&engine, ranges))
    }

    /// Find definitions for a local variable using VariableScopes (position-based lookup)
    fn find_local_variable_definitions_at_position(
        &self,
        name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let doc_arc = self.doc.as_ref()?;
        let document = doc_arc.read();

        let byte_offset = document.position_to_analysis_offset(position);
        document
            .local_variable_definition_range_before(name, byte_offset)
            .map(|range| vec![document.text_range_to_lsp_location(range)])
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
    receiver: &ruby_analysis::indexer::MethodReceiver,
    content: &str,
    position: Position,
) -> bool {
    matches!(
        receiver,
        ruby_analysis::indexer::MethodReceiver::None
            | ruby_analysis::indexer::MethodReceiver::Super
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

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DefinitionNavigationDemandKeys {
    pub(crate) project_key: Option<String>,
    pub(crate) dependency_key: Option<String>,
}

pub(crate) fn definition_navigation_demand_keys(
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<DefinitionNavigationDemandKeys> {
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
    let (identifier, _, ancestors, _, _) = analyzer.get_identifier(position);
    let (project_constant, dependency_constant) = match identifier? {
        Identifier::RubyConstant { iden, .. } => (iden.last().cloned(), iden.first().cloned()),
        Identifier::RubyMethod {
            namespace,
            receiver,
            ..
        } => match receiver {
            MethodReceiver::Constant(parts) => (parts.last().cloned(), parts.first().cloned()),
            MethodReceiver::None | MethodReceiver::SelfReceiver | MethodReceiver::Super => {
                (namespace.last().cloned(), namespace.first().cloned())
            }
            MethodReceiver::LocalVariable(_)
            | MethodReceiver::InstanceVariable(_)
            | MethodReceiver::ClassVariable(_)
            | MethodReceiver::GlobalVariable(_)
            | MethodReceiver::MethodCall { .. }
            | MethodReceiver::Literal(_)
            | MethodReceiver::Expression => (ancestors.last().cloned(), ancestors.first().cloned()),
        },
        Identifier::YardType { type_name, .. } => {
            let mut parts = type_name.split("::").filter(|part| !part.is_empty());
            let first = parts.next().map(ToString::to_string);
            let last = parts.last().or(first.as_deref()).map(ToString::to_string);
            return normalized_definition_navigation_keys(last.as_deref(), first.as_deref());
        }
        Identifier::RubyLocalVariable { .. }
        | Identifier::RubyInstanceVariable { .. }
        | Identifier::RubyClassVariable { .. }
        | Identifier::RubyGlobalVariable { .. } => return None,
    };
    normalized_definition_navigation_keys(
        project_constant
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        dependency_constant
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
    )
}

fn normalized_definition_navigation_keys(
    project_name: Option<&str>,
    dependency_name: Option<&str>,
) -> Option<DefinitionNavigationDemandKeys> {
    let project_key = project_name
        .map(crate::navigation_demand::normalize_navigation_key)
        .filter(|key| !key.is_empty());
    let dependency_key = dependency_name
        .map(crate::navigation_demand::normalize_navigation_key)
        .filter(|key| !key.is_empty());
    (project_key.is_some() || dependency_key.is_some()).then_some(DefinitionNavigationDemandKeys {
        project_key,
        dependency_key,
    })
}

#[cfg(test)]
mod navigation_demand_tests {
    use super::*;

    #[test]
    fn constant_definition_request_exposes_exact_project_and_dependency_keys() {
        let source = "GoshPosh::Platform::Users::UserPmm.by_username(name)\n";
        let uri = Url::parse("file:///project/caller.rb").unwrap();

        let demand = definition_navigation_demand_keys(&uri, Position::new(0, 35), source).unwrap();

        assert_eq!(demand.project_key.as_deref(), Some("userpmm"));
        assert_eq!(demand.dependency_key.as_deref(), Some("goshposh"));
    }

    #[test]
    fn constant_receiver_method_request_prioritizes_its_owning_constant() {
        let source = "BSON::ObjectId.new\n";
        let uri = Url::parse("file:///project/caller.rb").unwrap();

        let demand = definition_navigation_demand_keys(&uri, Position::new(0, 16), source).unwrap();

        assert_eq!(demand.project_key.as_deref(), Some("objectid"));
        assert_eq!(demand.dependency_key.as_deref(), Some("bson"));
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
            &engine,
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
            &engine,
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
            &engine,
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
            &engine,
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
            &engine,
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
            .symbol_facts_for(&fqn)
            .into_iter()
            .filter(|fact| fact.kind == SymbolKind::LocalVariable)
            .filter(|fact| fact.range.file_id == file_id)
            .filter(|fact| fact.range.start_byte < byte_offset)
            .max_by_key(|fact| fact.range.start_byte)
            .map(|fact| fact.range)?;
        non_empty_locations(locations_for_ranges(&engine, vec![range]))
    }
}
