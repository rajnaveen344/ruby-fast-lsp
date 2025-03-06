# Ruby Indexer

The indexer component is responsible for parsing Ruby source code and building a searchable index of all Ruby entities (classes, modules, methods, constants, etc.) found in the codebase. This index is a critical part of the Ruby Fast LSP, enabling features like code navigation, autocompletion, and symbol search.

## Key Components

### RubyIndex

The `RubyIndex` struct is the core data structure that stores all indexed entities. It maintains several maps for efficient lookups:

- `entries`: Maps fully qualified names to entry objects
- `uri_to_entries`: Maps file URIs to their entries for efficient updates
- `methods_by_name`: Maps method names to their entries for quick method lookup
- `constants_by_name`: Maps constant names to their entries
- `namespace_tree`: Maintains the hierarchy of namespaces

### Entry

Each indexed entity is represented by an `Entry` struct with the following properties:

- `name`: The name of the entity
- `fully_qualified_name`: Complete namespace path (e.g., `Module::Class#method`)
- `location`: Where this entry is defined (file URI and range)
- `entry_type`: Type of entry (Class, Module, Method, Constant, etc.)
- `visibility`: Public, Protected, or Private (for methods)
- `metadata`: Additional information about the entry

### RubyIndexer

The `RubyIndexer` struct is responsible for traversing the Ruby AST and populating the index. It uses Tree-sitter to parse Ruby code and extract relevant information. Key methods include:

- `index_file`: Indexes a single Ruby file
- `process_file`: Processes a parsed AST to extract entities
- `traverse_node`: Recursively traverses the AST
- `process_class`, `process_module`, `process_method`: Process specific node types
- `process_attribute_methods`: Handles Ruby's `attr_accessor`, `attr_reader`, and `attr_writer` methods

## Tree-sitter Node Structure

The indexer relies heavily on the Tree-sitter AST structure for Ruby. Understanding this structure is crucial for working with the codebase:

### Common Node Types

- `class`: Represents a Ruby class definition
- `module`: Represents a Ruby module definition
- `method`: Represents a method definition
- `singleton_method`: Represents a class method definition
- `call`: Represents a method call (including `attr_*` calls)
- `identifier`: Represents variable or method names
- `constant`: Represents constant names
- `simple_symbol`: Represents symbol literals (e.g., `:name` in `attr_accessor :name`)
- `argument_list`: Represents method arguments

### Node Field Access

Tree-sitter nodes can be accessed in two primary ways:

1. By index: `node.child(i)` - Get the i-th child node
2. By field name: `node.child_by_field_name("field")` - Get a child by its field name

Common field names in Ruby nodes:
- `name`: The name of a class, module, or method
- `method`: The method name in a method call
- `arguments`: The arguments to a method call
- `body`: The body of a class, module, or method

Example of node structure for `attr_accessor :name, :age`:
```
call
├── method: identifier("attr_accessor")
└── arguments: argument_list
    ├── simple_symbol(":name")
    ├── ,
    └── simple_symbol(":age")
```

## Implementation Details

### AST Traversal

The indexer traverses the AST recursively using the `traverse_node` method. For each node, it:

1. Checks the node kind
2. Dispatches to an appropriate handler method based on the kind
3. Recursively processes child nodes

```rust
fn traverse_node(&mut self, node: Node, uri: &Url, source_code: &str, context: &mut TraversalContext) -> Result<(), String> {
    match node.kind() {
        "class" => self.process_class(node, uri, source_code, context)?,
        "module" => self.process_module(node, uri, source_code, context)?,
        "method" | "singleton_method" => self.process_method(node, uri, source_code, context)?,
        "call" => {
            self.process_attribute_methods(node, uri, source_code, context)?;
            // Process children...
        },
        // Other cases...
    }
    // ...
}
```

### Handling Ruby's Attribute Methods

The `process_attribute_methods` function handles Ruby's `attr_accessor`, `attr_reader`, and `attr_writer` methods. These methods dynamically generate getter and setter methods based on symbol arguments.

Here's how it works:

1. Check if the method name is one of the attribute methods
2. Extract symbol arguments from the call
3. For each symbol:
   - Create a getter method for `attr_accessor` and `attr_reader`
   - Create a setter method for `attr_accessor` and `attr_writer`
   - Add entries to the index

```rust
fn process_attribute_methods(&mut self, node: Node, uri: &Url, source_code: &str, context: &mut TraversalContext) -> Result<(), String> {
    if let Some(method_node) = node.child_by_field_name("method") {
        let method_name = self.get_node_text(method_node, source_code);

        // Only process attribute methods
        if method_name != "attr_accessor" && method_name != "attr_reader" && method_name != "attr_writer" {
            return Ok(());
        }

        if let Some(args_node) = node.child_by_field_name("arguments") {
            for i in 0..args_node.child_count() {
                if let Some(arg_node) = args_node.child(i) {
                    // Important: Symbol arguments are "simple_symbol", not "symbol"
                    if arg_node.kind() != "simple_symbol" {
                        continue;
                    }

                    // Process the symbol and create appropriate methods...
                }
            }
        }
    }
    Ok(())
}
```

### Key Index Updates

When an entry is added to the index, multiple maps are updated:

1. `entries` is updated with the fully qualified name
2. `uri_to_entries` is updated for the file
3. `methods_by_name` or `constants_by_name` is updated based on the entry type

This ensures efficient lookups by different criteria.

## Debugging Strategies

### Debugging AST Traversal

When debugging issues with the indexer, consider these techniques:

1. **Print Node Structure**: Add debug prints to understand the structure of nodes:
   ```rust
   println!("Node kind: {}, text: {}", node.kind(), self.get_node_text(node, source_code));
   for i in 0..node.child_count() {
       if let Some(child) = node.child(i) {
           println!("  Child {}: kind={}, text={}", i, child.kind(), self.get_node_text(child, source_code));
       }
   }
   ```

2. **Trace Field Access**: Check if fields are being found correctly:
   ```rust
   if let Some(field_node) = node.child_by_field_name("field_name") {
       println!("Found field: {}", self.get_node_text(field_node, source_code));
   } else {
       println!("Field not found");
   }
   ```

3. **Verify Index Updates**: Check if entries are actually added to the index:
   ```rust
   if let Some(entries) = self.index.methods_by_name.get(&method_name) {
       println!("Method {} has {} entries in the index", method_name, entries.len());
   } else {
       println!("Method {} not found in the index", method_name);
   }
   ```

## Common Pitfalls

1. **Incorrect Node Types**: Always verify the actual node types in the AST. For example, Ruby symbols in `attr_accessor` calls are represented as `simple_symbol` nodes, not `symbol` nodes.

2. **Missing Field Names**: Ensure you're using the correct field names when accessing nodes with `child_by_field_name`.

3. **Namespace Context**: Ruby entities need to be indexed with their correct namespace. Always check that the context's namespace stack is being properly maintained.

4. **Index Map Updates**: When adding entries to the index, ensure they're being added to all the relevant maps, especially the type-specific ones like `methods_by_name`.

5. **Handling of Commas and Whitespace**: When processing lists of items (like arguments), remember to skip non-relevant nodes like commas and whitespace.

## Before/After Fix Examples

### Example: Fixing Attribute Methods Indexing

Before:
```rust
// Incorrect: Checks for "symbol" nodes
if arg_node.kind() != "symbol" {
    continue;
}
```

After:
```rust
// Correct: Checks for "simple_symbol" nodes
if arg_node.kind() != "simple_symbol" {
    continue;
}
```

## Recent Updates

- Fixed indexing of Ruby's attribute methods (`attr_accessor`, `attr_reader`, `attr_writer`)
- The indexer now correctly identifies and indexes both getter and setter methods created by these attribute methods
- Fixed node type detection for symbol arguments in attribute method calls (using `simple_symbol` instead of `symbol`)

## Usage

The indexer is used by the LSP server to build and maintain an index of the Ruby codebase. It's typically initialized when the server starts and updated incrementally as files change.

```rust
// Example usage
let mut indexer = RubyIndexer::new()?;
indexer.index_file(file_path, source_code)?;

// Get the index
let index = indexer.index();

// Find a definition
if let Some(entry) = index.find_definition("Module::Class#method") {
    // Use the entry location
}
```

## Testing

The indexer includes comprehensive tests that verify its functionality, including:

- Indexing of classes, modules, methods, and constants
- Handling of nested namespaces
- Processing of attribute methods
- Removal of entries when files are deleted
- Finding definitions by fully qualified name

Run the tests with:

```
cargo test indexer
```
