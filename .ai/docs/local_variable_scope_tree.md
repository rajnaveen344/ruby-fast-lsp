# Local Variable Scope Tree - Design Document

## Current State

### How Local Variables Are Stored

```
RubyDocument
├── lvars: BTreeMap<LVScopeId, Vec<Entry>>
│   └── Entry { fqn_id, location, kind: LocalVariable(LocalVariableData) }
│
├── lvar_references: HashMap<(LVScopeId, ustr), Vec<LspLocation>>
│   └── Key: (scope_id, variable_name) → all read locations
│
└── var_types: BTreeMap<usize, HashMap<String, RubyType>>
    └── Key: offset → variable types at that point
```

### Scope Tracking (During Indexing)

```
ScopeTracker
├── frames: Vec<ScopeFrame>        # namespace/class tracking
└── lv_stack: LVScopeStack         # local variable scopes
    └── [LVScope(id, location, kind), ...]
```

**Scope Kinds:**
- `Constant` - class/module body (hard boundary)
- `InstanceMethod` - def method_name (hard boundary)
- `ClassMethod` - def self.method_name (hard boundary)
- `Block` - do...end, lambdas (NOT a hard boundary - captures outer vars)
- `Rescue` - rescue => e (special)
- `ExplicitBlockLocal` - |x; y| (explicit block-local)

---

## Problem

For rename refactoring, we need to find ALL references to a local variable, including:
1. The definition(s)
2. All reads (references)
3. All writes (assignments)
4. Variables from OUTER scopes that are captured in blocks

The current flat `LVScopeId` doesn't capture:
- Parent-child scope relationships
- Variable capture semantics (which outer variables does a block use?)
- Shadowing (is this a new variable or same as outer?)

---

## Proposed Design: Scope Tree

### New Data Structures

```rust
// New file: src/types/scope_tree.rs

use slotmap::{SlotMap, DefaultKey, OpaqueKey};
use tower_lsp::lsp_types::{Location, Range};

/// A tree structure representing the nesting of local variable scopes
/// during parsing/indexing. Each node represents a scope with its
/// variables and relationships.
pub struct ScopeTree {
    /// All scope nodes indexed by their key
    scopes: SlotMap<ScopeKey, ScopeNode>,
    /// Root scope (file-level Constant scope)
    root: ScopeKey,
}

pub struct ScopeNode {
    /// Unique identifier for this scope
    pub id: ScopeKey,
    /// Parent scope (None for root)
    pub parent: Option<ScopeKey>,
    /// Child scopes (nested blocks, methods, etc.)
    pub children: Vec<ScopeKey>,
    
    /// What kind of scope this is
    pub kind: LVScopeKind,
    
    /// Source range of this scope in the document
    pub range: Range,
    
    /// Local variables DEFINED in this scope (not from outer scopes)
    /// Key: variable name → VariableNode
    pub local_variables: HashMap<ustr::Ustr, VariableNode>,
    
    /// References to variables from OUTER scopes (captured variables)
    /// This is filled during reference gathering
    pub captured_variables: Vec<CaptureRef>,
}

/// A single local variable with its full def-use chain
#[derive(Clone)]
pub struct VariableNode {
    /// Unique identifier
    pub id: VariableKey,
    
    /// The scope where this variable is defined
    pub definition_scope: ScopeKey,
    
    /// Variable name
    pub name: ustr::Ustr,
    
    /// Definition location (first assignment)
    pub definition_location: Location,
    
    /// All locations where this variable is read/referenced
    pub read_locations: Vec<Location>,
    
    /// All locations where this variable is written (assignments)
    pub write_locations: Vec<Location>,
}

/// A reference to a variable defined in an outer scope
#[derive(Clone)]
pub struct CaptureRef {
    /// The captured variable
    pub variable_key: VariableKey,
    /// Which scope captured it
    pub captured_by_scope: ScopeKey,
    /// Where in the scope the capture occurs (for analysis)
    pub capture_location: Location,
}
```

### Key Types

```rust
// Using slotmap for efficient IDs
slotmap::new_key_type! {
    pub struct ScopeKey;
    pub struct VariableKey;
}
```

---

## Scope Tree Construction

### During Indexing (Visitor Pattern)

```rust
impl ScopeTree {
    /// Called when entering a new scope (method, block, class body, etc.)
    pub fn enter_scope(&mut self, kind: LVScopeKind, range: Range) -> ScopeKey {
        let parent = self.current_scope();
        let key = self.scopes.insert(ScopeNode {
            id: /* new key */,
            parent,
            children: Vec::new(),
            kind,
            range,
            local_variables: HashMap::new(),
            captured_variables: Vec::new(),
        });
        
        // Register as child of parent
        if let Some(p) = parent {
            self.scopes[p].children.push(key);
        }
        
        self.current = Some(key);
        key
    }
    
    /// Called when exiting a scope
    pub fn exit_scope(&mut self) {
        self.current = self.scopes[self.current?].parent;
    }
    
    /// Add a variable definition to current scope
    pub fn define_variable(&mut self, name: &str, location: Location) -> VariableKey {
        let key = self.current?;
        let var = VariableNode {
            id: /* new key */,
            definition_scope: key,
            name: ustr::ustr(name),
            definition_location: location,
            read_locations: Vec::new(),
            write_locations: Vec::new(),
        };
        
        self.scopes[key].local_variables.insert(var.name, var);
        var.id
    }
    
    /// Record a reference to a variable.
    /// If not found in current scope, walks up parent scopes.
    /// Returns (variable_key, was_captured) - captures if from outer scope
    pub fn reference_variable(&mut self, name: &str, location: Location) -> Option<(VariableKey, bool)> {
        let mut current = self.current?;
        let mut captured = false;
        
        loop {
            if let Some(var) = self.scopes[current].local_variables.get(ustr::ustr(name)) {
                // Found! Record the reference
                var.read_locations.push(location.clone());
                
                // If we came from an inner scope, this is a capture
                if captured {
                    self.scopes[current].captured_variables.push(CaptureRef {
                        variable_key: var.id,
                        captured_by_scope: /* current scope when called */,
                        capture_location: location,
                    });
                }
                
                return Some((var.id, captured));
            }
            
            // Check if this scope allows access to outer vars
            if self.scopes[current].kind.is_hard_scope_boundary() {
                return None; // Can't access outer vars
            }
            
            // Move to parent
            match self.scopes[current].parent {
                Some(p) => {
                    captured = true;
                    current = p;
                }
                None => return None,
            }
        }
    }
}
```

---

## Rename Algorithm

### Using the Scope Tree

```rust
impl ScopeTree {
    /// Find ALL references to a local variable by name in a given scope context
    /// This includes:
    /// 1. Variables defined in the same scope
    /// 2. Variables captured from outer scopes (that the current scope can see)
    pub fn find_all_references(&self, name: &str, from_scope: ScopeKey) -> Vec<ReferenceInfo> {
        let mut results = Vec::new();
        
        // Walk from current scope up to root, collecting all matches
        let mut current = Some(from_scope);
        
        while let Some(scope_key) = current {
            let scope = &self.scopes[scope_key];
            
            // Check if variable is defined here
            if let Some(var) = scope.local_variables.get(ustr::ustr(name)) {
                results.push(ReferenceInfo {
                    variable_id: var.id,
                    scope_id: scope_key,
                    definition: Some(var.definition_location.clone()),
                    reads: var.read_locations.clone(),
                    writes: var.write_locations.clone(),
                });
            }
            
            // If this is a hard scope boundary, stop
            if scope.kind.is_hard_scope_boundary() {
                break;
            }
            
            current = scope.parent;
        }
        
        results
    }
    
    /// Get all locations that need to be updated for a rename
    /// This flattens the results into a simple list of (Location, EditType)
    pub fn collect_rename_targets(&self, name: &str, from_scope: ScopeKey) -> Vec<RenameTarget> {
        let refs = self.find_all_references(name, from_scope);
        
        let mut targets = Vec::new();
        
        for r in refs {
            // Add definition
            if let Some(def) = r.definition {
                targets.push(RenameTarget {
                    location: def,
                    edit_type: EditType::Definition,
                });
            }
            
            // Add all reads
            for loc in r.reads {
                targets.push(RenameTarget {
                    location: loc,
                    edit_type: EditType::Read,
                });
            }
            
            // Add all writes
            for loc in r.writes {
                targets.push(RenameTarget {
                    location: loc,
                    edit_type: EditType::Write,
                });
            }
        }
        
        targets
    }
}
```

---

## Example: Ruby Code Analysis

```ruby
class Counter
  count = 0                    # scope: Constant (class body)
  
  def increment                # scope: InstanceMethod (hard boundary)
    count += 1                 # scope: InstanceMethod
                               # Reference to 'count' → NOT FOUND (hard boundary!)
                               # This creates a NEW local variable
    
    lambda { |n|               # scope: Block (captures)
      count + n                # scope: Block
                               # Reference to 'count' → captures from InstanceMethod? NO
                               # Wait, Block is inside InstanceMethod but InstanceMethod 
                               # is a hard boundary. This creates ANOTHER new local var
    }
  end
end
```

**With scope tree:**
1. Root scope (Constant, id=0): `count` defined at line 2
2. Method scope (InstanceMethod, id=1, parent=0): `count` NOT found (hard boundary)
   - Creates new `count` at line 4
3. Block scope (Block, id=2, parent=1): `count` NOT found in parent chain (hard boundary at 1)
   - Creates new `count` at line 6

**Result:** Three separate variables named `count`, correctly isolated.

---

## Example: Variable Capture

```ruby
def outer
  x = 1                        # scope: InstanceMethod
  
  [1,2].each do |item|         # scope: Block (child of InstanceMethod)
    puts x + item              # scope: Block
                                # Reference to 'x' → found in parent InstanceMethod
                                # This is a CAPTURE
  end
end
```

**With scope tree:**
1. Method scope (InstanceMethod, id=1): `x` defined at line 2
2. Block scope (Block, id=2, parent=1):
   - Reference to `x` at line 3
   - `x` not in local_variables
   - Walk to parent (InstanceMethod), find `x`
   - Record as `captured_variables` in Block scope

**Result:** Rename of `x` in outer scope should also update the block reference.

---

## Integration with RubyDocument

```rust
impl RubyDocument {
    /// Replace the flat lvars storage with scope tree
    pub scope_tree: Option<ScopeTree>,
    
    /// For backward compatibility during migration, keep both temporarily
    // pub lvars: BTreeMap<LVScopeId, Vec<Entry>>,
    // pub lvar_references: HashMap<(LVScopeId, ustr), Vec<LspLocation>>,
}
```

---

## Migration Path

1. **Phase 1**: Add `ScopeTree` as new field (not replacing existing)
2. **Phase 2**: Build tree during indexing alongside existing structures
3. **Phase 3**: Add rename handler using tree, compare with old implementation
4. **Phase 4**: Remove old structures once confident

---

## Benefits

1. **Rename correctness**: Captures variable references through blocks correctly
2. **Scope analysis**: Hard vs soft boundaries properly modeled
3. **Shadowing detection**: Know when inner scope shadows outer
4. **Future enhancements**: 
   - Better type inference (flow analysis)
   - "Where used" views
   - Unused variable detection
   - Variable lifetime analysis
