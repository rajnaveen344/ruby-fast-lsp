# Indexing Optimization Analysis

## Problem Statement

**Symptom**: `did_open` and `did_change` events take **~1.3s-2s** for a single file, causing noticeable latency.

**Goal**: Reduce `did_change` latency to **<50ms sync** + **~300ms async debounce**.

## Current Performance (2025-12-12)

| Component                      | Before    | After     | Status        |
| ------------------------------ | --------- | --------- | ------------- |
| Parsing                        | 26ms      | 26ms      | ✅ OK         |
| Cleanup (`remove_entries`)     | 175ms     | 160ms     | ⚠️ Acceptable |
| IndexVisitor Walk              | **447ms** | **430ms** | ❌ HIGH       |
| Cross-file: `mark_references`  | **985ms** | **57ms**  | ✅ FIXED      |
| Cross-file: `clear_resolved`   | 3ms       | 3ms       | ✅ OK         |
| Cross-file Diagnostics (total) | **1s**    | **88ms**  | ✅ FIXED      |
| Index Definitions (total)      | 1.6s      | 688ms     | ⚠️ Improved   |
| ReferenceVisitor Walk          | 353ms     | 326ms     | ⚠️ High       |
| Index References               | 500ms     | 412ms     | ⚠️ High       |
| **Total**                      | **2.1s**  | **1.15s** | ⚠️ 45% faster |

## Optimization Plan

### ✅ Completed

1. **`clear_resolved_entries` O(1) lookup** - Added `unresolved_by_name` reverse index
2. **`mark_references_as_unresolved` HashSet dedup** - O(N²) → O(N)
3. **`UnresolvedIndex` refactor** - Encapsulated forward/reverse maps atomically
4. **`YardParser` Optimization** - Uses Prism comments API to avoid O(N²) line scanning

### 🔄 Remaining

| Optimization                      | Expected Impact | Complexity |
| --------------------------------- | --------------- | ---------- |
| **Debounced Indexing**            | <50ms sync UX   | Medium     |
| **ReferenceVisitor Batching**     | -200ms          | Medium     |
| **Memory: Entry/RubyType intern** | Reduce allocs   | Low        |
| **HashMap Pre-sizing**            | Reduce reallocs | Low        |
