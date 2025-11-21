# Initialization Improvements - Faster Syntax Highlighting

## Summary

This update significantly improves the initial load time of the Ruby Fast LSP by making syntax highlighting available immediately while workspace indexing happens in the background.

## Key Changes

### 1. **Moved Workspace Indexing to Background Task**

**Before:** Workspace indexing happened during the `initialize` LSP call, blocking the initialization response for ~10 seconds.

**After:** 
- The LSP responds to `initialize` immediately with its capabilities
- Workspace indexing starts in the background during the `initialized` notification
- Syntax highlighting works immediately since it only requires parsing the current file

### 2. **Added Progress Reporting**

The LSP now sends detailed progress notifications to the VSCode extension during indexing:

- **Phase Detection** (5%): "Detecting Ruby version..."
- **Library Discovery** (10%): "Discovering Ruby library paths..."
- **Project Indexing** (20%): "Indexing project files..."
- **Stdlib Indexing** (50%): "Indexing Ruby standard library..."
- **Gem Indexing** (70%): "Indexing gems..."
- **Mixin Resolution** (80%): "Resolving mixin references..."
- **Reference Indexing** (90%): "Indexing references..."
- **Finalization** (95%): "Finalizing..."
- **Completion** (100%): "Indexing complete"

### 3. **Enhanced VSCode Extension**

The extension now displays a status bar item showing:
- A spinning icon during indexing: `$(sync~spin) Ruby Fast LSP: Indexing workspace...`
- Progress percentage and current phase
- A checkmark when complete: `$(check) Ruby Fast LSP: Indexing complete`
- Auto-hides after 3 seconds of completion

## Files Changed

### Rust (LSP Server)

1. **`src/server.rs`**
   - Added `workspace_uri` field to store workspace URI for later indexing
   - Added `set_workspace_uri()` and `get_workspace_uri()` methods

2. **`src/handlers/notification.rs`**
   - Modified `handle_initialize()`: Stores workspace URI but doesn't block on indexing
   - Modified `handle_initialized()`: Spawns background task for workspace indexing with progress notifications

3. **`src/indexer/coordinator.rs`**
   - Modified `run_complete_indexing()`: Now sends progress updates at each phase
   - Added `send_progress()` helper method for sending progress notifications

### JavaScript (VSCode Extension)

4. **`vsix/extension.js`**
   - Added progress notification listener that displays status bar updates
   - Shows spinning icon during indexing with percentage and phase information
   - Auto-hides status bar after completion

## Benefits

1. **Immediate Syntax Highlighting**: Users see syntax highlighting as soon as they open a Ruby file, with no waiting
2. **Transparent Progress**: Users can see exactly what the LSP is doing during initialization
3. **Better UX**: The LSP feels more responsive and users understand what's happening
4. **Non-Blocking**: Other LSP features (like diagnostics) work immediately for open files while indexing happens in the background

## Testing

### Build the Project
```bash
cargo build --release
```

### Run Tests
```bash
cargo test --release -- --test-threads=1
```

### Manual Testing
1. Open a Ruby project in VSCode with the Ruby Fast LSP extension
2. Observe that syntax highlighting appears immediately when opening a Ruby file
3. Look at the status bar (bottom left) to see the indexing progress
4. The status bar should show: "$(sync~spin) Ruby Fast LSP: Indexing workspace... X%"
5. After indexing completes, it should show "$(check) Ruby Fast LSP: Indexing complete" and then disappear

### Create VSIX and Install
```bash
./create_vsix.sh --current-platform-only
# Then install the generated .vsix file in VSCode
```

## Technical Details

### LSP Protocol Used

The implementation uses the standard LSP `$/progress` notification:

```rust
client.send_notification::<tower_lsp::lsp_types::notification::Progress>(
    tower_lsp::lsp_types::ProgressParams {
        token: NumberOrString::String("indexing".to_string()),
        value: ProgressParamsValue::WorkDone(
            WorkDoneProgress::Report(WorkDoneProgressReport {
                message: Some(message),
                percentage: Some(percentage),
                cancellable: Some(false),
            })
        ),
    }
).await
```

### Why Syntax Highlighting Works Immediately

The semantic tokens implementation (syntax highlighting) in `src/capabilities/semantic_tokens.rs` only requires parsing the current file's content using the Ruby Prism parser. It doesn't depend on the workspace index, so it works immediately even before indexing completes.

### Background Indexing

The workspace indexing runs in a separate Tokio task spawned during the `initialized` notification. This allows the LSP to respond to other requests (like semantic tokens, diagnostics, etc.) while indexing happens asynchronously.

## Backward Compatibility

This change is fully backward compatible. All existing features continue to work as before, but with better initial responsiveness:

- Go-to-definition: Works immediately for the current file, and for cross-file references once indexing completes
- Find references: Same as above
- Completion: Works immediately for local variables, and for project-wide symbols once indexing completes
- Syntax highlighting: Works immediately

## Future Enhancements

Potential future improvements:
1. Add cancellation support for the indexing process
2. Implement incremental indexing for faster updates
3. Add configuration option to disable/enable progress notifications
4. Show more detailed statistics (e.g., "Indexed 500/1000 files")

