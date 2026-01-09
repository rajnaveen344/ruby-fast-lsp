//! Hover Query - Query methods for hover information
//!
//! Provides query methods that can be used by capabilities/hover.rs
//! to get type information for various constructs.

use crate::indexer::entry::entry_kind::EntryKind;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use tower_lsp::lsp_types::{Range, Url};

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

impl IndexQuery<'_> {
    /// Get hover info for a constant (class or module).
    pub fn get_constant_hover_info(&self, constant_path: &[RubyConstant]) -> Option<HoverInfo> {
        let fqn = FullyQualifiedName::namespace(constant_path.to_vec());
        let entries = self.index.get(&fqn)?;

        // Find if it's a class or module
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
    pub fn get_instance_variable_hover_info(&self, uri: &Url, name: &str) -> Option<HoverInfo> {
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
    pub fn get_class_variable_hover_info(&self, uri: &Url, name: &str) -> Option<HoverInfo> {
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
    pub fn get_global_variable_hover_info(&self, uri: &Url, name: &str) -> Option<HoverInfo> {
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

    /// Get type for an instance variable.
    pub fn get_instance_variable_type(&self, uri: &Url, name: &str) -> Option<RubyType> {
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
    pub fn get_class_variable_type(&self, uri: &Url, name: &str) -> Option<RubyType> {
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
    pub fn get_global_variable_type(&self, uri: &Url, name: &str) -> Option<RubyType> {
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

    /// Check if a FQN is a module (not a class).
    pub fn is_module(&self, fqn: &FullyQualifiedName) -> bool {
        self.index
            .get(fqn)
            .map(|entries| {
                entries
                    .iter()
                    .any(|e| matches!(e.kind, EntryKind::Module(_)))
            })
            .unwrap_or(false)
    }

    /// Check if a FQN is a class.
    pub fn is_class(&self, fqn: &FullyQualifiedName) -> bool {
        self.index
            .get(fqn)
            .map(|entries| {
                entries
                    .iter()
                    .any(|e| matches!(e.kind, EntryKind::Class(_)))
            })
            .unwrap_or(false)
    }

    /// Get hover info for a method call.
    pub fn get_call_node_hover_info(
        &self,
        receiver: &crate::analyzer_prism::MethodReceiver,
        method_name: &str,
        ancestors: &[RubyConstant],
        _type_narrowing: Option<&crate::inferrer::TypeNarrowingEngine>,
        _uri: &Url,
        _position: tower_lsp::lsp_types::Position,
        _content: &str,
    ) -> Option<HoverInfo> {
        use crate::analyzer_prism::MethodReceiver;

        match receiver {
            MethodReceiver::Constant(_path) => {
                // Future TODO: Implement class method hover lookup
                None
            }
            MethodReceiver::None | MethodReceiver::SelfReceiver => {
                // Implicit receiver: use our new helper
                self.get_method_info_for_implicit_receiver(method_name, ancestors)
                    .map(|info| {
                        let content = if let Some(doc) = &info.documentation {
                            format!("**{}**\n\n{}", info.fqn, doc)
                        } else {
                            format!("**{}**", info.fqn)
                        };
                        let ty = info.return_type.unwrap_or(RubyType::Unknown);
                        HoverInfo::with_type(content, ty)
                    })
            }
            _ => {
                // Future TODO: Implement resolved type receiver hover
                None
            }
        }
    }
}
