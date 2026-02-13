# Scope Tree Visualization

## Example 1: Simple Variable Scope

```ruby
# File: counter.rb
class Counter
  count = 0                    # ← Line 2
  
  def increment                # ← Line 4 (hard boundary)
    count += 1                # ← Line 5 (NEW variable!)
  end
  
  def decrement
    lambda { |n|
      count + n               # ← Line 9 (hard boundary blocks, NEW!)
    }
  end
end
```

### Scope Tree Structure

```
ScopeTree (counter.rb)
│
├── [Scope: Constant] id=0 (class body)
│   │
│   ├── local_variables:
│   │   └── "count" @ line 2
│   │
│   ├── children:
│   │   │
│   │   └── [Scope: InstanceMethod] id=1 "increment"
│   │       │
│   │       ├── local_variables:
│   │       │   └── "count" @ line 5  ← DIFFERENT variable!
│   │       │       (hard boundary prevents access to outer count)
│   │       │
│   │       └── children: []
│   │
│   └── children:
│       │
│       └── [Scope: InstanceMethod] id=2 "decrement"
│           │
│           ├── local_variables: []
│           │
│           └── children:
│               │
│               └── [Scope: Block] id=3 "lambda"
│                   │
│                   ├── local_variables:
│                   │   └── "n" @ line 8
│                   │
│                   └── captured_variables: []
│                       (can't capture from InstanceMethod - hard boundary!)
```

---

## Example 2: Variable Capture in Block

```ruby
# File: capture.rb
def outer_method
  x = 1                        # ← Line 2
  
  [1,2].each do |item|        # ← Line 4
    puts x + item             # ← Line 5: captures 'x' from outer
  end
end
```

### Scope Tree Structure

```
ScopeTree (capture.rb)
│
├── [Scope: InstanceMethod] id=0 "outer_method"
│   │
│   ├── local_variables:
│   │   └── "x" @ line 2
│   │       definition_location: line 2
│   │       read_locations: []
│   │       write_locations: []
│   │
│   └── children:
│       │
│       └── [Scope: Block] id=1 "each"
│           │
│           ├── local_variables:
│           │   └── "item" @ line 4 (block param)
│           │
│           └── captured_variables:
│               └── CaptureRef {
│                   variable_key: x's VariableKey,
│                   captured_by_scope: id=1,
│                   capture_location: line 5
│               }
```

### Rename Analysis

```
Wanted: rename 'x' to 'counter'

Current scope at cursor: Block (id=1)

Algorithm walks up:
1. Block (id=1): "x" not in local_variables
2. Parent: InstanceMethod (id=0): "x" FOUND!

collect_rename_targets:
├── definition: line 2 (x = 1)
└── captured:  line 5 (x + item)
```

---

## Example 3: Complex Nesting

```ruby
# File: nested.rb
def process(data)
  result = 0                   # Line 2
  
  if data
    result = data.length       # Line 4
  else
    result = 0                # Line 6
  end
  
  [1,2].map do |n|
    result + n                # Line 9: captures 'result'
  end.each do |final|
    puts final                # Line 11: captures 'result', 'final'
  end
  
  result                      # Line 13: final value
end
```

### Scope Tree Structure

```
ScopeTree (nested.rb)
│
└── [Scope: InstanceMethod] id=0 "process"
    │
    ├── local_variables:
    │   └── "result" @ line 2
    │       definition_location: line 2
    │       read_locations: [line 9, line 13]
    │       write_locations: [line 4, line 6]
    │
    └── children:
        │
        ├── [Scope: Block] id=1 "map"     (child of method)
        │   │
        │   ├── local_variables:
        │   │   └── "n" @ line 8
        │   │
        │   └── captured_variables:
        │       └── CaptureRef { variable: "result", location: line 9 }
        │
        └── [Scope: Block] id=2 "each"    (child of map block)
            │
            ├── local_variables:
            │   └── "final" @ line 10
            │
            └── captured_variables:
                ├── CaptureRef { variable: "result", location: line 11 }
                └── CaptureRef { variable: "final", location: line 11 }
```

### Rename 'result' → 'total'

```
collect_rename_targets("result", from_scope=id=0):

1. Definition:      line 2  (result = 0)
2. Write:           line 4  (result = data.length)
3. Write:           line 6  (result = 0)
4. Captured read:   line 9  (result + n)         ← from Block id=1
5. Captured read:   line 11 (puts final)        ← from Block id=2
6. Read:            line 13 (result)             ← in method scope
```

---

## Example 4: Shadowing

```ruby
# File: shadow.rb
x = "outer"                   # Line 1

def method
  x = "inner"               # Line 4 - shadows outer!
  
  [1].each do |x|           # Line 6 - shadows method's x!
    puts x                   # Line 7 - block param x
  end
  
  puts x                    # Line 9 - method's x
end

puts x                      # Line 12 - outer x
```

### Scope Tree Structure

```
ScopeTree (shadow.rb)
│
├── [Scope: Constant] id=0 (file level - root)
│   │
│   ├── local_variables:
│   │   └── "x" @ line 1 ("outer")
│   │
│   └── children:
│       │
│       └── [Scope: InstanceMethod] id=1 "method"
│           │
│           ├── local_variables:
│           │   └── "x" @ line 4 ("inner")   ← shadows line 1's x
│           │
│           └── children:
│               │
│               └── [Scope: Block] id=2 "each"
│                   │
│                   ├── local_variables:
│                   │   └── "x" @ line 6 (block param)  ← shadows method's x
│                   │
│                   └── captured_variables: []
│                       (no captures - block params shadow method vars)
```

### Rename Analysis at Different Positions

| Cursor Position | Scope | Variables Found | Which "x"? |
|-----------------|-------|----------------|------------|
| Line 7 (puts x) | Block id=2 | local "x" | block param |
| Line 9 (puts x) | Method id=1 | local "x" | line 4 |
| Line 12 (puts x) | Constant id=0 | local "x" | line 1 |

**Important:** Rename in method (line 9) should NOT affect line 12 or line 7!

---

## Data Structure Visualization

### Full Tree in Memory

```
SlotMap<ScopeKey, ScopeNode>
┌─────────┬────────────────────────────────────────────────────┐
│ Key     │ ScopeNode                                          │
├─────────┼────────────────────────────────────────────────────┤
│ id=0    │ { parent: None,    kind: Constant,    children: [1,2], ... } │
│ id=1    │ { parent: Some(0), kind: InstanceMethod, children: [3],    ... } │
│ id=2    │ { parent: Some(1), kind: Block,         children: [],       ... } │
│ id=3    │ { parent: Some(2), kind: Block,         children: [],       ... } │
└─────────┴────────────────────────────────────────────────────┘

SlotMap<VariableKey, VariableNode>
┌─────────┬────────────────────────────────────────────────────┐
│ Key     │ VariableNode                                       │
├─────────┼────────────────────────────────────────────────────┤
│ var_id0 │ { name: "result", definition_scope: id=0, ... }  │
│ var_id1 │ { name: "n",      definition_scope: id=1, ... }  │
│ var_id2 │ { name: "final",  definition_scope: id=2, ... }   │
└─────────┴────────────────────────────────────────────────────┘
```

### Lookup Path for Reference

```
User types: result  (at line 9 in block id=1)

1. current_scope = id=1 (Block "map")
   Check: scopes[id=1].local_variables.get("result") → None

2. parent = id=0 (InstanceMethod "process")  
   Check: scopes[id=0].local_variables.get("result") → FOUND var_id0!
   
3. Is hard boundary? InstanceMethod.kind = InstanceMethod → YES
   BUT we already found it! Continue.

4. Record read: var_id0.read_locations.push(line 9)
   Is captured? current != definition_scope → YES
   Add to captured_variables for scope id=1
```
