# Testing Strategy for Ruby Fast LSP

## Integration Testing

The Ruby Fast LSP project employs a comprehensive integration testing strategy designed to ensure high confidence in LSP functionality while maintaining fast execution times and actionable diagnostics.

### Test Architecture

#### Test Harness (`TestHarness`)

The integration tests are built around a centralized `TestHarness` struct located in <mcfile name="integration_test.rs" path="src/test/integration_test.rs"></mcfile>. This harness provides:

- **Server Initialization**: Creates and initializes a fresh `RubyLanguageServer` instance for each test
- **Fixture Loading**: Supports both single-file and directory-based fixture loading
- **LSP Request Simulation**: Provides helper methods for common LSP operations

Key features of the test harness:

```rust
pub struct TestHarness {
    server: RubyLanguageServer,
}

impl TestHarness {
    pub async fn new() -> Self
    pub async fn open_fixture_dir(&self, scenario: &str)
    pub fn server(&self) -> &RubyLanguageServer
}
```

#### Fixture Organization

Test fixtures are organized in <mcfolder name="fixtures" path="src/test/fixtures"></mcfolder> with the following structure:

- **Single-file scenarios**: Direct `.rb` files for isolated testing
- **Multi-file scenarios**: Subdirectories containing related Ruby files
- **Feature-based grouping**: Organized by LSP capability (goto, references, etc.)

Current fixture categories:
- `goto/` - Go-to-definition test cases
- Various Ruby language constructs (classes, modules, methods, constants)
- Cross-file reference scenarios

#### Snapshot Testing

The project uses the `insta` crate for snapshot testing, which provides:

- **JSON Response Capture**: LSP responses are serialized to JSON and compared against stored snapshots
- **Path Normalization**: Absolute file paths are replaced with `$PROJECT_ROOT` placeholders for cross-platform compatibility
- **Automatic Diff Generation**: Clear visual diffs when test expectations change

Snapshot files are stored in <mcfolder name="snapshots" path="src/test/snapshots"></mcfolder> with descriptive names like:
- `ruby_fast_lsp__test__integration_test__foo_class_def.snap`
- `ruby_fast_lsp__test__integration_test__value_const_ref.snap`

### Test Categories

#### 1. Definition Tests (`definitions.rs`)

Tests the "Go to Definition" LSP capability across various Ruby constructs:

- **Class definitions**: Navigation from class references to class declarations
- **Module definitions**: Module reference resolution
- **Constant definitions**: Constant usage to definition mapping
- **Method definitions**: Both instance and class method resolution
- **Nested namespaces**: Complex constant path resolution (e.g., `Alpha::Beta::Gamma::Foo`)

Example test structure:
```rust
#[tokio::test]
async fn goto_single_file_defs() {
    let harness = TestHarness::new().await;
    harness.open_fixture_dir("goto/const_single.rb").await;
    
    snapshot_definitions(&harness, "goto/const_single.rb", 12, 14, "foo_class_def").await;
}
```

#### 2. Reference Tests (`references.rs`)

Tests the "Find All References" LSP capability:

- **Module references**: Finding all usages of a module
- **Class references**: Locating all class instantiations and references
- **Constant references**: Tracking constant usage across files
- **Nested constant references**: Complex namespace reference tracking

#### 3. Unit Tests (`unit/`)

Focused unit tests for specific AST node processing:

- **Class node processing**: Validates proper indexing of class declarations
- **Method node processing**: Tests method definition extraction
- **Module node processing**: Ensures correct module handling
- **Edge cases**: Invalid syntax, empty bodies, deep nesting

### Testing Utilities

#### Snapshot Helper Functions

Two primary snapshot functions provide consistent testing patterns:

1. **`snapshot_definitions`**: Captures go-to-definition responses
2. **`snapshot_references`**: Captures find-references responses

Both functions:
- Accept file path, line, and character coordinates
- Execute the corresponding LSP request
- Normalize file paths for cross-platform compatibility
- Generate named snapshots for easy identification

#### Path Normalization

The `relativize_uris` function ensures test portability by:
- Converting absolute file paths to relative `$PROJECT_ROOT` references
- Handling cross-platform path separators
- Maintaining consistent snapshot format across development environments

### Test Execution Strategy

#### Fixture Loading Modes

The test harness supports two fixture loading modes:

1. **Single-file mode**: Opens one specific Ruby file for isolated testing
2. **Directory mode**: Recursively opens all `.rb` files in a directory to simulate workspace scenarios

#### Async Test Pattern

All integration tests use the `#[tokio::test]` attribute to support:
- Asynchronous LSP server operations
- Proper document indexing completion
- Realistic LSP request/response cycles

### Coverage Goals

The integration test suite aims to cover:

- **High-confidence LSP behavior**: Core functionality works correctly
- **Wide language feature coverage**: Classes, modules, mixins, metaprogramming
- **Fast execution**: Complete suite runs in under 30 seconds
- **Actionable diagnostics**: Clear failure messages and diff output

### Current Test Coverage

#### Implemented Features
- ✅ Go-to-definition for classes, modules, constants, methods
- ✅ Find references for all major Ruby constructs
- ✅ Single-file and multi-file scenarios
- ✅ Nested namespace resolution
- ✅ Cross-file reference tracking

#### Planned Expansions
Based on the integration test plan, future coverage will include:
- Hover information
- Code completion
- Document symbols
- Workspace symbols
- Diagnostics
- Semantic tokens
- Rename operations
- Formatting

### Best Practices

#### Test Organization
- Group tests by LSP capability in separate modules
- Use descriptive test names that indicate the scenario being tested
- Maintain clear separation between unit and integration tests

#### Fixture Design
- Keep fixtures minimal and focused on specific scenarios
- Use realistic Ruby code patterns
- Include both positive and negative test cases

#### Snapshot Management
- Use descriptive snapshot names that clearly indicate the test scenario
- Review snapshot changes carefully during test updates
- Ensure snapshots are platform-independent

### Maintenance Guidelines

- Every new LSP feature must include integration tests
- Keep the test harness API stable and extensible
- Monitor test execution time to maintain fast feedback cycles
- Regular review of test coverage and identification of gaps

The integration testing strategy provides a robust foundation for ensuring the Ruby Fast LSP server delivers reliable and accurate language server functionality across a wide range of Ruby language constructs and usage patterns.