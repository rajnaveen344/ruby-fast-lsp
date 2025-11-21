# Module CodeLens Implementation Summary

## Overview

Successfully implemented the CodeLens feature for Ruby modules that shows how many times a module is used via `include`, `prepend`, and `extend` across the workspace.

## Implementation Details

### 1. Core Data Structures

#### MixinType Enum (`src/indexer/entry/mod.rs`)

```rust
pub enum MixinType {
    Include,
    Prepend,
    Extend,
}
```

#### MixinUsage Struct (`src/indexer/index.rs`)

```rust
pub struct MixinUsage {
    pub user_fqn: FullyQualifiedName,
    pub mixin_type: MixinType,
    pub location: Location,
}
```

### 2. Index Updates

Enhanced `RubyIndex` to track detailed mixin usage information:

- Added `mixin_usages: HashMap<FullyQualifiedName, Vec<MixinUsage>>` field
- Updated `update_reverse_mixins()` to track mixin types and locations
- Added `get_mixin_usages()` method for CodeLens queries
- Updated `remove_entries_for_uri()` to clean up mixin usages

### 3. CodeLens Provider (`src/capabilities/code_lens.rs`)

Implemented a complete CodeLens provider with:

- **ModuleCodeLensVisitor**: AST visitor that finds module definitions
- **FQN Resolution**: Computes fully-qualified names respecting Ruby's namespace rules
- **Usage Counting**: Aggregates mixin usages by type
- **Label Formatting**: Displays counts as "X include | Y prepend | Z extend"
- **References Integration**: Clicking CodeLens opens references view

Key features:

- Handles nested modules (e.g., `Outer::Inner`)
- Handles namespaced module declarations (e.g., `module A::B`)
- Only shows CodeLens when at least one usage exists
- Respects the `Object` top-level namespace

### 4. LSP Integration

#### Server Handler (`src/server.rs`)

- Added `code_lens()` async method to `LanguageServer` trait implementation
- Integrated with request routing

#### Request Handler (`src/handlers/request.rs`)

- Added `handle_code_lens()` function
- Includes performance logging

#### Capability Registration (`src/handlers/notification.rs`)

- Registered `code_lens_provider` in server capabilities
- Set `resolve_provider: false` (no lazy resolution needed)

### 5. Configuration Support (`src/config.rs`)

Added configuration option:

```rust
#[serde(rename = "codeLens.modules.enabled")]
pub code_lens_modules_enabled: Option<bool>  // Default: true
```

### 6. Testing (`src/test/code_lens.rs`)

Comprehensive test suite covering:

- ✅ Basic include usage
- ✅ Basic prepend usage
- ✅ Basic extend usage
- ✅ Multiple mixin categories
- ✅ No usages (no CodeLens shown)
- ✅ Nested modules

All tests passing.

## Usage Examples

### Example 1: Single Include

```ruby
module MyModule
end

class MyClass
  include MyModule
end
```

**CodeLens**: `1 include`

### Example 2: Multiple Categories

```ruby
module MyModule
end

class MyClass
  include MyModule
end

class AnotherClass
  extend MyModule
end

module AnotherModule
  prepend MyModule
end
```

**CodeLens**: `1 include | 1 prepend | 1 extend`

### Example 3: Nested Module

```ruby
module Outer
  module Inner
  end
end

class MyClass
  include Outer::Inner
end
```

**CodeLens on Inner**: `1 include`

## Technical Decisions

1. **Mixin Location Storage**: Currently stores the location of the class/module definition that uses the mixin, not the exact location of the `include`/`prepend`/`extend` call. This is sufficient for the references view.

2. **Namespace Handling**: The CodeLens visitor starts with `Object` as the top-level namespace to match Ruby's implicit namespace structure.

3. **Performance**: Uses the existing index infrastructure, so queries are O(1) lookups with O(k) iteration over usages.

4. **Command Integration**: Uses the standard `editor.action.showReferences` command, which is supported by VS Code and other LSP clients.

## Files Modified

- `src/indexer/entry/mod.rs` - Added MixinType enum
- `src/indexer/index.rs` - Added MixinUsage tracking
- `src/capabilities/code_lens.rs` - New CodeLens provider
- `src/capabilities/mod.rs` - Registered code_lens module
- `src/handlers/request.rs` - Added CodeLens handler
- `src/handlers/notification.rs` - Registered capability
- `src/server.rs` - Added LanguageServer trait method
- `src/config.rs` - Added configuration option
- `src/test/code_lens.rs` - New test suite
- `src/test/mod.rs` - Registered test module

## Bug Fixes

### Issue: Command Argument Serialization Error

**Problem**: When clicking the CodeLens, VS Code showed an error: "argument does not match one of these constraints"

**Root Cause**: VS Code's `editor.action.showReferences` command expects proper VS Code types (`vscode.Uri`, `vscode.Position`, `vscode.Location`), but LSP communication serializes everything as JSON. The JSON-serialized arguments don't match VS Code's type constraints.

**Solution**: Created a wrapper command in the VS Code extension that converts JSON arguments to proper VS Code types. This is the standard solution for LSP servers invoking VS Code commands.

**Implementation**:

1. **VS Code Extension** (`vsix/extension.js`): Registered custom command

```javascript
vscode.commands.registerCommand(
  "ruby-fast-lsp.showReferences",
  (uriStr, position, locations) => {
    const uri = vscode.Uri.parse(uriStr);
    const pos = new vscode.Position(position.line, position.character);
    const locs = locations.map(
      (loc) =>
        new vscode.Location(
          vscode.Uri.parse(loc.uri),
          new vscode.Range(
            new vscode.Position(
              loc.range.start.line,
              loc.range.start.character
            ),
            new vscode.Position(loc.range.end.line, loc.range.end.character)
          )
        )
    );
    return vscode.commands.executeCommand(
      "editor.action.showReferences",
      uri,
      pos,
      locs
    );
  }
);
```

2. **Rust LSP Server** (`src/capabilities/code_lens.rs`): Use custom command

```rust
command: "ruby-fast-lsp.showReferences".to_string()
```

## Enhancements

### ✅ Precise Call Locations (Implemented)

The references now point to the exact `include`/`prepend`/`extend` call lines, not the entire class/module definition. This makes it much easier to see exactly where a module is being mixed in.

**Implementation**: Modified `IndexVisitor` to track the `CallNode` location when processing mixin calls, storing the precise location in `MixinUsage`.

## Future Enhancements

Potential improvements for future versions:

1. **Category Filtering**: Allow filtering references by mixin type in the references view
2. **External Gems**: Optional configuration to include/exclude usages from external gems
3. **Refinements Support**: Add support for `using` (refinements) in a separate category
4. **Performance Optimization**: Cache CodeLens results per document with invalidation on index updates

## Compliance with Requirements

✅ All functional requirements met:

- Detects module definitions including nested and namespaced forms
- Computes fully-qualified module names correctly
- Queries index for mixin usages (include, prepend, extend)
- Counts usages per mixin type
- Renders CodeLens label correctly
- Does not render CodeLens for modules with zero usages
- Supports cross-file usages

✅ All non-functional requirements met:

- Performance: O(1) index lookups
- Correctness: Name resolution respects Ruby's constant lookup rules
- Robustness: Gracefully handles missing index data

✅ Configuration support:

- `rubyFastLSP.codeLens.modules.enabled` (default: true)

✅ All test criteria met:

- Single usage cases work correctly
- Multiple categories render correctly
- Zero-count categories are omitted
- Cross-file usages are counted
- Nested/namespaced modules resolve properly
