use log::warn;
use ruby_analysis_core::{
    FullyQualifiedName, MethodParamKind, NamespaceKind, RubyMethod, TypeFact, TypeProvenance,
    TypeSubject,
};
use ruby_analysis_indexer::LocalScopeKind as LVScopeKind;
use ruby_prism::*;
use tower_lsp::lsp_types::Position;

use crate::analyzer_prism::utils;
use ruby_analysis_inference::r#type::literal::LiteralAnalyzer;
use ruby_analysis_inference::type_tracker::TypeTracker;
use ruby_analysis_inference::RubyType;

use crate::yard::{YardMethodDoc, YardTypeConverter};

use super::FactCollector;

#[derive(Debug, Clone, PartialEq)]
struct MethodParamInfo {
    name: String,
    end_position: Position,
    kind: MethodParamKind,
}

impl MethodParamInfo {
    fn new(name: String, end_position: Position, kind: MethodParamKind) -> Self {
        Self {
            name,
            end_position,
            kind,
        }
    }
}

impl FactCollector {
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

        // Extract YARD documentation from comments preceding the method
        let method_start_offset = node.location().start_offset();
        let yard_doc = self.extract_doc_comments(method_start_offset);

        // Extract parameter info with positions for inlay hints
        let params = self.extract_method_params(node);

        // Determine return type position (after closing paren or after method name if no params)
        let _return_type_position = if let Some(rparen_loc) = node.rparen_loc() {
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
        self.scope_tracker.push_method_fqn(Some(fqn.clone()));

        // Owner FQN uses Namespace variant with kind to distinguish instance vs singleton methods
        let _owner_fqn =
            FullyQualifiedName::namespace_with_kind(namespace_parts.clone(), actual_namespace_kind);

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

        // Convert YARD types to RubyType for type inference
        // Use namespace-aware conversion to resolve relative type names
        let (yard_return_type, param_types) = if let Some(ref doc) = yard_doc {
            let return_type = if !doc.returns.is_empty() {
                let all_return_types: Vec<String> =
                    doc.returns.iter().flat_map(|r| r.types.clone()).collect();
                if all_return_types.is_empty() {
                    None
                } else {
                    Some(YardTypeConverter::convert_multiple(&all_return_types))
                }
            } else {
                None
            };
            let param_types = doc
                .params
                .iter()
                .filter_map(|p| {
                    if p.types.is_empty() {
                        None
                    } else {
                        Some((
                            p.name.clone(),
                            YardTypeConverter::convert_multiple(&p.types),
                        ))
                    }
                })
                .collect();
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

            ruby_analysis_inference::rbs::get_rbs_method_return_type_as_ruby_type(
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
            let mut tracker = TypeTracker::new(self.document.content.as_bytes());
            tracker = tracker.with_analysis_engine(self.analysis_engine.clone());
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

        self.validate_declared_return_type(node, &return_type, &instance_owner_fqn);
    }

    fn validate_declared_return_type(
        &mut self,
        node: &DefNode,
        return_type: &Option<RubyType>,
        _instance_owner_fqn: &FullyQualifiedName,
    ) {
        let Some(expected_type) = return_type else {
            return;
        };
        let return_values = infer_return_values_for_declared_type_check(node);

        for (inferred_ty, start, end) in return_values {
            if inferred_ty == RubyType::Unknown {
                continue;
            }

            if !inferred_ty.is_subtype_of(expected_type) {
                let range = self.text_range_from_offsets(start, end);
                self.push_warning_diagnostic(
                    range,
                    "declared-return-type-mismatch",
                    format!(
                        "Expected return type {}, but found {}",
                        expected_type, inferred_ty
                    ),
                );
            }
        }
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.scope_tracker.pop_method_fqn();
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
                    MethodParamKind::Required,
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
                    MethodParamKind::Optional,
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
                        params.push(MethodParamInfo::new(
                            param_name,
                            end_pos,
                            MethodParamKind::Rest,
                        ));
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
                    MethodParamKind::RequiredKeyword,
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
                    MethodParamKind::OptionalKeyword,
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
                            MethodParamKind::KeywordRest,
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
                    params.push(MethodParamInfo::new(
                        param_name,
                        end_pos,
                        MethodParamKind::Block,
                    ));
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
            self.push_warning_diagnostic(
                self.text_range_from_lsp_range(range, "YARD unknown param"),
                "yard-unknown-param",
                format!(
                    "YARD @param '{}' does not match any method parameter",
                    yard_param.name
                ),
            );
        }

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

        self.push_warning_diagnostic(
            self.text_range_from_lsp_range(range, "YARD RBS mismatch"),
            "yard-rbs-mismatch",
            format!(
                "YARD return type '{}' conflicts with RBS type '{}'",
                yard_type, rbs_type
            ),
        );
    }
}

fn infer_return_values_for_declared_type_check(
    def_node: &DefNode,
) -> Vec<(RubyType, usize, usize)> {
    let Some(body) = def_node.body() else {
        return Vec::new();
    };
    let analyzer = LiteralAnalyzer::new();

    if let Some(statements) = body.as_statements_node() {
        let Some(last) = statements.body().iter().last() else {
            let loc = def_node.name_loc();
            return vec![(RubyType::nil_class(), loc.start_offset(), loc.end_offset())];
        };
        return infer_return_value_from_node(&analyzer, &last);
    }

    infer_return_value_from_node(&analyzer, &body)
}

fn infer_return_value_from_node(
    analyzer: &LiteralAnalyzer,
    node: &Node,
) -> Vec<(RubyType, usize, usize)> {
    if let Some(return_node) = node.as_return_node() {
        let loc = return_node.location();
        let return_type = return_node
            .arguments()
            .and_then(|args| {
                let args = args.arguments().iter().collect::<Vec<_>>();
                match args.len() {
                    0 => Some(RubyType::nil_class()),
                    1 => analyzer.analyze_literal(&args[0]),
                    _ => Some(RubyType::Array(
                        args.into_iter()
                            .map(|arg| analyzer.analyze_literal(&arg).unwrap_or(RubyType::Unknown))
                            .collect(),
                    )),
                }
            })
            .unwrap_or_else(RubyType::nil_class);
        return vec![(return_type, loc.start_offset(), loc.end_offset())];
    }

    if let Some(return_type) = analyzer.analyze_literal(node) {
        let loc = node.location();
        return vec![(return_type, loc.start_offset(), loc.end_offset())];
    }

    Vec::new()
}
