//! Identifier, receiver, variable, constant, and method queries over the
//! Prism analyzer. Shared assertion helpers live here; each child module
//! covers one identifier family.

mod constants;
mod identifiers;
mod local_variables;
mod methods;
mod nonlocal_variables;
mod receiver_kinds;

use crate::core::{RubyConstant, SourcePosition as Position};
use crate::indexer::LVScopeId;
use crate::indexer::*;
use url::Url;

// Helper function to parse content and create an analyzer
fn create_analyzer(content: &str) -> RubyPrismAnalyzer {
    RubyPrismAnalyzer::new(Url::parse("file:///dummy.rb").unwrap(), content.to_string())
}

// Test helper - wraps get_identifier to return 3-tuple for backward compatibility
fn get_identifier_for_test(
    analyzer: &RubyPrismAnalyzer,
    position: Position,
) -> (Option<Identifier>, Vec<RubyConstant>, LVScopeId) {
    let (identifier, _identifier_type, ns_stack, lv_scope_id, _namespace_kind) =
        analyzer.get_identifier_at_position(position);
    (identifier, ns_stack, lv_scope_id)
}

// Helper functions for test assertions with the new Identifier enum structure

/// Assert that an identifier is a RubyConstant with the expected constant path
pub fn assert_constant_identifier(identifier: &Identifier, expected_path: &[&str]) {
    match identifier {
        Identifier::RubyConstant { namespace: _, iden } => {
            assert_eq!(
                iden.len(),
                expected_path.len(),
                "Expected constant path length {}, got {}",
                expected_path.len(),
                iden.len()
            );
            for (i, expected) in expected_path.iter().enumerate() {
                assert_eq!(
                    iden[i].to_string(),
                    *expected,
                    "Expected constant at position {} to be '{}', got '{}'",
                    i,
                    expected,
                    iden[i]
                );
            }
        }
        _ => panic!("Expected RubyConstant identifier, got {:?}", identifier),
    }
}

/// Assert that an identifier is a RubyMethod with the expected method name and receiver
pub fn assert_method_identifier(
    identifier: &Identifier,
    expected_method: &str,
    expected_receiver: MethodReceiver,
) {
    match identifier {
        Identifier::RubyMethod { receiver, iden, .. } => {
            // Compare receiver discriminants (ignoring inner data for variable types)
            let receiver_matches = match (&expected_receiver, receiver) {
                (MethodReceiver::None, MethodReceiver::None) => true,
                (MethodReceiver::SelfReceiver, MethodReceiver::SelfReceiver) => true,
                (MethodReceiver::Constant(_), MethodReceiver::Constant(_)) => true,
                (MethodReceiver::LocalVariable(_), MethodReceiver::LocalVariable(_)) => true,
                (MethodReceiver::InstanceVariable(_), MethodReceiver::InstanceVariable(_)) => true,
                (MethodReceiver::ClassVariable(_), MethodReceiver::ClassVariable(_)) => true,
                (MethodReceiver::GlobalVariable(_), MethodReceiver::GlobalVariable(_)) => true,
                (MethodReceiver::MethodCall { .. }, MethodReceiver::MethodCall { .. }) => true,
                (MethodReceiver::Expression, MethodReceiver::Expression) => true,
                _ => false,
            };
            assert!(
                receiver_matches,
                "Expected receiver {:?}, got {:?}",
                expected_receiver, receiver
            );
            assert_eq!(
                iden.to_string(),
                expected_method,
                "Expected method name '{}', got '{}'",
                expected_method,
                iden
            );
        }
        _ => panic!("Expected RubyMethod identifier, got {:?}", identifier),
    }
}

/// Assert that an identifier is a variable with the expected variable name
pub fn assert_variable_identifier(identifier: &Identifier, expected_name: &str) {
    match identifier {
        Identifier::RubyLocalVariable { name, .. } => {
            assert_eq!(
                name, expected_name,
                "Expected local variable name '{}', got '{}'",
                expected_name, name
            );
        }
        Identifier::RubyInstanceVariable { name, .. } => {
            assert_eq!(
                name, expected_name,
                "Expected instance variable name '{}', got '{}'",
                expected_name, name
            );
        }
        Identifier::RubyClassVariable { name, .. } => {
            assert_eq!(
                name, expected_name,
                "Expected class variable name '{}', got '{}'",
                expected_name, name
            );
        }
        Identifier::RubyGlobalVariable { name, .. } => {
            assert_eq!(
                name, expected_name,
                "Expected global variable name '{}', got '{}'",
                expected_name, name
            );
        }
        _ => panic!("Expected variable identifier, got {:?}", identifier),
    }
}

/// Assert that the namespace context has the expected length and contents
pub fn assert_namespace_context(namespace: &[RubyConstant], expected_namespace: &[&str]) {
    assert_eq!(
        namespace.len(),
        expected_namespace.len(),
        "Expected namespace length {}, got {}",
        expected_namespace.len(),
        namespace.len()
    );
    for (i, expected) in expected_namespace.iter().enumerate() {
        assert_eq!(
            namespace[i].to_string(),
            *expected,
            "Expected namespace at position {} to be '{}', got '{}'",
            i,
            expected,
            namespace[i]
        );
    }
}
