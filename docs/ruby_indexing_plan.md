# Ruby Code Indexing for LSP: Implementation Plan

## Overview

This document outlines a plan for implementing a Ruby code indexing system for our Language Server Protocol (LSP) implementation. The focus is on supporting goto_definition and references functionality, taking into account the unique characteristics and limitations of Ruby as a language.

## Challenges with Ruby

When designing an indexing system for Ruby, we must consider:

1. **Dynamic Nature**: Ruby is highly dynamic, allowing for meta-programming and runtime modifications.
2. **Duck Typing**: Methods may not be explicitly defined but expected to exist on objects.
3. **Open Classes**: Ruby allows reopening classes to add or modify methods.
4. **Lack of Type Annotations**: Without explicit type annotations, inferring types can be challenging.
5. **Module Inclusion/Extension**: Ruby's module system allows for complex inheritance hierarchies.
6. **Namespace Resolution**: Ruby's constant resolution rules can make it difficult to determine the exact reference.

## Data Structure Design

Our indexer will maintain the following data structures:

### 1. Entry Structure

We'll define different types of entries:

```rust
enum EntryType {
    Class,
    Module,
    Method,
    Constant,
    ConstantAlias,
    UnresolvedAlias,
}

struct Entry {
    name: String,           // The name of the entity
    fully_qualified_name: String, // Complete namespace path
    location: Location,     // Where this entry is defined
    entry_type: EntryType,  // Type of entry
    visibility: Visibility, // Public, protected, private
    metadata: HashMap<String, String>, // Additional information
}

struct Location {
    uri: String,            // File URI
    range: Range,           // Position in the file
}
```

### 2. Index Structure

```rust
struct RubyIndex {
    // Main index mapping fully qualified names to entries
    entries: HashMap<String, Vec<Entry>>,

    // Map file URIs to their entries for efficient updates
    uri_to_entries: HashMap<String, Vec<Entry>>,

    // Maps for quick lookups by specific criteria
    methods_by_name: HashMap<String, Vec<Entry>>,
    constants_by_name: HashMap<String, Vec<Entry>>,

    // Namespace hierarchy
    namespace_tree: HashMap<String, Vec<String>>,

    // Prefix tree for autocompletion suggestions
    prefix_tree: PrefixTree<Vec<Entry>>,
}
```

#### Examples of Index Data Structure Contents

To illustrate how these data structures would be populated, consider a Ruby project with the following files:

**user.rb**:
```ruby
module Authentication
  class User
    ROLES = [:admin, :user, :guest]

    attr_accessor :name, :email

    def initialize(name, email)
      @name = name
      @email = email
      @role = :user
    end

    def admin?
      @role == :admin
    end
  end

  module Helpers
    def self.validate_email(email)
      email.include?('@')
    end
  end
end
```

**account.rb**:
```ruby
class Account
  include Authentication::Helpers

  def initialize(user)
    @user = user
  end

  def authenticate
    validate_email(@user.email)
  end
end
```

Here's how the index properties would be populated:

##### 1. `entries` HashMap:

Maps fully qualified names to their corresponding entries:

```rust
// Format: "fully_qualified_name" => [Entry instances]
{
  "Authentication" => [
    Entry { name: "Authentication", fully_qualified_name: "Authentication", entry_type: Module, ... }
  ],
  "Authentication::User" => [
    Entry { name: "User", fully_qualified_name: "Authentication::User", entry_type: Class, ... }
  ],
  "Authentication::User::ROLES" => [
    Entry { name: "ROLES", fully_qualified_name: "Authentication::User::ROLES", entry_type: Constant, ... }
  ],
  "Authentication::User#initialize" => [
    Entry { name: "initialize", fully_qualified_name: "Authentication::User#initialize", entry_type: Method, ... }
  ],
  "Authentication::User#admin?" => [
    Entry { name: "admin?", fully_qualified_name: "Authentication::User#admin?", entry_type: Method, ... }
  ],
  "Authentication::Helpers" => [
    Entry { name: "Helpers", fully_qualified_name: "Authentication::Helpers", entry_type: Module, ... }
  ],
  "Authentication::Helpers.validate_email" => [
    Entry { name: "validate_email", fully_qualified_name: "Authentication::Helpers.validate_email", entry_type: Method, ... }
  ],
  "Account" => [
    Entry { name: "Account", fully_qualified_name: "Account", entry_type: Class, ... }
  ],
  "Account#initialize" => [
    Entry { name: "initialize", fully_qualified_name: "Account#initialize", entry_type: Method, ... }
  ],
  "Account#authenticate" => [
    Entry { name: "authenticate", fully_qualified_name: "Account#authenticate", entry_type: Method, ... }
  ]
}
```

##### 2. `uri_to_entries` HashMap:

Maps file URIs to all entries defined in that file:

```rust
// Format: "file_uri" => [Entry instances]
{
  "file:///path/to/user.rb" => [
    Entry { name: "Authentication", fully_qualified_name: "Authentication", ... },
    Entry { name: "User", fully_qualified_name: "Authentication::User", ... },
    Entry { name: "ROLES", fully_qualified_name: "Authentication::User::ROLES", ... },
    Entry { name: "initialize", fully_qualified_name: "Authentication::User#initialize", ... },
    Entry { name: "admin?", fully_qualified_name: "Authentication::User#admin?", ... },
    Entry { name: "Helpers", fully_qualified_name: "Authentication::Helpers", ... },
    Entry { name: "validate_email", fully_qualified_name: "Authentication::Helpers.validate_email", ... }
  ],
  "file:///path/to/account.rb" => [
    Entry { name: "Account", fully_qualified_name: "Account", ... },
    Entry { name: "initialize", fully_qualified_name: "Account#initialize", ... },
    Entry { name: "authenticate", fully_qualified_name: "Account#authenticate", ... }
  ]
}
```

##### 3. `methods_by_name` HashMap:

Maps method names to entries, regardless of their namespace:

```rust
// Format: "method_name" => [Entry instances]
{
  "initialize" => [
    Entry { name: "initialize", fully_qualified_name: "Authentication::User#initialize", ... },
    Entry { name: "initialize", fully_qualified_name: "Account#initialize", ... }
  ],
  "admin?" => [
    Entry { name: "admin?", fully_qualified_name: "Authentication::User#admin?", ... }
  ],
  "validate_email" => [
    Entry { name: "validate_email", fully_qualified_name: "Authentication::Helpers.validate_email", ... }
  ],
  "authenticate" => [
    Entry { name: "authenticate", fully_qualified_name: "Account#authenticate", ... }
  ]
}
```

##### 4. `constants_by_name` HashMap:

Maps constant names (including classes and modules) to entries:

```rust
// Format: "constant_name" => [Entry instances]
{
  "Authentication" => [
    Entry { name: "Authentication", fully_qualified_name: "Authentication", entry_type: Module, ... }
  ],
  "User" => [
    Entry { name: "User", fully_qualified_name: "Authentication::User", entry_type: Class, ... }
  ],
  "ROLES" => [
    Entry { name: "ROLES", fully_qualified_name: "Authentication::User::ROLES", entry_type: Constant, ... }
  ],
  "Helpers" => [
    Entry { name: "Helpers", fully_qualified_name: "Authentication::Helpers", entry_type: Module, ... }
  ],
  "Account" => [
    Entry { name: "Account", fully_qualified_name: "Account", entry_type: Class, ... }
  ]
}
```

##### 5. `namespace_tree` HashMap:

Maps namespace names to their direct children:

```rust
// Format: "namespace" => [child namespace names]
{
  "" => ["Authentication", "Account"],
  "Authentication" => ["User", "Helpers"],
  "Authentication::User" => ["ROLES"],
  "Authentication::Helpers" => []
}
```

##### 6. `prefix_tree` PrefixTree:

A trie-like structure for quick prefix-based lookups:

```
// Conceptual representation of a prefix tree
root
├── A
│   ├── u (Account)
│   │   ├── t (Account#authenticate)
│   └── c (Account)
└── a
    ├── d (Authentication::User#admin?)
    └── u (Account#authenticate)
```

#### Module/Class Inclusion and Extension

When a module is included or extended, the indexer would track these relationships. For example, if we have `include Authentication::Helpers` in the `Account` class:

1. Track that `Authentication::Helpers` is included in `Account`.
2. When resolving a call to `validate_email` in `Account#authenticate`, consider methods from included modules.

#### Handling Ruby's Open Classes

When classes are reopened, we add additional entries with the same fully qualified name but different locations:

```ruby
# in file1.rb
class User
  def method1; end
end

# in file2.rb
class User  # reopening the class
  def method2; end
end
```

The index would contain:
```rust
{
  "User" => [
    Entry { name: "User", fully_qualified_name: "User", location: { uri: "file:///path/to/file1.rb", ... }, ... },
    Entry { name: "User", fully_qualified_name: "User", location: { uri: "file:///path/to/file2.rb", ... }, ... }
  ],
  "User#method1" => [...],
  "User#method2" => [...]
}
```

#### References Tracking

For "find references" functionality, we would need to track locations where entities are referenced:

```rust
// Additional structure for tracking references
references: HashMap<String, Vec<Location>>,

// Example:
{
  "Authentication::User" => [
    Location { uri: "file:///path/to/account.rb", range: ... },  // Where User is referenced
    ...
  ],
  "Authentication::Helpers.validate_email" => [
    Location { uri: "file:///path/to/account.rb", range: ... },  // Where the method is called
    ...
  ]
}
```

This comprehensive approach allows us to handle the complex relationships in Ruby code, supporting goto_definition and references while accounting for Ruby's dynamic features.

## Implementation Plan

### Phase 1: Basic Infrastructure

1. **Create the Index Structure**
   - Implement the basic data structures as outlined above
   - Define the Entry and Location types
   - Set up the main index and related lookup maps

2. **Integrate with Server**
   - Modify the RubyLanguageServer to maintain an index
   - Initialize the index during server startup

3. **Setup Document Parsing**
   - Use tree-sitter to parse Ruby files
   - Extract AST (Abstract Syntax Tree) for analysis

### Phase 2: Indexing Implementation

4. **Implement AST Traversal**
   - Create a visitor pattern for traversing the Ruby AST
   - Identify classes, modules, methods, and constants

5. **Build Index Population Logic**
   - Extract definitions from AST nodes
   - Insert entries into the index with proper metadata
   - Handle namespaces and nested definitions

6. **Implement File Watching**
   - Track file changes using LSP's file watching capabilities
   - Update the index when files are changed, created, or deleted

### Phase 3: Lookup Implementation

7. **Implement Goto Definition**
   - Find definition entries based on reference position
   - Handle namespace resolution correctly
   - Return location information for the definition

8. **Implement References Lookup**
   - Track references during indexing
   - Provide reverse lookup from definition to all references

### Phase 4: Advanced Features

9. **Handle Ruby-Specific Challenges**
   - Implement module inclusion/extension tracking
   - Handle class reopening correctly
   - Support for method_missing and other meta-programming patterns

10. **Optimize for Performance**
    - Implement incremental updates to avoid full re-indexing
    - Add caching mechanisms for frequent lookups
    - Consider background indexing for large codebases

11. **Add Heuristics for Ambiguous Cases**
    - Implement "best guess" resolution for dynamic code
    - Use naming conventions to improve accuracy

## Integration with LSP

### LSP Event Handling

We'll hook into these LSP events to maintain the index:

1. **initialize**: Initialize the index during server startup
2. **textDocument/didOpen**: Add new files to the index
3. **textDocument/didChange**: Update the index when files change
4. **textDocument/didClose**: Handle file closing correctly
5. **workspace/didChangeWatchedFiles**: React to file system changes

### LSP Request Handling

We'll implement these LSP requests using our index:

1. **textDocument/definition**: Use index to find definitions
2. **textDocument/references**: Use index to find references

## Testing Strategy

1. **Unit Tests**: Test individual components of the indexing system
2. **Integration Tests**: Test the indexing system with the LSP server
3. **Test Cases**: Create specific test cases for Ruby edge cases
   - Module inclusion/extension
   - Class reopening
   - Dynamic method definition
   - Constant resolution in nested namespaces

## Future Enhancements

After the initial implementation, we can consider:

1. **Type Inference**: Basic type inference to improve goto definition accuracy
2. **Documentation**: Extract and provide documentation for symbols
3. **Completion**: Use index for better completion suggestions
4. **Workspace Symbols**: Support for searching symbols across the workspace
5. **Rename Refactoring**: Support for safe symbol renaming

## Conclusion

This phased approach will allow us to build a robust indexing system for Ruby that supports goto_definition and references functionality while addressing the unique challenges of the Ruby language. The system is designed to be extensible for future LSP features beyond the initial scope.
