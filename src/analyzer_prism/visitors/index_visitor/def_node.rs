use log::warn;
use ruby_analysis_core::{TypeFact, TypeProvenance, TypeSubject};
use ruby_prism::*;

use crate::analyzer_prism::utils;
use crate::inferrer::r#type::ruby::RubyType;
use crate::inferrer::type_tracker::TypeTracker;
use crate::{
    indexer::entry::{
        entry_kind::{EntryKind, MethodParamInfo, ParamKind},
        MethodOrigin, MethodVisibility, NamespaceKind,
    },
    inferrer::return_type::infer_return_values_for_node,
};

use crate::types::scope::LVScopeKind;
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod};
use crate::yard::{YardMethodDoc, YardTypeConverter};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        let method_name_id = node.name();
        let method_name_bytes = method_name_id.as_slice();
        let method_name_str = String::from_utf8_lossy(method_name_bytes);

        // Determine namespace kind based on receiver and scope. Only support:
        //   * `def self.foo`            (receiver: self)
        //   * `def Foo.foo` inside `class Foo`  (constant read matching current class/module)
        // Otherwise skip indexing.
        let (namespace_kind, skip_method) = utils::get_method_namespace_kind(
            node.receiver(),
            &self.scope_tracker.get_ns_stack(),
            self.scope_tracker.in_singleton(),
        );

        if skip_method {
            warn!("Skipping method with unsupported receiver");
            return;
        }

        // Validate method name using centralized validation
        if !RubyMethod::is_valid_ruby_method_name(method_name_str.as_ref()) {
            warn!("Skipping invalid method name: {}", method_name_str);
            return;
        }

        let mut method = RubyMethod::new(method_name_str.as_ref()).unwrap();
        let mut actual_namespace_kind = namespace_kind;

        if method.as_str() == "initialize" {
            method = RubyMethod::new("new").unwrap();
            actual_namespace_kind = NamespaceKind::Singleton;
        }

        let name_location = node.name_loc();
        // Use full method body range (def to end) for entry.location, consistent with class/module
        let full_location = node.location();
        let lsp_location = self.document.prism_location_to_lsp_location(&full_location);
        // Convert to CompactLocation
        let file_id = self.index.lock().get_or_insert_file(&self.document.uri);
        let location =
            crate::types::compact_location::CompactLocation::new(file_id, lsp_location.range);

        // Extract YARD documentation from comments preceding the method
        let method_start_offset = node.location().start_offset();
        let yard_doc = self.extract_doc_comments(method_start_offset);

        // Extract parameter info with positions for inlay hints
        let params = self.extract_method_params(node);

        // Determine return type position (after closing paren or after method name if no params)
        let return_type_position = if let Some(rparen_loc) = node.rparen_loc() {
            Some(self.document.offset_to_position(rparen_loc.end_offset()))
        } else if let Some(params_node) = node.parameters() {
            // No parentheses, put after the last parameter
            Some(
                self.document
                    .offset_to_position(params_node.location().end_offset()),
            )
        } else {
            // No params at all, put after method name
            Some(self.document.offset_to_position(name_location.end_offset()))
        };

        let namespace_parts = self.scope_tracker.get_ns_stack();

        let fqn = FullyQualifiedName::method(namespace_parts.clone(), method.clone());

        // Owner FQN uses Namespace variant with kind to distinguish instance vs singleton methods
        let owner_fqn =
            FullyQualifiedName::namespace_with_kind(namespace_parts.clone(), actual_namespace_kind);

        // Convert YARD types to RubyType for type inference
        // Use namespace-aware conversion to resolve relative type names
        let (yard_return_type, param_types) = if let Some(ref doc) = yard_doc {
            // Get index for namespace resolution
            let index = self.index.lock();

            // Convert return type from YARD with namespace resolution
            let return_type = if !doc.returns.is_empty() {
                let all_return_types: Vec<String> =
                    doc.returns.iter().flat_map(|r| r.types.clone()).collect();
                if all_return_types.is_empty() {
                    None
                } else {
                    Some(YardTypeConverter::convert_multiple_with_namespace(
                        &all_return_types,
                        &index,
                        &namespace_parts,
                    ))
                }
            } else {
                None
            };

            // Convert parameter types with namespace resolution
            let param_types: Vec<(String, RubyType)> = doc
                .params
                .iter()
                .filter_map(|p| {
                    if p.types.is_empty() {
                        None
                    } else {
                        Some((
                            p.name.clone(),
                            YardTypeConverter::convert_multiple_with_namespace(
                                &p.types,
                                &index,
                                &namespace_parts,
                            ),
                        ))
                    }
                })
                .collect();

            drop(index); // Release the lock before continuing
            (return_type, param_types)
        } else {
            (None, Vec::new())
        };

        // Try to look up in RBS
        let rbs_return_type = {
            let class_name = namespace_parts
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let is_singleton = actual_namespace_kind == NamespaceKind::Singleton;

            crate::inferrer::rbs::get_rbs_method_return_type_as_ruby_type(
                &class_name,
                &method_name_str,
                is_singleton,
            )
        };

        if let Some(ref doc) = yard_doc {
            self.emit_yard_diagnostics(
                doc,
                &params,
                rbs_return_type.as_ref(),
                yard_return_type.as_ref(),
            );
        }

        // Prioritize: RBS > YARD > TypeTracker inference
        // Always store the inferred type - Unknown displays as "?" in hints
        // For owner_fqn in inference, use instance namespace for proper class resolution
        let instance_owner_fqn = FullyQualifiedName::namespace(namespace_parts.clone());
        let (return_type, return_type_provenance) = if let Some(return_type) = rbs_return_type {
            (Some(return_type), TypeProvenance::Rbs)
        } else if let Some(return_type) = yard_return_type {
            (Some(return_type), TypeProvenance::Yard)
        } else {
            // Infer return type from method body using TypeTracker
            let mut tracker = TypeTracker::new(
                self.document.content.as_bytes(),
                self.index.clone(),
                &self.document.uri,
            );
            // Set the current class context for self resolution
            if !namespace_parts.is_empty() {
                tracker.set_current_class(Some(instance_owner_fqn.clone()));
            }
            (Some(tracker.track_method(node)), TypeProvenance::Inferred)
        };

        if let Some(return_type) = &return_type {
            self.type_store.add(TypeFact::new(
                TypeSubject::MethodReturn(fqn.clone()),
                return_type.clone(),
                self.document.prism_location_to_text_range(&full_location),
                return_type_provenance,
            ));
        }
        for (param_name, param_type) in &param_types {
            if *param_type == RubyType::Unknown {
                continue;
            }
            self.type_store.add(TypeFact::new(
                TypeSubject::Parameter {
                    method: fqn.clone(),
                    name: param_name.clone(),
                },
                param_type.clone(),
                self.document.prism_location_to_text_range(&full_location),
                TypeProvenance::Yard,
            ));
        }

        let entry = {
            let mut index = self.index.lock();
            crate::indexer::entry::EntryBuilder::new()
                .fqn(fqn)
                .compact_location(location)
                .kind(EntryKind::new_method(
                    method.clone(),
                    params,
                    owner_fqn.clone(),
                    MethodVisibility::Public,
                    MethodOrigin::Direct,
                    None,
                    yard_doc,
                    return_type_position,
                    return_type.clone(),
                    param_types,
                ))
                .build(&mut index)
                .unwrap()
        };

        self.add_entry(entry);

        // Validate return type if declared
        if let Some(expected_type) = &return_type {
            let return_values = {
                let mut index = self.index.lock();
                let file_contents_map = std::collections::HashMap::from([(
                    &self.document.uri,
                    self.document.content.as_bytes(),
                )]);
                infer_return_values_for_node(
                    &mut index,
                    self.document.content.as_bytes(),
                    node,
                    Some(instance_owner_fqn.clone()),
                    Some(&file_contents_map),
                )
            };

            for (inferred_ty, start, end) in return_values {
                // If inferred type is Unknown, we skip partial validation to avoid false positives
                if inferred_ty == RubyType::Unknown {
                    continue;
                }

                // If inferred type is subtype of expected, it's fine.
                if !inferred_ty.is_subtype_of(expected_type) {
                    let start_pos = self.document.offset_to_position(start);
                    let end_pos = self.document.offset_to_position(end);
                    let range = tower_lsp::lsp_types::Range::new(start_pos, end_pos);

                    self.push_diagnostic(tower_lsp::lsp_types::Diagnostic {
                        range,
                        severity: Some(tower_lsp::lsp_types::DiagnosticSeverity::WARNING),
                        message: format!(
                            "Expected return type {}, but found {}",
                            expected_type, inferred_ty
                        ),
                        source: Some("ruby-fast-lsp".to_string()),
                        ..Default::default()
                    });
                }
            }
        }

        let body_loc = utils::get_body_location(
            node.body().map(|b| b.location()),
            &node.location(),
            &self.document,
        );

        let scope_kind = match namespace_kind {
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
        };
        self.scope_tracker.push_scope_kind(scope_kind);

        self.document.variable_scopes_mut().enter_scope(
            scope_kind,
            body_loc.range,
            Some(method_name_str.to_string()),
        );
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.scope_tracker.pop_scope_kind();
        self.document.variable_scopes_mut().exit_scope();
    }

    /// Extract parameter information from a DefNode for inlay hints
    fn extract_method_params(&self, node: &DefNode) -> Vec<MethodParamInfo> {
        let mut params = Vec::new();

        let Some(params_node) = node.parameters() else {
            return params;
        };

        // Process required parameters
        for required in params_node.requireds().iter() {
            if let Some(param) = required.as_required_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                let end_pos = self
                    .document
                    .offset_to_position(param.location().end_offset());
                params.push(MethodParamInfo::new(
                    param_name,
                    end_pos,
                    ParamKind::Required,
                ));
            }
        }

        // Process optional parameters (with default values)
        for optional in params_node.optionals().iter() {
            if let Some(param) = optional.as_optional_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                // For optional params, position after the name, not after the default value
                let end_pos = self
                    .document
                    .offset_to_position(param.name_loc().end_offset());
                params.push(MethodParamInfo::new(
                    param_name,
                    end_pos,
                    ParamKind::Optional,
                ));
            }
        }

        // Process rest parameter (*args)
        if let Some(rest) = params_node.rest() {
            if let Some(param) = rest.as_rest_parameter_node() {
                if let Some(name) = param.name() {
                    let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                    if let Some(name_loc) = param.name_loc() {
                        let end_pos = self.document.offset_to_position(name_loc.end_offset());
                        params.push(MethodParamInfo::new(param_name, end_pos, ParamKind::Rest));
                    }
                }
            }
        }

        // Process keyword parameters (name: or name: default)
        // These already have a colon in the syntax, so we don't add another
        for keyword in params_node.keywords().iter() {
            if let Some(param) = keyword.as_required_keyword_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                // Remove trailing colon from keyword param name for matching with YARD
                let param_name = param_name.trim_end_matches(':').to_string();
                let end_pos = self
                    .document
                    .offset_to_position(param.name_loc().end_offset());
                params.push(MethodParamInfo::new(
                    param_name,
                    end_pos,
                    ParamKind::RequiredKeyword,
                ));
            } else if let Some(param) = keyword.as_optional_keyword_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                // Remove trailing colon from keyword param name for matching with YARD
                let param_name = param_name.trim_end_matches(':').to_string();
                let end_pos = self
                    .document
                    .offset_to_position(param.name_loc().end_offset());
                params.push(MethodParamInfo::new(
                    param_name,
                    end_pos,
                    ParamKind::OptionalKeyword,
                ));
            }
        }

        // Process keyword rest parameter (**kwargs)
        if let Some(kwrest) = params_node.keyword_rest() {
            if let Some(param) = kwrest.as_keyword_rest_parameter_node() {
                if let Some(name) = param.name() {
                    let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                    if let Some(name_loc) = param.name_loc() {
                        let end_pos = self.document.offset_to_position(name_loc.end_offset());
                        params.push(MethodParamInfo::new(
                            param_name,
                            end_pos,
                            ParamKind::KeywordRest,
                        ));
                    }
                }
            }
        }

        // Process block parameter (&block)
        if let Some(block) = params_node.block() {
            if let Some(name) = block.name() {
                let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                if let Some(name_loc) = block.name_loc() {
                    let end_pos = self.document.offset_to_position(name_loc.end_offset());
                    params.push(MethodParamInfo::new(param_name, end_pos, ParamKind::Block));
                }
            }
        }

        params
    }

    fn emit_yard_diagnostics(
        &mut self,
        yard_doc: &YardMethodDoc,
        method_params: &[MethodParamInfo],
        rbs_return_type: Option<&RubyType>,
        yard_return_type: Option<&RubyType>,
    ) {
        let actual_param_names: Vec<&str> = method_params.iter().map(|p| p.name.as_str()).collect();

        for (yard_param, range) in yard_doc.find_unmatched_params(&actual_param_names) {
            self.push_diagnostic(tower_lsp::lsp_types::Diagnostic {
                range,
                severity: Some(tower_lsp::lsp_types::DiagnosticSeverity::WARNING),
                code: Some(tower_lsp::lsp_types::NumberOrString::String(
                    "yard-unknown-param".to_string(),
                )),
                source: Some("ruby-fast-lsp".to_string()),
                message: format!(
                    "YARD @param '{}' does not match any method parameter",
                    yard_param.name
                ),
                ..Default::default()
            });
        }

        let unknown_type_diagnostics = {
            let index = self.index.lock();
            let mut diagnostics = Vec::new();
            for param in &yard_doc.params {
                let result =
                    YardTypeConverter::convert_multiple_with_validation(&param.types, Some(&index));
                for unresolved in result.unresolved_types {
                    let diagnostic_range = param.types_range.or(param.range);
                    if let Some(range) = diagnostic_range {
                        diagnostics.push(tower_lsp::lsp_types::Diagnostic {
                            range,
                            severity: Some(tower_lsp::lsp_types::DiagnosticSeverity::ERROR),
                            code: Some(tower_lsp::lsp_types::NumberOrString::String(
                                "yard-unknown-type".to_string(),
                            )),
                            source: Some("ruby-fast-lsp".to_string()),
                            message: format!(
                                "Unknown type '{}' in YARD @param documentation",
                                unresolved.type_name
                            ),
                            ..Default::default()
                        });
                    }
                }
            }
            diagnostics
        };
        self.diagnostics.extend(unknown_type_diagnostics);

        let Some(rbs_type) = rbs_return_type else {
            return;
        };
        let Some(yard_type) = yard_return_type else {
            return;
        };
        if *rbs_type == RubyType::Unknown || yard_type == rbs_type {
            return;
        }

        let Some(first_return) = yard_doc.returns.first() else {
            return;
        };
        let Some(range) = first_return.types_range.or(first_return.range) else {
            return;
        };

        self.push_diagnostic(tower_lsp::lsp_types::Diagnostic {
            range,
            severity: Some(tower_lsp::lsp_types::DiagnosticSeverity::WARNING),
            code: Some(tower_lsp::lsp_types::NumberOrString::String(
                "yard-rbs-mismatch".to_string(),
            )),
            source: Some("ruby-fast-lsp".to_string()),
            message: format!(
                "YARD return type '{}' conflicts with RBS type '{}'",
                yard_type, rbs_type
            ),
            ..Default::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;
    use crate::indexer::index_ref::{Index, Unlocked};
    use parking_lot::RwLock;
    use ruby_analysis_core::{SourceFileId, TypeResolution};
    use ruby_prism::Visit;
    use std::sync::Arc;
    use tower_lsp::lsp_types::Url;

    fn create_test_index() -> Index<Unlocked> {
        Index::new(Arc::new(RwLock::new(RubyIndex::new())))
    }

    #[test]
    fn def_node_emits_method_return_type_fact() {
        let content = "def name\n  'Ada'\nend";
        let uri = Url::parse("file:///test.rb").expect("test URI must parse");
        let index = create_test_index();
        let document =
            crate::types::ruby_document::RubyDocument::new(uri.clone(), content.to_string(), 1);
        let mut visitor = IndexVisitor::new(index, document);
        let parse_result = ruby_prism::parse(content.as_bytes());

        visitor.visit(&parse_result.node());

        let method = RubyMethod::new("name").expect("method name must be valid");
        let subject = TypeSubject::MethodReturn(FullyQualifiedName::method(Vec::new(), method));
        match visitor.type_store.type_at(&subject, SourceFileId(0), 8) {
            TypeResolution::Resolved(fact) => {
                assert_eq!(fact.ruby_type, RubyType::string());
                assert_eq!(fact.provenance, TypeProvenance::Inferred);
            }
            other => panic!("expected method return type fact, got {other:?}"),
        }
    }
}
