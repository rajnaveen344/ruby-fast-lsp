# Design Document

## Overview

This design document outlines the enhancement of the Ruby Fast LSP's identifier system to provide comprehensive contextual information at any cursor position. The enhancement introduces a new `ReceiverKind` enum and restructures the `Identifier` enum to capture detailed context including namespace stacks, receiver information, constant paths, and scope information for precise code navigation and analysis.

## Architecture

### Current Architecture

The current `Identifier` enum has three variants:
- `RubyConstant(Vec<RubyConstant>)` - Simple constant path
- `RubyMethod(Vec<RubyConstant>, RubyMethod)` - Namespace and method
- `RubyVariable(RubyVariable)` - Variable information

### Enhanced Architecture

The enhanced system introduces:

1. **ReceiverKind Enum**: Categorizes method receiver types
2. **Restructured Identifier Enum**: Captures comprehensive contextual information
3. **Enhanced Context Tracking**: Maintains namespace and scope stacks for precise resolution

## Components and Interfaces

### ReceiverKind Enum

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReceiverKind {
    /// No receiver, e.g., "method_a"
    None,

    /// Self receiver, e.g., "self.method_a"
    SelfReceiver,

    /// Constant receiver, e.g., "Class.method_a" or "Module::Class.method_a"
    Constant,

    /// Expression receiver, e.g., "a.method_a" or "(a + b).method_a"
    Expr,
}
```

### Enhanced Identifier Enum

```rust
#[derive(Debug, Clone)]
pub enum Identifier {
    /// Ruby constant with namespace context and identifier path
    /// - namespace: Current namespace stack (where the cursor is located)
    /// - iden: The constant path being referenced
    RubyConstant {
        namespace: Vec<RubyConstant>,
        iden: Vec<RubyConstant>,
    },

    /// Ruby method with comprehensive context
    /// - namespace: Current namespace stack (where the cursor is located)
    /// - receiver_kind: Type of method receiver
    /// - iden: The method being called
    RubyMethod {
        namespace: Vec<RubyConstant>,
        receiver_kind: ReceiverKind,
        iden: RubyMethod,
    },

    /// Ruby variable with appropriate scope context
    /// - iden: The variable information (includes its own scope context)
    RubyVariable {
        iden: RubyVariable,
    },
}
```

### Context Information Mapping

| Identifier Type | Context Information |
|----------------|-------------------|
| `RubyConstant` | `namespace` (current location), `iden` (constant path) |
| `RubyMethod` | `namespace` (current location), `receiver_kind`, `iden` (method) |
| `RubyVariable` | Variable's internal scope context (Local: LVScopeStack, Class/Instance: namespace, Global: none) |

## Data Models

### ReceiverKind Classification Logic

```rust
impl ReceiverKind {
    pub fn from_call_node(node: &CallNode) -> Self {
        match node.receiver() {
            None => ReceiverKind::None,
            Some(receiver) => {
                if receiver.as_self_node().is_some() {
                    ReceiverKind::SelfReceiver
                } else if receiver.as_constant_path_node().is_some()
                       || receiver.as_constant_read_node().is_some() {
                    ReceiverKind::Constant
                } else {
                    ReceiverKind::Expr
                }
            }
        }
    }
}
```

### Context Resolution Examples

#### Constant Resolution
```ruby
module Outer
  module Inner
    CONST_A = 10
  end

  val = Inner::CONST_A  # Cursor on CONST_A
end
```

Result:
```rust
Identifier::RubyConstant {
    namespace: vec![RubyConstant("Object"), RubyConstant("Outer")], // Where cursor is
    iden: vec![RubyConstant("Inner"), RubyConstant("CONST_A")],     // What's being referenced
}
```

#### Method Resolution
```ruby
module MyModule
  class MyClass
    def instance_method
      self.helper_method  # Cursor on helper_method
    end
  end
end
```

Result:
```rust
Identifier::RubyMethod {
    namespace: vec![RubyConstant("Object"), RubyConstant("MyModule"), RubyConstant("MyClass")],
    receiver_kind: ReceiverKind::SelfReceiver,
    iden: RubyMethod("helper_method", MethodKind::Instance),
}
```

#### Variable Resolution
```ruby
class MyClass
  def my_method
    local_var = 10  # Cursor on local_var
  end
end
```

Result:
```rust
Identifier::RubyVariable {
    iden: RubyVariable("local_var", RubyVariableType::Local(lv_scope_stack)),
}
```

## Error Handling

### Validation Strategy

1. **ReceiverKind Validation**: Ensure proper classification based on AST node types
2. **Context Consistency**: Verify namespace and scope stacks are correctly maintained
3. **Backward Compatibility**: Ensure existing code continues to work with new structure

### Error Cases

- **Invalid Receiver Classification**: Fallback to `ReceiverKind::Expr` for unknown receiver types
- **Missing Context**: Use empty vectors for missing namespace/scope information
- **Malformed Identifiers**: Maintain existing validation in underlying types

## Testing Strategy

### Unit Tests

1. **ReceiverKind Classification Tests**
   - Test all receiver types: None, Self, Constant, Expression
   - Test edge cases and complex expressions

2. **Identifier Construction Tests**
   - Test each identifier variant with various contexts
   - Test namespace and scope stack accuracy

3. **Context Resolution Tests**
   - Test nested module/class scenarios
   - Test variable scope resolution across different contexts

### Integration Tests

1. **End-to-End Identifier Resolution**
   - Test complete identifier resolution pipeline
   - Test interaction with existing LSP capabilities

2. **Backward Compatibility Tests**
   - Ensure existing tests continue to pass
   - Test migration from old to new enum structure

### Test Data Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receiver_kind_classification() {
        // Test cases for each receiver type
    }

    #[test]
    fn test_constant_identifier_with_context() {
        // Test constant resolution with namespace context
    }

    #[test]
    fn test_method_identifier_with_receiver() {
        // Test method resolution with different receiver kinds
    }

    #[test]
    fn test_variable_identifier_scope_context() {
        // Test variable resolution with appropriate scope context
    }
}
```

## Implementation Considerations

### Migration Strategy

1. **Direct Replacement**: Replace the existing `Identifier` enum with the new structure immediately
2. **Compilation-Driven Fixes**: Use Rust compiler errors to identify all usage sites that need updating
3. **Systematic Updates**: Update each module that uses the `Identifier` enum to work with the new structure
4. **Test Fixes**: Update all tests to work with the new enum structure

### Performance Considerations

- **Memory Usage**: New structure may use more memory due to additional context
- **Computation**: Context resolution may require additional processing

### Compatibility Considerations

- **Display Implementation**: Update `Display` trait to handle new structure
- **Serialization**: Ensure new structure can be serialized if needed
- **API Stability**: No need to maintain stable public API during transition as the product is not released yet
