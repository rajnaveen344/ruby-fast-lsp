# Indexing Optimization Analysis

## Problem Statement

**Symptom**: `did_open` and `did_change` events take **~1.3s-2s** for a single file, causing noticeable latency.

**Goal**: Reduce `did_change` latency to **<50ms sync** + **~300ms async debounce**.

## Current Performance (2025-12-13) - `commerce.rb`

| Component                      | Previous (12/12) | Current (12/13) | Status        |
| ------------------------------ | ---------------- | --------------- | ------------- |
| Parsing                        | 26ms             | ~25ms           | ✅ OK         |
| Cleanup (`remove_entries`)     | 160ms            | **125ms**       | ⚠️ Needs Work |
| IndexVisitor Walk              | 430ms            | **87ms**        | ✅ FIXED      |
| Cross-file: `mark_references`  | 57ms             | 53ms            | ✅ OK         |
| Cross-file Diagnostics (total) | 88ms             | 86ms            | ✅ OK         |
| Index Definitions (total)      | 688ms            | **310ms**       | ✅ Improved   |
| ReferenceVisitor Walk          | 326ms            | **322ms**       | ❌ HIGH       |
| Index References               | 412ms            | **375ms**       | ❌ HIGH       |
| **Total**                      | **1.15s**        | **~730ms**      | ⚠️ 36% faster |

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
