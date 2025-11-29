//! # RBS Parser
//!
//! A parser for Ruby RBS (Ruby Signature) files using tree-sitter.
//!
//! RBS is a language to describe the structure of Ruby programs, including
//! classes, modules, methods, and their types.
//!
//! ## Example
//!
//! ```rust
//! use rbs_parser::Parser;
//!
//! let source = r#"
//! class String
//!   def length: () -> Integer
//!   def upcase: () -> String
//! end
//! "#;
//!
//! let mut parser = Parser::new();
//! let declarations = parser.parse(source).unwrap();
//!
//! // Access parsed declarations
//! for decl in &declarations {
//!     println!("{:?}", decl);
//! }
//! ```

mod converter;
mod embedded;
mod loader;
mod parser;
mod types;
mod visitor;

pub use converter::{
    get_base_class_name, is_nilable, rbs_type_to_string, rbs_type_to_yard, unwrap_nilable,
};
pub use embedded::{core_file_count, stdlib_file_count};
pub use loader::{LoadError, Loader};
pub use parser::Parser;
pub use types::*;

/// Parse RBS source code and return declarations
pub fn parse(source: &str) -> Result<Vec<Declaration>, ParseError> {
    let mut parser = Parser::new();
    parser.parse(source)
}

/// Parse a single type expression from a string
pub fn parse_type(source: &str) -> Result<RbsType, ParseError> {
    let mut parser = Parser::new();
    parser.parse_type(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_class() {
        let source = r#"
class String
  def length: () -> Integer
end
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        let declarations = result.unwrap();
        assert_eq!(declarations.len(), 1);

        if let Declaration::Class(class) = &declarations[0] {
            assert_eq!(class.name, "String");
            assert_eq!(class.methods.len(), 1);
            assert_eq!(class.methods[0].name, "length");
        } else {
            panic!("Expected class declaration");
        }
    }

    #[test]
    fn test_parse_method_with_params() {
        let source = r#"
class String
  def []: (Integer index) -> String?
  def slice: (Integer start, ?Integer length) -> String?
end
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        let declarations = result.unwrap();
        if let Declaration::Class(class) = &declarations[0] {
            assert_eq!(class.methods.len(), 2);
        }
    }

    #[test]
    fn test_parse_generic_class() {
        let source = r#"
class Array[Elem]
  def first: () -> Elem?
  def push: (Elem item) -> self
end
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        let declarations = result.unwrap();
        if let Declaration::Class(class) = &declarations[0] {
            assert_eq!(class.name, "Array");
            assert_eq!(class.type_params.len(), 1);
            assert_eq!(class.type_params[0].name, "Elem");
        }
    }

    #[test]
    fn test_parse_union_type() {
        let source = r#"
class Foo
  def bar: () -> (String | Integer | nil)
end
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
    }

    #[test]
    fn test_parse_module() {
        let source = r#"
module Enumerable[Elem]
  def map: [U] () { (Elem) -> U } -> Array[U]
end
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        let declarations = result.unwrap();
        if let Declaration::Module(module) = &declarations[0] {
            assert_eq!(module.name, "Enumerable");
        }
    }

    #[test]
    fn test_parse_interface() {
        let source = r#"
interface _ToS
  def to_s: () -> String
end
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
    }

    #[test]
    fn test_parse_type_alias() {
        let source = r#"
type json = String | Integer | Float | bool | nil | Array[json] | Hash[String, json]
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());
    }

    #[test]
    fn test_load_bundled_rbs_files() {
        // Load bundled core types
        let loader = Loader::with_core_types();
        assert!(
            loader.is_ok(),
            "Failed to load bundled RBS files: {:?}",
            loader.err()
        );

        let loader = loader.unwrap();
        println!("Total declarations: {}", loader.declaration_count());
        println!("Total methods: {}", loader.method_count());

        // Check that we can look up String class
        let string_class = loader.get_class("String");
        assert!(string_class.is_some(), "String class should be loaded");

        if let Some(class) = string_class {
            println!("String class has {} methods", class.methods.len());
            assert!(!class.methods.is_empty(), "String should have methods");
        }

        // Check that we can look up String#length
        let length_method = loader.get_instance_method("String", "length");
        assert!(length_method.is_some(), "String#length should be available");

        if let Some(method) = length_method {
            println!("String#length return type: {:?}", method.return_type());
        }
    }

    #[test]
    fn test_parse_real_string_rbs() {
        // A simplified version of string.rbs to test our parser
        let source = r#"
class String
  def self.try_convert: (String object) -> String

  def initialize: (?string source, ?encoding: encoding, ?capacity: int) -> void

  def %: (array[untyped] positional_args) -> String

  def *: (int amount) -> String

  def +: (string other_string) -> String

  def <<: (string | Integer str_or_codepoint) -> self

  def <=>: (string) -> (-1 | 0 | 1)

  def ==: (untyped other) -> bool

  def =~: (Regexp regex) -> Integer?

  def []: (int start, ?int length) -> String?

  def ascii_only?: () -> bool

  def bytes: () -> Array[Integer]

  def bytesize: () -> Integer

  def capitalize: () -> String

  def capitalize!: () -> self?

  def chars: () -> Array[String]

  def chomp: (?string? separator) -> String

  def clear: () -> self

  def downcase: () -> String

  def each_byte: () -> Enumerator[Integer, self]
                | () { (Integer byte) -> void } -> self

  def each_char: () -> Enumerator[String, self]
               | () { (String char) -> void } -> self

  def empty?: () -> bool

  def encode: (?encoding dst_encoding) -> String

  def encoding: () -> Encoding

  def end_with?: (*string suffixes) -> bool

  def gsub: (Regexp | string pattern, string replacement) -> String
          | (Regexp | string pattern) { (String match) -> string } -> String
          | (Regexp | string pattern) -> Enumerator[String, String]

  def include?: (string other_string) -> bool

  def index: (Regexp | string pattern, ?int offset) -> Integer?

  def inspect: () -> String

  def length: () -> Integer

  def lines: (?string separator, ?chomp: bool) -> Array[String]

  def match: (Regexp | string pattern, ?int offset) -> MatchData?

  def replace: (string other) -> self

  def reverse: () -> String

  def size: () -> Integer

  def split: (?Regexp | string pattern, ?int limit) -> Array[String]

  def start_with?: (*string prefixes) -> bool

  def strip: () -> String

  def sub: (Regexp | string pattern, string replacement) -> String

  def to_i: (?int base) -> Integer

  def to_s: () -> self

  def to_str: () -> self

  def to_sym: () -> Symbol

  def upcase: () -> String
            | (:ascii | :lithuanian | :turkic) -> String
            | (:lithuanian, :turkic) -> String
            | (:turkic, :lithuanian) -> String

  def upcase!: () -> self?

  def valid_encoding?: () -> bool
end
"#;
        let result = parse(source);
        assert!(result.is_ok(), "Failed to parse: {:?}", result.err());

        let declarations = result.unwrap();
        assert_eq!(declarations.len(), 1);

        let class = declarations.first().unwrap();
        if let Declaration::Class(class) = class {
            let upcase_method = class.methods.iter().find(|m| m.name == "upcase");
            assert!(upcase_method.is_some(), "upcase method should exist");
            let upcase_method = upcase_method.unwrap();
            println!("upcase has {} overloads", upcase_method.overloads.len());
            for (i, overload) in upcase_method.overloads.iter().enumerate() {
                println!("  Overload {}: {:?}", i, overload.return_type);
            }
            // Check that upcase returns String, not self?
            assert!(
                matches!(upcase_method.return_type(), Some(RbsType::Class(name)) if name == "String"),
                "upcase should return String, got {:?}",
                upcase_method.return_type()
            );
        }

        if let Declaration::Class(class) = &declarations[0] {
            assert_eq!(class.name, "String");
            println!("Parsed String class with {} methods", class.methods.len());

            // Verify some specific methods
            let method_names: Vec<&str> = class.methods.iter().map(|m| m.name.as_str()).collect();
            assert!(
                method_names.contains(&"length"),
                "Should have length method"
            );
            assert!(
                method_names.contains(&"upcase"),
                "Should have upcase method"
            );
            assert!(method_names.contains(&"split"), "Should have split method");
        }
    }
}
