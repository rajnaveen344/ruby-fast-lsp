//! Debug Query — Index queries for debug/inspection commands.
//!
//! Moves all index-heavy logic out of the capability handlers so they
//! become thin wrappers that extract params → call query → return response.

use std::collections::HashMap;

use crate::capabilities::debug::{
    AncestorEntry, AncestorsResponse, ExportGraphResponse, FileMethodCount, GraphNodeSnapshot,
    InferenceStatsResponse, LookupEntry, LookupResponse, MethodEntry, MethodsResponse,
    StatsResponse,
};
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::NamespaceKind;
use crate::indexer::graph::NodeKind;
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::IndexQuery;

impl IndexQuery {
    /// Query the index for a fully qualified name, returning matching entries.
    pub fn debug_lookup(&self, fqn_str: &str) -> LookupResponse {
        let index = self.index.lock();

        // Parse the FQN string
        let fqn = parse_fqn(fqn_str);

        match fqn {
            Some(fqn) => {
                // Try as parsed FQN first
                let mut entries = index.get(&fqn);

                // If not found and it's a Constant, also try as Namespace (class/module)
                if entries.is_none() {
                    if let FullyQualifiedName::Constant(parts) = &fqn {
                        let namespace_fqn = FullyQualifiedName::Namespace(
                            parts.clone(),
                            crate::indexer::entry::NamespaceKind::Instance,
                        );
                        entries = index.get(&namespace_fqn);
                    }
                }

                match entries {
                    Some(entries) => {
                        let lookup_entries: Vec<LookupEntry> = entries
                            .iter()
                            .map(|entry| {
                                let (kind, visibility, return_type, parameters) = match &entry.kind
                                {
                                    EntryKind::Class(data) => {
                                        let superclass = data
                                            .superclass
                                            .as_ref()
                                            .map(|s| format!(" < {}", format_mixin_ref(s)))
                                            .unwrap_or_default();
                                        (format!("Class{}", superclass), None, None, None)
                                    }
                                    EntryKind::Module(_) => {
                                        ("Module".to_string(), None, None, None)
                                    }
                                    EntryKind::Method(data) => {
                                        let vis = format!("{:?}", data.visibility);
                                        let ret = data.return_type.as_ref().map(|t| t.to_string());
                                        let params: Vec<String> =
                                            data.params.iter().map(|p| p.name.clone()).collect();
                                        // Get kind from owner namespace
                                        let kind = data.owner.namespace_kind().unwrap_or(
                                            crate::indexer::entry::NamespaceKind::Instance,
                                        );
                                        (
                                            format!("Method({:?})", kind),
                                            Some(vis),
                                            ret,
                                            if params.is_empty() {
                                                None
                                            } else {
                                                Some(params)
                                            },
                                        )
                                    }
                                    EntryKind::Constant(data) => {
                                        let vis =
                                            data.visibility.as_ref().map(|v| format!("{:?}", v));
                                        ("Constant".to_string(), vis, None, None)
                                    }
                                    EntryKind::InstanceVariable(data) => {
                                        let type_str = if data.r#type
                                            != crate::inferrer::r#type::ruby::RubyType::Unknown
                                        {
                                            Some(data.r#type.to_string())
                                        } else {
                                            None
                                        };
                                        ("InstanceVariable".to_string(), None, type_str, None)
                                    }
                                    EntryKind::ClassVariable(data) => {
                                        let type_str = if data.r#type
                                            != crate::inferrer::r#type::ruby::RubyType::Unknown
                                        {
                                            Some(data.r#type.to_string())
                                        } else {
                                            None
                                        };
                                        ("ClassVariable".to_string(), None, type_str, None)
                                    }
                                    EntryKind::GlobalVariable(data) => {
                                        let type_str = if data.r#type
                                            != crate::inferrer::r#type::ruby::RubyType::Unknown
                                        {
                                            Some(data.r#type.to_string())
                                        } else {
                                            None
                                        };
                                        ("GlobalVariable".to_string(), None, type_str, None)
                                    }
                                    EntryKind::LocalVariable(data) => {
                                        let type_str = if let Some(last_assignment) =
                                            data.assignments.last()
                                        {
                                            if last_assignment.r#type
                                                != crate::inferrer::r#type::ruby::RubyType::Unknown
                                            {
                                                Some(last_assignment.r#type.to_string())
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };
                                        ("LocalVariable".to_string(), None, type_str, None)
                                    }
                                    EntryKind::Reference(_) => {
                                        ("Reference".to_string(), None, None, None)
                                    }
                                };

                                // Get location string - return full URI for proper navigation
                                let location = index
                                    .get_file_url(entry.location.file_id)
                                    .map(|url| {
                                        format!(
                                            "{}:{}:{}",
                                            url.as_str(),
                                            entry.location.range.start.line,
                                            entry.location.range.start.character
                                        )
                                    })
                                    .unwrap_or_else(|| "unknown".to_string());

                                LookupEntry {
                                    fqn: index
                                        .get_fqn(entry.fqn_id)
                                        .map(|f| f.to_string())
                                        .unwrap_or_else(|| fqn_str.to_string()),
                                    kind,
                                    location,
                                    visibility,
                                    return_type,
                                    parameters,
                                }
                            })
                            .collect();

                        LookupResponse {
                            found: true,
                            entries: lookup_entries,
                        }
                    }
                    None => LookupResponse {
                        found: false,
                        entries: vec![],
                    },
                }
            }
            None => LookupResponse {
                found: false,
                entries: vec![],
            },
        }
    }

    /// Return index statistics.
    pub fn debug_stats(&self, indexing_complete: bool) -> StatsResponse {
        let index = self.index.lock();

        let mut classes = 0;
        let mut modules = 0;
        let mut methods = 0;
        let mut constants = 0;
        let mut instance_variables = 0;
        let mut total_entries = 0;

        // Count entries by kind
        for entry in index.all_entries() {
            total_entries += 1;
            match &entry.kind {
                EntryKind::Class(_) => classes += 1,
                EntryKind::Module(_) => modules += 1,
                EntryKind::Method(_) => methods += 1,
                EntryKind::Constant(_) => constants += 1,
                EntryKind::InstanceVariable(_) => instance_variables += 1,
                EntryKind::ClassVariable(_) => {}
                EntryKind::GlobalVariable(_) => {}
                EntryKind::LocalVariable(_) => {}
                EntryKind::Reference(_) => {}
            }
        }

        StatsResponse {
            total_definitions: index.definitions_len(),
            total_entries,
            classes,
            modules,
            methods,
            constants,
            instance_variables,
            files_indexed: index.files_count(),
            indexing_complete,
        }
    }

    /// Get inheritance chain for a class or module.
    pub fn debug_ancestors(&self, class_name: &str) -> AncestorsResponse {
        let index = self.index.lock();

        let mut ancestors = Vec::new();

        // Parse the class name into an FQN
        let parts: Vec<&str> = class_name.split("::").collect();
        let namespace: Vec<crate::types::ruby_namespace::RubyConstant> = parts
            .iter()
            .filter_map(|p| crate::types::ruby_namespace::RubyConstant::new(p).ok())
            .collect();

        let fqn = FullyQualifiedName::Constant(namespace);

        // Look up the class
        if let Some(entries) = index.get(&fqn) {
            for entry in entries {
                match &entry.kind {
                    EntryKind::Class(data) => {
                        // Add superclass
                        if let Some(superclass) = &data.superclass {
                            ancestors.push(AncestorEntry {
                                name: format_mixin_ref(superclass),
                                kind: "superclass".to_string(),
                            });
                        }

                        // Add includes
                        for mixin in &data.includes {
                            ancestors.push(AncestorEntry {
                                name: format_mixin_ref(mixin),
                                kind: "include".to_string(),
                            });
                        }

                        // Add extends
                        for mixin in &data.extends {
                            ancestors.push(AncestorEntry {
                                name: format_mixin_ref(mixin),
                                kind: "extend".to_string(),
                            });
                        }

                        // Add prepends
                        for mixin in &data.prepends {
                            ancestors.push(AncestorEntry {
                                name: format_mixin_ref(mixin),
                                kind: "prepend".to_string(),
                            });
                        }
                    }
                    EntryKind::Module(data) => {
                        // Add includes
                        for mixin in &data.includes {
                            ancestors.push(AncestorEntry {
                                name: format_mixin_ref(mixin),
                                kind: "include".to_string(),
                            });
                        }

                        // Add extends
                        for mixin in &data.extends {
                            ancestors.push(AncestorEntry {
                                name: format_mixin_ref(mixin),
                                kind: "extend".to_string(),
                            });
                        }

                        // Add prepends
                        for mixin in &data.prepends {
                            ancestors.push(AncestorEntry {
                                name: format_mixin_ref(mixin),
                                kind: "prepend".to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        AncestorsResponse {
            class: class_name.to_string(),
            ancestors,
        }
    }

    /// List all methods for a class.
    pub fn debug_methods(&self, class_name: &str) -> MethodsResponse {
        let index = self.index.lock();

        let mut methods = Vec::new();

        // Parse the class name into namespace parts
        let parts: Vec<&str> = class_name.split("::").collect();
        let namespace: Vec<crate::types::ruby_namespace::RubyConstant> = parts
            .iter()
            .filter_map(|p| crate::types::ruby_namespace::RubyConstant::new(p).ok())
            .collect();

        // Look for all methods with this namespace prefix
        for entry in index.all_entries() {
            if let EntryKind::Method(data) = &entry.kind {
                // Check if this method belongs to the requested class
                if let Some(fqn) = index.get_fqn(entry.fqn_id) {
                    if let FullyQualifiedName::Method(ns, method) = fqn {
                        if *ns == namespace {
                            // Get kind from owner namespace
                            let kind = data
                                .owner
                                .namespace_kind()
                                .unwrap_or(crate::indexer::entry::NamespaceKind::Instance);
                            methods.push(MethodEntry {
                                name: method.to_string(),
                                kind: format!("{:?}", kind),
                                visibility: format!("{:?}", data.visibility),
                                return_type: data.return_type.as_ref().map(|t| t.to_string()),
                            });
                        }
                    }
                }
            }
        }

        MethodsResponse {
            class: class_name.to_string(),
            methods,
        }
    }

    /// Get type inference statistics and coverage.
    pub fn debug_inference_stats(&self) -> InferenceStatsResponse {
        let index = self.index.lock();

        let mut total_methods = 0;
        let mut methods_with_return_type = 0;
        let mut file_method_counts: HashMap<String, usize> = HashMap::new();

        for entry in index.all_entries() {
            if let EntryKind::Method(data) = &entry.kind {
                total_methods += 1;

                if data.return_type.is_some()
                    && data.return_type.as_ref()
                        != Some(&crate::inferrer::r#type::ruby::RubyType::Unknown)
                {
                    methods_with_return_type += 1;
                }

                // Count methods per file
                if let Some(url) = index.get_file_url(entry.location.file_id) {
                    let file_name = url
                        .path()
                        .split('/')
                        .last()
                        .unwrap_or("unknown")
                        .to_string();
                    *file_method_counts.entry(file_name).or_insert(0) += 1;
                }
            }
        }

        let methods_without_return_type = total_methods - methods_with_return_type;
        let inference_coverage_percent = if total_methods > 0 {
            (methods_with_return_type as f64 / total_methods as f64) * 100.0
        } else {
            0.0
        };

        // Get top 10 files by method count
        let mut file_counts: Vec<_> = file_method_counts.into_iter().collect();
        file_counts.sort_by(|a, b| b.1.cmp(&a.1));
        let top_files_by_method_count: Vec<FileMethodCount> = file_counts
            .into_iter()
            .take(10)
            .map(|(file, count)| FileMethodCount {
                file,
                method_count: count,
            })
            .collect();

        InferenceStatsResponse {
            total_methods,
            methods_with_return_type,
            methods_without_return_type,
            inference_coverage_percent,
            top_files_by_method_count,
        }
    }

    /// Export the inheritance graph as a snapshot.
    pub fn debug_export_graph(&self) -> ExportGraphResponse {
        let index = self.index.lock();
        let graph = index.get_graph();

        let mut nodes = HashMap::new();

        // Helper to convert FQN to a readable key
        // Instance: "A::B::C"
        // Singleton: "#<Class:A::B::C>"
        let fqn_to_key = |fqn: &FullyQualifiedName| -> String {
            match fqn {
                FullyQualifiedName::Namespace(parts, NamespaceKind::Instance) => parts
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join("::"),
                FullyQualifiedName::Namespace(parts, NamespaceKind::Singleton) => {
                    let name = parts
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join("::");
                    format!("#<Class:{}>", name)
                }
                other => other.to_string(),
            }
        };

        // Resolve FqnId to readable key
        let resolve_key = |id| index.get_fqn(id).map(|f| fqn_to_key(f));

        // Iterate through all definitions
        for (fqn, _entries) in index.definitions() {
            // Only process namespaces (classes/modules)
            let FullyQualifiedName::Namespace(_, _) = fqn else {
                continue;
            };

            let Some(fqn_id) = index.get_fqn_id(fqn) else {
                continue;
            };

            let Some(node) = graph.get_node(fqn_id) else {
                continue;
            };

            let key = fqn_to_key(fqn);

            // Compute MRO using the graph's method_lookup_chain
            let mro: Vec<String> = graph
                .method_lookup_chain(fqn_id)
                .iter()
                .filter_map(|&id| resolve_key(id))
                .collect();

            // For modules, find all classes that ultimately include this module
            let included_by_classes = if node.kind == NodeKind::Module {
                find_including_classes(fqn, &index, &fqn_to_key)
            } else {
                Vec::new()
            };

            nodes.insert(
                key,
                GraphNodeSnapshot {
                    kind: format!("{:?}", node.kind),
                    superclass: node.superclass.and_then(|id| resolve_key(id)),
                    includes: node
                        .includes
                        .iter()
                        .filter_map(|&id| resolve_key(id))
                        .collect(),
                    prepends: node
                        .prepends
                        .iter()
                        .filter_map(|&id| resolve_key(id))
                        .collect(),
                    included_by: node
                        .included_by
                        .iter()
                        .filter_map(|&id| resolve_key(id))
                        .collect(),
                    prepended_by: node
                        .prepended_by
                        .iter()
                        .filter_map(|&id| resolve_key(id))
                        .collect(),
                    children: node
                        .children
                        .iter()
                        .filter_map(|&id| resolve_key(id))
                        .collect(),
                    included_by_classes,
                    mro,
                },
            );
        }

        ExportGraphResponse {
            node_count: nodes.len(),
            nodes,
        }
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Parse a string FQN into a FullyQualifiedName.
fn parse_fqn(fqn_str: &str) -> Option<FullyQualifiedName> {
    // Check if it's a method (contains # or .)
    if let Some(hash_pos) = fqn_str.find('#') {
        // Instance method: Foo::Bar#baz
        let namespace_str = &fqn_str[..hash_pos];
        let method_name = &fqn_str[hash_pos + 1..];

        let namespace = parse_namespace(namespace_str)?;
        let method = crate::types::ruby_method::RubyMethod::new(method_name).ok()?;

        Some(FullyQualifiedName::Method(namespace, method))
    } else if let Some(dot_pos) = fqn_str.rfind('.') {
        // Check if it's a class method (Foo.bar) or just namespace (Foo::Bar)
        let before_dot = &fqn_str[..dot_pos];
        let after_dot = &fqn_str[dot_pos + 1..];

        // If before_dot contains ::, treat as class method
        if before_dot.contains("::")
            || before_dot
                .chars()
                .next()
                .map(|c| c.is_uppercase())
                .unwrap_or(false)
        {
            let namespace = parse_namespace(before_dot)?;
            let method = crate::types::ruby_method::RubyMethod::new(after_dot).ok()?;
            Some(FullyQualifiedName::Method(namespace, method))
        } else {
            // Just a namespace
            Some(FullyQualifiedName::Constant(parse_namespace(fqn_str)?))
        }
    } else {
        // Just a namespace: Foo::Bar
        Some(FullyQualifiedName::Constant(parse_namespace(fqn_str)?))
    }
}

/// Parse a namespace string into a vector of RubyConstants.
fn parse_namespace(namespace_str: &str) -> Option<Vec<crate::types::ruby_namespace::RubyConstant>> {
    let parts: Vec<&str> = namespace_str.split("::").collect();
    let namespace: Vec<crate::types::ruby_namespace::RubyConstant> = parts
        .iter()
        .filter_map(|p| crate::types::ruby_namespace::RubyConstant::new(p.trim()).ok())
        .collect();

    if namespace.len() == parts.len() {
        Some(namespace)
    } else {
        None
    }
}

/// Find all classes that ultimately include a module by traversing included_by/prepended_by edges.
/// Uses the index's `including_classes` method which does BFS through intermediate modules.
fn find_including_classes(
    module_fqn: &FullyQualifiedName,
    index: &RubyIndex,
    fqn_to_key: &impl Fn(&FullyQualifiedName) -> String,
) -> Vec<String> {
    let mut classes: Vec<String> = index
        .including_classes(module_fqn)
        .iter()
        .map(|(fqn, _via_modules)| fqn_to_key(fqn))
        .collect();

    classes.sort();
    classes
}

/// Format a MixinRef as a string.
fn format_mixin_ref(mixin: &crate::indexer::entry::MixinRef) -> String {
    let prefix = if mixin.absolute { "::" } else { "" };
    let parts: Vec<String> = mixin.parts.iter().map(|p| p.to_string()).collect();
    format!("{}{}", prefix, parts.join("::"))
}
