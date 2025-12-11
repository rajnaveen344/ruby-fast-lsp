# Indexing Optimization Analysis

## Problem Statement

**Symptom**: `did_change` events take **~1.3 seconds** for a single file, causing noticeable latency for the user.

**Goal**: Reduce `did_change` latency to **< 200ms**.

## Performance Breakdown

Logs from `src/indexer/file_processor.rs` reveal the following breakdown for a single file update:

| Component                      | Time       | Status                               |
| ------------------------------ | ---------- | ------------------------------------ |
| **Parsing**                    | ~30ms      | ✅ OK                                |
| **Cleanup (`remove_entries`)** | **~150ms** | ❌ **Bottleneck** (Should be < 5ms)  |
| **IndexVisitor Walk**          | **~420ms** | ❌ **Bottleneck** (Should be < 50ms) |
| **Cross-file Diagnostics**     | **~185ms** | ❌ **Bottleneck** (Should be < 20ms) |
| **Index References**           | ~400ms     | ⚠️ High, but secondary               |
| **Mixin Resolution**           | Skipped    | ✅ OK                                |

**Total Overhead**: ~755ms (60% of total time) is spent in pure overhead (cleanup, walking, diagnostics), not core logic.

---

## Root Cause Analysis

### 1. Cleanup Overhead (`remove_entries_for_uri`) - 150ms

**Cause**: Inefficient data structures in `RubyIndex`.

- `methods_by_name`: Stores a `Vec<Entry>` for each method name (e.g., `initialize`, `run`).
- **Algorithm**: To remove entries for a file, we perform a linear scan (`retain`) on these vectors.
- **Scale**: Popular methods like `initialize` can have thousands of entries. Scanning them on every keystroke is O(N \* M) where N is number of methods in file and M is total definitions of those methods in project.

### 2. Cross-file Diagnostics Overhead - 185ms

**Cause**: `mark_references_as_unresolved` iterates linearly.

- When an identifier is removed, we check _every_ unresolved entry in the system to see if it matches.
- This is an O(Unresolved \* Removed) operation.

### 3. IndexVisitor Overhead - 420ms

**Cause**: Excessive allocation and parsing during AST traversal.

- **ScopeTracker**: Re-allocates the namespace stack `Vec` on every node visit? (Need to verify)
- **YardParser**: Runs regex-based parsing on every method definition, even if comments are empty or simple.

---

## Optimization Plan

### Phase 1: Optimize `RubyIndex` Data Structures (Target: < 20ms combined)

Refactor `src/indexer/index.rs` to use O(1) lookups for removal.

1.  **Optimize `methods_by_name`**:

    - Change `HashMap<String, Vec<Entry>>` -> `HashMap<String, HashMap<Url, Entry>>` (or similar).
    - **Result**: Removal becomes O(1) hash lookup instead of O(N) scan.

2.  **Optimize `unresolved_entries`**:
    - Index unresolved entries by name for faster lookup during invalidation.

### Phase 2: Optimize `IndexVisitor` (Target: < 100ms)

1.  **Refactor `ScopeTracker`**:

    - Use a persistent/copy-on-write stack or just references to avoid `Vec` cloning in hot loops.

2.  **Optimize `YardParser`**:
    - Add a "fast path" to skip regex allocation if the comment block doesn't look like YARD (e.g., doesn't contain `@param`, `@return`).

### Phase 3: Reference Indexing

- Parallelize or defer reference indexing if it remains a bottleneck after fixing the overheads.
