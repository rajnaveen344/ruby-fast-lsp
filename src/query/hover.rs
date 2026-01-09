//! Hover Query - Query methods for hover information
//!
//! Provides unified query methods for hover information.
//! The primary entry point is `get_hover_at_position`.

use crate::analyzer_prism::{Identifier, MethodReceiver, RubyPrismAnalyzer};
use crate::indexer::entry::entry_kind::EntryKind;
use crate::inferrer::r#type::ruby::RubyType;
use crate::inferrer::TypeNarrowingEngine;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use tower_lsp::lsp_types::{Position, Range, Url};

use super::IndexQuery;

/// Hover information for a symbol.
#[derive(Debug, Clone)]
pub struct HoverInfo {
    /// The markdown content to display.
    pub content: String,
    /// The range of the hovered symbol.
    pub range: Option<Range>,
    /// The inferred type (if applicable).
    pub ruby_type: Option<RubyType>,
}

impl HoverInfo {
    /// Create hover info with just text content.
    pub fn text(content: String) -> Self {
        Self {
            content,
            range: None,
            ruby_type: None,
        }
    }

    /// Create hover info with type.
    pub fn with_type(content: String, ruby_type: RubyType) -> Self {
        Self {
            content,
            range: None,
            ruby_type: Some(ruby_type),
        }
    }
}

// =============================================================================
// Public API - One unified entry point per feature
// =============================================================================

impl IndexQuery<'_> {
    /// Get hover info for the symbol at position.
    ///
    /// This is the unified entry point for hover requests. It handles:
    /// - Local variables
    /// - Instance/class/global variables
    /// - Constants (classes, modules)
    /// - Methods
    /// - YARD type references
    pub fn get_hover_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
        _type_narrowing: Option<&TypeNarrowingEngine>,
    ) -> Option<HoverInfo> {
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier_opt, _identifier_type, ancestors, _scope_id) =
            analyzer.get_identifier(position);

        let identifier = identifier_opt?;

        self.get_hover_for_identifier(uri, &identifier, &ancestors)
    }

    /// Resolve the type of the symbol at position.
    ///
    /// This is the unified entry point for type resolution. It handles:
    /// - Local variables
    /// - Instance/class/global variables
    /// - Constants
    /// - Method call return types
    pub fn resolve_type_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
        _type_narrowing: Option<&TypeNarrowingEngine>,
    ) -> Option<RubyType> {
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier_opt, _identifier_type, ancestors, _scope_id) =
            analyzer.get_identifier(position);

        let identifier = identifier_opt?;

        self.resolve_type_for_identifier(uri, &identifier, &ancestors)
    }
}

// =============================================================================
// Private helpers
// =============================================================================

impl IndexQuery<'_> {
    /// Get hover info for a specific identifier.
    fn get_hover_for_identifier(
        &self,
        uri: &Url,
        identifier: &Identifier,
        ancestors: &[RubyConstant],
    ) -> Option<HoverInfo> {
        match identifier {
            Identifier::RubyLocalVariable { name, .. } => {
                // For local variables, just show the name for now
                // Full type inference requires TypeQuery which needs more context
                Some(HoverInfo::text(name.clone()))
            }
            Identifier::RubyConstant { iden, .. } => self.get_constant_hover_info(iden),
            Identifier::RubyMethod {
                iden,
                receiver,
                namespace: _,
            } => {
                let method_name = iden.to_string();
                self.get_method_hover_info(receiver, &method_name, ancestors)
            }
            Identifier::RubyInstanceVariable { name, .. } => {
                self.get_instance_variable_hover_info(uri, name)
            }
            Identifier::RubyClassVariable { name, .. } => {
                self.get_class_variable_hover_info(uri, name)
            }
            Identifier::RubyGlobalVariable { name, .. } => {
                self.get_global_variable_hover_info(uri, name)
            }
            Identifier::YardType { type_name, .. } => Some(HoverInfo::text(type_name.clone())),
        }
    }

    /// Resolve type for a specific identifier.
    fn resolve_type_for_identifier(
        &self,
        uri: &Url,
        identifier: &Identifier,
        ancestors: &[RubyConstant],
    ) -> Option<RubyType> {
        match identifier {
            Identifier::RubyLocalVariable { .. } => {
                // Local variable type inference requires TypeQuery
                None
            }
            Identifier::RubyConstant { iden, .. } => {
                let fqn = FullyQualifiedName::namespace(iden.clone());
                Some(RubyType::Class(fqn))
            }
            Identifier::RubyMethod {
                iden,
                receiver,
                namespace: _,
            } => {
                let method_name = iden.to_string();
                self.resolve_method_return_type(receiver, &method_name, ancestors)
            }
            Identifier::RubyInstanceVariable { name, .. } => {
                self.get_instance_variable_type(uri, name)
            }
            Identifier::RubyClassVariable { name, .. } => self.get_class_variable_type(uri, name),
            Identifier::RubyGlobalVariable { name, .. } => self.get_global_variable_type(uri, name),
            Identifier::YardType { .. } => None,
        }
    }

    /// Get hover info for a constant (class or module).
    fn get_constant_hover_info(&self, constant_path: &[RubyConstant]) -> Option<HoverInfo> {
        let fqn = FullyQualifiedName::namespace(constant_path.to_vec());
        let entries = self.index.get(&fqn)?;

        let entry_kind = entries.iter().find_map(|entry| match &entry.kind {
            EntryKind::Class(_) => Some("class"),
            EntryKind::Module(_) => Some("module"),
            _ => None,
        });

        let fqn_str = constant_path
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("::");

        let content = match entry_kind {
            Some("class") => format!("class {}", fqn_str),
            Some("module") => format!("module {}", fqn_str),
            _ => fqn_str,
        };

        Some(HoverInfo::text(content))
    }

    /// Get hover info for an instance variable.
    fn get_instance_variable_hover_info(&self, uri: &Url, name: &str) -> Option<HoverInfo> {
        let entries = self.index.file_entries(uri);

        for entry in entries {
            if let EntryKind::InstanceVariable(data) = &entry.kind {
                if &data.name == name && data.r#type != RubyType::Unknown {
                    let content = format!("{}: {}", name, data.r#type);
                    return Some(HoverInfo::with_type(content, data.r#type.clone()));
                }
            }
        }

        Some(HoverInfo::text(name.to_string()))
    }

    /// Get hover info for a class variable.
    fn get_class_variable_hover_info(&self, uri: &Url, name: &str) -> Option<HoverInfo> {
        let entries = self.index.file_entries(uri);

        for entry in entries {
            if let EntryKind::ClassVariable(data) = &entry.kind {
                if &data.name == name && data.r#type != RubyType::Unknown {
                    let content = format!("{}: {}", name, data.r#type);
                    return Some(HoverInfo::with_type(content, data.r#type.clone()));
                }
            }
        }

        Some(HoverInfo::text(name.to_string()))
    }

    /// Get hover info for a global variable.
    fn get_global_variable_hover_info(&self, uri: &Url, name: &str) -> Option<HoverInfo> {
        let entries = self.index.file_entries(uri);

        for entry in entries {
            if let EntryKind::GlobalVariable(data) = &entry.kind {
                if &data.name == name && data.r#type != RubyType::Unknown {
                    let content = format!("{}: {}", name, data.r#type);
                    return Some(HoverInfo::with_type(content, data.r#type.clone()));
                }
            }
        }

        Some(HoverInfo::text(name.to_string()))
    }

    /// Get hover info for a method.
    fn get_method_hover_info(
        &self,
        receiver: &MethodReceiver,
        method_name: &str,
        ancestors: &[RubyConstant],
    ) -> Option<HoverInfo> {
        match receiver {
            MethodReceiver::None | MethodReceiver::SelfReceiver => self
                .get_method_info_for_implicit_receiver(method_name, ancestors)
                .map(|info| {
                    let content = if let Some(doc) = &info.documentation {
                        format!("**{}**\n\n{}", info.fqn, doc)
                    } else {
                        format!("**{}**", info.fqn)
                    };
                    let ty = info.return_type.unwrap_or(RubyType::Unknown);
                    HoverInfo::with_type(content, ty)
                }),
            _ => {
                // For other receivers, just show the method name
                Some(HoverInfo::text(format!("def {}", method_name)))
            }
        }
    }

    /// Resolve return type for a method.
    fn resolve_method_return_type(
        &self,
        receiver: &MethodReceiver,
        method_name: &str,
        ancestors: &[RubyConstant],
    ) -> Option<RubyType> {
        match receiver {
            MethodReceiver::None | MethodReceiver::SelfReceiver => self
                .get_method_info_for_implicit_receiver(method_name, ancestors)
                .and_then(|info| info.return_type),
            _ => None,
        }
    }

    /// Get type for an instance variable.
    fn get_instance_variable_type(&self, uri: &Url, name: &str) -> Option<RubyType> {
        let entries = self.index.file_entries(uri);

        for entry in entries {
            if let EntryKind::InstanceVariable(data) = &entry.kind {
                if &data.name == name && data.r#type != RubyType::Unknown {
                    return Some(data.r#type.clone());
                }
            }
        }
        None
    }

    /// Get type for a class variable.
    fn get_class_variable_type(&self, uri: &Url, name: &str) -> Option<RubyType> {
        let entries = self.index.file_entries(uri);

        for entry in entries {
            if let EntryKind::ClassVariable(data) = &entry.kind {
                if &data.name == name && data.r#type != RubyType::Unknown {
                    return Some(data.r#type.clone());
                }
            }
        }
        None
    }

    /// Get type for a global variable.
    fn get_global_variable_type(&self, uri: &Url, name: &str) -> Option<RubyType> {
        let entries = self.index.file_entries(uri);

        for entry in entries {
            if let EntryKind::GlobalVariable(data) = &entry.kind {
                if &data.name == name && data.r#type != RubyType::Unknown {
                    return Some(data.r#type.clone());
                }
            }
        }
        None
    }
}
