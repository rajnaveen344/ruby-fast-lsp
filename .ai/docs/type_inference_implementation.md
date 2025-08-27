# Type Inference Implementation for Ruby Fast LSP

## Requirements

### Core Requirements
- **Union Types**: Variables can have multiple possible types (e.g., `String | Integer | nil`)
- **Type Flow Analysis**: Track how types flow through method calls, assignments, and control structures
- **Method Return Type Inference**: Infer return types based on method body analysis
- **Context-Aware Completion**: Provide completions based on inferred types
- **Type Narrowing**: Refine types based on conditionals and type checks
- **Performance**: Maintain LSP responsiveness with incremental type analysis

### Integration Requirements
- Build on existing `RubyPrismAnalyzer` and indexing infrastructure
- Extend current `Identifier` enum to include type information
- Integrate with existing completion system
- Maintain compatibility with current LSP features

## Architecture Overview

### Type System Components

```rust
// New type system structures
pub enum RubyType {
    // Built-in Ruby classes (everything is an object in Ruby)
    Class(FullyQualifiedName),
    Module(FullyQualifiedName),
    // Class reference - represents a class object that can be used for instantiation
    ClassReference(FullyQualifiedName),
    // Module reference - represents a module object that can be used for inclusion/extension
    ModuleReference(FullyQualifiedName),
    
    // Parameterized collection types with union type support
    Array(Vec<RubyType>),
    Hash(Vec<RubyType>, Vec<RubyType>),
    
    // Special types for type system
    Union(Vec<RubyType>),
    Unknown,
    Any,
}

// Helper constructors for common Ruby classes
impl RubyType {
    pub fn string() -> Self {
        RubyType::Class(FullyQualifiedName::from_str("String").unwrap())
    }
    
    pub fn integer() -> Self {
        RubyType::Class(FullyQualifiedName::from_str("Integer").unwrap())
    }
    
    pub fn float() -> Self {
        RubyType::Class(FullyQualifiedName::from_str("Float").unwrap())
    }
    
    pub fn nil_class() -> Self {
        RubyType::Class(FullyQualifiedName::from_str("NilClass").unwrap())
    }
    
    pub fn symbol() -> Self {
        RubyType::Class(FullyQualifiedName::from_str("Symbol").unwrap())
    }
    
    pub fn true_class() -> Self {
        RubyType::Class(FullyQualifiedName::from_str("TrueClass").unwrap())
    }
    
    pub fn false_class() -> Self {
        RubyType::Class(FullyQualifiedName::from_str("FalseClass").unwrap())
    }
    
    pub fn boolean() -> Self {
        RubyType::Union(vec![Self::true_class(), Self::false_class()])
    }
    
    pub fn array_of(element_type: RubyType) -> Self {
        RubyType::Array(vec![element_type])
    }
    
    pub fn hash_of(key_type: RubyType, value_type: RubyType) -> Self {
        RubyType::Hash(vec![key_type], vec![value_type])
    }
    
    // Helper for class and module references
    pub fn class_reference(class_name: &str) -> Self {
        RubyType::ClassReference(FullyQualifiedName::from_str(class_name).unwrap())
    }
    
    pub fn module_reference(module_name: &str) -> Self {
        RubyType::ModuleReference(FullyQualifiedName::from_str(module_name).unwrap())
    }
    
    // For polymorphic collections, use Vec types directly:
    // Example: RubyType::Array(vec![integer(), string()])
    // Example: RubyType::Hash(
    //   vec![symbol(), string()],
    //   vec![integer(), string()]
    // )
}

pub struct TypedVariable {
    pub variable: RubyVariable,
    pub inferred_type: RubyType,
    pub confidence: TypeConfidence,
    pub source: TypeSource,
}

pub enum TypeConfidence {
    High,    // Explicit type annotation or literal assignment
    Medium,  // Inferred from method return or flow analysis
    Low,     // Heuristic-based inference
}

pub enum TypeSource {
    Literal,           // x = "hello"
    MethodReturn,      // x = some_method()
    Parameter,         // def foo(x)
    FlowAnalysis,      // if x.is_a?(String)
    Assignment,        // x = y (where y has known type)
}
```

### Core Modules

#### 1. Type Inference Engine (`src/type_inference/`)

```
type_inference/
├── mod.rs                    # Main type inference interface
├── type_system.rs           # Core type definitions
├── inference_engine.rs      # Main inference logic
├── flow_analyzer.rs         # Control flow type analysis
├── method_analyzer.rs       # Method return type inference
├── literal_analyzer.rs      # Literal type detection
├── union_resolver.rs        # Union type operations
└── type_cache.rs           # Performance optimization cache
```

#### 2. Enhanced Analyzer (`src/analyzer_prism/`)

Extend existing analyzer with type-aware visitors:

```
visitors/
├── type_inference_visitor.rs  # Main type inference visitor
├── assignment_visitor.rs       # Track variable assignments
├── method_call_visitor.rs      # Analyze method calls for types
└── control_flow_visitor.rs     # Handle conditionals and loops
```

#### 3. Type-Aware Completion (`src/capabilities/completion/`)

```
completion/
├── type_completion.rs          # Type-based completion logic
├── method_completion.rs        # Method completions based on receiver type
└── smart_completion.rs         # Context-aware intelligent completions
```

## Implementation Plan

### Phase 1: Core Type System (Weeks 1-2)

1. **Create Type System Foundation**
   - Implement `RubyType` enum with basic types
   - Create `TypedVariable` and related structures
   - Add union type operations (merge, intersect, narrow)

2. **Literal Type Detection**
   - String literals → `RubyType::string()` (String class)
   - Numeric literals → `RubyType::integer()`/`RubyType::float()` (Integer/Float classes)
   - Boolean literals → `RubyType::true_class()`/`RubyType::false_class()` (TrueClass/FalseClass)
   - Nil literal → `RubyType::nil_class()` (NilClass)
   - Symbol literals → `RubyType::symbol()` (Symbol class)
   - Class references → `RubyType::class_reference("ClassName")` (Class objects)
   - Module references → `RubyType::module_reference("ModuleName")` (Module objects)
   - Array literals:
     - Homogeneous: `[1, 2, 3]` → `RubyType::Array(vec![RubyType::integer()])`
     - Polymorphic: `[1, 'a']` → `RubyType::Array(vec![RubyType::integer(), RubyType::string()])`
     - Empty: `[]` → `RubyType::Array(vec![RubyType::Any])`
   - Hash literals:
     - Homogeneous: `{"a" => 1, "b" => 2}` → `RubyType::Hash(vec![string()], vec![integer()])`
     - Polymorphic keys: `{a: 1, 'abc': 'abc'}` → `RubyType::Hash(vec![symbol(), string()], vec![integer(), string()])`
     - Mixed symbol/string keys: `{:name => "app", "debug" => true}` → `RubyType::Hash(vec![symbol(), string()], vec![string(), true_class()])`
     - Empty: `{}` → `RubyType::Hash(vec![RubyType::Any], vec![RubyType::Any])`

3. **Basic Assignment Tracking**
   - Track variable assignments with literal types
   - Store type information in scope tracker
   - Handle local variable type propagation

4. **Class and Module Reference Type Inference Examples**
   ```ruby
   # Class reference assignment
   klass = String
   # Type: RubyType::class_reference("String")
   
   my_class = MyCustomClass
   # Type: RubyType::class_reference("MyCustomClass")
   
   # Module reference assignment
   mod = Enumerable
   # Type: RubyType::module_reference("Enumerable")
   
   my_module = MyCustomModule
   # Type: RubyType::module_reference("MyCustomModule")
   
   # Class instantiation
   instance = klass.new
   # Type: RubyType::Class("String") - instance of String class
   
   custom_instance = my_class.new
   # Type: RubyType::Class("MyCustomClass") - instance of MyCustomClass
   
   # Class methods on references
   class_name = klass.name
   # Type: RubyType::string() - "String"
   
   parent = klass.superclass
   # Type: RubyType::class_reference("Object")
   
   # Module methods on references
   module_name = mod.name
   # Type: RubyType::string() - "Enumerable"
   
   # Dynamic class assignment
   klass = condition ? String : Integer
   # Type: RubyType::Union(vec![class_reference("String"), class_reference("Integer")])
   
   instance = klass.new
   # Type: RubyType::Union(vec![Class("String"), Class("Integer")])
   
   # Array of classes
   classes = [String, Integer, Float]
   # Type: RubyType::Array(vec![
   #     class_reference("String"),
   #     class_reference("Integer"),
   #     class_reference("Float")
   # ])
   
   instances = classes.map(&:new)
   # Type: RubyType::Array(vec![
   #     Class("String"),
   #     Class("Integer"),
   #     Class("Float")
   # ])
   
   # Array of modules
   modules = [Enumerable, Comparable]
   # Type: RubyType::Array(vec![
   #     module_reference("Enumerable"),
   #     module_reference("Comparable")
   # ])
   
   # Mixed class and module references
   mixed_refs = [String, Enumerable, Integer]
   # Type: RubyType::Array(vec![
   #     class_reference("String"),
   #     module_reference("Enumerable"),
   #     class_reference("Integer")
   # ])
   ```

5. **Polymorphic Type Inference Examples**
   ```ruby
   # Homogeneous array
   numbers = [1, 2, 3]  # Array<Integer>
   # RubyType::Array(vec![RubyType::integer()])
   
   # Polymorphic array (your example)
   mixed = [1, 'a']  # Array<Integer | String>
   # RubyType::Array(vec![RubyType::integer(), RubyType::string()])
   
   # Array operations preserve polymorphism
   mixed.push(3.14)  # Array<Integer | String | Float>
   # RubyType::Array(vec![RubyType::integer(), RubyType::string(), RubyType::float()])
   first_item = mixed.first  # Integer | String | Float | NilClass
   
   # Homogeneous hash
   scores = {"alice" => 95, "bob" => 87}  # Hash<String, Integer>
   # RubyType::Hash(vec![RubyType::string()], vec![RubyType::integer()])
   
   # Polymorphic hash (your example)
   config = {a: 1, 'abc': 'abc'}  # Hash<Symbol | String, Integer | String>
   # RubyType::Hash(
   #   vec![RubyType::symbol(), RubyType::string()],
   #   vec![RubyType::integer(), RubyType::string()]
   # )
   
   # Hash operations
   keys = config.keys    # Array<Symbol | String>
   values = config.values  # Array<Integer | String>
   
   # Method chaining with type flow
   result = [1, "2", 3.0]
     .map(&:to_s)        # Array<String>
     .select { |x| x.length > 1 }  # Array<String>
     .first              # String | NilClass
   ```

### Phase 2: Method and Flow Analysis (Weeks 3-4)

1. **Method Return Type Inference**
   - Analyze method bodies to infer return types
   - Handle multiple return paths with union types
   - Cache method signatures for performance

2. **Method Call Type Resolution**
   - Resolve receiver types for method calls
   - Infer result types based on known method signatures
   - Handle built-in Ruby method types

3. **Control Flow Analysis**
   - Type narrowing in conditional branches
   - Handle `is_a?`, `kind_of?`, `nil?` checks
   - Union type refinement in if/else blocks

## Algorithmic Considerations

### Hindley-Milner Type Inference

While Ruby's dynamic nature makes it challenging to apply classical type inference algorithms like Hindley-Milner directly, we can adapt some of its principles:

#### Core Concepts from Hindley-Milner:
1. **Unification**: Combining type constraints to find the most general type
2. **Type Variables**: Representing unknown types that can be unified later
3. **Constraint Generation**: Collecting type equations from the program
4. **Constraint Solving**: Resolving type variables through unification

#### Adaptation for Ruby:
```rust
// Type variables for unknown types
#[derive(Debug, Clone, PartialEq)]
pub enum RubyType {
    // ... existing variants ...
    TypeVariable(String),  // For unknown types during inference
    Constraint(Box<RubyType>, Vec<TypeConstraint>),  // Type with constraints
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeConstraint {
    Equals(RubyType),           // T = String
    Subtype(RubyType),          // T <: Numeric
    Responds(String),           // T responds to method_name
    Union(Vec<RubyType>),       // T ∈ {String, Integer, Symbol}
}

// Unification algorithm
impl TypeInference {
    pub fn unify(&mut self, t1: &RubyType, t2: &RubyType) -> Result<RubyType, TypeError> {
        match (t1, t2) {
            (RubyType::TypeVariable(var), concrete_type) => {
                self.substitute_type_variable(var, concrete_type)
            }
            (RubyType::Integer, RubyType::Integer) => Ok(RubyType::Integer),
            (RubyType::String, RubyType::String) => Ok(RubyType::String),
            (RubyType::Union(types1), RubyType::Union(types2)) => {
                // Find intersection or create broader union
                self.unify_unions(types1, types2)
            }
            _ => {
                // Try to find common supertype or create union
                Ok(RubyType::Union(vec![t1.clone(), t2.clone()]))
            }
        }
    }
    
    pub fn generate_constraints(&mut self, node: &AstNode) -> Vec<TypeConstraint> {
        match node {
            AstNode::MethodCall { receiver, method, args } => {
                let receiver_type = self.infer_type(receiver);
                let method_constraint = TypeConstraint::Responds(method.clone());
                vec![TypeConstraint::Constraint(receiver_type, vec![method_constraint])]
            }
            AstNode::Assignment { target, value } => {
                let value_type = self.infer_type(value);
                vec![TypeConstraint::Equals(value_type)]
            }
            // ... other node types
        }
    }
}
```

#### Benefits of HM-inspired approach:
- **Correctness**: Systematic constraint solving reduces type errors
- **Completeness**: Can infer more precise types through unification
- **Consistency**: Type variables ensure coherent type assignments
- **Gradual Typing**: Can handle partially-typed Ruby code

#### Challenges for Ruby:
- **Duck Typing**: Ruby's structural typing doesn't map directly to HM
- **Runtime Flexibility**: Method definitions can change at runtime
- **Metaprogramming**: Dynamic method creation complicates static analysis
- **Performance**: Full constraint solving may be too slow for LSP

#### Hybrid Approach:
We can combine HM principles with Ruby-specific heuristics:

```rust
pub struct HybridTypeInference {
    // Fast path for common cases
    simple_inference: SimpleTypeInference,
    // HM-based inference for complex cases
    constraint_solver: ConstraintSolver,
    // Heuristics for Ruby-specific patterns
    ruby_patterns: RubyPatternMatcher,
}

impl HybridTypeInference {
    pub fn infer_type(&mut self, node: &AstNode) -> RubyType {
        // Try simple inference first (fast)
        if let Some(simple_type) = self.simple_inference.try_infer(node) {
            return simple_type;
        }
        
        // Fall back to constraint-based inference
        let constraints = self.generate_constraints(node);
        self.constraint_solver.solve(constraints)
    }
}
```

This approach provides the benefits of formal type inference while maintaining the performance needed for an LSP.

### Other Relevant Algorithms

#### Flow-Sensitive Type Analysis
For Ruby's dynamic nature, flow-sensitive analysis can track type changes:

```ruby
x = "hello"     # x: String
if condition
  x = 42         # x: Integer in this branch
end
# x: String | Integer after the conditional
```

#### Cartesian Product Algorithm (CPA)
Useful for analyzing method calls with multiple possible receiver types:

```ruby
def process(obj)
  obj.to_s  # Need to analyze for all possible types of obj
end

process(42)      # Integer#to_s -> String
process("hi")    # String#to_s -> String
process([1,2])   # Array#to_s -> String
```

#### Abstract Interpretation
Can handle Ruby's dynamic features through abstract domains:

- **Value Domain**: Track possible values (useful for constants)
- **Type Domain**: Track possible types
- **Shape Domain**: Track object structure (for duck typing)
- **Effect Domain**: Track side effects and mutations

#### Gradual Typing (Siek & Taha)
Perfect fit for Ruby's optional typing:

```ruby
# Gradually typed Ruby
def greet(name: String) -> String
  "Hello, #{name}!"
end

def process(data)  # Untyped parameter
  data.each { |item| puts item }  # Infer from usage
end
```

#### Type State Analysis
Track object state changes through method calls:

```ruby
file = File.open("data.txt")  # file: File (open state)
content = file.read           # file: File (read state)
file.close                    # file: File (closed state)
file.read                     # Error: reading from closed file
```

### Recommended Approach for Ruby LSP

Given Ruby's characteristics and LSP performance requirements:

1. **Start Simple**: Basic type inference with literal detection and method signatures
2. **Add Constraints**: Introduce type variables and simple unification for complex cases
3. **Flow Analysis**: Add flow-sensitive analysis for conditionals and loops
4. **Gradual Enhancement**: Incrementally add more sophisticated algorithms as needed

```rust
pub struct RubyTypeInference {
    // Phase 1: Basic inference
    literal_inference: LiteralTypeInference,
    method_signatures: MethodSignatureDatabase,
    
    // Phase 2: Constraint-based
    type_variables: HashMap<String, RubyType>,
    constraints: Vec<TypeConstraint>,
    
    // Phase 3: Flow-sensitive
    control_flow: ControlFlowGraph,
    type_states: HashMap<VariableId, Vec<RubyType>>,
    
    // Phase 4: Advanced
    abstract_interpreter: AbstractInterpreter,
    shape_analyzer: ShapeAnalyzer,
}
```

This layered approach allows for incremental implementation while maintaining good performance for the LSP use case.

### Integration with Existing Ruby Type Systems

## Signature-First Type Inference Approach

Instead of inferring types purely from method usage patterns, we can adopt a **signature-first approach** where method signatures are explicitly defined and serve as the authoritative source of type information. This approach simplifies type inference by making it more declarative and provides better error reporting.

### Core Principles

1. **Explicit Signatures**: Method signatures defined above methods are the absolute truth
2. **Call-site Validation**: Validate method calls against defined signatures
3. **Return Type Validation**: Validate method returns against declared return types
4. **Error Reporting**: Show errors at call sites or return statements when types don't match

### Signature Definition Formats

#### RBS-style Signatures
```ruby
# @sig (String, Integer) -> String
def format_message(message, count)
  "#{message}: #{count}"
end

# @sig (T) -> Array[T]
def wrap_in_array(item)
  [item]
end
```

#### Sorbet-style Signatures
```ruby
sig { params(message: String, count: Integer).returns(String) }
def format_message(message, count)
  "#{message}: #{count}"
end

sig { type_parameters(:T).params(item: T.type_parameter(:T)).returns(T::Array[T.type_parameter(:T)]) }
def wrap_in_array(item)
  [item]
end
```

#### YARD-style Documentation
```ruby
# @param [String] message The message to format
# @param [Integer] count The count to include
# @return [String] The formatted message
def format_message(message, count)
  "#{message}: #{count}"
end
```

### Rust Implementation

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSignature {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: RubyType,
    pub type_parameters: Vec<String>, // For generics
    pub visibility: Visibility,
    pub source: SignatureSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub param_type: RubyType,
    pub optional: bool,
    pub keyword: bool,
    pub splat: bool, // *args
    pub double_splat: bool, // **kwargs
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignatureSource {
    RBS,
    Sorbet,
    YARD,
    Inferred, // Fallback to structural inference
}

#[derive(Debug, Clone, PartialEq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

pub struct SignatureBasedTypeInference {
    signatures: HashMap<FullyQualifiedName, MethodSignature>,
    signature_parsers: Vec<Box<dyn SignatureParser>>,
    fallback_inference: StructuralTypeInference,
}

trait SignatureParser {
    fn parse_signature(&self, method_node: &Node, source: &str) -> Option<MethodSignature>;
    fn source_type(&self) -> SignatureSource;
}

impl SignatureBasedTypeInference {
    pub fn new() -> Self {
        Self {
            signatures: HashMap::new(),
            signature_parsers: vec![
                Box::new(RBSSignatureParser::new()),
                Box::new(SorbetSignatureParser::new()),
                Box::new(YARDSignatureParser::new()),
            ],
            fallback_inference: StructuralTypeInference::new(),
        }
    }

    pub fn register_method_signature(&mut self, fqn: FullyQualifiedName, signature: MethodSignature) {
        self.signatures.insert(fqn, signature);
    }

    pub fn infer_method_call(&self, 
        receiver_type: &RubyType, 
        method_name: &str, 
        args: &[RubyType]
    ) -> TypeInferenceResult {
        let method_fqn = self.resolve_method_fqn(receiver_type, method_name);
        
        if let Some(signature) = self.signatures.get(&method_fqn) {
            // Validate call against signature
            match self.validate_call(signature, args) {
                Ok(_) => TypeInferenceResult::success(signature.return_type.clone()),
                Err(error) => TypeInferenceResult::error(error),
            }
        } else {
            // Fallback to structural inference
            self.fallback_inference.infer_method_call(receiver_type, method_name, args)
        }
    }

    fn validate_call(&self, signature: &MethodSignature, args: &[RubyType]) -> Result<(), TypeError> {
        // Validate argument count
        let required_params = signature.parameters.iter().filter(|p| !p.optional).count();
        let max_params = signature.parameters.len();
        
        if args.len() < required_params {
            return Err(TypeError::TooFewArguments {
                expected: required_params,
                actual: args.len(),
            });
        }
        
        if args.len() > max_params && !signature.parameters.iter().any(|p| p.splat || p.double_splat) {
            return Err(TypeError::TooManyArguments {
                expected: max_params,
                actual: args.len(),
            });
        }

        // Validate argument types
        for (i, arg_type) in args.iter().enumerate() {
            if let Some(param) = signature.parameters.get(i) {
                if !self.is_assignable(arg_type, &param.param_type) {
                    return Err(TypeError::ArgumentTypeMismatch {
                        parameter: param.name.clone(),
                        expected: param.param_type.clone(),
                        actual: arg_type.clone(),
                    });
                }
            }
        }

        Ok(())
    }

    pub fn validate_return_type(&self, 
        method_fqn: &FullyQualifiedName, 
        actual_return: &RubyType
    ) -> Result<(), TypeError> {
        if let Some(signature) = self.signatures.get(method_fqn) {
            if !self.is_assignable(actual_return, &signature.return_type) {
                return Err(TypeError::ReturnTypeMismatch {
                    expected: signature.return_type.clone(),
                    actual: actual_return.clone(),
                });
            }
        }
        Ok(())
    }

    fn is_assignable(&self, from: &RubyType, to: &RubyType) -> bool {
        match (from, to) {
            // Exact match
            (a, b) if a == b => true,
            
            // Nil can be assigned to any nilable type
            (RubyType::NilClass, RubyType::Union(types)) => {
                types.contains(&RubyType::NilClass)
            },
            
            // Union type assignment
            (RubyType::Union(from_types), to) => {
                from_types.iter().all(|t| self.is_assignable(t, to))
            },
            (from, RubyType::Union(to_types)) => {
                to_types.iter().any(|t| self.is_assignable(from, t))
            },
            
            // Class hierarchy (simplified)
            (RubyType::Class(from_class), RubyType::Class(to_class)) => {
                self.is_subclass(from_class, to_class)
            },
            
            // Generic types
            (RubyType::Array(from_elem), RubyType::Array(to_elem)) => {
                from_elem.iter().zip(to_elem.iter())
                    .all(|(f, t)| self.is_assignable(f, t))
            },
            
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeError {
    TooFewArguments { expected: usize, actual: usize },
    TooManyArguments { expected: usize, actual: usize },
    ArgumentTypeMismatch { parameter: String, expected: RubyType, actual: RubyType },
    ReturnTypeMismatch { expected: RubyType, actual: RubyType },
    MethodNotFound { receiver: RubyType, method: String },
    UnknownType { type_name: String },
}

#[derive(Debug, Clone)]
pub struct TypeInferenceResult {
    pub inferred_type: Option<RubyType>,
    pub errors: Vec<TypeError>,
    pub warnings: Vec<String>,
}

impl TypeInferenceResult {
    pub fn success(ruby_type: RubyType) -> Self {
        Self {
            inferred_type: Some(ruby_type),
            errors: vec![],
            warnings: vec![],
        }
    }
    
    pub fn error(error: TypeError) -> Self {
        Self {
            inferred_type: None,
            errors: vec![error],
            warnings: vec![],
        }
    }
}
```

### Benefits of Signature-First Approach

1. **Explicit Intent**: Developers explicitly declare their intent for method signatures
2. **Better Error Messages**: Precise error reporting at call sites and return statements
3. **Performance**: No need to analyze method bodies for type inference
4. **Consistency**: Same signature format across the codebase
5. **IDE Support**: Better autocomplete and refactoring support
6. **Documentation**: Signatures serve as living documentation

### Error Reporting Examples

```ruby
# @sig (String, Integer) -> String
def format_message(message, count)
  "#{message}: #{count}"
end

# Error at call site
format_message(123, "hello")  # Error: Argument type mismatch
                              # Parameter 'message': expected String, got Integer
                              # Parameter 'count': expected Integer, got String

# Error at return statement
# @sig (String) -> Integer
def parse_number(str)
  str.to_i.to_s  # Error: Return type mismatch
                 # Expected Integer, got String
end
```

### Integration Strategy

1. **Phase 1**: Implement signature parsers for RBS, Sorbet, and YARD
2. **Phase 2**: Build signature-based type checker
3. **Phase 3**: Integrate with LSP for real-time error reporting
4. **Phase 4**: Add fallback to structural inference for unsigned methods
5. **Phase 5**: Provide signature generation suggestions for unsigned methods

#### RBS (Ruby Signature)
RBS is Ruby's official type signature format that should be a primary source for type information:

```ruby
# RBS signature file (.rbs)
class User
  attr_reader name: String
  attr_reader age: Integer
  
  def initialize: (name: String, age: Integer) -> void
  def greet: () -> String
  def adult?: () -> bool
end

module Enumerable[T]
  def map: [U] () { (T) -> U } -> Array[U]
  def select: () { (T) -> bool } -> Array[T]
end
```

**Integration Strategy:**
```rust
pub struct RBSTypeProvider {
    signatures: HashMap<FullyQualifiedName, RBSClassSignature>,
    method_signatures: HashMap<(FullyQualifiedName, String), RBSMethodSignature>,
    generics: HashMap<String, Vec<RBSTypeParameter>>,
}

#[derive(Debug, Clone)]
pub struct RBSMethodSignature {
    pub parameters: Vec<RBSParameter>,
    pub return_type: RBSType,
    pub type_parameters: Vec<String>,
    pub block: Option<RBSBlockSignature>,
}

#[derive(Debug, Clone)]
pub enum RBSType {
    Class(String),
    Generic(String, Vec<RBSType>),  // Array[String]
    Union(Vec<RBSType>),            // String | Integer
    Intersection(Vec<RBSType>),     // _ToS & _ToI
    Proc(Vec<RBSType>, RBSType),    // ^(String) -> Integer
    Literal(RBSLiteral),            // "hello" | 42 | true
    Void,
    Any,
    Top,
    Bot,
}

impl TypeInference {
    pub fn load_rbs_signatures(&mut self, rbs_path: &Path) -> Result<(), RBSError> {
        let rbs_content = std::fs::read_to_string(rbs_path)?;
        let signatures = self.rbs_parser.parse(&rbs_content)?;
        
        for signature in signatures {
            self.rbs_provider.register_signature(signature);
        }
        Ok(())
    }
    
    pub fn infer_with_rbs(&mut self, node: &AstNode) -> RubyType {
        // First try RBS signatures
        if let Some(rbs_type) = self.try_rbs_inference(node) {
            return self.convert_rbs_to_ruby_type(rbs_type);
        }
        
        // Fall back to structural inference
        self.infer_type_structural(node)
    }
}
```

#### Sorbet Type System
Sorbet provides runtime and static type checking with inline type annotations:

```ruby
# Sorbet typed Ruby code
# typed: strict
class User
  extend T::Sig
  
  sig { params(name: String, age: Integer).void }
  def initialize(name, age)
    @name = T.let(name, String)
    @age = T.let(age, Integer)
  end
  
  sig { returns(String) }
  def greet
    "Hello, #{@name}!"
  end
  
  sig { returns(T::Boolean) }
  def adult?
    @age >= 18
  end
end

# Generic types
sig { type_parameters(:U).params(items: T::Array[T.type_parameter(:U)]).returns(T.type_parameter(:U)) }
def first_item(items)
  items.first
end
```

**Sorbet Integration:**
```rust
pub struct SorbetTypeProvider {
    sig_database: HashMap<MethodId, SorbetSignature>,
    type_annotations: HashMap<VariableId, SorbetType>,
    type_parameters: HashMap<String, SorbetTypeParameter>,
}

#[derive(Debug, Clone)]
pub struct SorbetSignature {
    pub params: Vec<SorbetParam>,
    pub returns: SorbetType,
    pub type_parameters: Vec<String>,
    pub abstract: bool,
    pub override: bool,
}

#[derive(Debug, Clone)]
pub enum SorbetType {
    Simple(String),                    // String, Integer
    Generic(String, Vec<SorbetType>),  // T::Array[String]
    Union(Vec<SorbetType>),            // T.any(String, Integer)
    Nilable(Box<SorbetType>),          // T.nilable(String)
    TypeParameter(String),             // T.type_parameter(:U)
    Proc(Vec<SorbetType>, SorbetType), // T.proc.params(x: String).returns(Integer)
    Boolean,                           // T::Boolean
    Void,                             // void
    Untyped,                          // T.untyped
}

impl TypeInference {
    pub fn extract_sorbet_signatures(&mut self, ast: &AstNode) {
        // Extract sig blocks and T.let annotations
        let visitor = SorbetSignatureVisitor::new();
        visitor.visit(ast, &mut |node| {
            match node {
                AstNode::MethodDef { name, sig, .. } => {
                    if let Some(sig_block) = sig {
                        let signature = self.parse_sorbet_sig(sig_block)?;
                        self.sorbet_provider.register_method_signature(name, signature);
                    }
                }
                AstNode::Assignment { target, value } => {
                    if let Some(t_let_type) = self.extract_t_let_type(value) {
                        self.sorbet_provider.register_variable_type(target, t_let_type);
                    }
                }
                _ => {}
            }
        });
    }
}
```

#### Unified Type System Integration
Combine RBS, Sorbet, and structural inference:

```rust
pub struct UnifiedTypeInference {
    rbs_provider: RBSTypeProvider,
    sorbet_provider: SorbetTypeProvider,
    structural_inference: StructuralTypeInference,
    type_cache: HashMap<AstNodeId, RubyType>,
}

impl UnifiedTypeInference {
    pub fn infer_type(&mut self, node: &AstNode) -> RubyType {
        // Check cache first
        if let Some(cached_type) = self.type_cache.get(&node.id()) {
            return cached_type.clone();
        }
        
        let inferred_type = match node {
            AstNode::MethodCall { receiver, method, .. } => {
                // Priority order: Sorbet sig > RBS signature > structural inference
                if let Some(sorbet_type) = self.sorbet_provider.get_method_type(receiver, method) {
                    self.convert_sorbet_to_ruby_type(sorbet_type)
                } else if let Some(rbs_type) = self.rbs_provider.get_method_type(receiver, method) {
                    self.convert_rbs_to_ruby_type(rbs_type)
                } else {
                    self.structural_inference.infer_method_call(receiver, method)
                }
            }
            AstNode::Variable { name } => {
                // Check for T.let annotations or RBS instance variables
                if let Some(sorbet_type) = self.sorbet_provider.get_variable_type(name) {
                    self.convert_sorbet_to_ruby_type(sorbet_type)
                } else if let Some(rbs_type) = self.rbs_provider.get_instance_variable_type(name) {
                    self.convert_rbs_to_ruby_type(rbs_type)
                } else {
                    self.structural_inference.infer_variable(name)
                }
            }
            _ => self.structural_inference.infer_type(node)
        };
        
        // Cache the result
        self.type_cache.insert(node.id(), inferred_type.clone());
        inferred_type
    }
    
    pub fn validate_type_consistency(&self) -> Vec<TypeInconsistency> {
        let mut inconsistencies = Vec::new();
        
        // Check RBS vs Sorbet conflicts
        for (method_id, rbs_sig) in &self.rbs_provider.method_signatures {
            if let Some(sorbet_sig) = self.sorbet_provider.sig_database.get(method_id) {
                if !self.signatures_compatible(&rbs_sig, &sorbet_sig) {
                    inconsistencies.push(TypeInconsistency::SignatureMismatch {
                        method: method_id.clone(),
                        rbs_type: rbs_sig.return_type.clone(),
                        sorbet_type: sorbet_sig.returns.clone(),
                    });
                }
            }
        }
        
        inconsistencies
    }
}
```

#### Benefits of Integration:
1. **Accuracy**: Leverage explicit type annotations from developers
2. **Completeness**: RBS covers standard library, Sorbet covers application code
3. **Consistency**: Validate type annotations against inferred types
4. **Performance**: Skip inference for explicitly typed code
5. **Gradual Adoption**: Works with partially typed codebases

#### Implementation Priority:
1. **RBS Integration**: Start with standard library signatures
2. **Sorbet Parsing**: Extract sig blocks and T.let annotations
3. **Type Conversion**: Map RBS/Sorbet types to internal representation
4. **Conflict Resolution**: Handle inconsistencies between type systems
5. **Incremental Updates**: Update types when signatures change

### Phase 3: Advanced Features (Weeks 5-6)

1. **Class and Module Types**
   - Track class inheritance for type relationships
   - Handle module inclusion/extension
   - Instance variable type tracking

2. **Collection Types**
   - Array element type inference
   - Hash key/value type tracking
   - Enumerable method type propagation

3. **Type-Aware Completion**
   - Method completions based on receiver type
   - Smart variable completions
   - Context-sensitive suggestions

### Phase 4: Performance and Polish (Week 7)

1. **Performance Optimization**
   - Incremental type analysis
   - Type cache implementation
   - Memory usage optimization

2. **Integration Testing**
   - End-to-end type inference tests
   - Performance benchmarks
   - Real-world codebase validation

## Technical Implementation Details

### Type Inference Visitor

```rust
pub struct TypeInferenceVisitor {
    scope_tracker: ScopeTracker,
    type_context: TypeContext,
    variable_types: HashMap<RubyVariable, RubyType>,
    method_signatures: HashMap<FullyQualifiedName, MethodSignature>,
}

impl TypeInferenceVisitor {
    fn visit_local_variable_write(&mut self, node: &LocalVariableWriteNode) {
        let var_name = node.name();
        let value_type = self.infer_expression_type(&node.value());
        
        // Create typed variable
        let variable = RubyVariable::new(var_name, RubyVariableType::Local(self.scope_tracker.get_lv_stack()));
        
        // Store type information
        self.variable_types.insert(variable, value_type);
    }
    
    fn visit_call_node(&mut self, node: &CallNode) -> RubyType {
        let receiver_type = self.infer_receiver_type(node.receiver());
        let method_name = node.name();
        
        // Look up method signature or infer from receiver type
        self.resolve_method_return_type(receiver_type, method_name)
    }
    
    fn infer_method_call(&mut self, receiver: &RubyType, method_name: &str, args: &[RubyType]) -> RubyType {
        // Special handling for class reference instantiation
        if let RubyType::ClassReference(class_name) = receiver {
            if method_name == "new" {
                // Return an instance of the class, not the class reference
                return RubyType::Class(class_name.clone());
            }
        }
        
        // Special handling for union of class references
        if let RubyType::Union(types) = receiver {
            if method_name == "new" && types.iter().all(|t| matches!(t, RubyType::ClassReference(_))) {
                let instance_types: Vec<RubyType> = types.iter()
                    .filter_map(|t| {
                        if let RubyType::ClassReference(class_name) = t {
                            Some(RubyType::Class(class_name.clone()))
                        } else {
                            None
                        }
                    })
                    .collect();
                return RubyType::Union(instance_types);
            }
        }
        
        // Special handling for array element access
        if let RubyType::Array(element_types) = receiver {
            if method_name == "first" || method_name == "last" || method_name == "[]" {
                if element_types.len() == 1 {
                    return RubyType::Union(vec![element_types[0].clone(), RubyType::nil_class()]);
                } else if element_types.len() > 1 {
                    let mut with_nil = element_types.clone();
                    with_nil.push(RubyType::nil_class());
                    return RubyType::Union(with_nil);
                }
            }
        }
        
        // Check built-in method signatures
        if let Some(signature) = self.builtin_signatures.get_signature(receiver, method_name) {
            return signature.return_type.clone();
        }
        
        // Fall back to indexed method information
        if let Some(method_info) = self.index.get_method(receiver, method_name) {
            return method_info.return_type.clone();
        }
        
        RubyType::Unknown
    }
}
```

### Union Type Operations

```rust
impl RubyType {
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
            (type1, type2) if type1 == type2 => type1,
            (type1, type2) => RubyType::Union(vec![type1, type2]),
        }
    }
    
    pub fn narrow_to(self, target_type: RubyType) -> RubyType {
        match self {
            RubyType::Union(types) => {
                let narrowed: Vec<_> = types.into_iter()
                    .filter(|t| t.is_compatible_with(&target_type))
                    .collect();
                    
                match narrowed.len() {
                    0 => RubyType::Unknown,
                    1 => narrowed.into_iter().next().unwrap(),
                    _ => RubyType::Union(narrowed),
                }
            }
            t if t.is_compatible_with(&target_type) => t,
            _ => RubyType::Unknown,
        }
    }
}

pub struct UnionTypeOps;

impl UnionTypeOps {
    pub fn union(types: Vec<RubyType>) -> RubyType {
        // Remove duplicates and flatten nested unions
        let mut flattened = Vec::new();
        for ty in types {
            match ty {
                RubyType::Union(inner) => flattened.extend(inner),
                _ => flattened.push(ty),
            }
        }
        
        // Remove duplicates
        flattened.sort();
        flattened.dedup();
        
        match flattened.len() {
            0 => RubyType::Unknown,
            1 => flattened.into_iter().next().unwrap(),
            _ => RubyType::Union(flattened),
        }
    }
    
    pub fn intersect(a: &RubyType, b: &RubyType) -> RubyType {
        // Type narrowing logic
        match (a, b) {
            (RubyType::Union(types), other) | (other, RubyType::Union(types)) => {
                let filtered: Vec<_> = types.iter()
                    .filter(|&t| Self::is_assignable_to(t, other))
                    .cloned()
                    .collect();
                Self::union(filtered)
            }
            _ if Self::is_assignable_to(a, b) => a.clone(),
            _ => RubyType::Unknown,
        }
    }
    
    // Handle polymorphic collection operations
    pub fn merge_array_element_types(arr1: &RubyType, arr2: &RubyType) -> RubyType {
        match (arr1, arr2) {
            (RubyType::Array(elem1), RubyType::Array(elem2)) => {
                let mut merged_types = elem1.clone();
                merged_types.extend(elem2.clone());
                RubyType::Array(merged_types)
            }
            _ => RubyType::Unknown,
        }
    }
    
    pub fn merge_hash_types(hash1: &RubyType, hash2: &RubyType) -> RubyType {
        match (hash1, hash2) {
            (RubyType::Hash(k1, v1), RubyType::Hash(k2, v2)) => {
                let mut merged_keys = k1.clone();
                merged_keys.extend(k2.clone());
                let mut merged_values = v1.clone();
                merged_values.extend(v2.clone());
                RubyType::Hash(merged_keys, merged_values)
            }
            _ => RubyType::Unknown,
        }
    }
    
    // Extract element type from polymorphic array
    pub fn array_element_type(array_type: &RubyType) -> RubyType {
        match array_type {
            RubyType::Array(element_types) => {
                if element_types.len() == 1 {
                    element_types[0].clone()
                } else {
                    RubyType::Union(element_types.clone())
                }
            }
            _ => RubyType::Unknown,
        }
    }
    
    // Extract key/value types from polymorphic hash
    pub fn hash_key_type(hash_type: &RubyType) -> RubyType {
        match hash_type {
            RubyType::Hash(key_types, _) => {
                if key_types.len() == 1 {
                    key_types[0].clone()
                } else {
                    RubyType::Union(key_types.clone())
                }
            }
            _ => RubyType::Unknown,
        }
    }
    
    pub fn hash_value_type(hash_type: &RubyType) -> RubyType {
        match hash_type {
            RubyType::Hash(_, value_types) => {
                if value_types.len() == 1 {
                    value_types[0].clone()
                } else {
                    RubyType::Union(value_types.clone())
                }
            }
            _ => RubyType::Unknown,
        }
    }
    
    fn is_assignable_to(from: &RubyType, to: &RubyType) -> bool {
        // Simplified assignability check
        match (from, to) {
            (a, b) if a == b => true,
            (_, RubyType::Any) => true,
            (RubyType::Unknown, _) => false,
            _ => false,
        }
    }
}
```

### Built-in Method Signatures

```rust
pub struct BuiltinSignatures {
    signatures: HashMap<(RubyType, String), MethodSignature>,
}

impl BuiltinSignatures {
    pub fn new() -> Self {
        let mut signatures = HashMap::new();
        
        // String methods
        signatures.insert(
            (RubyType::string(), "length".to_string()),
            MethodSignature::new(vec![], RubyType::integer())
        );
        signatures.insert(
            (RubyType::string(), "upcase".to_string()),
            MethodSignature::new(vec![], RubyType::string())
        );
        signatures.insert(
            (RubyType::string(), "downcase".to_string()),
            MethodSignature::new(vec![], RubyType::string())
        );
        signatures.insert(
            (RubyType::string(), "to_i".to_string()),
            MethodSignature::new(vec![], RubyType::integer())
        );
        signatures.insert(
            (RubyType::string(), "to_f".to_string()),
            MethodSignature::new(vec![], RubyType::float())
        );
        signatures.insert(
            (RubyType::string(), "to_sym".to_string()),
            MethodSignature::new(vec![], RubyType::symbol())
        );
        
        // Integer methods
        signatures.insert(
            (RubyType::integer(), "to_s".to_string()),
            MethodSignature::new(vec![], RubyType::string())
        );
        signatures.insert(
            (RubyType::integer(), "to_f".to_string()),
            MethodSignature::new(vec![], RubyType::float())
        );
        
        // Float methods
        signatures.insert(
            (RubyType::float(), "to_s".to_string()),
            MethodSignature::new(vec![], RubyType::string())
        );
        signatures.insert(
            (RubyType::float(), "to_i".to_string()),
            MethodSignature::new(vec![], RubyType::integer())
        );
        
        // Array methods (generic over element type T)
        // Note: In practice, we'd need a more sophisticated system to handle generics
        // For now, we'll register common array operations
        signatures.insert(
            (RubyType::Array(vec![RubyType::Any]), "length".to_string()),
            MethodSignature::new(vec![], RubyType::integer())
        );
        signatures.insert(
            (RubyType::Array(vec![RubyType::Any]), "size".to_string()),
            MethodSignature::new(vec![], RubyType::integer())
        );
        signatures.insert(
            (RubyType::Array(vec![RubyType::Any]), "empty?".to_string()),
            MethodSignature::new(vec![], RubyType::boolean())
        );
        signatures.insert(
            (RubyType::Array(vec![RubyType::Any]), "first".to_string()),
            MethodSignature::new(vec![], RubyType::Union(vec![RubyType::Any, RubyType::nil_class()]))
        );
        signatures.insert(
            (RubyType::Array(vec![RubyType::Any]), "last".to_string()),
            MethodSignature::new(vec![], RubyType::Union(vec![RubyType::Any, RubyType::nil_class()]))
        );
        signatures.insert(
            (RubyType::Array(vec![RubyType::Any]), "push".to_string()),
            MethodSignature::new(vec![RubyType::Any], RubyType::Array(vec![RubyType::Any]))
        );
        
        // Hash methods (generic over key type K and value type V)
        signatures.insert(
            (RubyType::Hash(vec![RubyType::Any], vec![RubyType::Any]), "length".to_string()),
            MethodSignature::new(vec![], RubyType::integer())
        );
        signatures.insert(
            (RubyType::Hash(vec![RubyType::Any], vec![RubyType::Any]), "size".to_string()),
            MethodSignature::new(vec![], RubyType::integer())
        );
        signatures.insert(
            (RubyType::Hash(vec![RubyType::Any], vec![RubyType::Any]), "empty?".to_string()),
            MethodSignature::new(vec![], RubyType::boolean())
        );
        signatures.insert(
            (RubyType::Hash(vec![RubyType::Any], vec![RubyType::Any]), "keys".to_string()),
            MethodSignature::new(vec![], RubyType::Array(vec![RubyType::Any]))
        );
        signatures.insert(
            (RubyType::Hash(vec![RubyType::Any], vec![RubyType::Any]), "values".to_string()),
            MethodSignature::new(vec![], RubyType::Array(vec![RubyType::Any]))
        );
        
        // Symbol methods
        signatures.insert(
            (RubyType::symbol(), "to_s".to_string()),
            MethodSignature::new(vec![], RubyType::string())
        );
        
        // Object methods (available on all classes)
        for ruby_type in [RubyType::string(), RubyType::integer(), RubyType::float(), 
                         RubyType::nil_class(), RubyType::symbol()] {
            signatures.insert(
                (ruby_type.clone(), "class".to_string()),
                MethodSignature::new(vec![], RubyType::Class(FullyQualifiedName::from_str("Class").unwrap()))
            );
            signatures.insert(
                (ruby_type.clone(), "nil?".to_string()),
                MethodSignature::new(vec![], RubyType::boolean())
            );
            signatures.insert(
                (ruby_type, "to_s".to_string()),
                MethodSignature::new(vec![], RubyType::string())
            );
        }
        
        // Class methods (available on class references)
        // Note: The `new` method should return an instance of the class
        // This is handled specially in the type inference engine's infer_method_call method
        // We don't register a generic signature for `new` here since it's context-dependent
        signatures.insert(
            (RubyType::class_reference("Class"), "name".to_string()),
            MethodSignature::new(vec![], RubyType::string())
        );
        signatures.insert(
            (RubyType::class_reference("Class"), "superclass".to_string()),
            MethodSignature::new(vec![], RubyType::class_reference("Class"))
        );
        
        Self { signatures }
    }
}
```

### Integration with Existing Completion

```rust
// Enhanced completion with type information
pub async fn find_typed_completion_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
    context: Option<CompletionContext>,
) -> CompletionResponse {
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), document.content.clone());
    let (identifier, namespace, lv_stack) = analyzer.get_identifier(position);
    
    // Get type information for the identifier
    let type_info = server.type_inference_engine.get_type_at_position(&uri, position);
    
    match identifier {
        Some(Identifier::RubyMethod { receiver_kind: ReceiverKind::Expr, .. }) => {
            // Use type information to provide method completions
            if let Some(receiver_type) = type_info.receiver_type {
                return provide_method_completions_for_type(receiver_type, server).await;
            }
        }
        Some(Identifier::RubyVariable { .. }) => {
            // Provide variable completions with type hints
            return provide_typed_variable_completions(type_info, server).await;
        }
        _ => {}
    }
    
    // Fall back to existing completion logic
    find_completion_at_position(server, uri, position, context).await
}
```

## Testing Strategy

### Unit Tests
- Type inference for literals
- Union type operations
- Method signature resolution
- Control flow type narrowing

### Integration Tests
- End-to-end type inference scenarios
- Performance benchmarks
- Real Ruby code analysis

### Test Cases

```ruby
# Basic type inference
x = "hello"          # x: String
y = 42               # y: Integer
z = x.length         # z: Integer

# Union types
def maybe_string(flag)
  flag ? "hello" : nil
end

result = maybe_string(true)  # result: String | nil

# Type narrowing
if result.is_a?(String)
  # result: String (narrowed from String | nil)
  puts result.upcase
end

# Method return inference
def process_data(items)
  items.map { |item| item.to_s.upcase }
end
# Returns: Array<String>

# Class types
class User
  def initialize(name)
    @name = name  # @name: String
  end
  
  def greet
    "Hello, #{@name}"  # Returns: String
  end
end

user = User.new("Alice")  # user: User
greeting = user.greet     # greeting: String
```

## Performance Considerations

### Incremental Analysis
- Only re-analyze changed files and their dependencies
- Cache type information between analysis runs
- Use dependency tracking to minimize re-computation

### Memory Management
- Limit type cache size with LRU eviction
- Use weak references for cross-file type dependencies
- Optimize union type storage to avoid duplication

### Lazy Evaluation
- Defer complex type inference until needed
- Prioritize visible code over background files
- Use progressive enhancement for type accuracy

## Future Enhancements

### Type Annotations
- Support for RBS (Ruby Signature) files
- Inline type comments (e.g., `# @type [String]`)
- Gradual typing integration

### Advanced Features
- Generic type parameters
- Structural typing for duck typing
- Type-based refactoring suggestions
- Cross-file type propagation

### IDE Integration
- Type information in hover tooltips
- Type-aware error detection
- Smart refactoring based on types
- Type visualization tools

## Migration Strategy

### Backward Compatibility
- All existing LSP features continue to work
- Type inference is additive, not replacing existing logic
- Graceful degradation when type inference fails

### Rollout Plan
1. **Alpha**: Basic literal type inference
2. **Beta**: Method call type resolution
3. **Stable**: Full union type support with performance optimization
4. **Enhanced**: Advanced features and IDE integration

## Conclusion

This type inference implementation will significantly enhance the Ruby Fast LSP by providing TypeScript-like type awareness while maintaining the dynamic nature of Ruby. The phased approach ensures steady progress while maintaining system stability and performance.

The union type system addresses Ruby's dynamic nature, while the incremental analysis approach ensures the LSP remains responsive even on large codebases. Integration with the existing architecture minimizes disruption while maximizing the benefits of type-aware tooling.