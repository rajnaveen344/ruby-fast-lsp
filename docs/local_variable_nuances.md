# Ruby Local Variable Nuances in the LSP

This document explains how local variables are handled in the Ruby LSP, covering scoping rules, shadowing, and implementation details.

## Table of Contents
- [Basic Scoping](#basic-scoping)
- [Block Parameters and Shadowing](#block-parameters-and-shadowing)
- [Implementation Details](#implementation-details)
- [Edge Cases](#edge-cases)
- [Examples](#examples)

## Basic Scoping

### Method-Local Variables
```ruby
def example
  x = 1
  if true
    y = 2  # Method-local, available throughout the method
  end
  puts y  # => 2
end
```
- Variables defined in method bodies are method-local
- Accessible throughout the entire method, even across conditionals
- `if`/`elsif`/`else` do not create new scopes

### Block-Local Variables
```ruby
x = 1
[1].each do |y|  # y is block-local
  z = 2          # Also block-local
  x = 3          # Modifies outer x
end
puts x  # => 3
puts z  # NameError
```
- Block parameters (`|y|`) create new variables
- Variables defined inside blocks are block-local
- Can access and modify variables from outer scopes

## Top-Level Local Variables

### File-Local Scope
```ruby
# file1.rb
top_level_var = "I'm a top-level local"

# Accessible anywhere in the same file scope
puts top_level_var  # => "I'm a top-level local"

# But not in method definitions
def some_method
  puts top_level_var  # NameError: undefined local variable or method `top_level_var'
end
```

### Not Accessible Across Files
```ruby
# file2.rb
require_relative 'file1'
puts top_level_var  # NameError: undefined local variable or method `top_level_var'
```

### Loading vs Requiring
```ruby
# file1.rb
$LOADED_FEATURES << "file1.rb" unless $LOADED_FEATURES.include?("file1.rb")
top_level_var = "I'm a top-level local"

# file2.rb
load 'file1.rb'  # Executes the code, but top_level_var is still not accessible
puts top_level_var  # NameError
```

### Key Points
- Top-level local variables are scoped to their file
- Not accessible in method definitions within the same file
- Not accessible in other files, even when required or loaded

## Block Parameters and Shadowing

### Block Parameters
```ruby
def example
  x = 1
  [1,2,3].each do |x|  # Creates a new 'x' that shadows the outer 'x'
    puts x  # Prints block parameter values: 1, 2, 3
  end
  puts x  # Still 1, not affected by block
end
```
- Block parameters create new variables in the block's scope
- These shadow any variables with the same name from outer scopes
- Changes to block parameters don't affect outer variables

### Explicit Block-Local Variables
```ruby
x = 1
y = 2
[1,2,3].each do |item; x, z|  # x and z are explicit block-locals
  x = "changed"  # Doesn't affect outer x
  z = "new"      # New variable, only exists in block
  y = "modified" # Modifies outer y
end
puts x  # => 1 (unchanged)
puts y  # => "modified"
puts z  # NameError
```
- Variables listed after semicolon are block-local
- They shadow outer variables with the same name
- They're initialized as nil at the start of each block iteration

### Nested Blocks and Scope
```ruby
x = "outer"
[1].each do |_|
  x = "middle"  # Modifies outer x
  [2].each do |_|
    x = "inner"  # Modifies middle x (which is outer x)
  end
end
puts x  # => "inner"
```
- Blocks can access and modify variables from all enclosing scopes
- Each block creates a new scope for its parameters and local variables
- Variables flow inward, but not outward

## Implementation Details

### Tracking Local Variables

In the Ruby LSP, local variables are tracked using the `RubyVariableType::Local` variant which now contains two key properties:

1. **Scope Depth** (`LVScopeDepth`)
   - Numerical value indicating nesting level
   - Higher values = deeper nesting
   - Used to determine variable visibility
   - Stored as the first parameter in `RubyVariableType::Local(depth, kind)`

2. **Scope Kind** (`LVScopeKind`)
   - Indicates the type of scope boundary
   - Different kinds have different variable visibility rules
   - Examples: `TopLevel`, `Method`, `Block`, `ExplicitBlockLocal`
   - Stored as the second parameter in `RubyVariableType::Local(depth, kind)`

### Variable Type Implementation

```rust
pub enum RubyVariableType {
    Local(LVScopeDepth, LVScopeKind),
    Instance,
    Class,
    Global,
}
```

This design encapsulates scope information directly in the variable type, making it:
- Type-safe: Scope information only exists for local variables
- Efficient: No need for separate scope tracking structures
- Clear: The relationship between variables and their scopes is explicit

### Scope Management

The LSP implements a sophisticated scope management system that:
1. Tracks the semantic scope of each variable
2. Handles Ruby's complex scoping rules accurately
3. Enables correct variable resolution in all contexts
4. Supports advanced features like explicit block-locals and rescue parameters

### Variable Resolution Algorithm

1. **Lookup Process**
   - First check current scope (same `scope_depth`)
   - Then check parent scopes in order of increasing depth
   - Different `scope_kind` values create new scoping boundaries

2. **Variable Creation**
   - New variables are created in the current scope
   - Block parameters always create new variables in the block's scope
   - Explicit block-locals create variables in `ExplicitBlockLocal` scope

3. **Shadowing Behavior**
   - Inner scopes can shadow variables from outer scopes
   - Shadowing depends on both `LVScopeDepth` and `LVScopeKind` stored in the `Local` variant
   - `ExplicitBlockLocal` prevents shadowing by creating a new scope

4. **Special Cases**
   - Rescue parameters create a new `Rescue` scope
   - Each block creates a new `Block` scope
   - Top-level variables use `TopLevel` scope

## Edge Cases

### Blocks with Same-Named Parameters
```ruby
def test
  x = 1
  [1].each do |x|  # Shadows outer x
    # This is a different x
  end
end
```

### Nested Blocks
```ruby
x = 1
[1].each do |y|
  [1].each do |x|  # Shadows outer x
    # Inner block
  end
  # Middle block
end
```

### Rescue/Ensure Blocks
```ruby
begin
  x = 1
rescue => e  # e is block-local to rescue
  y = 2
ensure
  z = 3  # Available after ensure
end
```

## Examples

### Method with Block Parameter
```ruby
def process(items)
  result = []
  items.each do |item|  # item is block-local
    result << item * 2  # Can access outer result
  end
  result
end
```

### Block with Shadowing
```ruby
def counter
  count = 0
  [1,2,3].map do |count|  # Shadows outer count
    count * 2  # Refers to block parameter
  end
  count  # Still 0
end
```

### Explicit Block Locals
```ruby
def logger
  level = :info
  [1,2,3].each do |item; level|  # level is block-local
    level = :debug  # Doesn't affect outer level
  end
end
```

## Conclusion
Understanding Ruby's local variable scoping is crucial for:
- Accurate "go to definition"
- Proper variable highlighting
- Correct code completion
- Refactoring operations

The LSP tracks these scoping rules using `LVScopeDepth` and `LVScopeKind` stored directly in the `RubyVariableType::Local` variant to maintain correct variable resolution.