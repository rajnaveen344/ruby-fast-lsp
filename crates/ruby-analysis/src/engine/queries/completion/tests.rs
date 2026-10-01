use super::*;
use crate::invariant::ExpectInvariant;

#[test]
fn rbs_method_matches_include_string_methods() {
    let matches = rbs_method_matches_for_type(&RubyType::string(), "", NamespaceKind::Instance);
    let names = matches
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&"length"));
    assert!(names.contains(&"upcase"));
}

#[test]
fn rbs_method_matches_filter_by_partial() {
    let matches = rbs_method_matches_for_type(&RubyType::string(), "up", NamespaceKind::Instance);

    invariant!(
        matches
            .iter()
            .all(|candidate| candidate.name.starts_with("up")),
        what = "RBS method completion returned a method outside the requested prefix",
        why = "completion filtering must be deterministic before LSP mapping",
        fix = "apply the partial filter before returning completion candidates",
    );
}

#[test]
fn rbs_method_matches_for_union_require_every_member() {
    let ty = RubyType::union(vec![RubyType::string(), RubyType::integer()]);
    let matches = rbs_method_matches_for_type(&ty, "", NamespaceKind::Instance);
    let names = matches
        .iter()
        .map(|candidate| candidate.name.as_str())
        .collect::<Vec<_>>();

    assert!(names.contains(&"to_s"));
    assert!(!names.contains(&"upcase"));
    assert!(!names.contains(&"abs"));
}

#[test]
fn unresolved_array_literal_does_not_drop_unknown_elements() {
    assert_eq!(
        infer_literal_type_from_expression("[1, dynamic_value]"),
        Some(RubyType::Array(vec![RubyType::Unknown]))
    );
}

#[test]
fn rbs_method_match_carries_return_type() {
    let matches =
        rbs_method_matches_for_type(&RubyType::string(), "length", NamespaceKind::Instance);
    let length = matches
        .iter()
        .find(|candidate| candidate.name == "length")
        .expect_invariant(
            "String#length missing from RBS completion candidates",
            "bundled RBS must expose String#length",
            "check RBS loading and completion class-name mapping",
        );

    assert!(length.return_type.is_some());
}
