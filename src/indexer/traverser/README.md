# Ruby Indexer Traverser

This module contains the core traversal and indexing logic for the Ruby Fast LSP.

## Overview

The traverser is responsible for parsing Ruby code using Ruby Prism and building an index of all Ruby language elements: classes, modules, methods, constants, variables, and other symbols. This index is then used to power various Language Server Protocol (LSP) features like:

- Go to Definition
- Find References
- Completion
- Document Symbols
- Semantic Highlighting
- Rename Symbol
- Hover Documentation

## Architecture

The traverser is organized into a modular system where specific node types are processed by dedicated modules:

- `mod.rs`: The core coordinator that dispatches processing to specialized node handlers
- `class_node.rs`: Processes class declarations and their members
- `module_node.rs`: Processes module declarations
- `method_node.rs`: Processes method definitions and their parameters
- `constant_node.rs`: Handles constants and their references
- `variable_node.rs`: Processes local, instance, and class variables
- `call_node.rs`: Handles method calls and attribute methods (attr_accessor, etc.)
- `parameter_node.rs`: Processes method parameters and block parameters
- `block_node.rs`: Handles blocks and their parameters
- `utils.rs`: Common utility functions used by multiple processors

## Indexing Process

The indexing process involves:

1. Parsing Ruby code using Ruby Prism to create an AST (Abstract Syntax Tree)
2. Traversing the AST recursively, identifying Ruby language constructs
3. Creating entries in the index for each definition (class, method, variable, etc.)
4. Recording references to these definitions throughout the code
5. Maintaining proper namespace context during traversal

The `TraversalContext` struct is used to maintain state during traversal, including:
- Current namespace stack (for properly qualifying names)
- Current method context (for method-local variables)
- Current visibility (public, private, protected)

## What is Indexed

### Classes and Modules

- Class and module definitions
- Inheritance relationships
- Nested class and module hierarchies
- Reopened classes and modules

```ruby
# All indexed with proper namespace qualification
module MyModule
  class MyClass
    # Nested inside MyModule::MyClass
  end
end

# Reopened class - indexed as the same entity
class MyModule::MyClass
  # Additional methods
end
```

### Methods

- Instance methods
- Class methods (singleton methods)
- Constructor methods
- Method parameters
- Visibility (public, private, protected)
- Method calls and references

```ruby
class Calculator
  # Instance method with parameters
  def add(a, b, options = {})
    a + b + (options[:extra] || 0)
  end

  # Class method
  def self.version
    "1.0.0"
  end

  private
  # Private method
  def validate(num)
    raise unless num.is_a?(Numeric)
  end
end
```

### Constants

- Constant definitions and assignments
- Constant references (both qualified and unqualified)
- Nested constants

```ruby
module Config
  VERSION = "1.0.0"

  module Defaults
    TIMEOUT = 30
  end
end

# References to Constants
puts Config::VERSION
puts Config::Defaults::TIMEOUT
```

### Variables

#### Local Variables

- Definitions and assignments
- References within method scope
- Block-local variables

```ruby
def process_data(input)
  result = input * 2  # 'result' is indexed as a local variable
  result += 10        # Reference to 'result'
end
```

#### Instance Variables

- Definitions and assignments
- References within instance methods
- Proper qualification with class/module context

```ruby
class Person
  def initialize(name)
    @name = name  # '@name' indexed as instance variable of Person
  end

  def greet
    "Hello, #{@name}!"  # Reference to '@name'
  end
end
```

#### Class Variables

- Definitions and assignments
- References across instance and class methods
- Proper qualification with class/module context

```ruby
class Counter
  @@count = 0  # '@@count' indexed as class variable

  def increment
    @@count += 1  # Reference to '@@count'
  end

  def self.get_count
    @@count  # Reference to '@@count'
  end
end
```

### Attribute Methods

- `attr_accessor`, `attr_reader`, and `attr_writer` expansions
- Generated getter and setter methods

```ruby
class Person
  attr_accessor :name  # Indexes both 'name' and 'name=' methods
  attr_reader :age     # Indexes 'age' method only
  attr_writer :email   # Indexes 'email=' method only
end
```

### Blocks and Block Parameters

- Block definitions and their parameters
- Block parameter scoping

```ruby
[1, 2, 3].each do |num|  # 'num' is indexed as a block parameter
  puts num * 2
end

names = ["Alice", "Bob"]
names.map { |name| name.upcase }  # 'name' is indexed as a block parameter
```

### Method Parameters

- Regular parameters
- Optional parameters with default values
- Keyword parameters and keyword argument spreads
- Block parameters

```ruby
def search(query, limit = 10, **options, &block)
  # 'query', 'limit', 'options', and 'block' are all indexed
end
```

## LSP Features Implementation

### Go to Definition

The indexer maps each symbol's fully qualified name to its definition location. When a user requests "Go to Definition" on a symbol, the LSP server:

1. Identifies the symbol at the cursor position
2. Determines the fully qualified name based on the current context
3. Looks up the definition in the index
4. Returns the location (URI + range) of the definition

This works for:
- Class and module names
- Method names
- Variable references (local, instance, class)
- Constants
- Parameters

### Find References

The indexer tracks all references to symbols throughout the codebase. When "Find References" is requested:

1. The definition is located first (using Go to Definition logic)
2. All references to the symbol are collected from the index
3. References are returned as a list of locations

The indexer maintains references with both simple names and fully qualified names, allowing it to find references even when:
- A method is called with or without the receiver
- A constant is referenced with or without namespace
- A variable is referenced in different scopes

### Completion

Completion is powered by the index information about:
- Available methods in the current class/module context
- Local variables in scope
- Constants and their namespaces
- Parameter names for method calls

For example, when typing `something.`, the completion engine can suggest available methods for the inferred type of `something`.

### Document Symbols

The index contains a complete hierarchy of symbols in each document, including:
- Classes and modules with their nesting structure
- Methods organized by class/module
- Constants organized by namespace
- Variables organized by scope

This allows for displaying an outline view of the document structure.

### Semantic Highlighting

The index's knowledge of symbol types allows for enhanced syntax highlighting:
- Methods vs. local variables vs. parameters
- Constants vs. class names
- Instance variables vs. class variables
- Private vs. public methods

### Handling Edge Cases

The indexer addresses various Ruby language complexities:

#### Reopened Classes and Modules

```ruby
class MyClass
  def method1; end
end

# Later in another file
class MyClass
  def method2; end
end
```

Both definitions contribute to the same entry in the index, correctly associating all methods with `MyClass`.

#### Dynamic Method Definitions

The indexer handles cases where methods are defined dynamically:

```ruby
# Using define_method
define_method(:dynamic_method) do |arg|
  # Method body
end

# Using attr_* family
attr_accessor :name, :age
```

#### Nested Scopes and Qualified Names

The indexer maintains proper namespace resolution for nested classes and modules:

```ruby
module A
  module B
    class C
      CONSTANT = 1
    end
  end
end

# Properly indexed as A::B::C::CONSTANT
```

#### Method Calls with Different Receiver Syntaxes

```ruby
# Direct call
obj.method

# Call on self (implicit)
method

# Call on class
MyClass.method

# Call on nested class
Module::Class.method
```

All forms are correctly indexed as references to the appropriate method.

## Utility Functions

The `utils.rs` module provides common functions used across different node processors:

- `node_to_range`: Converts Prism node positions to LSP ranges
- `get_node_text`: Extracts text from a node based on source code
- `get_fqn`: Builds fully qualified names based on namespace and name

## Understanding the Index

The index consists of multiple data structures:

1. `entries`: Maps fully qualified names to their definition entries
2. `methods_by_name`: Maps method names to their entries for quick lookup
3. `references`: Maps fully qualified names to all locations where they're referenced
4. `namespace_tree`: Maintains the hierarchy of namespaces for completion and navigation

## Conclusion

The Ruby indexer provides a comprehensive understanding of Ruby code structure, enabling accurate and responsive LSP features. By carefully tracking definitions, references, and scopes, it provides the foundation for all language intelligence features in the Ruby Fast LSP.
