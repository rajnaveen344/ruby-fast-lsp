# Ruby Prism Analyzer

This module provides Ruby code analysis capabilities using the [ruby-prism](https://docs.rs/ruby-prism/latest/ruby_prism/) parser, which is a Rust binding for the official Ruby Prism parser.

## Overview

The `analyzer_prism` module is designed to analyze Ruby code to provide language server features such as:

- Finding identifiers at a specific position
- Resolving fully qualified names for constants and namespaces
- Tracking namespace context
- Converting between LSP positions and Prism byte offsets

## Architecture

The analyzer follows a visitor pattern to traverse the Ruby AST:

```
analyzer_prism/
├── mod.rs                 # Main module entry point and RubyPrismAnalyzer implementation
├── position.rs            # Utilities for converting between LSP positions and Prism byte offsets
├── visitors/              # AST visitor implementations
│   ├── mod.rs             # Visitor module exports
│   └── identifier_visitor.rs # Visitor for finding identifiers at a position
└── README.md              # This file
```

### Main Components

#### RubyPrismAnalyzer

The main analyzer class that:
1. Parses Ruby code using the ruby-prism parser
2. Provides methods to analyze the code
3. Tracks namespace context
4. Finds identifiers at a given position

```rust
pub struct RubyPrismAnalyzer {
    code: String,
    parse_result: Option<ParseResult<'static>>,
    namespace_stack: Vec<RubyNamespace>,
}
```

#### Position Utilities

The `position.rs` module provides utilities for converting between LSP positions (line/column) and Prism byte offsets:

- `lsp_pos_to_prism_loc`: Converts an LSP position to a Prism byte offset
- `prism_offset_to_lsp_pos`: Converts a Prism byte offset to an LSP position
- `prism_loc_to_lsp_range`: Converts a Prism location to an LSP range

#### Visitors

The visitors implement the `Visit` trait from ruby-prism to traverse the AST and perform specific analyses:

- `IdentifierVisitor`: Finds identifiers at a specific position in the code

## How It Works

1. **Parsing**: The analyzer parses Ruby code using the ruby-prism parser, which produces an AST.

2. **AST Traversal**: Visitors traverse the AST to find nodes of interest.

3. **Position Mapping**: The analyzer maps between LSP positions (line/column) and Prism byte offsets.

4. **Identifier Resolution**: When finding identifiers, the analyzer:
   - Determines the node at the given position
   - Resolves the fully qualified name based on the node type and context
   - Tracks namespace context as it traverses through modules and classes

5. **Result**: The analyzer returns the fully qualified name and namespace context.

## Example Usage

```rust
// Create an analyzer with Ruby code
let analyzer = RubyPrismAnalyzer::new("module Foo; CONST = 1; end".to_string());

// Find the identifier at a specific position (line 0, column 10)
let position = Position::new(0, 10);
let (fqn_opt, namespaces) = analyzer.get_identifier(position);

// Process the result
if let Some(fqn) = fqn_opt {
    match fqn {
        FullyQualifiedName::Constant(ns, constant) => {
            println!("Found constant: {}", constant);
            println!("In namespace: {:?}", ns);
        }
        FullyQualifiedName::Namespace(ns) => {
            println!("Found namespace: {:?}", ns);
        }
        // Handle other cases...
    }
}
```

## Future Improvements

- Complete implementation of the `Visit` trait for `IdentifierVisitor`
- Add more visitors for different analysis tasks
- Improve position mapping for better precision
- Add support for method calls and other Ruby constructs
- Implement caching for better performance
