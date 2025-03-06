# Ruby Analyzer

The analyzer component is responsible for analyzing Ruby source code to provide semantic information for the Language Server Protocol (LSP) features. It works with the Tree-sitter parser to extract meaningful information from Ruby code at specific positions, enabling features like go-to-definition, hover information, and code completion.

## Key Components

### RubyAnalyzer

The `RubyAnalyzer` struct is the main entry point for code analysis. It uses Tree-sitter to parse Ruby code and provides methods to extract semantic information. Key methods include:

- `find_identifier_at_position`: Finds the identifier at a given position in the document
- `find_node_at_point`: Locates a specific node in the AST at a given point
- `determine_fully_qualified_name`: Determines the fully qualified name of a node
- `determine_method_call_fqn`: Determines the fully qualified name of a method call

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

    // Check if the point is within this node's range
    if !node_contains_point(node, point) {
        return None;
    }

    // Check if any children contain the point
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if node_contains_point(child, point) {
                cursor.reset(child);
                if let Some(found) = self.find_node_at_point(cursor, point) {
                    return Some(found);
                }
            }
        }
    }

    // If no children contain the point, return this node
    Some(node)
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

### Context Resolution

When analyzing a node, the analyzer determines its context by traversing up the AST:

```rust
fn get_surrounding_context(&self, node: Node) -> (Option<String>, Option<String>) {
    let mut current = node;
    let mut class_name = None;
    let mut method_name = None;

    while let Some(parent) = current.parent() {
        match parent.kind() {
            "class" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    class_name = Some(self.get_node_text(name_node));
                    break;
                }
            },
            "method" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    method_name = Some(self.get_node_text(name_node));
                }
            },
            // Handle other node types...
            _ => {}
        }
        current = parent;
    }

    (class_name, method_name)
}
```

### Method Call Resolution

For method calls, the analyzer needs to determine the target and method name:

```rust
fn determine_method_call_fqn(&self, node: Node, position: Position) -> String {
    // Find receiver and method name
    let receiver = node.child_by_field_name("receiver");
    let method = node.child_by_field_name("method");

    if let Some(method_node) = method {
        let method_name = self.get_node_text(method_node);

        if let Some(receiver_node) = receiver {
            // Determine the receiver's type/class
            let receiver_type = self.determine_node_type(receiver_node);
            return format!("{}#{}", receiver_type, method_name);
        } else {
            // No explicit receiver, use context
            let (class_context, _) = self.get_surrounding_context(node);
            if let Some(class) = class_context {
                return format!("{}#{}", class, method_name);
            } else {
                return method_name;
            }
        }
    }

    // Fallback
    self.get_node_text(node)
}
```

### Handling Ruby's Special Methods

Ruby has special methods like attribute accessors that require special handling:

```rust
fn is_attribute_accessor(&self, method_name: &str, class_name: &str) -> bool {
    // Check if this is a getter/setter created by attr_accessor
    if let Some(entries) = self.index.methods_by_name.get(method_name) {
        for entry in entries {
            if entry.fully_qualified_name.starts_with(&format!("{}#", class_name)) {
                return true;
            }
        }
    }
    false
}
```

## Debugging Strategies

When debugging analyzer issues, consider these techniques:

### Position Debugging

Verify that positions are correctly translated between LSP and Tree-sitter:

```rust
fn debug_position(&self, position: Position, point: Point) {
    println!("LSP Position: line={}, character={}", position.line, position.character);
    println!("Tree-sitter Point: row={}, column={}", point.row, point.column);
}
```

### Node Path Tracing

Trace the path from a node up to the root to understand its context:

```rust
fn trace_node_path(&self, node: Node) {
    let mut current = node;
    let mut path = vec![format!("{}({})", current.kind(), self.get_node_text(current))];

    while let Some(parent) = current.parent() {
        path.push(format!("{}({})", parent.kind(), self.get_node_text(parent)));
        current = parent;
    }

    println!("Node path (leaf to root): {}", path.join(" -> "));
}
```

### Field Availability Check

Check if expected fields are available on nodes:

```rust
fn check_fields(&self, node: Node) {
    let fields = ["name", "receiver", "method", "arguments", "body"];
    for field in fields {
        if let Some(field_node) = node.child_by_field_name(field) {
            println!("Field '{}' exists: {}", field, self.get_node_text(field_node));
        } else {
            println!("Field '{}' does not exist", field);
        }
    }
}
```

## Common Pitfalls

1. **Node Range Overlap**: Multiple nodes can overlap a position. Always check for the most specific node.

2. **Method vs Property Access**: Ruby's syntax doesn't distinguish between method calls without parentheses and property access. The analyzer must handle this ambiguity.

3. **Incomplete AST Information**: Sometimes the necessary information is not directly available in the AST and must be inferred from context.

4. **Position Off-by-One Errors**: LSP positions and Tree-sitter points may have subtle differences. Be careful with conversions.

5. **Ruby's Dynamic Nature**: Ruby's highly dynamic nature means that precise static analysis is challenging. The analyzer can only provide best-effort results.

## Code Examples

### Example: Finding Constant References

```rust
fn find_constant_references(&self, document: &str, constant_name: &str) -> Vec<Range> {
    let mut references = Vec::new();
    let tree = self.parser.parse(document, None).unwrap();
    let mut cursor = tree.root_node().walk();

    self.traverse_for_constants(&mut cursor, document, constant_name, &mut references);

    references
}

fn traverse_for_constants(&self, cursor: &mut TreeCursor, document: &str, target: &str, results: &mut Vec<Range>) {
    let node = cursor.node();

    if node.kind() == "constant" && self.get_node_text(node) == target {
        results.push(node_to_range(node));
    }

    // Traverse children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            cursor.reset(child);
            self.traverse_for_constants(cursor, document, target, results);
        }
    }
}
```

### Example: LSP to Tree-sitter Position Conversion

```rust
fn lsp_position_to_tree_sitter_point(position: Position) -> Point {
    Point {
        row: position.line as usize,
        column: position.character as usize,
    }
}

fn tree_sitter_point_to_lsp_position(point: Point) -> Position {
    Position {
        line: point.row as u32,
        character: point.column as u32,
    }
}
```

## Features

The analyzer supports several key features:

1. **Identifier Resolution**: Determines what identifier is at a given position
2. **Context Awareness**: Understands the context of code (class/module/method scope)
3. **Method Call Analysis**: Analyzes method calls to determine their targets
4. **Variable Scope Analysis**: Tracks variable scopes to provide accurate information

## Recent Updates

- Improved handling of method calls and identifier resolution
- Enhanced support for Ruby's attribute methods (`attr_accessor`, `attr_reader`, `attr_writer`)
- Fixed issues with position-to-node mapping for better cursor position handling

## Usage

The analyzer is used by the LSP server to provide semantic information about Ruby code. It's typically used to respond to requests like "go to definition" or "hover".

```rust
// Example usage
let mut analyzer = RubyAnalyzer::new();
if let Some(identifier) = analyzer.find_identifier_at_position(document, position) {
    // Use the identifier for further processing
    // e.g., look up in the index to find its definition
}
```

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
- Handling of complex Ruby classes and modules

Run the tests with:

```
cargo test analyzer
```
