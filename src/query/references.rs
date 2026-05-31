//! Reference Query - Find usages of symbols
//!
//! Consolidates reference logic from `capabilities/references.rs`.

use log::info;
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::core::NamespaceKind;
use ruby_analysis::core::RubyConstant;
use ruby_analysis::core::RubyMethod;
use ruby_analysis::indexer::fact_collector::{FactCollector, NullFactCollectorExtensionHost};
use ruby_analysis::indexer::yard::YardTypeConverter;
use ruby_analysis::indexer::{Identifier, MethodReceiver, RubyPrismAnalyzer};
use ruby_prism::Visit;
use std::path::Path;
use std::sync::Arc;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

use super::analysis_location::{locations_for_ranges, non_empty_locations};
use super::EngineQuery;

impl EngineQuery {
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

        self.find_references_for_identifier(
            &identifier,
            &ancestors,
            namespace_kind,
            position,
            content,
        )
    }

    /// Find references to a constant by FQN.
    fn find_constant_references(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        if let Some(entries) = self.reference_locations_for_fqn_from_analysis(fqn) {
            info!("Found {} constant references to: {}", entries.len(), fqn);
            return Some(entries);
        }
        None
    }

    /// Find references to a variable (instance, class, or global).
    fn find_variable_references(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        if let Some(entries) = self.variable_reference_locations_from_analysis(fqn) {
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
        content: &str,
    ) -> Option<Vec<Location>> {
        // `def initialize` is indexed as `new` (singleton) — map accordingly
        if method.as_str() == "initialize" {
            if let Ok(new_method) = RubyMethod::new("new") {
                let namespace_fqn = FullyQualifiedName::namespace_with_kind(
                    ancestors.to_vec(),
                    NamespaceKind::Singleton,
                );
                return self.method_reference_locations_for_namespace_from_analysis(
                    &namespace_fqn,
                    &new_method,
                );
            }
        }

        let locations = match receiver {
            MethodReceiver::Constant(receiver_ns) => self
                .method_reference_locations_for_constant_receiver_from_analysis(
                    receiver_ns,
                    ancestors,
                    method,
                ),
            MethodReceiver::Super => self.method_reference_locations_for_super_from_analysis(
                ancestors,
                namespace_kind,
                method,
            ),
            MethodReceiver::None => self
                .method_reference_locations_for_current_scope_from_analysis(
                    ancestors,
                    namespace_kind,
                    method,
                ),
            MethodReceiver::SelfReceiver => {
                let namespace_fqn =
                    FullyQualifiedName::namespace_with_kind(ancestors.to_vec(), namespace_kind);
                self.method_reference_locations_for_protected_receiver_from_analysis(
                    &namespace_fqn,
                    method,
                    &namespace_fqn,
                )
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
                if static_send_symbol_at_position(content, position) {
                    self.method_reference_locations_for_namespace_from_analysis(
                        &resolved_ns,
                        method,
                    )
                } else {
                    let caller_namespace_fqn =
                        FullyQualifiedName::namespace_with_kind(ancestors.to_vec(), namespace_kind);
                    self.method_reference_locations_for_protected_receiver_from_analysis(
                        &resolved_ns,
                        method,
                        &caller_namespace_fqn,
                    )
                }
            }
        }?;

        Some(self.filter_invalid_private_method_reference_locations(method, locations))
    }

    fn filter_invalid_private_method_reference_locations(
        &self,
        method: &RubyMethod,
        locations: Vec<Location>,
    ) -> Vec<Location> {
        let Some(doc_arc) = self.doc.as_ref() else {
            return locations;
        };
        let document = doc_arc.read();
        if !document_declares_private_method(&document.content, method.as_str())
            && !self.analysis_engine_has_private_method(method)
        {
            return locations;
        }
        let Some(uri) = self.uri.as_ref() else {
            return locations;
        };
        locations
            .into_iter()
            .filter(|location| {
                let Some(content) = self.location_content(location, uri, document.content.as_str())
                else {
                    return true;
                };
                !range_uses_invalid_private_receiver(&content, location.range)
                    || explicit_receiver_constant_parts(&content, location.range).is_none()
                    || self.private_receiver_allowed_by_visibility_override(
                        method,
                        &content,
                        location.range,
                    )
            })
            .collect()
    }

    fn private_receiver_allowed_by_visibility_override(
        &self,
        method: &RubyMethod,
        content: &str,
        range: Range,
    ) -> bool {
        let Some(receiver_parts) = explicit_receiver_constant_parts(content, range) else {
            return false;
        };
        let Some(engine) = self.analysis_engine() else {
            return false;
        };
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        let receiver_fqn =
            FullyQualifiedName::namespace_with_kind(receiver_parts, NamespaceKind::Instance);
        query
            .resolve_public_method_callees(&receiver_fqn, method)
            .is_some_and(|callees| {
                callees
                    .iter()
                    .any(|callee| !callee.definition_ranges.is_empty())
            })
    }

    fn analysis_engine_has_private_method(&self, method: &RubyMethod) -> bool {
        let Some(engine) = self.analysis_engine() else {
            return false;
        };
        let engine = engine.read();
        engine.all_method_facts().iter().any(|fact| {
            let FullyQualifiedName::Method(_, fact_method) = &fact.fqn else {
                return false;
            };
            *fact_method == *method
                && fact.visibility == ruby_analysis::core::method_store::MethodVisibility::Private
        }) || engine.all_method_visibility_overrides().iter().any(|fact| {
            fact.method == *method
                && fact.visibility == ruby_analysis::core::method_store::MethodVisibility::Private
        })
    }

    fn location_content(
        &self,
        location: &Location,
        current_uri: &Url,
        current_content: &str,
    ) -> Option<String> {
        if &location.uri == current_uri {
            return Some(current_content.to_string());
        }

        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        let path = location.uri.to_file_path().ok()?;
        if let Some(file_id) = query.file_id(&path) {
            return query.file(file_id)?.source.clone();
        }
        if let Ok(relative) = path.strip_prefix(Path::new("/")) {
            if let Some(file_id) = query.file_id(relative) {
                return query.file(file_id)?.source.clone();
            }
            return engine
                .files()
                .find(|file| file.path.ends_with(relative))
                .and_then(|file| file.source.clone());
        }
        None
    }

    /// Find references to a local variable using VariableScopes.
    fn find_local_variable_references(
        &self,
        name: &str,
        position: Position,
    ) -> Option<Vec<Location>> {
        let doc_arc = self.doc.as_ref()?;
        let document = doc_arc.read();

        let byte_offset = document.position_to_analysis_offset(position);
        let ranges = document.local_variable_reference_ranges_at(name, byte_offset);
        if !ranges.is_empty() {
            return Some(
                ranges
                    .into_iter()
                    .map(|range| document.text_range_to_lsp_location(range))
                    .collect(),
            );
        }
        drop(document);

        self.rebuild_local_variable_scopes_for_open_document()?;
        let document = doc_arc.read();
        let ranges = document.local_variable_reference_ranges_at(name, byte_offset);
        if ranges.is_empty() {
            return None;
        }
        Some(
            ranges
                .into_iter()
                .map(|range| document.text_range_to_lsp_location(range))
                .collect(),
        )
    }
}

fn document_declares_private_method(content: &str, method: &str) -> bool {
    let mut visibility_private = false;
    for line in content.lines() {
        let trimmed = line.trim_start();
        match trimmed {
            "private" => {
                visibility_private = true;
                continue;
            }
            "protected" | "public" => {
                visibility_private = false;
                continue;
            }
            _ => {}
        }
        let Some(rest) = trimmed.strip_prefix("def ") else {
            if visibility_line_mentions_method(trimmed, "private", method) {
                return true;
            }
            continue;
        };
        let name = rest
            .split(|ch: char| ch.is_whitespace() || matches!(ch, '(' | ';'))
            .next()
            .unwrap_or("");
        if name == method && visibility_private {
            return true;
        }
    }
    false
}

fn visibility_line_mentions_method(line: &str, keyword: &str, method: &str) -> bool {
    let Some(rest) = line.strip_prefix(keyword) else {
        return false;
    };
    rest.split(',')
        .map(|part| {
            part.trim()
                .trim_start_matches(':')
                .trim_matches('"')
                .trim_matches('\'')
        })
        .any(|name| name == method)
}

fn range_uses_invalid_private_receiver(content: &str, range: Range) -> bool {
    let Some(line) = content.lines().nth(range.start.line as usize) else {
        return false;
    };
    let before = line
        .chars()
        .take(range.start.character as usize)
        .collect::<String>();
    let trimmed = before.trim_end();
    trimmed.ends_with('.')
        || trimmed.ends_with("public_send(:")
        || trimmed.ends_with("public_send(\"")
}

fn explicit_receiver_constant_parts(content: &str, range: Range) -> Option<Vec<RubyConstant>> {
    let line = content.lines().nth(range.start.line as usize)?;
    let before = line
        .chars()
        .take(range.start.character as usize)
        .collect::<String>();
    let receiver = before.trim_end().strip_suffix('.')?.trim_end();
    let receiver = receiver.strip_suffix(".new").unwrap_or(receiver);
    let token = receiver.split_whitespace().last()?;
    let mut parts = Vec::new();
    for part in token.split("::") {
        parts.push(RubyConstant::new(part).ok()?);
    }
    (!parts.is_empty()).then_some(parts)
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
    /// Find references for a given identifier.
    fn find_references_for_identifier(
        &self,
        identifier: &Identifier,
        ancestors: &[RubyConstant],
        namespace_kind: NamespaceKind,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        match identifier {
            Identifier::RubyConstant { namespace: _, iden } => {
                self.constant_reference_locations_from_analysis(iden, ancestors)
            }
            Identifier::RubyMethod {
                namespace: _,
                receiver,
                iden,
            } => self.find_method_references(
                receiver,
                iden,
                ancestors,
                namespace_kind,
                position,
                content,
            ),
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

    fn method_reference_locations_for_namespace_from_analysis(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(crate::utils::deduplicate_locations(locations_for_ranges(
            &engine,
            query.method_reference_ranges(namespace_fqn, method),
        )))
    }

    fn method_reference_locations_for_protected_receiver_from_analysis(
        &self,
        namespace_fqn: &FullyQualifiedName,
        method: &RubyMethod,
        caller_namespace_fqn: &FullyQualifiedName,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(crate::utils::deduplicate_locations(locations_for_ranges(
            &engine,
            query.method_reference_ranges_protected_receiver(
                namespace_fqn,
                method,
                caller_namespace_fqn,
            ),
        )))
    }

    fn method_reference_locations_for_constant_receiver_from_analysis(
        &self,
        receiver_path: &[RubyConstant],
        ancestors: &[RubyConstant],
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(crate::utils::deduplicate_locations(locations_for_ranges(
            &engine,
            query.method_reference_ranges_for_constant_receiver_public(
                receiver_path,
                ancestors,
                method,
            ),
        )))
    }

    fn method_reference_locations_for_current_scope_from_analysis(
        &self,
        ancestors: &[RubyConstant],
        namespace_kind: NamespaceKind,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        let namespace_fqn =
            FullyQualifiedName::namespace_with_kind(ancestors.to_vec(), namespace_kind);
        non_empty_locations(crate::utils::deduplicate_locations(locations_for_ranges(
            &engine,
            query.method_reference_ranges(&namespace_fqn, method),
        )))
    }

    fn method_reference_locations_for_super_from_analysis(
        &self,
        ancestors: &[RubyConstant],
        namespace_kind: NamespaceKind,
        method: &RubyMethod,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        let namespace_fqn =
            FullyQualifiedName::namespace_with_kind(ancestors.to_vec(), namespace_kind);
        non_empty_locations(crate::utils::deduplicate_locations(locations_for_ranges(
            &engine,
            query.super_method_reference_ranges(&namespace_fqn, method),
        )))
    }

    fn constant_reference_locations_from_analysis(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine,
            query.constant_reference_ranges(constant_path, ancestors),
        ))
    }

    fn variable_reference_locations_from_analysis(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine,
            query.variable_reference_ranges(fqn),
        ))
    }

    fn reference_locations_for_fqn_from_analysis(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<Vec<Location>> {
        let engine = self.analysis_engine()?;
        let engine = engine.read();
        let query = ruby_analysis::engine::AnalysisQuery::new(&engine);
        non_empty_locations(locations_for_ranges(
            &engine,
            query.reference_ranges_for_fqn(fqn),
        ))
    }

    fn rebuild_local_variable_scopes_for_open_document(&self) -> Option<()> {
        let doc_arc = self.doc.as_ref()?;
        let document = doc_arc.read().clone();
        let content = document.content.clone();
        let parse_result = ruby_prism::parse(content.as_bytes());
        let node = parse_result.node();
        let mut collector = FactCollector::analysis_only(
            document,
            Arc::new(NullFactCollectorExtensionHost),
            self.analysis_engine()?.clone(),
        )
        .without_analysis_method_return_resolution()
        .without_expression_receiver_inference()
        .without_diagnostics();
        collector.visit(&node);
        doc_arc.write().variable_scopes = collector.document.variable_scopes;
        Some(())
    }
}
