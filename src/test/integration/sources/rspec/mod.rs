//! RSpec extension facts: generated helpers, example-group scope, and
//! execution contexts.
//!
//! Each case runs in a project that locks `rspec-core` 3.x, through either
//! the native fallback or the `extensions/rspec-ruby` package, and returns a
//! transcript of what it observed so the two implementations can be compared.
//! `semantic` runs every case through the implementation the server uses.

mod harness;
mod helpers;
mod lifecycle;
mod scopes;

use harness::{Rspec, RspecEditor};

/// The implementation the semantic cases exercise.
const SEMANTIC_IMPLEMENTATION: Rspec = Rspec::NativeFallback;

macro_rules! rspec_cases {
    ($($module:ident::$case:ident),* $(,)?) => {
        mod semantic {
            $(
                #[tokio::test]
                async fn $case() {
                    super::$module::$case(super::SEMANTIC_IMPLEMENTATION).await;
                }
            )*
        }
    };
}

rspec_cases! {
    helpers::let_defines_helper_method,
    helpers::subject_with_name_defines_helper_method,
    helpers::bang_subject_defines_subject_helper_method,
    helpers::dsl_macros_do_not_report_unresolved_methods,
    helpers::dsl_macros_do_not_report_wrong_arity,
    helpers::inferred_helper_diagnostics_follow_return_type_edits,
    helpers::generated_helper_rename_follows_global_method_rename_policy,
    helpers::extension_requires_resolved_rspec_constant,
    scopes::include_makes_helper_methods_visible,
    scopes::extend_makes_helper_methods_visible_on_singleton_scope,
    scopes::extension_does_not_treat_other_describe_as_rspec_scope,
    scopes::extension_does_not_apply_include_outside_rspec_scope,
    scopes::example_group_owns_direct_method_definitions,
    scopes::example_group_owns_define_method_declarations,
    scopes::nested_group_inherits_outer_hidden_owner,
    scopes::nested_group_owns_and_isolates_its_methods,
    scopes::sibling_groups_do_not_share_direct_method_definitions,
    scopes::sibling_groups_isolate_method_references,
    lifecycle::execution_context_and_methods_are_replaced_after_edit,
    lifecycle::before_hook_runtime_methods_flow_to_examples,
    lifecycle::cross_file_shared_context_helpers_flow_to_including_group,
    lifecycle::shared_context_identity_is_isolated_between_projects,
    lifecycle::example_runtime_methods_do_not_leak_to_siblings,
    lifecycle::runtime_blocks_preserve_lexical_constant_scope,
}

/// The intended difference: the package applies only to projects that lock
/// `rspec-core` 3.x, while the native fallback runs everywhere.
#[tokio::test]
async fn package_requires_locked_rspec_core() {
    let source = r#"module RSpec
end

RSpec.describe Object do
  let(:actor) { Object.new }

  it "uses the helper" do
    actor
  end
end
"#;
    for (rspec, expected) in [(Rspec::NativeFallback, 1), (Rspec::Package, 0)] {
        let mut editor = RspecEditor::with_projects(rspec, &[false]).await;
        let filename = editor.path("spec/unlocked_spec.rb");
        editor.open(&filename, source).await;
        assert_eq!(
            editor.goto_def_at(&filename, 7, 6).await.len(),
            expected,
            "{rspec:?} in a project without a locked rspec-core"
        );
    }
}
