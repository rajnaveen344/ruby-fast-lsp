# Error Handling Skill

Use this skill when implementing error handling, debugging error flows, or standardizing error patterns in the Ruby Fast LSP project. Provides consistent patterns for Result vs Option, error logging, and graceful degradation. Triggers: error handling, Result, Option, error, exception, failure, logging errors, error pattern.

---

## Core Principle

> "Almost all catastrophic system failures result from incorrect handling of non-fatal errors explicitly signaled in software."

**Never ignore errors. Every error must be:**

1. Propagated to caller, OR
2. Logged with context, OR
3. Explicitly documented as intentionally ignored (rare)

---

## When to Use Result vs Option

### Use `Result<T, E>` When:

- Operation can fail due to external factors
- Caller needs to know WHY it failed
- Error should be propagated up
- Operation is expected to succeed normally

```rust
// File operations
fn read_config(path: &Path) -> Result<Config, ConfigError>

// Network operations
fn send_request(req: Request) -> Result<Response, NetworkError>

// Parsing operations
fn parse_ruby(source: &str) -> Result<Ast, ParseError>

// Validation
fn validate_fqn(name: &str) -> Result<Fqn, ValidationError>
```

### Use `Option<T>` When:

- Absence is normal/expected, not an error
- Lookup that may not find anything
- Optional configuration or feature

```rust
// Lookups (not finding is normal)
fn find_definition(name: &str) -> Option<Definition>

// Optional values
fn get_documentation(symbol: &Symbol) -> Option<String>

// First/last operations
fn first_method(&self) -> Option<&Method>

// Configuration with defaults
fn get_custom_path(&self) -> Option<PathBuf>
```

---

## Error Type Design

### Custom Error Types

Create domain-specific error types:

```rust
// src/errors.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("File not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("Parse error in {file}: {message}")]
    ParseError { file: PathBuf, message: String },

    #[error("Invalid FQN: {name}")]
    InvalidFqn { name: String },

    #[error("Index locked")]
    IndexLocked,
}

#[derive(Error, Debug)]
pub enum QueryError {
    #[error("Document not indexed: {uri}")]
    DocumentNotIndexed { uri: Url },

    #[error("Position out of bounds: line {line}, char {character}")]
    PositionOutOfBounds { line: u32, character: u32 },
}
```

### Error Context

Always add context when propagating:

```rust
use anyhow::{Context, Result};

// GOOD: Context added
fn load_workspace(root: &Path) -> Result<Workspace> {
    let config = read_config(root)
        .context("Failed to read workspace configuration")?;

    let files = discover_files(root)
        .with_context(|| format!("Failed to discover files in {}", root.display()))?;

    Ok(Workspace { config, files })
}

// BAD: No context
fn load_workspace(root: &Path) -> Result<Workspace> {
    let config = read_config(root)?;  // Which file?
    let files = discover_files(root)?; // What went wrong?
    Ok(Workspace { config, files })
}
```

---

## Error Handling Patterns

### Pattern 1: Propagate with ?

For errors that should bubble up:

```rust
fn process_file(path: &Path) -> Result<ProcessedFile> {
    let content = std::fs::read_to_string(path)?;
    let ast = parse(&content)?;
    let symbols = extract_symbols(&ast)?;
    Ok(ProcessedFile { path: path.to_owned(), symbols })
}
```

### Pattern 2: Convert with map_err

For error type conversion:

```rust
fn find_definition(name: &str) -> Result<Location, QueryError> {
    self.index
        .get(name)
        .ok_or_else(|| QueryError::NotFound { name: name.to_string() })
}
```

### Pattern 3: Log and Continue

For non-critical operations:

```rust
fn index_file(&mut self, path: &Path) {
    match self.process_file(path) {
        Ok(processed) => {
            self.add_to_index(processed);
        }
        Err(e) => {
            warn!("Failed to index {}: {}", path.display(), e);
            // Continue with other files
        }
    }
}
```

### Pattern 4: Log and Return Default

For optional enhancements:

```rust
fn get_hover_info(&self, pos: Position) -> Option<Hover> {
    match self.compute_hover(pos) {
        Ok(hover) => Some(hover),
        Err(e) => {
            debug!("Hover computation failed: {}", e);
            None  // Graceful degradation
        }
    }
}
```

### Pattern 5: Collect Errors

For batch operations:

```rust
fn index_workspace(&mut self, files: &[PathBuf]) -> IndexResult {
    let mut errors = Vec::new();
    let mut indexed = 0;

    for file in files {
        match self.index_file(file) {
            Ok(()) => indexed += 1,
            Err(e) => errors.push((file.clone(), e)),
        }
    }

    IndexResult { indexed, errors }
}
```

---

## Logging Standards

### Log Levels

| Level    | When to Use           | Example                             |
| -------- | --------------------- | ----------------------------------- |
| `error!` | Unrecoverable failure | "Failed to start server"            |
| `warn!`  | Recoverable failure   | "Failed to index file, skipping"    |
| `info!`  | Significant events    | "Indexed 500 files in 2.3s"         |
| `debug!` | Development info      | "Processing method call at line 42" |
| `trace!` | Verbose tracing       | "Entering visit_node"               |

### Error Logging Format

Always include:

1. What operation failed
2. Why it failed (error message)
3. Context (file, position, symbol name)

```rust
// GOOD
warn!(
    "Failed to resolve constant {} in {}: {}",
    constant_name,
    file_path.display(),
    error
);

// BAD
warn!("Error: {}", error);  // No context
```

### Performance Logging

Use consistent format for timing:

```rust
let start = Instant::now();
let result = expensive_operation();
info!("[PERF] Indexing completed in {:?}", start.elapsed());
```

---

## LSP-Specific Error Handling

### Request Handlers

Return `Result` with JSON-RPC errors:

```rust
use tower_lsp::jsonrpc::{Error, Result};

pub async fn handle_definition(
    server: &Server,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = &params.text_document_position_params.text_document.uri;

    // Document not found is not an error, just no result
    let doc = match server.get_document(uri) {
        Some(doc) => doc,
        None => return Ok(None),
    };

    // Internal errors should be logged but return gracefully
    match server.find_definition(&doc, params.position) {
        Ok(location) => Ok(location.map(GotoDefinitionResponse::Scalar)),
        Err(e) => {
            error!("Definition lookup failed: {}", e);
            Ok(None)  // Graceful degradation
        }
    }
}
```

### Notification Handlers

Log errors but don't fail:

```rust
pub async fn handle_did_save(
    server: &Server,
    params: DidSaveTextDocumentParams,
) {
    let uri = &params.text_document.uri;

    if let Err(e) = server.reindex_document(uri).await {
        warn!("Failed to reindex {} on save: {}", uri, e);
        // Don't propagate - editor doesn't expect response
    }
}
```

---

## Anti-Patterns to Avoid

### 1. Silent Swallowing

```rust
// BAD
let _ = file.write_all(data);

// GOOD
if let Err(e) = file.write_all(data) {
    warn!("Failed to write cache: {}", e);
}
```

### 2. Panic in Library Code

```rust
// BAD
fn get_index(&self) -> &Index {
    self.index.as_ref().unwrap()  // Panics!
}

// GOOD
fn get_index(&self) -> Option<&Index> {
    self.index.as_ref()
}

// OR with invariant assertion
fn get_index(&self) -> &Index {
    self.index.as_ref().expect("Index must be initialized before use")
}
```

### 3. Unwrap in Async Code

```rust
// BAD - Panics crash the server
async fn handle_request(&self) {
    let doc = self.docs.lock().unwrap();  // Don't unwrap locks
}

// GOOD
async fn handle_request(&self) {
    let doc = match self.docs.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            error!("Lock poisoned, recovering");
            poisoned.into_inner()
        }
    };
}
```

### 4. Generic Error Messages

```rust
// BAD
Err(anyhow!("Failed"))

// GOOD
Err(anyhow!("Failed to parse method at {}:{}", file, line))
```

---

## Error Handling Checklist

### For Every Function

- [ ] Return type is appropriate (Result vs Option)
- [ ] Errors include context (file, line, symbol name)
- [ ] No silent swallowing of errors
- [ ] No unwrap() in production paths

### For Error Types

- [ ] Domain-specific error types defined
- [ ] Error messages are actionable
- [ ] Errors implement std::error::Error

### For Logging

- [ ] Appropriate log level used
- [ ] Context included in message
- [ ] Performance-sensitive paths use debug!/trace!
