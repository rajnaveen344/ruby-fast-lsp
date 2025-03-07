mod context;
/// # Ruby Analyzer Module
///
/// The analyzer module provides functionality for analyzing Ruby code,
/// including identifier resolution, determining code context,
/// and parsing Ruby syntax.
///
/// ## Main Components
///
/// - `RubyAnalyzer`: The central type that provides methods for code analysis
/// - Position utilities: Functions for converting between different position representations
///
/// ## Example Usage
///
/// ```rust
/// use lsp_types::Position;
/// use ruby_fast_lsp::analyzer::RubyAnalyzer;
///
/// let mut analyzer = RubyAnalyzer::new();
/// let ruby_code = "class User\n  def initialize(name)\n    @name = name\n  end\nend";
/// let position = Position::new(2, 5); // Line 2, column 5 (at @name)
///
/// if let Some(identifier) = analyzer.find_identifier_at_position(ruby_code, position) {
///     println!("Found identifier: {}", identifier);
/// }
/// ```
// Module declarations
mod core;
mod identifier;
mod position;

// Re-exports
pub use self::core::RubyAnalyzer;
pub use self::position::{
    get_line_starts, offset_to_position, position_to_offset, position_to_point,
};
