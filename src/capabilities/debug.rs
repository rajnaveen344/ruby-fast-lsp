//! Debug capabilities for the LSP REPL.
//!
//! This module provides custom debug methods that can be invoked via the
//! `$/listCommands` protocol, allowing tools like `lsp-repl` to discover
//! and execute debug commands.

use log::debug;
use serde::{Deserialize, Serialize};

use crate::indexer::entry::entry_kind::EntryKind;
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;

// ============================================================================
// Protocol Types
// ============================================================================

/// A parameter definition for a custom command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandParam {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A custom command definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub name: String,
    pub method: String,
    pub description: String,
    #[serde(default)]
    pub params: Vec<CommandParam>,
}

/// Response from `$/listCommands`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListCommandsResponse {
    pub commands: Vec<CommandDefinition>,
}

// ============================================================================
// Lookup Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/lookup`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupParams {
    /// The fully qualified name to look up (e.g., "User#find", "Foo::Bar")
    pub fqn: String,
}

/// An entry in the lookup response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupEntry {
    pub fqn: String,
    pub kind: String,
    pub location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<String>>,
}

/// Response from `ruby-fast-lsp/debug/lookup`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResponse {
    pub found: bool,
    pub entries: Vec<LookupEntry>,
}

// ============================================================================
// Stats Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/stats`.
/// Empty struct to satisfy tower-lsp custom method requirements.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatsParams {}

/// Response from `ruby-fast-lsp/debug/stats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub total_definitions: usize,
    pub total_entries: usize,
    pub classes: usize,
    pub modules: usize,
    pub methods: usize,
    pub constants: usize,
    pub instance_variables: usize,
    pub files_indexed: usize,
    pub indexing_complete: bool,
}

// ============================================================================
// Ancestors Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/ancestors`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AncestorsParams {
    /// The class/module name to get ancestors for
    pub class: String,
}

/// An ancestor entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AncestorEntry {
    pub name: String,
    pub kind: String, // "superclass", "include", "extend", "prepend"
}

/// Response from `ruby-fast-lsp/debug/ancestors`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AncestorsResponse {
    pub class: String,
    pub ancestors: Vec<AncestorEntry>,
}

// ============================================================================
// Handlers
// ============================================================================

/// Handle `$/listCommands` - return the list of available custom commands.
pub fn handle_list_commands() -> ListCommandsResponse {
    debug!("[DEBUG] Listing available commands");

    let commands = vec![
        CommandDefinition {
            name: "lookup".to_string(),
            method: "ruby-fast-lsp/debug/lookup".to_string(),
            description: "Query index for a fully qualified name (e.g., User#find, Foo::Bar)"
                .to_string(),
            params: vec![CommandParam {
                name: "fqn".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: Some("The FQN to look up".to_string()),
            }],
        },
        CommandDefinition {
            name: "stats".to_string(),
            method: "ruby-fast-lsp/debug/stats".to_string(),
            description: "Show index statistics".to_string(),
            params: vec![],
        },
        CommandDefinition {
            name: "ancestors".to_string(),
            method: "ruby-fast-lsp/debug/ancestors".to_string(),
            description: "Show inheritance and mixin chain for a class".to_string(),
            params: vec![CommandParam {
                name: "class".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: Some("The class name".to_string()),
            }],
        },
        CommandDefinition {
            name: "methods".to_string(),
            method: "ruby-fast-lsp/debug/methods".to_string(),
            description: "List all methods for a class".to_string(),
            params: vec![CommandParam {
                name: "class".to_string(),
                param_type: "string".to_string(),
                required: true,
                description: Some("The class name".to_string()),
            }],
        },
        CommandDefinition {
            name: "inference-stats".to_string(),
            method: "ruby-fast-lsp/debug/inference-stats".to_string(),
            description: "Show type inference statistics and coverage".to_string(),
            params: vec![],
        },
    ];

    ListCommandsResponse { commands }
}

/// Handle `ruby-fast-lsp/debug/lookup` - query the index for an FQN.
pub fn handle_lookup(server: &RubyLanguageServer, params: LookupParams) -> LookupResponse {
    debug!("[DEBUG] Looking up FQN: {}", params.fqn);

    let index = server.index.lock();

    // Parse the FQN string
    let fqn = parse_fqn(&params.fqn);

    match fqn {
        Some(fqn) => {
            let entries = index.get(&fqn);

            match entries {
                Some(entries) => {
                    let lookup_entries: Vec<LookupEntry> = entries
                        .iter()
                        .map(|entry| {
                            let (kind, visibility, return_type, parameters) = match &entry.kind {
                                EntryKind::Class(data) => {
                                    let superclass = data
                                        .superclass
                                        .as_ref()
                                        .map(|s| format!(" < {}", format_mixin_ref(s)))
                                        .unwrap_or_default();
                                    (format!("Class{}", superclass), None, None, None)
                                }
                                EntryKind::Module(_) => ("Module".to_string(), None, None, None),
                                EntryKind::Method(data) => {
                                    let vis = format!("{:?}", data.visibility);
                                    let ret = data.return_type.as_ref().map(|t| t.to_string());
                                    let params: Vec<String> =
                                        data.params.iter().map(|p| p.name.clone()).collect();
                                    (
                                        format!("Method({:?})", data.name.get_kind()),
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
                                    let vis = data.visibility.as_ref().map(|v| format!("{:?}", v));
                                    ("Constant".to_string(), vis, None, None)
                                }
                                EntryKind::InstanceVariable(data) => {
                                    let type_str = if data.r#type
                                        != crate::type_inference::ruby_type::RubyType::Unknown
                                    {
                                        Some(data.r#type.to_string())
                                    } else {
                                        None
                                    };
                                    ("InstanceVariable".to_string(), None, type_str, None)
                                }
                                EntryKind::ClassVariable(data) => {
                                    let type_str = if data.r#type
                                        != crate::type_inference::ruby_type::RubyType::Unknown
                                    {
                                        Some(data.r#type.to_string())
                                    } else {
                                        None
                                    };
                                    ("ClassVariable".to_string(), None, type_str, None)
                                }
                                EntryKind::GlobalVariable(data) => {
                                    let type_str = if data.r#type
                                        != crate::type_inference::ruby_type::RubyType::Unknown
                                    {
                                        Some(data.r#type.to_string())
                                    } else {
                                        None
                                    };
                                    ("GlobalVariable".to_string(), None, type_str, None)
                                }
                                EntryKind::LocalVariable(data) => {
                                    let type_str = if data.r#type
                                        != crate::type_inference::ruby_type::RubyType::Unknown
                                    {
                                        Some(data.r#type.to_string())
                                    } else {
                                        None
                                    };
                                    ("LocalVariable".to_string(), None, type_str, None)
                                }
                                EntryKind::Reference => ("Reference".to_string(), None, None, None),
                            };

                            // Get location string
                            let location = index
                                .get_file_url(entry.location.file_id)
                                .map(|url| {
                                    let path = url.path();
                                    let file_name = path.split('/').last().unwrap_or(path);
                                    format!(
                                        "{}:{}:{}",
                                        file_name,
                                        entry.location.range.start.line + 1,
                                        entry.location.range.start.character + 1
                                    )
                                })
                                .unwrap_or_else(|| "unknown".to_string());

                            LookupEntry {
                                fqn: index
                                    .get_fqn(entry.fqn_id)
                                    .map(|f| f.to_string())
                                    .unwrap_or_else(|| params.fqn.clone()),
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

/// Handle `ruby-fast-lsp/debug/stats` - return index statistics.
pub fn handle_stats(server: &RubyLanguageServer) -> StatsResponse {
    debug!("[DEBUG] Getting index stats");

    let index = server.index.lock();

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
            EntryKind::Reference => {}
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
        indexing_complete: server.is_indexing_complete(),
    }
}

/// Handle `ruby-fast-lsp/debug/ancestors` - get inheritance chain for a class.
pub fn handle_ancestors(server: &RubyLanguageServer, params: AncestorsParams) -> AncestorsResponse {
    debug!("[DEBUG] Getting ancestors for: {}", params.class);

    let index = server.index.lock();

    let mut ancestors = Vec::new();

    // Parse the class name into an FQN
    let parts: Vec<&str> = params.class.split("::").collect();
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
        class: params.class,
        ancestors,
    }
}

/// Parameters for `ruby-fast-lsp/debug/methods`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodsParams {
    /// The class name to list methods for
    pub class: String,
}

/// A method entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodEntry {
    pub name: String,
    pub kind: String,
    pub visibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
}

/// Response from `ruby-fast-lsp/debug/methods`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodsResponse {
    pub class: String,
    pub methods: Vec<MethodEntry>,
}

/// Handle `ruby-fast-lsp/debug/methods` - list methods for a class.
pub fn handle_methods(server: &RubyLanguageServer, params: MethodsParams) -> MethodsResponse {
    debug!("[DEBUG] Getting methods for: {}", params.class);

    let index = server.index.lock();

    let mut methods = Vec::new();

    // Parse the class name into namespace parts
    let parts: Vec<&str> = params.class.split("::").collect();
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
                        methods.push(MethodEntry {
                            name: method.to_string(),
                            kind: format!("{:?}", method.get_kind()),
                            visibility: format!("{:?}", data.visibility),
                            return_type: data.return_type.as_ref().map(|t| t.to_string()),
                        });
                    }
                }
            }
        }
    }

    MethodsResponse {
        class: params.class,
        methods,
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
        let method = crate::types::ruby_method::RubyMethod::new(
            method_name,
            crate::indexer::entry::MethodKind::Instance,
        )
        .ok()?;

        Some(FullyQualifiedName::Method(namespace, method))
    } else if let Some(dot_pos) = fqn_str.rfind('.') {
        // Check if it's a class method (Foo.bar) or just namespace (Foo::Bar)
        let before_dot = &fqn_str[..dot_pos];
        let after_dot = &fqn_str[dot_pos + 1..];

        // If before_dot contains ::, treat as class method
        if before_dot.contains("::") || before_dot.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            let namespace = parse_namespace(before_dot)?;
            let method = crate::types::ruby_method::RubyMethod::new(
                after_dot,
                crate::indexer::entry::MethodKind::Class,
            )
            .ok()?;
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

/// Format a MixinRef as a string.
fn format_mixin_ref(mixin: &crate::indexer::entry::MixinRef) -> String {
    let prefix = if mixin.absolute { "::" } else { "" };
    let parts: Vec<String> = mixin.parts.iter().map(|p| p.to_string()).collect();
    format!("{}{}", prefix, parts.join("::"))
}

// ============================================================================
// Inference Stats Types
// ============================================================================

/// Parameters for `ruby-fast-lsp/debug/inference-stats`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InferenceStatsParams {}

/// Response from `ruby-fast-lsp/debug/inference-stats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceStatsResponse {
    pub total_methods: usize,
    pub methods_with_return_type: usize,
    pub methods_without_return_type: usize,
    pub inference_coverage_percent: f64,
    pub top_files_by_method_count: Vec<FileMethodCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMethodCount {
    pub file: String,
    pub method_count: usize,
}

/// Handle `ruby-fast-lsp/debug/inference-stats` - get type inference statistics.
pub fn handle_inference_stats(server: &RubyLanguageServer) -> InferenceStatsResponse {
    debug!("[DEBUG] Getting inference stats");

    let index = server.index.lock();

    let mut total_methods = 0;
    let mut methods_with_return_type = 0;
    let mut file_method_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for entry in index.all_entries() {
        if let EntryKind::Method(data) = &entry.kind {
            total_methods += 1;
            
            if data.return_type.is_some() && data.return_type.as_ref() != Some(&crate::type_inference::ruby_type::RubyType::Unknown) {
                methods_with_return_type += 1;
            }

            // Count methods per file
            if let Some(url) = index.get_file_url(entry.location.file_id) {
                let file_name = url.path().split('/').last().unwrap_or("unknown").to_string();
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
        .map(|(file, count)| FileMethodCount { file, method_count: count })
        .collect();

    InferenceStatsResponse {
        total_methods,
        methods_with_return_type,
        methods_without_return_type,
        inference_coverage_percent,
        top_files_by_method_count,
    }
}

