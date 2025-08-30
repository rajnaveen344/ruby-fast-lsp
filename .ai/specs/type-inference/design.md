# Type Inference Design

## Overview

The type inference system for Ruby Fast LSP provides intelligent type analysis to enhance code completion, error detection, and developer experience. The system is designed to work incrementally and maintain high performance while providing accurate type information for Ruby's dynamic nature.

This design follows Ruby's object model where everything is an object, treating Integer, Float, String, etc. as classes rather than primitive types.

## Integration with Existing Components

### Core Dependencies
- **RubyIndex**: Enhanced to store type information alongside existing definitions
- **Identifier Enum**: Extended to include type metadata for variables, methods, and constants
- **ScopeTracker**: Modified to track type information across different scopes
- **Completion System**: Enhanced to use type information for better suggestions

### New Components

#### TypeInferenceEngine
The main orchestrator that coordinates type analysis across the codebase.

#### RubyType System
Core type representation supporting Ruby's object model:
- All types as classes (String, Integer, Float, TrueClass, FalseClass, NilClass, Symbol)
- Collection types (Array, Hash) with polymorphic element type support
- Union types for variables that can hold multiple types
- Class and module references for metaprogramming
- Unknown type for unresolved cases

#### FlowSensitiveAnalyzer
Tracks how types change through control flow:
- Conditional type narrowing
- Loop variable type evolution
- Exception handling type effects

## Core Data Structures

### RubyType System
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RubyType {
    // Built-in Ruby classes
    Class(FullyQualifiedName),
    Module(FullyQualifiedName),
    
    // Class and module references
    ClassReference(FullyQualifiedName),
    ModuleReference(FullyQualifiedName),
    
    // Parameterized collection types
    Array(Vec<RubyType>),
    Hash(Vec<RubyType>, Vec<RubyType>),
    
    // Type system constructs
    Union(Vec<RubyType>),
    TypeVariable(String),
    Unknown,
    Any,
}

impl RubyType {
    // Helper constructors for common types
    pub fn string() -> Self { RubyType::Class(FullyQualifiedName::from_str("String").unwrap()) }
    pub fn integer() -> Self { RubyType::Class(FullyQualifiedName::from_str("Integer").unwrap()) }
    pub fn float() -> Self { RubyType::Class(FullyQualifiedName::from_str("Float").unwrap()) }
    pub fn nil_class() -> Self { RubyType::Class(FullyQualifiedName::from_str("NilClass").unwrap()) }
    pub fn symbol() -> Self { RubyType::Class(FullyQualifiedName::from_str("Symbol").unwrap()) }
    pub fn true_class() -> Self { RubyType::Class(FullyQualifiedName::from_str("TrueClass").unwrap()) }
    pub fn false_class() -> Self { RubyType::Class(FullyQualifiedName::from_str("FalseClass").unwrap()) }
    pub fn boolean() -> Self { RubyType::Union(vec![Self::true_class(), Self::false_class()]) }
    
    pub fn array_of(element_type: RubyType) -> Self {
        RubyType::Array(vec![element_type])
    }
    
    pub fn hash_of(key_type: RubyType, value_type: RubyType) -> Self {
        RubyType::Hash(vec![key_type], vec![value_type])
    }
    
    // Type operations
    pub fn union(types: Vec<RubyType>) -> Self {
        if types.len() == 1 {
            types.into_iter().next().unwrap()
        } else {
            RubyType::Union(Self::deduplicate_union(types))
        }
    }
    
    pub fn is_nilable(&self) -> bool {
        match self {
            RubyType::Class(name) => name.to_string() == "NilClass",
            RubyType::Union(types) => types.iter().any(|t| t.is_nilable()),
            _ => false,
        }
    }
    
    pub fn make_nilable(self) -> Self {
        if self.is_nilable() {
            self
        } else {
            RubyType::Union(vec![self, RubyType::nil_class()])
        }
    }
    
    // Union operations
    pub fn union_with(self, other: RubyType) -> RubyType {
        match (self, other) {
            (RubyType::Union(mut types1), RubyType::Union(types2)) => {
                types1.extend(types2);
                RubyType::Union(Self::deduplicate_union(types1))
            }
            (RubyType::Union(mut types), other) | (other, RubyType::Union(mut types)) => {
                types.push(other);
                RubyType::Union(Self::deduplicate_union(types))
            }
            (t1, t2) if t1 == t2 => t1,
            (t1, t2) => RubyType::Union(vec![t1, t2]),
        }
    }
    
    fn deduplicate_union(mut types: Vec<RubyType>) -> Vec<RubyType> {
        types.sort_by(|a, b| format!("{:?}", a).cmp(&format!("{:?}", b)));
        types.dedup();
        types
    }
    
    pub fn is_assignable_to(&self, target: &RubyType) -> bool {
        // Implementation for type compatibility checking
        match (self, target) {
            (_, RubyType::Any) => true,
            (RubyType::Unknown, _) => true,
            (t1, t2) if t1 == t2 => true,
            (RubyType::Union(types), target) => {
                types.iter().all(|t| t.is_assignable_to(target))
            }
            (source, RubyType::Union(types)) => {
                types.iter().any(|t| source.is_assignable_to(t))
            }
            // Add inheritance and mixin checking here
            _ => false,
        }
    }
    
    pub fn display_name(&self) -> String {
        match self {
            RubyType::Class(name) | RubyType::Module(name) => name.to_string(),
            RubyType::ClassReference(name) => format!("Class<{}>", name),
            RubyType::ModuleReference(name) => format!("Module<{}>", name),
            RubyType::Array(types) => {
                if types.is_empty() {
                    "Array".to_string()
                } else {
                    format!("Array<{}>", types.iter().map(|t| t.display_name()).collect::<Vec<_>>().join(", "))
                }
            }
            RubyType::Hash(key_types, value_types) => {
                let keys = if key_types.is_empty() { "Object".to_string() } else { key_types.iter().map(|t| t.display_name()).collect::<Vec<_>>().join(", ") };
                let values = if value_types.is_empty() { "Object".to_string() } else { value_types.iter().map(|t| t.display_name()).collect::<Vec<_>>().join(", ") };
                format!("Hash<{}, {}>", keys, values)
            }
            RubyType::Union(types) => {
                types.iter().map(|t| t.display_name()).collect::<Vec<_>>().join(" | ")
            }
            RubyType::TypeVariable(name) => name.clone(),
            RubyType::Unknown => "?".to_string(),
            RubyType::Any => "Object".to_string(),
        }
    }
}
```

### TypedVariable
```rust
#[derive(Debug, Clone)]
pub struct TypedVariable {
    pub variable: RubyVariable,
    pub inferred_type: RubyType,
    pub confidence: TypeConfidence,
    pub source: TypeSource,
    pub location: Range,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeConfidence {
    High,    // Explicit signature or literal assignment
    Medium,  // Inferred from method return or flow analysis
    Low,     // Heuristic-based inference
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeSource {
    Literal,           // x = "hello"
    Signature,         // Explicit type annotation
    MethodReturn,      // x = some_method()
    Parameter,         // def foo(x)
    FlowAnalysis,      // if x.is_a?(String)
    Assignment,        // x = y (where y has known type)
}
```

### MethodSignature
```rust
#[derive(Debug, Clone)]
pub struct MethodSignature {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: RubyType,
    pub type_parameters: Vec<String>,
    pub visibility: Visibility,
    pub source: SignatureSource,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub param_type: RubyType,
    pub optional: bool,
    pub keyword: bool,
    pub splat: bool,
    pub double_splat: bool,
}

#[derive(Debug, Clone)]
pub enum SignatureSource {
    RBS,
    Sorbet,
    YARD,
    Inferred,
}
```

## Implementation Strategy

### Phase 1: Core Type System

1. **Type System Foundation**
   - Implement `RubyType` enum with basic operations
   - Create union type handling and deduplication
   - Add type compatibility checking

2. **Literal Type Detection**
   - String literals → `String` class
   - Numeric literals → `Integer`/`Float` classes
   - Boolean literals → `TrueClass`/`FalseClass`
   - Array/Hash literals with element type inference
   - Class/Module reference detection

3. **Basic Variable Tracking**
   - Extend scope tracker with type information
   - Track variable assignments with types
   - Handle local variable type propagation

### Phase 2: Signature-First Inference

1. **Signature Parsing**
   - RBS signature parser for method types
   - Sorbet signature parser for `sig` blocks
   - YARD comment parser for type annotations
   - Signature validation and error handling

2. **Method Type Resolution**
   - Use signatures as authoritative type source
   - Validate method calls against signatures
   - Validate return statements against declared types
   - Error reporting for type mismatches

3. **Fallback Inference**
   - Structural inference when signatures unavailable
   - Method body analysis for return types
   - Parameter type inference from usage

### Phase 3: Flow-Sensitive Analysis

1. **Control Flow Graph**
   - Build CFG for method bodies
   - Track type states across basic blocks
   - Handle branching and merging

2. **Type Narrowing**
   - `is_a?` and `kind_of?` type guards
   - `nil?` checks for nil elimination
   - Case statement type refinement
   - Rescue block exception typing

3. **Union Type Refinement**
   - Type intersection in conditional branches
   - Type widening at merge points
   - Dead code elimination based on types

## Integration Points

### Enhanced Completion System
```rust
// In src/capabilities/completion/mod.rs
pub struct TypeAwareCompletion {
    type_inference: Arc<TypeInferenceEngine>,
    existing_completion: CompletionEngine,
}

impl TypeAwareCompletion {
    pub fn complete_with_types(
        &self,
        context: &CompletionContext,
        receiver_type: Option<RubyType>,
    ) -> Vec<CompletionItem> {
        match receiver_type {
            Some(ruby_type) => self.complete_for_type(context, &ruby_type),
            None => self.existing_completion.complete(context),
        }
    }
    
    fn complete_for_type(
        &self,
        context: &CompletionContext,
        ruby_type: &RubyType,
    ) -> Vec<CompletionItem> {
        match ruby_type {
            RubyType::Class(class_name) => {
                self.complete_instance_methods(class_name)
            }
            RubyType::ClassReference(class_name) => {
                self.complete_class_methods(class_name)
            }
            RubyType::Union(types) => {
                types.iter()
                    .flat_map(|t| self.complete_for_type(context, t))
                    .collect()
            }
            _ => vec![],
        }
    }
}
```

### Enhanced Identifier Enum
```rust
// In src/types/mod.rs - extend existing Identifier
#[derive(Debug, Clone)]
pub enum Identifier {
    // Existing variants...
    LocalVariable(RubyVariable, Option<RubyType>),
    InstanceVariable(RubyVariable, Option<RubyType>),
    ClassVariable(RubyVariable, Option<RubyType>),
    GlobalVariable(RubyVariable, Option<RubyType>),
    // New typed variants
    TypedMethod(RubyMethod, MethodSignature),
    TypedConstant(FullyQualifiedName, RubyType),
}
```

### Type Cache for Performance
```rust
#[derive(Debug)]
pub struct TypeCache {
    variable_types: HashMap<VariableId, TypedVariable>,
    method_signatures: HashMap<MethodId, MethodSignature>,
    expression_types: HashMap<ExpressionId, RubyType>,
    file_versions: HashMap<PathBuf, u64>,
}

impl TypeCache {
    pub fn invalidate_file(&mut self, file_path: &Path) {
        // Remove all cached types for the given file
        self.variable_types.retain(|id, _| !id.belongs_to_file(file_path));
        self.method_signatures.retain(|id, _| !id.belongs_to_file(file_path));
        self.expression_types.retain(|id, _| !id.belongs_to_file(file_path));
    }
    
    pub fn get_variable_type(&self, var_id: &VariableId) -> Option<&RubyType> {
        self.variable_types.get(var_id).map(|tv| &tv.inferred_type)
    }
}
```

## Error Handling and Diagnostics

### Type Error Reporting
```rust
#[derive(Debug, Clone)]
pub enum TypeError {
    TypeMismatch {
        expected: RubyType,
        actual: RubyType,
        location: Range,
    },
    UnknownMethod {
        receiver_type: RubyType,
        method_name: String,
        location: Range,
    },
    InvalidSignature {
        signature: String,
        error: String,
        location: Range,
    },
    CircularDependency {
        cycle: Vec<String>,
    },
}

impl TypeError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        match self {
            TypeError::TypeMismatch { expected, actual, location } => {
                Diagnostic {
                    range: *location,
                    severity: Some(DiagnosticSeverity::ERROR),
                    message: format!(
                        "Type mismatch: expected {}, got {}",
                        expected.display_name(),
                        actual.display_name()
                    ),
                    ..Default::default()
                }
            }
            // Handle other error types...
        }
    }
}
```

## Performance Considerations

### Incremental Type Inference
- Only re-analyze files that have changed
- Propagate type changes to dependent files
- Use dependency tracking to minimize re-analysis
- Cache type information with file version tracking

### Memory Management
- Use interned strings for type names
- Share common type instances (String, Integer, etc.)
- Implement type garbage collection for unused types
- Limit union type size to prevent explosion

### Algorithmic Optimizations
- Use constraint-based inference for complex cases
- Implement fixed-point iteration for recursive types
- Use abstract interpretation for performance
- Employ heuristics for common Ruby patterns

## Testing Strategy

### Unit Tests
- Type system operations (union, intersection, compatibility)
- Literal type detection accuracy
- Signature parsing correctness
- Flow analysis precision

### Integration Tests
- End-to-end type inference on real Ruby code
- Performance benchmarks on large codebases
- Compatibility with existing LSP features
- Error handling and recovery

### Test Cases
- Common Ruby patterns and idioms
- Edge cases and malformed code
- Performance stress tests
- Signature format compatibility

This design provides a solid foundation for implementing type inference while maintaining the performance and reliability requirements of an LSP server.