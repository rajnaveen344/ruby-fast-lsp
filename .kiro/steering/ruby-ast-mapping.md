# Ruby Language Features to AST Node Mapping

This document provides a comprehensive mapping of Ruby language features to their corresponding AST node types in the ruby-prism parser (version 1.4.0).

## Classes and Modules

### Class Definitions
- `class Foo` → `ClassNode`
- `class Foo < Bar` → `ClassNode` (with superclass)
- `class << self` → `SingletonClassNode`

### Module Definitions
- `module Foo` → `ModuleNode`

### Constants
- `A` → `ConstantReadNode`
- `A = 1` → `ConstantWriteNode`
- `A::B::C` → `ConstantPathNode`
  - `C` = `ConstantPathNode`
  - `receiver: A::B` = `ConstantPathNode`
  - `B` = `ConstantPathNode`
  - `receiver: A` = `ConstantReadNode`

### Constant Operations
- `A &&= 1` → `ConstantAndWriteNode`
- `A ||= 1` → `ConstantOrWriteNode`
- `A += 1` → `ConstantOperatorWriteNode`
- `A::B = 1` → `ConstantPathWriteNode`
- `A::B &&= 1` → `ConstantPathAndWriteNode`
- `A::B ||= 1` → `ConstantPathOrWriteNode`
- `A::B += 1` → `ConstantPathOperatorWriteNode`

## Methods

### Method Definitions
- `def foo` → `DefNode`
- `def foo(a, b = 1, *args, **kwargs, &block)` → `DefNode` with `ParametersNode`

### Method Parameters
- `def foo(a)` → `RequiredParameterNode`
- `def foo(a = 1)` → `OptionalParameterNode`
- `def foo(*args)` → `RestParameterNode`
- `def foo(a:)` → `RequiredKeywordParameterNode`
- `def foo(a: 1)` → `OptionalKeywordParameterNode`
- `def foo(**kwargs)` → `KeywordRestParameterNode`
- `def foo(&block)` → `BlockParameterNode`
- `def foo(...)` → `ForwardingParameterNode`
- `def foo(**nil)` → `NoKeywordsParameterNode`

### Method Calls
- `foo` → `CallNode`
- `obj.foo` → `CallNode`
- `obj.foo(args)` → `CallNode` with `ArgumentsNode`
- `foo(&block)` → `CallNode` with `BlockArgumentNode`
- `foo(...)` → `CallNode` with `ForwardingArgumentsNode`

### Method Call Operations
- `obj.foo &&= 1` → `CallAndWriteNode`
- `obj.foo ||= 1` → `CallOrWriteNode`
- `obj.foo += 1` → `CallOperatorWriteNode`

## Variables

### Local Variables
- `x` → `LocalVariableReadNode`
- `x = 1` → `LocalVariableWriteNode`
- `x &&= 1` → `LocalVariableAndWriteNode`
- `x ||= 1` → `LocalVariableOrWriteNode`
- `x += 1` → `LocalVariableOperatorWriteNode`

### Instance Variables
- `@x` → `InstanceVariableReadNode`
- `@x = 1` → `InstanceVariableWriteNode`
- `@x &&= 1` → `InstanceVariableAndWriteNode`
- `@x ||= 1` → `InstanceVariableOrWriteNode`
- `@x += 1` → `InstanceVariableOperatorWriteNode`

### Class Variables
- `@@x` → `ClassVariableReadNode`
- `@@x = 1` → `ClassVariableWriteNode`
- `@@x &&= 1` → `ClassVariableAndWriteNode`
- `@@x ||= 1` → `ClassVariableOrWriteNode`
- `@@x += 1` → `ClassVariableOperatorWriteNode`

### Global Variables
- `$x` → `GlobalVariableReadNode`
- `$x = 1` → `GlobalVariableWriteNode`
- `$x &&= 1` → `GlobalVariableAndWriteNode`
- `$x ||= 1` → `GlobalVariableOrWriteNode`
- `$x += 1` → `GlobalVariableOperatorWriteNode`

### Special Variables
- `it` → `ItLocalVariableReadNode`
- `$1, $2, etc.` → `NumberedReferenceReadNode`
- `$&, $+, etc.` → `BackReferenceReadNode`

## Literals

### Numeric Literals
- `42` → `IntegerNode`
- `3.14` → `FloatNode`
- `1/2r` → `RationalNode`
- `1i` → `ImaginaryNode`

### String Literals
- `"hello"` → `StringNode`
- `"hello #{name}"` → `InterpolatedStringNode`
- `'hello'` → `StringNode`
- `` `command` `` → `XStringNode`
- `` `command #{arg}` `` → `InterpolatedXStringNode`

### Symbol Literals
- `:symbol` → `SymbolNode`
- `:"symbol #{var}"` → `InterpolatedSymbolNode`

### Regular Expressions
- `/pattern/` → `RegularExpressionNode`
- `/pattern #{var}/` → `InterpolatedRegularExpressionNode`
- `/pattern/` (in conditional context) → `MatchLastLineNode`
- `/pattern #{var}/` (in conditional context) → `InterpolatedMatchLastLineNode`

### Boolean and Nil
- `true` → `TrueNode`
- `false` → `FalseNode`
- `nil` → `NilNode`

## Collections

### Arrays
- `[1, 2, 3]` → `ArrayNode`
- `%w[a b c]` → `ArrayNode`
- `%i[a b c]` → `ArrayNode`

### Hashes
- `{a: 1, b: 2}` → `HashNode`
- `{a => 1, b => 2}` → `HashNode`
- `a: 1, b: 2` (keyword arguments) → `KeywordHashNode`

### Hash Elements
- `a: 1` → `AssocNode`
- `**hash` → `AssocSplatNode`

### Splat Operations
- `*array` → `SplatNode`
- `[*array]` → `ArrayNode` with `SplatNode`

## Control Flow

### Conditionals
- `if condition` → `IfNode`
- `unless condition` → `UnlessNode`
- `condition ? true_val : false_val` → `IfNode`
- `expr if condition` → `IfNode` (modifier form)
- `expr unless condition` → `UnlessNode` (modifier form)

### Case Statements
- `case expr; when val; end` → `CaseNode` with `WhenNode`
- `case expr; in pattern; end` → `CaseMatchNode` with `InNode`

### Loops
- `while condition` → `WhileNode`
- `until condition` → `UntilNode`
- `for x in array` → `ForNode`
- `expr while condition` → `WhileNode` (modifier form)
- `expr until condition` → `UntilNode` (modifier form)

### Loop Control
- `break` → `BreakNode`
- `next` → `NextNode`
- `redo` → `RedoNode`
- `retry` → `RetryNode`
- `return` → `ReturnNode`

## Blocks and Lambdas

### Blocks
- `{ |x| x + 1 }` → `BlockNode`
- `do |x|; x + 1; end` → `BlockNode`

### Block Parameters
- `{ |a, b = 1, *rest, **kwargs, &block| }` → `BlockParametersNode`
- Block local variables: `{ |x; local| }` → `BlockLocalVariableNode`

### Lambda
- `-> { }` → `LambdaNode`
- `lambda { }` → `CallNode` (method call to lambda)

### Parameter Types in Blocks
- Numbered parameters (`$1, $2`) → `NumberedParametersNode`
- `it` parameter → `ItParametersNode`

## Exception Handling

### Begin/Rescue/Ensure
- `begin; rescue; ensure; end` → `BeginNode`
- `rescue StandardError => e` → `RescueNode`
- `ensure` → `EnsureNode`
- `else` → `ElseNode`
- `expr rescue fallback` → `RescueModifierNode`

## Pattern Matching

### Pattern Types
- `case expr; in pattern; end` → `CaseMatchNode`
- `expr => pattern` → `MatchRequiredNode`
- `expr in pattern` → `MatchPredicateNode`
- `[a, *rest, b]` → `ArrayPatternNode`
- `{a:, **rest}` → `HashPatternNode`
- `pattern | other` → `AlternationPatternNode`
- `pattern => var` → `CapturePatternNode`
- `^expr` → `PinnedExpressionNode`
- `^var` → `PinnedVariableNode`
- `[*prefix, target, *suffix]` → `FindPatternNode`

## Operators and Expressions

### Logical Operators
- `a && b` → `AndNode`
- `a || b` → `OrNode`
- `a and b` → `AndNode`
- `a or b` → `OrNode`

### Range Operators
- `1..10` → `RangeNode`
- `1...10` → `RangeNode`
- `1..10` (flip-flop) → `FlipFlopNode`

### Assignment Targets
- Multiple assignment: `a, b = 1, 2` → `MultiWriteNode` with `MultiTargetNode`
- Array indexing: `arr[i] = val` → `IndexTargetNode`
- Method call: `obj.method = val` → `CallTargetNode`

### Index Operations
- `obj[key] &&= val` → `IndexAndWriteNode`
- `obj[key] ||= val` → `IndexOrWriteNode`
- `obj[key] += val` → `IndexOperatorWriteNode`

## Special Constructs

### Alias
- `alias new_name old_name` → `AliasMethodNode`
- `alias $new $old` → `AliasGlobalVariableNode`

### Undef
- `undef method_name` → `UndefNode`

### Super
- `super` → `ForwardingSuperNode`
- `super(args)` → `SuperNode`

### Yield
- `yield` → `YieldNode`
- `yield(args)` → `YieldNode`

### Defined
- `defined?(expr)` → `DefinedNode`

### Special Keywords
- `self` → `SelfNode`
- `__FILE__` → `SourceFileNode`
- `__LINE__` → `SourceLineNode`
- `__ENCODING__` → `SourceEncodingNode`

### BEGIN/END
- `BEGIN { }` → `PreExecutionNode`
- `END { }` → `PostExecutionNode`

## Interpolation and Embedding

### String Interpolation
- `"#{expr}"` → `EmbeddedStatementsNode`
- `"#{@var}"` → `EmbeddedVariableNode`

## Special Nodes

### Structural Nodes
- Top-level program → `ProgramNode`
- Statement sequences → `StatementsNode`
- Parenthesized expressions → `ParenthesesNode`
- Missing syntax → `MissingNode`
- Implicit nodes → `ImplicitNode`
- Implicit rest → `ImplicitRestNode`

### Assignment Context Nodes
- Variable targets in assignment contexts:
  - `LocalVariableTargetNode`
  - `InstanceVariableTargetNode`
  - `ClassVariableTargetNode`
  - `GlobalVariableTargetNode`
  - `ConstantTargetNode`
  - `ConstantPathTargetNode`

### Match Context
- `=~ /pattern/` → `MatchWriteNode` (when creating local variables from named captures)

### Shareable Constants
- Constants with `# shareable_constant_value` → `ShareableConstantNode`

---

*This mapping is based on ruby-prism version 1.4.0. For the most up-to-date information, refer to the [official documentation](https://docs.rs/ruby-prism/1.4.0/ruby_prism/).*
