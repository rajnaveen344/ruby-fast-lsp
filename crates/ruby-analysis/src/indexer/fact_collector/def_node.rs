use crate::core::{
    ConstantTypeEquation, ConstantTypeTarget, FullyQualifiedName, GraphEdgeKind, GraphNodeKind,
    MethodAvailability, MethodParamFact, MethodParamKind, MethodReturnEquation, NamespaceKind,
    RubyMethod, TextRange, TypeFact, TypeProvenance, TypeSubject, UnknownReason,
};
use crate::{get_method_namespace_kind, LocalScopeKind as LVScopeKind};
use log::warn;
use ruby_prism::*;

use crate::inference::r#type::literal::LiteralAnalyzer;
use crate::inference::type_tracker::{LocalReadType, TypeTracker};
use crate::inference::RubyType;

use crate::yard::{YardMethodDoc, YardParser, YardTypeConverter};

use super::FactCollector;

#[derive(Debug, Clone, PartialEq)]
struct MethodParamInfo {
    name: String,
    kind: MethodParamKind,
    range: TextRange,
}

impl MethodParamInfo {
    fn new(name: String, kind: MethodParamKind, range: TextRange) -> Self {
        Self { name, kind, range }
    }
}

impl FactCollector {
    pub fn process_def_node_entry(&mut self, node: &DefNode) -> bool {
        let method_name_id = node.name();
        let method_name_bytes = method_name_id.as_slice();
        let method_name_str = String::from_utf8_lossy(method_name_bytes);

        // Determine namespace kind based on receiver and scope. Only support:
        //   * `def self.foo`            (receiver: self)
        //   * `def Foo.foo` inside `class Foo`  (constant read matching current class/module)
        // Otherwise skip indexing.
        let (definition_namespace, namespace_kind, skip_method) = match node.receiver() {
            None => {
                let (namespace, kind) = self.scope_tracker.method_definition_context();
                (namespace, kind, false)
            }
            Some(receiver) if receiver.as_self_node().is_some() => {
                let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
                (
                    namespace,
                    NamespaceKind::Singleton,
                    receiver_kind != NamespaceKind::Singleton,
                )
            }
            Some(_) => {
                let namespace = self.scope_tracker.get_ns_stack();
                let (kind, skip) = get_method_namespace_kind(
                    node.receiver(),
                    &namespace,
                    self.scope_tracker.in_singleton(),
                );
                (namespace, kind, skip)
            }
        };

        if skip_method {
            warn!("Skipping method with unsupported receiver");
            return false;
        }

        // Validate method name using centralized validation
        if !RubyMethod::is_valid_ruby_method_name(method_name_str.as_ref()) {
            warn!("Skipping invalid method name: {}", method_name_str);
            return false;
        }

        let mut method = RubyMethod::new(method_name_str.as_ref()).unwrap();
        let mut actual_namespace_kind = namespace_kind;
        let definition_fqn = FullyQualifiedName::namespace(definition_namespace.clone());
        let direct_definition_kinds = self
            .direct_facts
            .graph_nodes
            .iter()
            .filter(|fact| fact.fqn == definition_fqn)
            .map(|fact| fact.kind)
            .collect::<Vec<_>>();
        let definition_is_proven_class = if direct_definition_kinds.is_empty() {
            crate::engine::AnalysisQuery::new(&self.analysis_engine.read())
                .namespace_node_kind(&definition_fqn)
                == Some(GraphNodeKind::Class)
        } else {
            direct_definition_kinds
                .iter()
                .all(|kind| *kind == GraphNodeKind::Class)
        };
        let is_constructor = method.as_str() == "initialize"
            && node.receiver().is_none()
            && namespace_kind == NamespaceKind::Instance
            && definition_is_proven_class;

        if is_constructor {
            method = RubyMethod::new("new").unwrap();
            actual_namespace_kind = NamespaceKind::Singleton;
        }

        let name_location = node.name_loc();
        // Use full method body range (def to end) for entry.location, consistent with class/module
        let full_location = node.location();

        // Extract YARD documentation from comments preceding the method
        let method_start_offset = node.location().start_offset();
        let method_start_line = self.document.offset_to_position(method_start_offset).line;
        let yard_doc = YardParser::extract_from_source_at_line(
            &self.document.content,
            method_start_offset,
            method_start_line,
        );

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

        let namespace_parts = definition_namespace;

        let fqn = FullyQualifiedName::method(namespace_parts.clone(), method);
        self.scope_tracker.push_method_fqn(Some(fqn.clone()));

        // Owner FQN uses Namespace variant with kind to distinguish instance vs singleton methods
        let _owner_fqn =
            FullyQualifiedName::namespace_with_kind(namespace_parts.clone(), actual_namespace_kind);

        let direct_params = params
            .iter()
            .map(|param| {
                let yard_param = yard_doc
                    .as_ref()
                    .and_then(|doc| doc.find_param(&param.name));
                MethodParamFact::new(param.name.clone(), param.kind).with_signature_metadata(
                    yard_param.and_then(|param| param.format_type()),
                    yard_param.and_then(|param| param.description.clone()),
                )
            })
            .collect();
        let availability = match yard_doc.as_ref() {
            Some(doc) => match (&doc.unavailable, &doc.absent) {
                (Some(reason), None) => MethodAvailability::Unavailable {
                    reason: reason.clone(),
                },
                (None, Some(reason)) => MethodAvailability::Absent {
                    reason: reason.clone(),
                },
                (None, None) => MethodAvailability::Available,
                (Some(_), Some(_)) => panic!(
                    "INVARIANT VIOLATED: method `{method}` is marked both @unavailable and @absent. \
                     This is a bug because a runtime API cannot simultaneously exist-but-fail and not exist. \
                     Fix: retain exactly one availability annotation in the owning stub."
                ),
            },
            None => MethodAvailability::Available,
        };
        self.direct_push_method_fact_with_signature_name_range_and_availability(
            namespace_parts.clone(),
            actual_namespace_kind,
            method,
            self.direct_range(&full_location),
            self.direct_range(&name_location),
            direct_params,
            yard_doc.as_ref().and_then(|doc| doc.description.clone()),
            yard_doc
                .as_ref()
                .and_then(YardMethodDoc::format_return_type),
            availability.clone(),
        );
        if node.receiver().is_none()
            && actual_namespace_kind == NamespaceKind::Instance
            && self.scope_tracker.module_function_mode_enabled()
        {
            self.direct_push_method_fact_with_signature_name_range_and_availability(
                namespace_parts.clone(),
                NamespaceKind::Singleton,
                method,
                self.direct_range(&full_location),
                self.direct_range(&name_location),
                params
                    .iter()
                    .map(|param| {
                        let yard_param = yard_doc
                            .as_ref()
                            .and_then(|doc| doc.find_param(&param.name));
                        MethodParamFact::new(param.name.clone(), param.kind)
                            .with_signature_metadata(
                                yard_param.and_then(|param| param.format_type()),
                                yard_param.and_then(|param| param.description.clone()),
                            )
                    })
                    .collect(),
                yard_doc.as_ref().and_then(|doc| doc.description.clone()),
                yard_doc
                    .as_ref()
                    .and_then(YardMethodDoc::format_return_type),
                availability,
            );
        }

        let body_range = self.body_text_range(node.body().map(|b| b.location()), &node.location());

        let scope_kind = match namespace_kind {
            NamespaceKind::Singleton => LVScopeKind::ClassMethod,
            NamespaceKind::Instance => LVScopeKind::InstanceMethod,
        };
        self.scope_tracker.push_scope_kind(scope_kind);
        self.scope_tracker.push_method_execution_context(
            namespace_parts.clone(),
            namespace_kind,
            namespace_parts.clone(),
            namespace_kind,
        );

        self.document.variable_scopes_mut().enter_scope(
            scope_kind,
            body_range,
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
            let param_types = params
                .iter()
                .filter_map(|param| {
                    let yard_param = doc.find_param(&param.name)?;
                    if yard_param.types.is_empty() {
                        None
                    } else {
                        Some((
                            param.name.clone(),
                            YardTypeConverter::convert_multiple(&yard_param.types),
                            param.range,
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

            crate::inference::rbs::get_rbs_method_return_type_as_ruby_type(
                &class_name,
                &method_name_str,
                is_singleton,
            )
        };
        let declared_return_type = rbs_return_type
            .as_ref()
            .or(yard_return_type.as_ref())
            .cloned();

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
        let (return_type, return_type_provenance, return_equation) = if is_constructor {
            let return_type =
                RubyType::Class(FullyQualifiedName::constant(namespace_parts.clone()));
            (
                Some(return_type.clone()),
                TypeProvenance::Inferred,
                Some(MethodReturnEquation::from_ruby_type(
                    fqn.clone(),
                    return_type,
                    UnknownReason::UnresolvedMethodReturn,
                )),
            )
        } else if let Some(return_type) = rbs_return_type {
            (
                Some(return_type.clone()),
                TypeProvenance::Rbs,
                Some(MethodReturnEquation::from_ruby_type(
                    fqn.clone(),
                    return_type,
                    UnknownReason::UnresolvedMethodReturn,
                )),
            )
        } else if let Some(return_type) = yard_return_type {
            (
                Some(return_type.clone()),
                TypeProvenance::Yard,
                Some(MethodReturnEquation::from_ruby_type(
                    fqn.clone(),
                    return_type,
                    UnknownReason::UnresolvedMethodReturn,
                )),
            )
        } else if !self.resolve_analysis_method_returns {
            (None, TypeProvenance::Inferred, None)
        } else {
            // Collect a compact return equation from the existing traversal.
            // The program-level solver resolves same-file SCCs after every
            // method equation is available, without another Prism walk.
            let mut tracker = TypeTracker::new(self.document.content.as_bytes());
            tracker = tracker.with_analysis_engine(self.analysis_engine.clone());
            tracker = tracker.with_analysis_query_cache(self.analysis_query_cache.clone());
            tracker = tracker.with_local_method_returns(self.local_method_returns_for_tracker());
            tracker = tracker.with_local_public_method_candidates(
                self.local_public_method_candidates_for_tracker(),
            );
            tracker = tracker.with_local_superclasses(self.local_superclasses_for_tracker());
            tracker = tracker.with_yield_param_types(self.yield_param_types_by_method.clone());
            tracker = tracker.with_parameter_types(
                param_types
                    .iter()
                    .map(|(name, ruby_type, _range)| (name.clone(), ruby_type.clone()))
                    .collect(),
            );
            if self.record_local_read_unknown_reasons {
                tracker = tracker.with_local_read_types();
            }
            // Set the current class context for self resolution
            if !namespace_parts.is_empty() {
                tracker.set_current_class(Some(instance_owner_fqn.clone()));
            }
            let equation = tracker.track_method_equation(
                node,
                fqn.clone(),
                self.local_method_candidates_for_tracker(),
            );
            if self.record_local_read_unknown_reasons {
                self.install_local_read_types(tracker.take_local_read_types());
            }
            let immediate = equation.immediate_outcome();
            (
                Some(immediate.into_ruby_type()),
                TypeProvenance::Inferred,
                Some(equation),
            )
        };

        if let Some(return_equation) = return_equation {
            self.method_return_equations
                .entry(namespace_parts.clone())
                .or_default()
                .push(return_equation);
        }

        if let Some(return_type) = &return_type {
            self.type_store.add(TypeFact::new(
                TypeSubject::MethodReturn(fqn.clone()),
                return_type.clone(),
                self.document.prism_location_to_text_range(&full_location),
                return_type_provenance,
            ));
        }
        for (param_name, param_type, param_range) in &param_types {
            if *param_type == RubyType::Unknown {
                continue;
            }
            self.type_store.add(TypeFact::new(
                TypeSubject::Parameter {
                    method: fqn.clone(),
                    name: param_name.clone(),
                },
                param_type.clone(),
                *param_range,
                TypeProvenance::Yard,
            ));
        }

        if !is_constructor {
            self.validate_declared_return_type(
                node,
                declared_return_type.as_ref(),
                &instance_owner_fqn,
            );
        }
        true
    }

    fn install_local_read_types(&mut self, reads: Vec<LocalReadType>) {
        if reads.is_empty() {
            return;
        }
        let scope_id = self.document.variable_scopes().current_scope().expect(
            "INVARIANT VIOLATED: TypeTracker local-read results have no active method scope. This is a bug because process_def_node_entry enters the scope before collecting its return equation. Fix: install flow evidence before exiting the definition.",
        );
        let mut installed_reads = Vec::with_capacity(reads.len());
        for read in reads {
            let range = self.text_range_from_offsets(read.start_offset, read.end_offset);
            if !read.constant_dependencies.is_empty() {
                self.constant_type_equations
                    .push(ConstantTypeEquation::from_dependencies(
                        ConstantTypeTarget::LocalRead(range),
                        read.constant_dependencies,
                    ));
            }
            installed_reads.push((read.name, range, read.ruby_type));
        }
        self.document
            .variable_scopes_mut()
            .install_flow_read_types(scope_id, installed_reads);
    }

    fn validate_declared_return_type(
        &mut self,
        node: &DefNode,
        return_type: Option<&RubyType>,
        _instance_owner_fqn: &FullyQualifiedName,
    ) {
        let Some(expected_type) = return_type else {
            return;
        };
        let return_values = infer_return_values_for_declared_type_check(node);

        for (inferred_ty, start, end) in return_values {
            if RubyType::contains_unknown(expected_type) || RubyType::contains_unknown(&inferred_ty)
            {
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
        self.scope_tracker.pop_execution_context();
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
                params.push(MethodParamInfo::new(
                    param_name,
                    MethodParamKind::Required,
                    self.direct_range(&param.location()),
                ));
            }
        }

        // Process optional parameters (with default values)
        for optional in params_node.optionals().iter() {
            if let Some(param) = optional.as_optional_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                // For optional params, position after the name, not after the default value
                params.push(MethodParamInfo::new(
                    param_name,
                    MethodParamKind::Optional,
                    self.direct_range(&param.name_loc()),
                ));
            }
        }

        // Process rest parameter (*args)
        if let Some(rest) = params_node.rest() {
            if let Some(param) = rest.as_rest_parameter_node() {
                if let Some(name) = param.name() {
                    let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                    params.push(MethodParamInfo::new(
                        param_name,
                        MethodParamKind::Rest,
                        self.direct_range(&param.location()),
                    ));
                } else {
                    params.push(MethodParamInfo::new(
                        "*".to_string(),
                        MethodParamKind::AnonymousRest,
                        self.direct_range(&param.location()),
                    ));
                }
            } else if let Some(param) = rest.as_forwarding_parameter_node() {
                params.push(MethodParamInfo::new(
                    "...".to_string(),
                    MethodParamKind::Forwarding,
                    self.direct_range(&param.location()),
                ));
            }
        }

        // Ruby post parameters are required positional parameters declared
        // after optional/rest parameters (`def render(prefix = nil, body)`).
        // Omitting them makes the stored signature accept too few arguments
        // and reject valid calls, so retain them in their actual call order.
        for post in params_node.posts().iter() {
            if let Some(param) = post.as_required_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                params.push(MethodParamInfo::new(
                    param_name,
                    MethodParamKind::Required,
                    self.direct_range(&param.location()),
                ));
            }
        }

        // Process keyword parameters (name: or name: default)
        // These already have a colon in the syntax, so we don't add another
        for keyword in params_node.keywords().iter() {
            if let Some(param) = keyword.as_required_keyword_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                // Remove trailing colon from keyword param name for matching with YARD
                let param_name = param_name.trim_end_matches(':').to_string();
                params.push(MethodParamInfo::new(
                    param_name,
                    MethodParamKind::RequiredKeyword,
                    self.direct_range(&param.name_loc()),
                ));
            } else if let Some(param) = keyword.as_optional_keyword_parameter_node() {
                let param_name = String::from_utf8_lossy(param.name().as_slice()).to_string();
                // Remove trailing colon from keyword param name for matching with YARD
                let param_name = param_name.trim_end_matches(':').to_string();
                params.push(MethodParamInfo::new(
                    param_name,
                    MethodParamKind::OptionalKeyword,
                    self.direct_range(&param.name_loc()),
                ));
            }
        }

        // Process keyword rest parameter (**kwargs)
        if let Some(kwrest) = params_node.keyword_rest() {
            if let Some(param) = kwrest.as_keyword_rest_parameter_node() {
                if let Some(name) = param.name() {
                    let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                    params.push(MethodParamInfo::new(
                        param_name,
                        MethodParamKind::KeywordRest,
                        self.direct_range(&param.location()),
                    ));
                } else {
                    params.push(MethodParamInfo::new(
                        "**".to_string(),
                        MethodParamKind::AnonymousKeywordRest,
                        self.direct_range(&param.location()),
                    ));
                }
            } else if let Some(param) = kwrest.as_forwarding_parameter_node() {
                params.push(MethodParamInfo::new(
                    "...".to_string(),
                    MethodParamKind::Forwarding,
                    self.direct_range(&param.location()),
                ));
            }
        }

        // Process block parameter (&block)
        if let Some(block) = params_node.block() {
            if let Some(name) = block.name() {
                let param_name = String::from_utf8_lossy(name.as_slice()).to_string();
                params.push(MethodParamInfo::new(
                    param_name,
                    MethodParamKind::Block,
                    self.direct_range(&block.location()),
                ));
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
                self.text_range_from_source_range(range, "YARD unknown param"),
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
            self.text_range_from_source_range(range, "YARD RBS mismatch"),
            "yard-rbs-mismatch",
            format!(
                "YARD return type '{}' conflicts with RBS type '{}'",
                yard_type, rbs_type
            ),
        );
    }

    fn local_method_returns_for_tracker(
        &self,
    ) -> std::collections::HashMap<FullyQualifiedName, RubyType> {
        self.type_store
            .known_method_return_types()
            .map(|(fqn, ruby_type)| (fqn.clone(), ruby_type.clone()))
            .collect()
    }

    fn local_method_candidates_for_tracker(
        &self,
    ) -> std::sync::Arc<std::collections::HashSet<FullyQualifiedName>> {
        std::sync::Arc::clone(&self.local_method_candidates)
    }

    fn local_public_method_candidates_for_tracker(
        &self,
    ) -> std::sync::Arc<std::collections::HashSet<FullyQualifiedName>> {
        std::sync::Arc::clone(&self.local_public_method_candidates)
    }

    fn local_superclasses_for_tracker(
        &self,
    ) -> std::collections::HashMap<FullyQualifiedName, FullyQualifiedName> {
        self.direct_facts
            .graph_edges
            .iter()
            .filter(|edge| edge.kind == GraphEdgeKind::Superclass)
            .map(|edge| (edge.source.clone(), edge.target.clone()))
            .collect()
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
