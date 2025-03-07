# Ruby Analyzer

The analyzer component is responsible for analyzing Ruby source code to provide semantic information for the Language Server Protocol (LSP) features. It works with the Tree-sitter parser to extract meaningful information from Ruby code at specific positions, enabling features like go-to-definition, hover information, and code completion.

## Module Structure

After refactoring, the analyzer is organized into several focused modules:

```
src/analyzer/
├── mod.rs        - Main module that re-exports public components
├── core.rs       - Core RubyAnalyzer implementation with basic functionality
├── position.rs   - Position conversion utilities
├── identifier.rs - Identifier resolution logic
└── context.rs    - Code context determination
```

### Core Module (`core.rs`)

Contains the fundamental `RubyAnalyzer` struct and basic parsing functionality:
- Parser initialization
- Tree-sitter integration
- Node text extraction
- Node traversal utilities

### Position Module (`position.rs`)

Provides utilities for working with positions:
- Converting between LSP positions and Tree-sitter points
- Line/column to offset conversion functions
- Offset to position mapping

### Identifier Module (`identifier.rs`)

Handles identifier resolution:
- Finding identifiers at a specific position
- Determining fully qualified names of symbols
- Instance variable detection
- Method call resolution

### Context Module (`context.rs`)

Provides code context awareness:
- Finding the current method at a position
- Determining class/module context at a position
- Building namespace hierarchies
- Module and class detection

## Key Components

### RubyAnalyzer

The `RubyAnalyzer` struct is the main entry point for code analysis. It uses Tree-sitter to parse Ruby code and provides methods to extract semantic information. Key methods include:

- `find_identifier_at_position`: Finds the identifier at a given position in the document
- `find_node_at_point`: Locates a specific node in the AST at a given point
- `determine_fully_qualified_name`: Determines the fully qualified name of a node
- `find_current_context`: Determines the class/module context at a position
- `find_current_method`: Finds the method containing a position

### Position and Node Handling

The analyzer includes utilities for working with positions and nodes:

- Converting between LSP positions and Tree-sitter points
- Finding nodes at specific positions in the document
- Extracting text from nodes
- Determining the context of a node (e.g., whether it's in a class, module, or method)

## Tree-sitter Node Analysis

The analyzer heavily depends on Tree-sitter's node structure to understand the semantic meaning of Ruby code:

### Position to Node Mapping

One of the most critical operations is mapping a cursor position to the relevant node:

```rust
fn find_node_at_point<'a>(&self, cursor: &mut TreeCursor<'a>, point: Point) -> Option<Node<'a>> {
    let node = cursor.node();

    // Check if point is within node bounds
    if !is_point_within_node(point, node) {
        return None;
    }

    // First check if any of the children contain the point
    if cursor.goto_first_child() {
        loop {
            if let Some(matching_node) = self.find_node_at_point(cursor, point) {
                return Some(matching_node);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }

    // If no child contains the point, return this node
    Some(node)
}
```

### Context Determination

A significant improvement in the refactored code is the robust context determination logic:

```rust
pub fn find_current_context(&self, tree: &Tree, position: Position) -> String {
    let root_node = tree.root_node();

    // Get all modules and classes in the file
    let mut module_nodes = Vec::new();
    self.find_modules_and_classes(root_node, &mut module_nodes);

    // Build a context hierarchy
    let mut contexts = Vec::new();

    // For each module/class, check if it contains the position
    for node in &module_nodes {
        // Get the range of lines this module/class covers
        let start_line = node.start_position().row as u32;
        let end_line = node.end_position().row as u32;

        // Get the name of the module/class
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = self.get_node_text(name_node);

            // Check if the position is within this module/class
            if position.line >= start_line && position.line <= end_line {
                // Keep track of it along with its extent
                contexts.push((name, node.start_byte(), node.end_byte()));
            }
        }
    }

    // Sort and organize the context nodes to build the full context string
    // ...
}
```

### Node Type Recognition

Different node types require different handling:

- `identifier`: Variable or method names
- `constant`: Constant or class/module names
- `call`: Method invocations
- `method`: Method definitions
- `class`: Class definitions
- `module`: Module definitions

The analyzer must correctly identify these nodes to provide accurate information.

## Implementation Details

### Method Call Resolution

For method calls, the analyzer needs to determine the target and method name:

```rust
fn determine_method_call_fqn(&self, tree: &Tree, node: Node, position: Position) -> String {
    let method_node = if node.kind() == "call" {
        node.child_by_field_name("method")
    } else {
        // If the node itself is an identifier that's part of a call
        let parent = node.parent();
        if let Some(p) = parent {
            if p.kind() == "call" && p.child_by_field_name("method") == Some(node) {
                Some(node)
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some(method_node) = method_node {
        let method_name = self.get_node_text(method_node);
        // ...determine the method's fully qualified name
        method_name
    } else {
        String::new()
    }
}
```

## Common Pitfalls

1. **Node Range Overlap**: Multiple nodes can overlap a position. Always check for the most specific node.

2. **Method vs Property Access**: Ruby's syntax doesn't distinguish between method calls without parentheses and property access. The analyzer must handle this ambiguity.

3. **Incomplete AST Information**: Sometimes the necessary information is not directly available in the AST and must be inferred from context.

4. **Position Off-by-One Errors**: LSP positions and Tree-sitter points may have subtle differences. Be careful with conversions.

5. **Ruby's Dynamic Nature**: Ruby's highly dynamic nature means that precise static analysis is challenging. The analyzer can only provide best-effort results.

## Features

The analyzer supports several key features:

1. **Identifier Resolution**: Determines what identifier is at a given position
2. **Context Awareness**: Understands the context of code (class/module/method scope)
3. **Method Call Analysis**: Analyzes method calls to determine their targets
4. **Variable Scope Analysis**: Tracks variable scopes to provide accurate information

## Integration with Indexer

The analyzer works closely with the indexer component:

1. The analyzer identifies what entity the user is interacting with
2. The indexer provides information about where that entity is defined
3. Together, they enable features like go-to-definition and find-references

### Flow Between Components

```
User action (e.g., Ctrl+Click on method name)
  ↓
Analyzer.find_identifier_at_position()
  ↓
Analyzer determines fully qualified name (e.g., "Class#method")
  ↓
Server looks up the FQN in the Indexer
  ↓
Indexer.find_definition() returns location information
  ↓
Server returns location to IDE for navigation
```

## Testing

The analyzer includes tests that verify its functionality, including:

- Identifier extraction
- Method call identification
- Node text extraction
- Context determination
- Position conversion utilities

Run the tests with:

```
cargo test analyzer
```
