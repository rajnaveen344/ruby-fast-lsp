//! Embedded RBS type definitions.
//!
//! This module provides access to RBS type definitions that are embedded
//! directly in the binary at compile time. This ensures the LSP works
//! without needing external RBS files.

// Include the generated code from build.rs
include!(concat!(env!("OUT_DIR"), "/embedded_rbs.rs"));

/// Get all embedded core RBS files as (name, content) pairs
pub fn core_rbs_files() -> impl Iterator<Item = (&'static str, &'static str)> {
    CORE_RBS_FILES.iter().filter_map(|(name, bytes)| {
        std::str::from_utf8(bytes)
            .ok()
            .map(|content| (*name, content))
    })
}

/// Get all embedded stdlib RBS files as (name, content) pairs
pub fn stdlib_rbs_files() -> impl Iterator<Item = (&'static str, &'static str)> {
    STDLIB_RBS_FILES.iter().filter_map(|(name, bytes)| {
        std::str::from_utf8(bytes)
            .ok()
            .map(|content| (*name, content))
    })
}

/// Get the number of embedded core files
pub fn core_file_count() -> usize {
    CORE_RBS_FILES.len()
}

/// Get the number of embedded stdlib files
pub fn stdlib_file_count() -> usize {
    STDLIB_RBS_FILES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_files_embedded() {
        let files: Vec<_> = core_rbs_files().collect();
        assert!(!files.is_empty(), "Core RBS files should be embedded");
        println!("Embedded {} core RBS files", files.len());

        // Check for essential files
        let file_names: Vec<&str> = files.iter().map(|(name, _)| *name).collect();
        assert!(
            file_names.iter().any(|n| n.contains("string")),
            "string.rbs should be embedded"
        );
        assert!(
            file_names.iter().any(|n| n.contains("integer")),
            "integer.rbs should be embedded"
        );
        assert!(
            file_names.iter().any(|n| n.contains("array")),
            "array.rbs should be embedded"
        );
    }

    #[test]
    fn test_stdlib_files_embedded() {
        let files: Vec<_> = stdlib_rbs_files().collect();
        println!("Embedded {} stdlib RBS files", files.len());
        // stdlib might be empty or have files, both are ok
    }

    #[test]
    fn test_string_rbs_content() {
        let files: Vec<_> = core_rbs_files().collect();
        let string_rbs = files.iter().find(|(name, _)| name.contains("string.rbs"));
        assert!(string_rbs.is_some(), "string.rbs should be embedded");

        let (_, content) = string_rbs.unwrap();
        assert!(
            content.contains("class String"),
            "string.rbs should define String class"
        );
        assert!(
            content.contains("def length"),
            "string.rbs should have length method"
        );
    }

    #[test]
    fn test_string_upcase_definition() {
        let files: Vec<_> = core_rbs_files().collect();
        let string_rbs = files.iter().find(|(name, _)| name.contains("string.rbs"));
        assert!(string_rbs.is_some(), "string.rbs should be embedded");

        let (_, content) = string_rbs.unwrap();

        // Find the upcase definition
        if let Some(pos) = content.find("def upcase:") {
            let end = content[pos..]
                .find("\n\n")
                .map(|e| pos + e)
                .unwrap_or(content.len());
            let upcase_def = &content[pos..end];
            println!("upcase definition:\n{}", upcase_def);

            // Verify it returns String, not self?
            assert!(
                upcase_def.contains("-> String"),
                "upcase should return String, got:\n{}",
                upcase_def
            );
        } else {
            panic!("upcase method not found in string.rbs");
        }
    }

    #[test]
    fn test_parse_embedded_string_upcase() {
        use crate::Declaration;
        use crate::Parser;

        let files: Vec<_> = core_rbs_files().collect();
        let string_rbs = files.iter().find(|(name, _)| name.contains("string.rbs"));
        assert!(string_rbs.is_some(), "string.rbs should be embedded");

        let (name, content) = string_rbs.unwrap();
        println!("Parsing {} ({} bytes)", name, content.len());

        let mut parser = Parser::new();
        let result = parser.parse(content);
        assert!(
            result.is_ok(),
            "Failed to parse string.rbs: {:?}",
            result.err()
        );

        let declarations = result.unwrap();
        println!("Parsed {} declarations", declarations.len());

        // Find the String class
        let string_class = declarations.iter().find(|d| {
            if let Declaration::Class(c) = d {
                c.name == "String"
            } else {
                false
            }
        });
        assert!(string_class.is_some(), "String class should be parsed");

        if let Declaration::Class(class) = string_class.unwrap() {
            println!("String class has {} methods", class.methods.len());

            // List all methods containing "upcase"
            println!("\nMethods containing 'upcase':");
            for method in class.methods.iter().filter(|m| m.name.contains("upcase")) {
                println!("  {} has {} overloads", method.name, method.overloads.len());
                for (i, overload) in method.overloads.iter().enumerate() {
                    println!("    Overload {}: {:?}", i, overload.return_type);
                }
            }

            let upcase_method = class.methods.iter().find(|m| m.name == "upcase");
            assert!(upcase_method.is_some(), "upcase method should exist");

            let upcase_method = upcase_method.unwrap();
            println!("\nupcase has {} overloads", upcase_method.overloads.len());
            for (i, overload) in upcase_method.overloads.iter().enumerate() {
                println!("  Overload {}: {:?}", i, overload.return_type);
            }

            // Check that upcase returns String
            let return_type = upcase_method.return_type();
            println!("upcase return_type(): {:?}", return_type);
        }
    }
}
