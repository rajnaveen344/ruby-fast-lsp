//! Diagnostics Query — Index-dependent diagnostic generation.
//!
//! Provides diagnostics for:
//! - Unresolved constants and methods
//! - YARD documentation issues (unknown params, unknown types, RBS mismatches)
//!
//! AST-only diagnostics (syntax errors/warnings) remain in `capabilities/diagnostics.rs`.

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::{RubyIndex, UnresolvedEntry};
use crate::yard::YardTypeConverter;
use log::debug;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Url};

use super::IndexQuery;

impl IndexQuery {
    /// Get diagnostics for unresolved entries (constants and methods) from the index.
    pub fn get_unresolved_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        let index = self.index.lock();
        let unresolved_list = index.get_unresolved_entries(uri);

        unresolved_list
            .iter()
            .map(|entry| match entry {
                UnresolvedEntry::Constant { name, location, .. } => Diagnostic {
                    range: location.range,
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("unresolved-constant".to_string())),
                    code_description: None,
                    source: Some("ruby-fast-lsp".to_string()),
                    message: format!("Unresolved constant `{}`", name),
                    related_information: None,
                    tags: None,
                    data: None,
                },
                UnresolvedEntry::Method {
                    name,
                    receiver_type,
                    location,
                } => {
                    if let Some(crate::inferrer::r#type::ruby::RubyType::Unknown) = receiver_type {
                        Diagnostic {
                            range: location.range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String(
                                "unknown-receiver-type".to_string(),
                            )),
                            code_description: None,
                            source: Some("ruby-fast-lsp".to_string()),
                            message: format!(
                                "Cannot determine receiver type for method call `{}`. Definition may be imprecise.",
                                name
                            ),
                            related_information: None,
                            tags: None,
                            data: None,
                        }
                    } else {
                        let message = match receiver_type {
                            Some(recv) => format!("Unresolved method `{}` on `{}`", name, recv),
                            None => format!("Unresolved method `{}`", name),
                        };

                        Diagnostic {
                            range: location.range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: Some(NumberOrString::String("unresolved-method".to_string())),
                            code_description: None,
                            source: Some("ruby-fast-lsp".to_string()),
                            message,
                            related_information: None,
                            tags: None,
                            data: None,
                        }
                    }
                }
            })
            .collect()
    }

    /// Generate diagnostics for YARD documentation issues.
    ///
    /// Checks for:
    /// - `@param` tags that reference non-existent parameters
    /// - Type references that don't exist in the index
    /// - Return type mismatches between YARD and RBS
    pub fn get_yard_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        let index = self.index.lock();
        generate_yard_diagnostics_inner(&index, uri)
    }
}

/// Core YARD diagnostic logic operating on a locked index.
///
/// Extracted so it can be used both by `IndexQuery::get_yard_diagnostics` and
/// directly by code that already holds a lock (e.g. test harness).
pub fn generate_yard_diagnostics_inner(index: &RubyIndex, uri: &Url) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let entries = index.file_entries(uri);
    for entry in entries {
        let (yard_doc, method_params) = match &entry.kind {
            EntryKind::Method(data) => match &data.yard_doc {
                Some(doc) => (doc, &data.params),
                None => continue,
            },
            _ => continue,
        };

        let actual_param_names: Vec<&str> = method_params.iter().map(|p| p.name.as_str()).collect();

        // Find YARD @param tags that don't match any actual parameter.
        let unmatched = yard_doc.find_unmatched_params(&actual_param_names);
        for (yard_param, range) in unmatched {
            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String("yard-unknown-param".to_string())),
                code_description: None,
                source: Some("ruby-fast-lsp".to_string()),
                message: format!(
                    "YARD @param '{}' does not match any method parameter",
                    yard_param.name
                ),
                related_information: None,
                tags: None,
                data: None,
            });
        }

        // Check for unresolved types in @param tags.
        for param in &yard_doc.params {
            let result =
                YardTypeConverter::convert_multiple_with_validation(&param.types, Some(index));
            for unresolved in result.unresolved_types {
                let diagnostic_range = param.types_range.or(param.range);
                if let Some(range) = diagnostic_range {
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("yard-unknown-type".to_string())),
                        code_description: None,
                        source: Some("ruby-fast-lsp".to_string()),
                        message: format!(
                            "Unknown type '{}' in YARD @param documentation",
                            unresolved.type_name
                        ),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }
            }
        }

        // Check for unresolved types in @return tags.
        for return_doc in &yard_doc.returns {
            let result =
                YardTypeConverter::convert_multiple_with_validation(&return_doc.types, Some(index));
            for unresolved in result.unresolved_types {
                debug!(
                    "Unresolved return type '{}' (no range available for diagnostic)",
                    unresolved.type_name
                );
            }
        }

        // Check for return type mismatch with RBS.
        if !yard_doc.returns.is_empty() {
            if let EntryKind::Method(data) = &entry.kind {
                let owner_parts = data.owner.namespace_parts();
                if !owner_parts.is_empty() {
                    let class_name = owner_parts
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join("::");

                    let method_name = data.name.get_name();
                    let is_singleton = matches!(
                        data.owner.namespace_kind(),
                        Some(crate::indexer::entry::NamespaceKind::Singleton)
                    );

                    if let Some(rbs_type) =
                        crate::inferrer::rbs::get_rbs_method_return_type_as_ruby_type(
                            &class_name,
                            &method_name,
                            is_singleton,
                        )
                    {
                        let yard_types_str: Vec<String> = yard_doc
                            .returns
                            .iter()
                            .flat_map(|r| r.types.clone())
                            .collect();

                        let yard_ruby_types = YardTypeConverter::convert_multiple(&yard_types_str);

                        if rbs_type != crate::inferrer::r#type::ruby::RubyType::Unknown
                            && yard_ruby_types != rbs_type
                        {
                            if let Some(first_return) = yard_doc.returns.first() {
                                if let Some(range) = first_return.types_range.or(first_return.range)
                                {
                                    diagnostics.push(Diagnostic {
                                        range,
                                        severity: Some(DiagnosticSeverity::WARNING),
                                        code: Some(NumberOrString::String(
                                            "yard-rbs-mismatch".to_string(),
                                        )),
                                        code_description: None,
                                        source: Some("ruby-fast-lsp".to_string()),
                                        message: format!(
                                            "YARD return type '{}' conflicts with RBS type '{}'",
                                            yard_ruby_types, rbs_type
                                        ),
                                        related_information: None,
                                        tags: None,
                                        data: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    diagnostics
}
