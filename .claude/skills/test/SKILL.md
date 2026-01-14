# Test Writing Skill

Use this skill when writing, debugging, or understanding integration tests for the Ruby Fast LSP project. Triggers: writing tests, adding tests, integration tests, test harness, test fixtures, test markers.

---

## Quick Commands

```bash
cargo test                          # Run all tests
cargo test -- --nocapture           # Run with output visible
cargo test test_name                # Run specific test
cargo test methods::                # Run tests in a module
cargo insta review                  # Review snapshot changes
```

## Test Architecture Overview

The project uses a **marker-based inline fixture testing system** located in `src/test/integration/`. Tests use embedded Ruby code with XML-like markers to define assertions.

### Directory Structure

```
src/test/integration/
├── mod.rs              # Module loader
├── classes/            # Class tests (goto, references, hover, type_hierarchy)
├── constants/          # Constant tests (goto, references)
├── methods/            # Method tests (largest, most complex)
│   └── inference/      # Return type inference tests
├── modules/            # Module tests (mixins, hover, code_lens)
└── variables/          # Variable tests by scope
    └── local/          # Local variable inference/hints
```

### Test Harness Location

Core utilities in `src/test/harness/`:

- `fixture.rs` - Marker extraction, server setup
- `check.rs` - Unified check function with auto-detection
- `inlay_hints.rs` - Inlay hint label extraction

## Writing Tests

### Single-File Test Pattern

```rust
use crate::test::harness::check;

#[tokio::test]
async fn test_goto_class() {
    check(r#"
<def>class Foo
end</def>

Foo$0.new
"#).await;
}
```

### Multi-File Test Pattern

```rust
use crate::test::harness::check_multi_file;

#[tokio::test]
async fn test_cross_file_inference() {
    check_multi_file(&[
        ("main.rb", r#"
class Main
  def greet<type label="String">
    Helper.get_name
  end
end
"#),
        ("helper.rb", r#"
class Helper
  # @return [String]
  def self.get_name
    "hello"
  end
end
"#),
    ]).await;
}
```

## Available Markers

### Navigation Markers

| Marker           | Example                | Purpose                                   |
| ---------------- | ---------------------- | ----------------------------------------- |
| `$0`             | `Foo$0.bar`            | Cursor position for goto/references/hover |
| `<def>...</def>` | `<def>class Foo</def>` | Expected goto definition target           |
| `<ref>...</ref>` | `<ref>foo</ref>`       | Expected reference location               |

### Type Assertion Markers

| Marker              | Example                         | Purpose                          |
| ------------------- | ------------------------------- | -------------------------------- |
| `<type label="T">`  | `<type label="String">`         | Verify inferred type at position |
| `<hint label="T">`  | `x<hint label="String"> = "hi"` | Expected inlay hint              |
| `<hover label="T">` | `name<hover label="String">`    | Verify hover contains label      |

### Diagnostic Markers

| Marker                | Example                           | Purpose                        |
| --------------------- | --------------------------------- | ------------------------------ |
| `<err>...</err>`      | `<err>bad code</err>`             | Expected error diagnostic      |
| `<warn>...</warn>`    | `<warn message="...">code</warn>` | Expected warning               |
| `<err none>...</err>` | `<err none>valid</err>`           | Negative assertion (no errors) |

### Other Markers

| Marker                             | Example                  | Purpose              |
| ---------------------------------- | ------------------------ | -------------------- |
| `<lens title="T">`                 | `<lens title="include">` | Expected code lens   |
| `<th supertypes="A" subtypes="B">` | With `$0` cursor         | Type hierarchy check |

## Test Examples by Feature

### Go-to-Definition Test

```rust
#[tokio::test]
async fn goto_nested_class() {
    check(r#"
module A
  <def>class B
  end</def>
end

A::B$0.new
"#).await;
}
```

### Find References Test

```rust
#[tokio::test]
async fn references_method() {
    check(r#"
class Foo
  <ref>def bar</ref>
  end

  def call_it
    <ref>bar</ref>
  end
end

Foo.new.<ref>bar$0</ref>
"#).await;
}
```

### Inlay Hints Test

```rust
#[tokio::test]
async fn inlay_hint_local_variable() {
    check(r#"
class Foo
  def test
    x<hint label="String"> = "hello"
    y<hint label="Integer"> = 42
  end
end
"#).await;
}
```

### Type Inference Test

```rust
#[tokio::test]
async fn method_return_type() {
    check(r#"
class Foo
  # @return [String]
  def greet<type label="String">
    "hello"
  end
end
"#).await;
}
```

### Hover Test

```rust
#[tokio::test]
async fn hover_shows_type() {
    check(r#"
class Foo
  # @param name [String] the name
  def greet(name<hover label="String">)
    name
  end
end
"#).await;
}
```

### Diagnostics Test

```rust
#[tokio::test]
async fn type_mismatch_warning() {
    check(r#"
class Foo
  # @return [String]
  def greet
    <warn message="Expected String">42</warn>
  end
end
"#).await;
}
```

### Negative Assertion (No Errors)

```rust
#[tokio::test]
async fn valid_code_no_errors() {
    check(r#"
<err none>
class Foo
  # @return [String]
  def greet
    "hello"
  end
end
</err>
"#).await;
}
```

### Code Lens Test

```rust
#[tokio::test]
async fn mixin_code_lens() {
    check(r#"
module M <lens title="include">
end

class Foo
  include M
end
"#).await;
}
```

### Type Hierarchy Test

```rust
#[tokio::test]
async fn class_hierarchy() {
    check(r#"
class Parent; end
class Child$0 < Parent <th supertypes="Parent" subtypes="">
end
"#).await;
}
```

## Combined Assertions

You can combine multiple markers in one test:

```rust
#[tokio::test]
async fn combined_assertions() {
    check(r#"
<def>class Foo
  # @param name [String]
  def greet(name<hover label="String">)
    msg<hint label="String"> = name
    msg
  end
end</def>

Foo$0.new.greet("test")
"#).await;
}
```

## How the Check Function Works

The `check()` function auto-detects which assertions to run based on markers present:

1. Extracts cursor position (`$0`) if present
2. Extracts all tag markers (def, ref, type, hint, etc.)
3. Sets up server with cleaned fixture text
4. Runs appropriate check functions:
   - Has `$0` + `<def>` → runs goto check
   - Has `$0` + `<ref>` → runs references check
   - Has `<type>` → runs type inference check
   - Has `<hint>` → runs inlay hints check
   - Has `<err>`/`<warn>` → runs diagnostics check
   - Has `<lens>` → runs code lens check
   - Has `<hover>` → runs hover check
   - Has `<th>` + `$0` → runs type hierarchy check

## Adding New Tests

1. **Identify the feature category** (classes, methods, variables, etc.)
2. **Find or create the appropriate test file** in `src/test/integration/`
3. **Write inline fixture** with appropriate markers
4. **Run `cargo test test_name`** to verify
5. **Use `cargo insta review`** if snapshots are involved

## Test Organization Conventions

- **Entity-driven structure**: classes/, methods/, variables/, modules/
- **Feature breakdown**: goto.rs, hover.rs, inlay_hints.rs, references.rs
- **Inference subfolder**: methods/inference/ for return type inference tests
- **Variable scopes**: variables/local/, variables/instance/, etc.

## Common Pitfalls

1. **LSP positions are 0-indexed** - Line 1 in editor = line 0 in LSP
2. **Markers must be properly nested** - No overlapping tags
3. **Cursor `$0` required for navigation tests** - goto, references, hover need it
4. **Multi-file order matters** - Files are indexed in array order
5. **Use `check_multi_file`** for cross-file scenarios, not multiple `check` calls

## Debugging Tests

```bash
# Run single test with output
cargo test test_name -- --nocapture

# Run tests matching pattern
cargo test goto -- --nocapture

# Show all test names
cargo test -- --list
```
