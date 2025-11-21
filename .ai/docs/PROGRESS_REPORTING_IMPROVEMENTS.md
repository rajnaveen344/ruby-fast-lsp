# Progress Reporting Improvements - File-Based Progress Tracking

## Summary

Enhanced the Ruby Fast LSP progress reporting to show accurate file counts and progress similar to rust-analyzer, replacing arbitrary percentages with real-time tracking of indexed files.

## Key Improvements

### **Before:**
- Progress used hardcoded percentages (5%, 20%, 50%, etc.)
- No indication of how many files were being processed
- Generic messages like "Indexing project files... 20%"
- No way to track actual progress

### **After:**
- Real-time file counting: `"Indexing: 3/100"`
- Module-based tracking for stdlib: `"Indexing Stdlib: 5/20"`
- Gem-based tracking: `"Indexing Gems: 2/10"`
- Reference tracking: `"Collecting References: 44/100"`
- Accurate percentage calculation based on actual progress

## Progress Messages

The LSP now shows progress in the following format:

1. **Project Indexing**: `Indexing: X/Y (Z%)`
   - Shows actual number of files indexed vs total project files

2. **Stdlib Indexing**: `Indexing Stdlib: X/Y (Z%)`
   - Shows number of stdlib modules indexed vs total required modules

3. **Gem Indexing**: `Indexing Gems: X/Y (Z%)`
   - Shows number of gems indexed vs total required gems

4. **Reference Collection**: `Collecting References: X/Y (Z%)`
   - Shows actual number of files processed for reference indexing

5. **Mixin Resolution**: `Resolving mixins...`
   - One-time operation after all definitions are indexed

## Implementation Details

### Progress Reporting Function

Added a centralized progress reporting function in `IndexingCoordinator`:

```rust
pub async fn send_progress_report(
    server: &RubyLanguageServer,
    message: String,
    current: usize,
    total: usize
)
```

This function:
- Calculates percentage based on current/total
- Formats message as "Message: current/total"
- Sends LSP progress notification with both message and percentage
- Only sends when total > 0 to avoid division by zero

### Indexer Updates

**1. Project Indexer (`indexer_project.rs`)**
- Counts total project files upfront
- Reports progress for each file during:
  - Definition indexing: `"Indexing: X/Y"`
  - Reference indexing: `"Collecting References: X/Y"`

**2. Stdlib Indexer (`indexer_stdlib.rs`)**
- Tracks total number of required modules
- Reports progress per module: `"Indexing Stdlib: X/Y"`

**3. Gem Indexer (`indexer_gem.rs`)**
- Tracks total number of required gems
- Reports progress per gem: `"Indexing Gems: X/Y"`

**4. Coordinator (`coordinator.rs`)**
- Orchestrates all indexing phases
- Passes server reference to indexers for progress reporting
- No longer uses arbitrary percentages

### VSCode Extension Updates

**extension.js** now displays progress more cleanly:
- Shows spinning icon: `$(sync~spin) Indexing: 15/100 (15%)`
- Percentage is shown in parentheses alongside the count
- Completion message: `$(check) Ruby Fast LSP: Indexing complete`

## Files Changed

### Rust (LSP Server)

1. **`src/indexer/coordinator.rs`**
   - Changed `send_progress` to `send_progress_report` with file count parameters
   - Removed hardcoded percentages
   - Added server parameter to `index_gems` method
   - Calculates percentage from actual counts

2. **`src/indexer/indexer_project.rs`**
   - Added `total_files` parameter to indexing methods
   - Reports progress for each file indexed
   - Uses `IndexingCoordinator::send_progress_report` for updates

3. **`src/indexer/indexer_stdlib.rs`**
   - Added `total_modules` parameter to `index_required_modules`
   - Reports progress for each stdlib module
   - Tracks module count during indexing

4. **`src/indexer/indexer_gem.rs`**
   - Added server parameter to `index_gems` and related methods
   - Added `total_gems` parameter for progress tracking
   - Reports progress for each gem indexed

### JavaScript (VSCode Extension)

5. **`vsix/extension.js`**
   - Improved progress message formatting
   - Shows percentage in parentheses: `(15%)`
   - Cleaner message display

## Example Progress Flow

When indexing a project with 100 files, 5 stdlib modules, and 3 gems:

```
$(sync~spin) Ruby Fast LSP: Indexing workspace...
$(sync~spin) Indexing: 1/100 (1%)
$(sync~spin) Indexing: 25/100 (25%)
$(sync~spin) Indexing: 50/100 (50%)
$(sync~spin) Indexing: 100/100 (100%)
$(sync~spin) Indexing Stdlib: 1/5 (20%)
$(sync~spin) Indexing Stdlib: 3/5 (60%)
$(sync~spin) Indexing Stdlib: 5/5 (100%)
$(sync~spin) Indexing Gems: 1/3 (33%)
$(sync~spin) Indexing Gems: 2/3 (66%)
$(sync~spin) Indexing Gems: 3/3 (100%)
$(sync~spin) Resolving mixins...
$(sync~spin) Collecting References: 1/100 (1%)
$(sync~spin) Collecting References: 50/100 (50%)
$(sync~spin) Collecting References: 100/100 (100%)
$(check) Ruby Fast LSP: Indexing complete
```

## Benefits

1. **Transparent Progress**: Users see exactly how many files are being processed
2. **Accurate Percentages**: Percentages are calculated from real data, not estimates
3. **Better UX**: Similar to rust-analyzer, users can gauge how long indexing will take
4. **Debugging**: Easier to identify if indexing is stuck on a particular file
5. **Performance Insights**: Users can see which phase (project/stdlib/gems) takes longest

## Testing

### Build
```bash
cargo build --release
```

### Tests
```bash
cargo test --release -- --test-threads=1
```

✅ All 359 tests passing

### Manual Testing

1. Open a Ruby project in VSCode
2. Observe the status bar showing file counts:
   - `Indexing: X/Y (Z%)`
   - `Indexing Stdlib: X/Y (Z%)`
   - `Indexing Gems: X/Y (Z%)`
   - `Collecting References: X/Y (Z%)`
3. Watch the percentages update based on actual progress

### Create VSIX
```bash
./create_vsix.sh --current-platform-only
```

## Technical Notes

### Progress Calculation

```rust
let percentage = if total > 0 {
    ((current as f64 / total as f64) * 100.0) as u32
} else {
    0
};
```

### Message Format

```rust
let full_message = if total > 0 {
    format!("{}: {}/{}", message, current, total)
} else {
    message
};
```

### LSP Protocol

Uses standard `$/progress` notification with `WorkDoneProgress::Report`:

```rust
WorkDoneProgressReport {
    message: Some(full_message),  // "Indexing: 3/100"
    percentage: Some(percentage),  // 3
    cancellable: Some(false),
}
```

## Future Enhancements

Potential improvements:
1. Add file-level progress for gems (show which gem files are being indexed)
2. Show estimated time remaining based on indexing speed
3. Add pause/resume functionality for indexing
4. Cache indexing results to speed up subsequent loads
5. Show which specific file is currently being indexed (for debugging slow files)

## Backward Compatibility

This change is fully backward compatible:
- All existing functionality works as before
- Progress notifications are optional (clients that don't handle them are unaffected)
- The core indexing logic remains unchanged

