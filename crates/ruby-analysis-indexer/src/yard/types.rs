//! YARD Type Definitions
//!
//! Contains the data structures for representing parsed YARD documentation.

use std::fmt::{self, Display, Formatter};
use tower_lsp::lsp_types::Range;

/// Represents a YARD parameter documentation
#[derive(Debug, Clone, PartialEq)]
pub struct YardParam {
    /// Parameter name
    pub name: String,
    /// Parameter types (can be multiple for union types)
    pub types: Vec<String>,
    /// Description of the parameter
    pub description: Option<String>,
    /// The range of the @param tag in the source (for diagnostics)
    /// This covers the entire @param line
    pub range: Option<Range>,
    /// The range of just the [Type] portion (for type-specific diagnostics)
    /// Used to highlight only the type when it's unknown
    pub types_range: Option<Range>,
}

impl YardParam {
    /// Create a new YARD parameter
    pub fn new(name: String, types: Vec<String>, description: Option<String>) -> Self {
        Self {
            name,
            types,
            description,
            range: None,
            types_range: None,
        }
    }

    /// Create a new YARD parameter with range information
    pub fn with_range(
        name: String,
        types: Vec<String>,
        description: Option<String>,
        range: Range,
    ) -> Self {
        Self {
            name,
            types,
            description,
            range: Some(range),
            types_range: None,
        }
    }

    /// Create a new YARD parameter with both line range and types range
    pub fn with_ranges(
        name: String,
        types: Vec<String>,
        description: Option<String>,
        range: Range,
        types_range: Range,
    ) -> Self {
        Self {
            name,
            types,
            description,
            range: Some(range),
            types_range: Some(types_range),
        }
    }

    /// Format the type as a string for display (e.g., "String" or "(String | Integer)")
    pub fn format_type(&self) -> Option<String> {
        if self.types.is_empty() {
            None
        } else if self.types.len() == 1 {
            Some(self.types[0].clone())
        } else {
            Some(format!("({})", self.types.join(" | ")))
        }
    }
}

impl Display for YardParam {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.types.is_empty() {
            write!(f, "{}", self.name)
        } else if self.types.len() == 1 {
            write!(f, "{}: {}", self.name, self.types[0])
        } else {
            write!(f, "{}: ({})", self.name, self.types.join(" | "))
        }
    }
}

/// Represents a YARD return type documentation
#[derive(Debug, Clone, PartialEq)]
pub struct YardReturn {
    /// Return types (can be multiple for union types)
    pub types: Vec<String>,
    /// Description of the return value
    pub description: Option<String>,
    /// The range of the @return tag in the source (for diagnostics)
    pub range: Option<Range>,
    /// The range of just the [Type] portion (for type-specific diagnostics)
    pub types_range: Option<Range>,
}

impl YardReturn {
    /// Create a new YARD return type
    pub fn new(types: Vec<String>, description: Option<String>) -> Self {
        Self {
            types,
            description,
            range: None,
            types_range: None,
        }
    }

    /// Create a new YARD return type with ranges
    pub fn with_ranges(
        types: Vec<String>,
        description: Option<String>,
        range: Range,
        types_range: Range,
    ) -> Self {
        Self {
            types,
            description,
            range: Some(range),
            types_range: Some(types_range),
        }
    }
}

impl Display for YardReturn {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.types.is_empty() {
            write!(f, "void")
        } else if self.types.len() == 1 {
            write!(f, "{}", self.types[0])
        } else {
            write!(f, "({})", self.types.join(" | "))
        }
    }
}

/// Represents a YARD @option tag for hash options
/// Format: @option hash_name [Type] :key_name (default) description
#[derive(Debug, Clone, PartialEq)]
pub struct YardOption {
    /// The parameter name this option belongs to (e.g., "opts" in @option opts)
    pub param_name: String,
    /// The option key name (without the colon)
    pub key_name: String,
    /// Option types (can be multiple for union types)
    pub types: Vec<String>,
    /// Default value (if specified in parentheses)
    pub default: Option<String>,
    /// Description of the option
    pub description: Option<String>,
    /// The range of the @option tag in the source (for diagnostics)
    pub range: Option<Range>,
}

impl YardOption {
    /// Create a new YARD option
    pub fn new(
        param_name: String,
        key_name: String,
        types: Vec<String>,
        default: Option<String>,
        description: Option<String>,
    ) -> Self {
        Self {
            param_name,
            key_name,
            types,
            default,
            description,
            range: None,
        }
    }

    /// Create a new YARD option with range information
    pub fn with_range(
        param_name: String,
        key_name: String,
        types: Vec<String>,
        default: Option<String>,
        description: Option<String>,
        range: Range,
    ) -> Self {
        Self {
            param_name,
            key_name,
            types,
            default,
            description,
            range: Some(range),
        }
    }

    /// Format the type as a string for display
    pub fn format_type(&self) -> Option<String> {
        if self.types.is_empty() {
            None
        } else if self.types.len() == 1 {
            Some(self.types[0].clone())
        } else {
            Some(format!("({})", self.types.join(" | ")))
        }
    }
}

impl Display for YardOption {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, ":{}", self.key_name)?;
        if !self.types.is_empty() {
            if self.types.len() == 1 {
                write!(f, ": {}", self.types[0])?;
            } else {
                write!(f, ": ({})", self.types.join(" | "))?;
            }
        }
        if let Some(default) = &self.default {
            write!(f, " = {}", default)?;
        }
        Ok(())
    }
}

/// Represents complete YARD documentation for a method
#[derive(Debug, Clone, PartialEq, Default)]
pub struct YardMethodDoc {
    /// Method description
    pub description: Option<String>,
    /// Parameter documentation
    pub params: Vec<YardParam>,
    /// Option documentation (for hash options via @option tag)
    /// These describe valid keys for a Hash parameter
    pub options: Vec<YardOption>,
    /// Return type documentation
    pub returns: Vec<YardReturn>,
    /// Yield parameter documentation (for blocks)
    /// Note: Currently parsed but not displayed in inlay hints.
    /// Reserved for future hover/completion support.
    pub yield_params: Vec<YardParam>,
    /// Yield return documentation
    /// Note: Currently parsed but not displayed in inlay hints.
    /// Reserved for future hover/completion support.
    pub yield_returns: Vec<YardReturn>,
    /// Exception types this method may raise
    /// Note: Currently parsed but not displayed.
    /// Reserved for future diagnostics/hover support.
    pub raises: Vec<String>,
    /// Deprecation message if method is deprecated
    /// Note: Currently parsed but not displayed.
    /// Reserved for future diagnostics support.
    pub deprecated: Option<String>,
}

impl YardMethodDoc {
    /// Create a new empty YARD method documentation
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this documentation has any type information
    pub fn has_type_info(&self) -> bool {
        !self.params.is_empty() || !self.returns.is_empty() || !self.options.is_empty()
    }

    /// Get parameter type string by name for inlay hints
    pub fn get_param_type_str(&self, name: &str) -> Option<String> {
        self.params.iter().find(|p| p.name == name)?.format_type()
    }

    /// Get options for a specific parameter (hash)
    pub fn get_options_for_param(&self, param_name: &str) -> Vec<&YardOption> {
        self.options
            .iter()
            .filter(|o| o.param_name == param_name)
            .collect()
    }

    /// Get the formatted return type string for inlay hints
    /// Returns None if no return type is documented
    pub fn format_return_type(&self) -> Option<String> {
        if self.returns.is_empty() {
            return None;
        }

        // Collect all types from all @return tags
        let all_types: Vec<&String> = self.returns.iter().flat_map(|r| &r.types).collect();

        if all_types.is_empty() {
            return None;
        }

        Some(if all_types.len() == 1 {
            all_types[0].clone()
        } else {
            format!(
                "({})",
                all_types
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            )
        })
    }

    /// Get the return description (from first @return tag)
    pub fn get_return_description(&self) -> Option<&String> {
        self.returns.first().and_then(|r| r.description.as_ref())
    }

    /// Format as a method signature hint (for inlay hints)
    /// Returns something like "(name: String, age: Integer) -> String"
    pub fn format_signature_hint(&self) -> String {
        let params_str = if self.params.is_empty() {
            String::new()
        } else {
            self.params
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };

        let return_str = self
            .format_return_type()
            .map(|t| format!(" -> {}", t))
            .unwrap_or_default();

        if params_str.is_empty() && return_str.is_empty() {
            String::new()
        } else if params_str.is_empty() {
            return_str
        } else {
            format!("({}){}", params_str, return_str)
        }
    }

    /// Find YARD @param tags that don't match any actual method parameter
    /// Returns a list of (param_name, range) for each unmatched @param
    pub fn find_unmatched_params(&self, actual_param_names: &[&str]) -> Vec<(&YardParam, Range)> {
        self.params
            .iter()
            .filter_map(|yard_param| {
                if !actual_param_names.contains(&yard_param.name.as_str()) {
                    yard_param.range.map(|range| (yard_param, range))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Find a parameter by name
    pub fn find_param(&self, name: &str) -> Option<&YardParam> {
        self.params.iter().find(|p| p.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yard_param_format_type() {
        let param = YardParam::new("name".to_string(), vec!["String".to_string()], None);
        assert_eq!(param.format_type(), Some("String".to_string()));

        let param = YardParam::new(
            "value".to_string(),
            vec!["String".to_string(), "Integer".to_string()],
            None,
        );
        assert_eq!(param.format_type(), Some("(String | Integer)".to_string()));

        let param = YardParam::new("empty".to_string(), vec![], None);
        assert_eq!(param.format_type(), None);
    }

    #[test]
    fn test_yard_method_doc_format_return_type() {
        let mut doc = YardMethodDoc::new();
        assert_eq!(doc.format_return_type(), None);

        doc.returns
            .push(YardReturn::new(vec!["String".to_string()], None));
        assert_eq!(doc.format_return_type(), Some("String".to_string()));

        // Multiple types in single @return
        let mut doc2 = YardMethodDoc::new();
        doc2.returns.push(YardReturn::new(
            vec!["String".to_string(), "nil".to_string()],
            None,
        ));
        assert_eq!(
            doc2.format_return_type(),
            Some("(String | nil)".to_string())
        );

        // Multiple @return tags
        let mut doc3 = YardMethodDoc::new();
        doc3.returns
            .push(YardReturn::new(vec!["String".to_string()], None));
        doc3.returns
            .push(YardReturn::new(vec!["Integer".to_string()], None));
        assert_eq!(
            doc3.format_return_type(),
            Some("(String | Integer)".to_string())
        );
    }

    #[test]
    fn test_yard_method_doc_format_signature() {
        let mut doc = YardMethodDoc::new();
        doc.params.push(YardParam::new(
            "name".to_string(),
            vec!["String".to_string()],
            None,
        ));
        doc.params.push(YardParam::new(
            "age".to_string(),
            vec!["Integer".to_string()],
            None,
        ));
        doc.returns
            .push(YardReturn::new(vec!["Boolean".to_string()], None));

        let hint = doc.format_signature_hint();
        assert_eq!(hint, "(name: String, age: Integer) -> Boolean");
    }

    #[test]
    fn test_yard_method_doc_empty() {
        let doc = YardMethodDoc::new();
        assert!(!doc.has_type_info());
        assert_eq!(doc.format_signature_hint(), "");
    }

    #[test]
    fn test_get_param_type_str() {
        let mut doc = YardMethodDoc::new();
        doc.params.push(YardParam::new(
            "name".to_string(),
            vec!["String".to_string()],
            None,
        ));
        doc.params.push(YardParam::new(
            "value".to_string(),
            vec!["Integer".to_string(), "nil".to_string()],
            None,
        ));

        assert_eq!(doc.get_param_type_str("name"), Some("String".to_string()));
        assert_eq!(
            doc.get_param_type_str("value"),
            Some("(Integer | nil)".to_string())
        );
        assert_eq!(doc.get_param_type_str("unknown"), None);
    }
}
