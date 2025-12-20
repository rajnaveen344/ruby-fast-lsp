use log::warn;
use ruby_prism::*;

use crate::indexer::entry::{
    entry_kind::{EntryKind, MethodParamInfo, ParamKind},
    MethodKind, MethodOrigin, MethodVisibility,
};
use crate::type_inference::ruby_type::RubyType;

use crate::types::scope::{LVScope, LVScopeKind};
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod};
use crate::yard::YardTypeConverter;

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        let method_name_id = node.name();
        let method_name_bytes = method_name_id.as_slice();
        let method_name_str = String::from_utf8_lossy(method_name_bytes);

        // Determine method kind based on receiver and scope. Only support:
        //   * `def self.foo`            (receiver: self)
        //   * `def Foo.foo` inside `class Foo`  (constant read matching current class/module)
        // Otherwise skip indexing.
        let mut method_kind = MethodKind::Instance;
        let mut skip_method = false;

        if let Some(receiver) = node.receiver() {
            if receiver.as_self_node().is_some() {
                method_kind = MethodKind::Class;
            } else if let Some(read_node) = receiver.as_constant_read_node() {
                let recv_name = String::from_utf8_lossy(read_node.name().as_slice()).to_string();
                // Current namespace last element (if any) should match receiver constant
                let ns_stack = self.scope_tracker.get_ns_stack();
                let last_ns = ns_stack.last();
                if let Some(last) = last_ns {
                    if last.to_string() == recv_name {
                        method_kind = MethodKind::Class;
                    } else {
                        skip_method = true;
                    }
                } else {
                    // No enclosing namespace -> unsupported
                    skip_method = true;
                }
            } else {
                // ConstantPathNode or other receiver types not supported
                skip_method = true;
            }
        } else if self.scope_tracker.in_singleton() {
            method_kind = MethodKind::Class;
        }

        if skip_method {
            warn!("Skipping method with unsupported receiver");
            return;
        }

        // Validate method name using centralized validation
        if !RubyMethod::is_valid_ruby_method_name(method_name_str.as_ref()) {
            warn!("Skipping invalid method name: {}", method_name_str);
            return;
        }

        let mut method = RubyMethod::new(method_name_str.as_ref(), method_kind).unwrap();

        if method.get_name() == "initialize" {
            method = RubyMethod::new("new", MethodKind::Class).unwrap();
        }

        let name_location = node.name_loc();
        let lsp_location = self.document.prism_location_to_lsp_location(&name_location);
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

        let owner_fqn = FullyQualifiedName::Constant(namespace_parts.clone());

        // Convert YARD types to RubyType for type inference
        let (yard_return_type, param_types) = if let Some(ref doc) = yard_doc {
            // Convert return type from YARD
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

            // Convert parameter types
            let param_types: Vec<(String, RubyType)> = doc
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
            let is_singleton = method_kind == MethodKind::Class;

            crate::type_inference::rbs_index::get_rbs_method_return_type_as_ruby_type(
                &class_name,
                &method_name_str,
                is_singleton,
            )
        };

        // Prioritize RBS over YARD
        let return_type = rbs_return_type.or(yard_return_type);

        let entry = {
            let mut index = self.index.lock();
            crate::indexer::entry::EntryBuilder::new()
                .fqn(fqn)
                .compact_location(location)
                .kind(EntryKind::new_method(
                    method.clone(),
                    params,
                    owner_fqn,
                    MethodVisibility::Public,
                    MethodOrigin::Direct,
                    None,
                    yard_doc,
                    return_type_position,
                    return_type,
                    param_types,
                ))
                .build(&mut index)
                .unwrap()
        };

        self.add_entry(entry);

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        let scope_id = self.document.position_to_offset(body_loc.range.start);
        let scope_kind = match method_kind {
            MethodKind::Class => LVScopeKind::ClassMethod,
            MethodKind::Instance => LVScopeKind::InstanceMethod,
            MethodKind::Unknown => LVScopeKind::InstanceMethod, // Default to instance method
        };
        self.scope_tracker
            .push_lv_scope(LVScope::new(scope_id, body_loc, scope_kind));
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.scope_tracker.pop_lv_scope();
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
                    ParamKind::Keyword,
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
                    ParamKind::Keyword,
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
}
