use super::*;
use ruby_fast_lsp_extension_api::{Keyword, ResolvedCallee};

fn range(line: u32, start: u32, end: u32) -> SourceRange {
    source_range(line, start, end)
}

fn argument(value: ArgumentValue, location: SourceRange) -> Argument {
    Argument {
        keyword: None,
        value,
        range: location,
    }
}

fn keyword(name: &str, value: ArgumentValue, location: SourceRange) -> Argument {
    Argument {
        keyword: Some(Keyword {
            name: name.to_string(),
            range: location,
        }),
        value,
        range: location,
    }
}

fn context(method: &str, value: &str) -> CallContext {
    let location = range(1, 13, 20);
    CallContext {
        project: None,
        method_name: method.to_string(),
        receiver: Receiver::None,
        arguments: vec![argument(ArgumentValue::Symbol(value.to_string()), location)],
        current_namespace: vec!["User".to_string()],
        namespace_kind: NamespaceKind::Instance,
        call_range: location,
        block_range: None,
        message_range: location,
        resolved_callees: Vec::<ResolvedCallee>::new(),
        enclosing_calls: Vec::new(),
    }
}

fn routes_draw_frame() -> ResolvedCall {
    ResolvedCall {
        method_name: "draw".to_string(),
        receiver: Receiver::MethodCall {
            method_name: "routes".to_string(),
        },
        arguments: Vec::new(),
        resolved_callees: Vec::new(),
        call_range: range(0, 0, 29),
        message_range: range(0, 25, 29),
        frame_extension_ids: vec!["rails-ruby".to_string()],
    }
}

fn method_from(patch: &IndexPatch) -> &DefineMethodPatch {
    let IndexPatch::DefineMethod(method) = patch else {
        panic!("INVARIANT VIOLATED: Rails unit test expected DefineMethod. This is a test bug because the selected patch position is behaviorally fixed. Fix: update the assertion when the public patch contract intentionally changes.");
    };
    method
}

fn reference_from(patch: &IndexPatch) -> &ReferencePatch {
    let IndexPatch::AddReference(reference) = patch else {
        panic!("INVARIANT VIOLATED: Rails unit test expected AddReference. This is a test bug because the selected patch position is behaviorally fixed. Fix: update the assertion when the public patch contract intentionally changes.");
    };
    reference
}

#[test]
fn associations_preserve_reference_reader_writer_and_structured_types() {
    let patches = rails_index_call(&context("belongs_to", "account"));
    assert_eq!(patches.len(), 3);
    assert_eq!(
        reference_from(&patches[0]).target,
        ReferenceTarget::Namespace(vec!["Account".to_string()])
    );
    assert_eq!(method_from(&patches[1]).name, "account");
    assert_eq!(
        method_from(&patches[1]).return_type,
        Some(RubyType::Union(vec![
            RubyType::Named("Account".to_string()),
            RubyType::Named("NilClass".to_string()),
        ]))
    );
    assert_eq!(method_from(&patches[2]).name, "account=");
    assert_eq!(
        method_from(&patches[2]).params[0].kind,
        MethodParamKind::Required
    );

    let many = rails_index_call(&context("has_many", "companies"));
    assert_eq!(
        method_from(&many[1]).return_type,
        Some(RubyType::Array(vec![RubyType::Named(
            "Company".to_string()
        )]))
    );
}

#[test]
fn association_options_are_precise_and_polymorphic_targets_fail_closed() {
    let mut explicit = context("belongs_to", "account");
    explicit.arguments.push(keyword(
        "class_name",
        ArgumentValue::String("Billing::Account".to_string()),
        range(1, 35, 51),
    ));
    let patches = rails_index_call(&explicit);
    assert_eq!(
        reference_from(&patches[0]).target,
        ReferenceTarget::Namespace(vec!["Billing".to_string(), "Account".to_string()])
    );
    assert_eq!(reference_from(&patches[0]).location, range(1, 35, 51));

    let mut polymorphic = context("belongs_to", "subject");
    polymorphic.arguments.push(keyword(
        "polymorphic",
        ArgumentValue::Boolean(true),
        range(1, 35, 39),
    ));
    let patches = rails_index_call(&polymorphic);
    assert_eq!(patches.len(), 2);
    assert_eq!(
        method_from(&patches[0]).return_type,
        Some(RubyType::Unknown)
    );
}

#[test]
fn callbacks_and_validations_emit_exact_instance_method_references() {
    for macro_name in ["before_save", "validate", "validates_presence_of"] {
        let patches = rails_index_call(&context(macro_name, "normalize_account"));
        assert_eq!(patches.len(), 1);
        assert_eq!(
            reference_from(&patches[0]).target,
            ReferenceTarget::Method {
                namespace: vec!["User".to_string()],
                owner_kind: NamespaceKind::Instance,
                name: "normalize_account".to_string(),
            }
        );
    }
}

#[test]
fn resource_routes_generate_controller_references_and_typed_helpers() {
    let mut resources = context("resources", "people");
    resources.enclosing_calls.push(routes_draw_frame());
    resources.enclosing_calls.push(ResolvedCall {
        method_name: "namespace".to_string(),
        receiver: Receiver::None,
        arguments: vec![argument(
            ArgumentValue::Symbol("admin".to_string()),
            range(1, 12, 18),
        )],
        resolved_callees: Vec::new(),
        call_range: range(1, 2, 18),
        message_range: range(1, 2, 11),
        frame_extension_ids: vec!["rails-ruby".to_string()],
    });
    let patches = rails_index_call(&resources);
    assert_eq!(
        reference_from(&patches[0]).target,
        ReferenceTarget::Namespace(vec!["Admin".to_string(), "PeopleController".to_string(),])
    );
    let names = patches[1..]
        .iter()
        .map(|patch| method_from(patch).name.as_str())
        .collect::<Vec<_>>();
    assert!(names.contains(&"admin_people_path"));
    assert!(names.contains(&"new_admin_person_path"));
    assert!(names.contains(&"admin_person_url"));
    assert!(method_from(&patches[1])
        .return_type
        .as_ref()
        .is_some_and(|ruby_type| ruby_type == &RubyType::Named("String".to_string())));
}

#[test]
fn named_route_splits_controller_action_ranges_and_defines_helpers() {
    let mut route = context("get", "/account");
    route.arguments.push(keyword(
        "to",
        ArgumentValue::String("users#show".to_string()),
        range(2, 25, 35),
    ));
    route.arguments.push(keyword(
        "as",
        ArgumentValue::Symbol("account".to_string()),
        range(2, 41, 48),
    ));
    route.enclosing_calls.push(routes_draw_frame());
    let patches = rails_index_call(&route);
    assert_eq!(
        reference_from(&patches[0]).target,
        ReferenceTarget::Namespace(vec!["UsersController".to_string()])
    );
    assert_eq!(reference_from(&patches[0]).location, range(2, 25, 30));
    assert_eq!(
        reference_from(&patches[1]).target,
        ReferenceTarget::Method {
            namespace: vec!["UsersController".to_string()],
            owner_kind: NamespaceKind::Instance,
            name: "show".to_string(),
        }
    );
    assert_eq!(reference_from(&patches[1]).location, range(2, 31, 35));
    assert_eq!(method_from(&patches[2]).name, "account_path");
    assert_eq!(method_from(&patches[3]).name, "account_url");
}

#[test]
fn routes_outside_routes_draw_and_dynamic_jobs_are_ignored() {
    assert!(rails_index_call(&context("resources", "users")).is_empty());
    let mut job = context("perform_later", "user");
    job.receiver = Receiver::LocalVariable("job_class".to_string());
    assert!(rails_index_call(&job).is_empty());
}

#[test]
fn active_job_entry_points_reference_instance_perform() {
    for entry_point in ["perform_later", "perform_now"] {
        let mut job = context(entry_point, "user");
        job.receiver = Receiver::Constant(vec!["Billing".to_string(), "EmailJob".to_string()]);
        let patches = rails_index_call(&job);
        assert_eq!(
            reference_from(&patches[0]).target,
            ReferenceTarget::Method {
                namespace: vec!["Billing".to_string(), "EmailJob".to_string()],
                owner_kind: NamespaceKind::Instance,
                name: "perform".to_string(),
            }
        );
    }
}

#[test]
fn controller_lenses_include_only_public_actions() {
    let document = DocumentContext {
        uri: "file:///repo/app/controllers/admin/users_controller.rb".to_string(),
        text:
            "class Admin::UsersController\n  def show\n  end\n  private\n  def secret\n  end\nend\n"
                .to_string(),
        project: None,
    };
    let output = code_lens_output(&document);
    assert_eq!(output.response_patches.len(), 1);
    let ResponsePatch::CodeLens(lens) = &output.response_patches[0] else {
        panic!("INVARIANT VIOLATED: Rails controller action emitted another response kind. This is a guest test bug because controller discovery owns only code lenses. Fix: keep response routing explicit.");
    };
    assert_eq!(lens.arguments[1], "admin/users");
    assert_eq!(lens.arguments[2], "show");
    assert_eq!(lens.range, range(1, 6, 10));
}
