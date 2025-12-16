#[cfg(test)]
mod tests {
    use crate::analyzer_prism::utils;
    use crate::indexer::entry::{entry_kind::EntryKind, Entry};
    use crate::indexer::index::RubyIndex;
    use crate::types::fully_qualified_name::FullyQualifiedName;
    use crate::types::ruby_namespace::RubyConstant;

    fn create_test_index() -> RubyIndex {
        let mut index = RubyIndex::new();

        // Add some test constants to the index
        let foo_fqn = FullyQualifiedName::Constant(vec![RubyConstant::new("Foo").unwrap()]);
        let foo_bar_fqn = FullyQualifiedName::Constant(vec![
            RubyConstant::new("Foo").unwrap(),
            RubyConstant::new("Bar").unwrap(),
        ]);
        let baz_fqn = FullyQualifiedName::Constant(vec![RubyConstant::new("Baz").unwrap()]);

        let foo_id = index.intern_fqn(foo_fqn);
        let foo_entry = Entry {
            fqn_id: foo_id,
            kind: EntryKind::new_module(),
            location: crate::types::compact_location::CompactLocation::default(),
        };
        index.add_entry(foo_entry);

        let foo_bar_id = index.intern_fqn(foo_bar_fqn);
        let foo_bar_entry = Entry {
            fqn_id: foo_bar_id,
            kind: EntryKind::new_class(None),
            location: crate::types::compact_location::CompactLocation::default(),
        };
        index.add_entry(foo_bar_entry);

        let baz_id = index.intern_fqn(baz_fqn);
        let baz_entry = Entry {
            fqn_id: baz_id,
            kind: EntryKind::new_class(None),
            location: crate::types::compact_location::CompactLocation::default(),
        };
        index.add_entry(baz_entry);

        index
    }

    #[test]
    fn test_resolve_constant_fqn_from_parts_absolute() {
        let index = create_test_index();
        let current_fqn =
            FullyQualifiedName::Constant(vec![RubyConstant::new("SomeModule").unwrap()]);

        // Test absolute constant resolution
        let parts = vec![RubyConstant::new("Foo").unwrap()];
        let result = utils::resolve_constant_fqn_from_parts(&index, &parts, true, &current_fqn);

        assert!(result.is_some());
        if let Some(FullyQualifiedName::Constant(resolved_parts)) = result {
            assert_eq!(resolved_parts.len(), 1);
            assert_eq!(resolved_parts[0].to_string(), "Foo");
        }
    }

    #[test]
    fn test_resolve_constant_fqn_from_parts_relative() {
        let index = create_test_index();
        let current_fqn = FullyQualifiedName::Constant(vec![RubyConstant::new("Foo").unwrap()]);

        // Test relative constant resolution - Bar should resolve to Foo::Bar
        let parts = vec![RubyConstant::new("Bar").unwrap()];
        let result = utils::resolve_constant_fqn_from_parts(&index, &parts, false, &current_fqn);

        assert!(result.is_some());
        if let Some(FullyQualifiedName::Constant(resolved_parts)) = result {
            assert_eq!(resolved_parts.len(), 2);
            assert_eq!(resolved_parts[0].to_string(), "Foo");
            assert_eq!(resolved_parts[1].to_string(), "Bar");
        }
    }

    #[test]
    fn test_resolve_constant_fqn_from_parts_fallback_to_root() {
        let index = create_test_index();
        let current_fqn =
            FullyQualifiedName::Constant(vec![RubyConstant::new("SomeModule").unwrap()]);

        // Test fallback to root - Baz should resolve to just Baz
        let parts = vec![RubyConstant::new("Baz").unwrap()];
        let result = utils::resolve_constant_fqn_from_parts(&index, &parts, false, &current_fqn);

        assert!(result.is_some());
        if let Some(FullyQualifiedName::Constant(resolved_parts)) = result {
            assert_eq!(resolved_parts.len(), 1);
            assert_eq!(resolved_parts[0].to_string(), "Baz");
        }
    }

    #[test]
    fn test_resolve_constant_fqn_from_parts_not_found() {
        let index = create_test_index();
        let current_fqn =
            FullyQualifiedName::Constant(vec![RubyConstant::new("SomeModule").unwrap()]);

        // Test non-existent constant
        let parts = vec![RubyConstant::new("NonExistent").unwrap()];
        let result = utils::resolve_constant_fqn_from_parts(&index, &parts, false, &current_fqn);

        assert!(result.is_none());
    }
}
