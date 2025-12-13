# SlotMap Index Architecture

## Overview

The `RubyIndex` has been refactored to use a centralized `SlotMap` storage model. This architecture solves performance bottlenecks related to re-indexing files (specifically `remove_entries_for_uri`) and unifies the handling of definitions and references.

## Core Components

### 1. Central Storage (`SlotMap<EntryId, Entry>`)

Instead of storing `Entry` objects directly in multiple `HashMap`s (e.g., definitions by FQN, entries by file), we store **all** entries in a single `SlotMap`.

```rust
struct RubyIndex {
    entries: SlotMap<EntryId, Entry>,
    // ...
}
```

- **`EntryId`**: A lightweight, roughly 8-byte handle (index + generation) that uniquely identifies an entry. References are valid as long as the entry isn't removed.
- **`Entry`**: The actual data (FQN, location, metadata).

### 2. Lightweight Secondary Indexes

Secondary indexes now store only `EntryId`s, not the entries themselves.

```rust
// Lookup by File (O(1) access to all entries in a file)
by_file: HashMap<Url, Vec<EntryId>>

// Lookup by FQN (O(1) access to definitions/references for a symbol)
by_fqn: HashMap<FullyQualifiedName, Vec<EntryId>>
```

## Performance Benefits

### 1. O(1) File Cleanup (`remove_entries_for_uri`)

**Problem**: The legacy system stored entries in `definitions: HashMap<FQN, Vec<Entry>>`. To remove entries for a file, we had to iterate _every single FQN_ in the entire project, check its list of entries, and remove those belonging to the file. This was `O(N)` where N is total project size.
**Solution**: With `SlotMap` + `by_file`, we simply look up the list of `EntryId`s for the file in `by_file` (O(1)). We then remove each ID from the `SlotMap` and secondary maps. The cost is proportional only to the number of entries _in that file_ (O(M)), regardless of project size.

### 2. Unified Reference Storage

**Problem**: References were stored in a separate `HashMap<FQN, Vec<Location>>`, duplicating storage logic and requiring separate cleanup passes.
**Solution**: References are now just another type of `Entry` (`EntryKind::Reference`). They live in the same `SlotMap`. Removing a file automatically removes its references because they are listed in `by_file` just like definitions.

### 3. Memory & Cache Locality

- `SlotMap` stores entries in a contiguous vector, improving CPU cache hit rates compared to scattered heap allocations of `HashMap` values.
- `Cloning` an `EntryId` is essentially free (copying an integer), whereas cloning an `Entry` is expensive (strings, vecs). Use `EntryId` everywhere in the application logic.

## Future Optimizations

The unified structure enables clear paths for:

- **String Interning**: Replacing `String` in `Entry` with `Ustr` (interned string IDs) to shrink `Entry` size.
- **Incremental Updates**: Since IDs are stable, we can track changes more easily.
