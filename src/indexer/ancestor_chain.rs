//! # Ancestor Chain Module
//!
//! This module handles the construction of ancestor chains for Ruby classes and modules,
//! which is essential for proper method lookup and resolution in Ruby's object model.
//!
//! ## Ruby's Method Lookup Order
//!
//! ### For Instance Methods (Class Context)
//! When calling a method on an instance of a class, Ruby searches in this order:
//! 1. **Current Class**: The class of the object
//! 2. **Included Modules**: Modules included in the current class (in reverse order of inclusion)
//! 3. **Parent Class**: The superclass of the current class
//! 4. **Parent's Modules**: Modules included in the parent class
//! 5. **Continue up the chain**: Repeat steps 3-4 until reaching BasicObject
//!
//! ### For Class Methods (Class Context with `extend`)
//! When calling a method on a class itself, Ruby searches:
//! 1. **Extended Modules**: Modules extended into the class (these become class methods)
//! 2. **Singleton Class**: The class's singleton class
//! 3. **Class's Class**: The class of the class (usually Class)
//! 4. **Continue up**: Follow the normal instance method lookup on the class hierarchy
//!
//! ## Examples with Graph Representation
//!
//! ### Example 1: Simple Inheritance
//! ```ruby
//! class C1
//!   def method_a; end
//! end
//!
//! class C2 < C1
//!   def method_b; end
//! end
//! ```
//!
//! **Ancestor Chain for C2 instance methods:**
//! ```text
//! C2 → C1 → Object → BasicObject
//! ```
//!
//! ### Example 2: Module Inclusion
//! ```ruby
//! module M1
//!   def method_m1; end
//! end
//!
//! module M2
//!   def method_m2; end
//! end
//!
//! class C1
//!   include M1
//!   include M2
//!   def method_c1; end
//! end
//! ```
//!
//! **Ancestor Chain for C1 instance methods:**
//! ```text
//! C1 → M2 → M1 → Object → BasicObject
//! ```
//! Note: M2 comes before M1 because it was included later
//!
//! ### Example 3: Module Prepending
//! ```ruby
//! module M1
//!   def method_shared; "from M1"; end
//! end
//!
//! class C1
//!   prepend M1
//!   def method_shared; "from C1"; end
//! end
//! ```
//!
//! **Ancestor Chain for C1 instance methods:**
//! ```text
//! M1 → C1 → Object → BasicObject
//! ```
//! Note: M1 comes before C1, so M1#method_shared will be called
//!
//! ### Example 4: Class Methods with Extend
//! ```ruby
//! module M1
//!   def class_method_m1; end
//! end
//!
//! class C1
//!   extend M1
//!   def self.class_method_c1; end
//! end
//! ```
//!
//! **Ancestor Chain for C1 class methods:**
//! ```text
//! C1 (extended: M1) → Class → Module → Object → BasicObject
//! ```
//!
//! ### Example 5: Complex Hierarchy
//! ```ruby
//! module M1
//!   def method_m1; end
//! end
//!
//! module M2
//!   def method_m2; end
//! end
//!
//! module M3
//!   include M1
//!   def method_m3; end
//! end
//!
//! class C1
//!   include M2
//!   def method_c1; end
//! end
//!
//! class C2 < C1
//!   include M3
//!   def method_c2; end
//! end
//! ```
//!
//! **Ancestor Chain for C2 instance methods:**
//! ```text
//! C2 → M3 → M1 → C1 → M2 → Object → BasicObject
//! ```
//!
//! **Graph Representation:**
//! ```text
//!     BasicObject
//!         ↑
//!      Object
//!         ↑
//!        M2 ← C1 ← M3 ← C2
//!             ↑    ↑
//!             │    M1
//!             │    ↑
//!             └────┘
//! ```
//!
//! ## Key Functions
//!
//! - `get_ancestor_chain`: Main entry point that dispatches to class or instance method chains
//! - `build_class_method_ancestor_chain`: Handles class method lookup (extends + instance chain)
//! - `build_instance_method_ancestor_chain`: Handles instance method lookup
//! - `process_mixins_for_ancestor_chain`: Processes includes/prepends/extends
//! - `build_chain_recursive`: Recursively builds the chain while preventing cycles

use super::index::RubyIndex;
use crate::analyzer_prism::utils;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MixinRef;
use crate::types::fully_qualified_name::FullyQualifiedName;
use std::collections::HashSet;

// ============================================================================
// Mixin Resolution
// ============================================================================

/// Resolves a mixin reference (include/prepend/extend) to a fully qualified name
/// Uses Ruby's constant lookup rules to resolve the reference in the given context
pub fn resolve_mixin_ref(
    index: &RubyIndex,
    mixin_ref: &MixinRef,
    current_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    utils::resolve_constant_fqn_from_parts(index, &mixin_ref.parts, mixin_ref.absolute, current_fqn)
}

// ============================================================================
// Ancestor Chain Building
// ============================================================================

/// Builds the complete ancestor chain for a given class or module
///
/// For class methods: includes singleton class + normal ancestor chain
/// For instance methods: includes normal ancestor chain (current class -> mixins -> superclass)
///
/// The chain represents the method lookup order in Ruby's method resolution
pub fn get_ancestor_chain(
    index: &RubyIndex,
    fqn: &FullyQualifiedName,
    is_class_method: bool,
) -> Vec<FullyQualifiedName> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();

    if is_class_method {
        build_class_method_ancestor_chain(index, fqn, &mut chain, &mut visited);
    } else {
        build_instance_method_ancestor_chain(index, fqn, &mut chain, &mut visited);
    }

    chain
}

// ============================================================================
// Internal Helpers
// ============================================================================

/// Builds ancestor chain for class methods
/// Includes extends (for class methods) + normal instance method chain
fn build_class_method_ancestor_chain(
    index: &RubyIndex,
    fqn: &FullyQualifiedName,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut HashSet<FullyQualifiedName>,
) {
    // Process extends for class methods
    if let Some(entries) = index.get(fqn) {
        if let Some(entry) = entries.first() {
            if let EntryKind::Class { extends, .. } | EntryKind::Module { extends, .. } =
                &entry.kind
            {
                process_mixins_for_ancestor_chain(index, extends, fqn, chain, visited, true);
            }
        }
    }

    // Also include the normal instance method chain for class methods
    build_instance_method_ancestor_chain(index, fqn, chain, visited);
}

/// Builds ancestor chain for instance methods
/// Follows Ruby's method lookup: current class -> prepends -> includes -> superclass
fn build_instance_method_ancestor_chain(
    index: &RubyIndex,
    fqn: &FullyQualifiedName,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut HashSet<FullyQualifiedName>,
) {
    build_chain_recursive(index, fqn, chain, visited);
}

/// Processes mixins (includes/prepends/extends) and adds them to the ancestor chain
fn process_mixins_for_ancestor_chain(
    index: &RubyIndex,
    mixins: &[crate::indexer::entry::MixinRef],
    current_fqn: &FullyQualifiedName,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut HashSet<FullyQualifiedName>,
    reverse_order: bool,
) {
    let mixins_iter: Box<dyn Iterator<Item = &MixinRef>> = if reverse_order {
        Box::new(mixins.iter().rev())
    } else {
        Box::new(mixins.iter())
    };

    for mixin_ref in mixins_iter {
        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, current_fqn) {
            build_chain_recursive(index, &resolved_fqn, chain, visited);
        }
    }
}

/// Recursively builds the ancestor chain following Ruby's method lookup order
fn build_chain_recursive(
    index: &RubyIndex,
    fqn: &FullyQualifiedName,
    chain: &mut Vec<FullyQualifiedName>,
    visited: &mut HashSet<FullyQualifiedName>,
) {
    if !visited.insert(fqn.clone()) {
        return;
    }

    if let Some(entries) = index.get(fqn) {
        if let Some(entry) = entries.first() {
            match &entry.kind {
                EntryKind::Class {
                    superclass,
                    includes,
                    prepends,
                    ..
                } => {
                    for mixin_ref in prepends.iter().rev() {
                        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, fqn) {
                            build_chain_recursive(index, &resolved_fqn, chain, visited);
                        }
                    }

                    chain.push(fqn.clone());

                    for mixin_ref in includes {
                        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, fqn) {
                            build_chain_recursive(index, &resolved_fqn, chain, visited);
                        }
                    }

                    if let Some(superclass_ref) = superclass {
                        if let Some(resolved_superclass) =
                            resolve_mixin_ref(index, superclass_ref, fqn)
                        {
                            build_chain_recursive(index, &resolved_superclass, chain, visited);
                        }
                    }
                }
                EntryKind::Module {
                    includes, prepends, ..
                } => {
                    for mixin_ref in prepends.iter().rev() {
                        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, fqn) {
                            build_chain_recursive(index, &resolved_fqn, chain, visited);
                        }
                    }
                    chain.push(fqn.clone());
                    for mixin_ref in includes {
                        if let Some(resolved_fqn) = resolve_mixin_ref(index, mixin_ref, fqn) {
                            build_chain_recursive(index, &resolved_fqn, chain, visited);
                        }
                    }
                }
                _ => {
                    chain.push(fqn.clone());
                }
            }
        } else {
            chain.push(fqn.clone());
        }
    } else {
        chain.push(fqn.clone());
    }
}
