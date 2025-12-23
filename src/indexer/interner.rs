//! Generic Interner for bidirectional ID ↔ Value lookups
//!
//! Combines a SlotMap (for stable, compact IDs) with a HashMap (for reverse lookup).
//! Used for interning URLs and FullyQualifiedNames to save memory.

use slotmap::{Key, SlotMap};
use std::collections::HashMap;
use std::hash::Hash;

/// A bidirectional intern table with SlotMap-based IDs.
///
/// Provides O(1) lookup in both directions:
/// - ID → Value (via SlotMap)
/// - Value → ID (via HashMap)
#[derive(Debug)]
pub struct Interner<K: Key, V: Eq + Hash + Clone> {
    id_to_value: SlotMap<K, V>,
    value_to_id: HashMap<V, K>,
}

impl<K: Key, V: Eq + Hash + Clone> Interner<K, V> {
    /// Create a new empty interner
    pub fn new() -> Self {
        Self {
            id_to_value: SlotMap::with_key(),
            value_to_id: HashMap::new(),
        }
    }

    /// Get or insert a value, returning its ID
    pub fn get_or_insert(&mut self, value: &V) -> K {
        if let Some(&id) = self.value_to_id.get(value) {
            return id;
        }
        let id = self.id_to_value.insert(value.clone());
        self.value_to_id.insert(value.clone(), id);
        id
    }

    /// Insert a value, returning its ID (always creates new entry)
    pub fn insert(&mut self, value: V) -> K {
        let id = self.id_to_value.insert(value.clone());
        self.value_to_id.insert(value, id);
        id
    }

    /// Get the ID for a value (if it exists)
    pub fn get_id(&self, value: &V) -> Option<&K> {
        self.value_to_id.get(value)
    }

    /// Get the value for an ID (if it exists)
    pub fn get(&self, id: K) -> Option<&V> {
        self.id_to_value.get(id)
    }

    /// Check if a value is interned
    pub fn contains(&self, value: &V) -> bool {
        self.value_to_id.contains_key(value)
    }

    /// Get the number of interned values
    pub fn len(&self) -> usize {
        self.id_to_value.len()
    }

    /// Check if the interner is empty
    pub fn is_empty(&self) -> bool {
        self.id_to_value.is_empty()
    }
}

impl<K: Key, V: Eq + Hash + Clone> Default for Interner<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::new_key_type;

    new_key_type! { struct TestId; }

    #[test]
    fn test_get_or_insert() {
        let mut interner: Interner<TestId, String> = Interner::new();

        let id1 = interner.get_or_insert(&"hello".to_string());
        let id2 = interner.get_or_insert(&"hello".to_string());
        let id3 = interner.get_or_insert(&"world".to_string());

        assert_eq!(id1, id2); // Same value → same ID
        assert_ne!(id1, id3); // Different value → different ID
    }

    #[test]
    fn test_bidirectional_lookup() {
        let mut interner: Interner<TestId, String> = Interner::new();

        let value = "test".to_string();
        let id = interner.get_or_insert(&value);

        assert_eq!(interner.get(id), Some(&value));
        assert_eq!(interner.get_id(&value), Some(&id));
    }
}
