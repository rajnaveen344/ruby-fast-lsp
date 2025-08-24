#[cfg(test)]
mod tests {
    use crate::analyzer_prism::utils;
    use crate::indexer::index::RubyIndex;
    use crate::indexer::entry::{Entry, entry_kind::EntryKind};
    use crate::types::fully_qualified_name::FullyQualifiedName;
    use crate::types::ruby_namespace::RubyConstant;
    use tower_lsp::lsp_types::{Location, Position, Range, Url};

    fn create_test_index() -> RubyIndex {
        let mut index = RubyIndex::new();
        
        // Add some test constants to the index
        let foo_fqn = FullyQualifiedName::Constant(vec![
            RubyConstant::new("Foo").unwrap()
        ]);
        let foo_bar_fqn = FullyQualifiedName::Constant(vec![
            RubyConstant::new("Foo").unwrap(),
            RubyConstant::new("Bar").unwrap()
        ]);
        let baz_fqn = FullyQualifiedName::Constant(vec![
            RubyConstant::new("Baz").unwrap()
        ]);
        
        let test_location = Location {
            uri: Url::parse("file:///test.rb").unwrap(),
            range: Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 10 },
            },
        };
        
        index.definitions.insert(foo_fqn, vec![Entry {
            fqn: FullyQualifiedName::Constant(vec![RubyConstant::new("Foo").unwrap()]),
            kind: EntryKind::Module { includes: vec![], prepends: vec![], extends: vec![] },
            location: test_location.clone(),
        }]);
        
        index.definitions.insert(foo_bar_fqn, vec![Entry {
            fqn: FullyQualifiedName::Constant(vec![
                RubyConstant::new("Foo").unwrap(),
                RubyConstant::new("Bar").unwrap()
            ]),
            kind: EntryKind::Class { 
                superclass: None, 
                includes: vec![], 
                prepends: vec![], 
                extends: vec![] 
            },
            location: test_location.clone(),
        }]);
        
        index.definitions.insert(baz_fqn, vec![Entry {
            fqn: FullyQualifiedName::Constant(vec![RubyConstant::new("Baz").unwrap()]),
            kind: EntryKind::Class { 
                superclass: None, 
                includes: vec![], 
                prepends: vec![], 
                extends: vec![] 
            },
            location: test_location.clone(),
        }]);
        
        index
    }

    #[test]
    fn test_resolve_constant_fqn_from_parts_absolute() {
        let index = create_test_index();
        let current_fqn = FullyQualifiedName::Constant(vec![
            RubyConstant::new("SomeModule").unwrap()
        ]);
        
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
        let current_fqn = FullyQualifiedName::Constant(vec![
            RubyConstant::new("Foo").unwrap()
        ]);
        
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
        let current_fqn = FullyQualifiedName::Constant(vec![
            RubyConstant::new("SomeModule").unwrap()
        ]);
        
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
        let current_fqn = FullyQualifiedName::Constant(vec![
            RubyConstant::new("SomeModule").unwrap()
        ]);
        
        // Test non-existent constant
        let parts = vec![RubyConstant::new("NonExistent").unwrap()];
        let result = utils::resolve_constant_fqn_from_parts(&index, &parts, false, &current_fqn);
        
        assert!(result.is_none());
    }
}