//! Acceptance contract for parameter-dependent callable-body inference.
//!
//! Keep these fixtures neutral and semantic. Direct calls and higher-order
//! calls must eventually consume one shared callable-body proof rather than
//! recognizing these examples in an LSP consumer.

use crate::test::harness::{check, check_multi_file, FakeEditor};

#[tokio::test]
async fn local_lambda_direct_call_instantiates_the_parameter_type() {
    check(
        r#"
stringify = ->(value) { value.to_s }
result<hint label="String"> = stringify.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn local_callable_calls_retain_definition_navigation() {
    check(
        r#"
<def>stringify</def> = ->(value) { value.to_s }
result = stringify$0.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_result_drives_hover_completion_chaining_and_diagnostics() {
    check(
        r#"
stringify = ->(value) { value.to_s }
result = stringify.call(1)
result<hover label="String">.u$0
<complete items="upcase">

result.<warn none code="unresolved-method">upcase</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn local_lambda_supplies_a_parameter_dependent_map_block() {
    check(
        r#"
stringify = ->(value) { value.to_s }
results<hint label="Array<String>"> = [1, 2].map(&stringify)
"#,
    )
    .await;
}

#[tokio::test]
async fn local_lambda_projects_a_shape_field() {
    check(
        r#"
read_name = ->(row) { row[:name] }
rows = [{ name: "Ada" }, { name: "Grace" }]
names<hint label="Array<String>"> = rows.map(&read_name)
"#,
    )
    .await;
}

#[tokio::test]
async fn local_lambda_preserves_every_exhaustive_result_member() {
    check(
        r#"
normalize = ->(value) do
  if condition
    value
  else
    value.to_s
  end
end

result<hint label="(Integer | String)"> = normalize.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn local_lambda_reads_a_proven_same_scope_capture() {
    check(
        r#"
prefix = "item"
decorate = ->(row) { { tag: prefix, value: row[:value] } }
result<hint label="{ tag: String, value: Integer }"> = decorate.call({ value: 1 })
"#,
    )
    .await;
}

#[tokio::test]
async fn strict_lambda_and_lenient_proc_keep_distinct_arity_semantics() {
    check(
        r#"
strict = ->(first, second) { first.to_s }
strict_result<hint label=": ?"> = strict.call(1)

lenient = Proc.new { |first, second| first.to_s }
lenient_result<hint label="String"> = lenient.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn capture_free_callable_constant_resolves_across_files() {
    check_multi_file(&[
        (
            "converters.rb",
            r#"
module Converters
  STRINGIFY = ->(value) { value.to_s }
end
"#,
        ),
        (
            "report.rb",
            r#"
labels<hint label="Array<String>"> = [1, "ready"].map(&Converters::STRINGIFY)
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn cross_file_callable_constants_retain_definition_navigation() {
    check_multi_file(&[
        (
            "callable_definition.rb",
            r#"
module CallableDefinition
  <def>CONVERT</def> = ->(value) { value.to_s }
end
"#,
        ),
        (
            "callable_consumer.rb",
            r#"
result = CallableDefinition::CONVERT$0.call(1)
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn repeated_callable_constant_file_orders_produce_one_result() {
    for definition_first in [true, false, false, true, false, true, true, false] {
        let definition = (
            "ordered_callable.rb",
            "module OrderedCallable\n  CONVERT = ->(value) { value.to_s }\nend\n",
        );
        let consumer = (
            "ordered_consumer.rb",
            "result<hint label=\"String\"> = OrderedCallable::CONVERT.call(1)\n",
        );
        if definition_first {
            check_multi_file(&[definition, consumer]).await;
        } else {
            check_multi_file(&[consumer, definition]).await;
        }
    }
}

#[tokio::test]
async fn local_callable_alias_uses_the_same_identity() {
    check(
        r#"
stringify = ->(value) { value.to_s }
alias_callable = stringify
result<hint label="String"> = alias_callable.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn non_callable_reassignment_invalidates_the_local_identity() {
    check(
        r#"
convert = ->(value) { value.to_s }
convert = 1
result<hint label=": ?"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn unresolved_callable_input_invalidates_the_complete_result() {
    check(
        r#"
convert = ->(value) { value.to_s }
result<hint label=": ?"> = convert.call(dynamic_value)
"#,
    )
    .await;
}

#[tokio::test]
async fn unsupported_proc_non_local_return_fails_closed() {
    check(
        r#"
convert = proc { |value| return value.to_s }
result<hint label=": ?"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn lambda_return_is_a_local_result_exit() {
    check(
        r#"
convert = ->(value) do
  return value.to_s
  value
end
result<hint label="String"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_next_is_a_local_result_exit() {
    check(
        r#"
convert = proc do |value|
  next value.to_s
  value
end
result<hint label="String"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn raising_callable_branch_does_not_reach_the_result_join() {
    check(
        r#"
convert = ->(value) do
  if condition
    raise "failed"
  else
    value.to_s
  end
end
result<hint label="String"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn unresolved_capture_invalidates_the_complete_callable_result() {
    check(
        r#"
prefix = "item"
decorate = ->(value) { [prefix, value] }
prefix = dynamic_value
result<hint label=": ?"> = decorate.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn write_to_captured_outer_local_fails_closed() {
    check(
        r#"
prefix = "item"
decorate = ->(value) do
  prefix = value.to_s
  prefix
end
result<hint label=": ?"> = decorate.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn recursive_callable_invocation_fails_closed() {
    check(
        r#"
convert = ->(value) { convert.call(value) }
result<hint label=": ?"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_parameter_boundary_is_accepted() {
    check(
        r#"
combine = ->(a, b, c, d) { a.to_s }
result<hint label="String"> = combine.call(1, 2, 3, 4)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_parameter_boundary_plus_one_fails_closed() {
    check(
        r#"
combine = ->(a, b, c, d, e) { a.to_s }
result<hint label=": ?"> = combine.call(1, 2, 3, 4, 5)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_identity_survives_identical_branch_flow() {
    check(
        r#"
convert = ->(value) { value.to_s }
if condition
  selected = convert
else
  selected = convert
end
result<hint label="String"> = selected.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn incompatible_callable_branch_identities_fail_closed() {
    check(
        r#"
if condition
  selected = ->(value) { value.to_s }
else
  selected = ->(value) { value }
end
result<hint label=": ?"> = selected.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_identity_survives_identical_case_flow() {
    check(
        r#"
convert = ->(value) { value.to_s }
case mode
when :first
  selected = convert
when :second
  selected = convert
else
  selected = convert
end
result<hint label="String"> = selected.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn incompatible_callable_case_identities_fail_closed() {
    check(
        r#"
case mode
when :first
  selected = ->(value) { value.to_s }
else
  selected = ->(value) { value }
end
result<hint label=": ?"> = selected.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_alias_boundary_is_accepted() {
    check(
        r#"
callable = ->(value) { value.to_s }
a1 = callable
a2 = callable
a3 = callable
a4 = callable
a5 = callable
a6 = callable
a7 = callable
result<hint label="String"> = a7.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_alias_boundary_plus_one_invalidates_the_identity() {
    check(
        r#"
callable = ->(value) { value.to_s }
a1 = callable
a2 = callable
a3 = callable
a4 = callable
a5 = callable
a6 = callable
a7 = callable
a8 = callable
result<hint label=": ?"> = callable.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn editing_a_callable_constant_replaces_cross_file_results() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "converters.rb",
            r#"module Converters
  CONVERT = ->(value) { value.to_s }
end
"#,
        )
        .await;
    editor
        .open_and_check_fixture(
            "consumer.rb",
            r#"result<hint label="String"> = Converters::CONVERT.call(1)
"#,
        )
        .await;
    editor
        .set(
            "converters.rb",
            r#"module Converters
  CONVERT = ->(value) { value }
end
"#,
        )
        .await;
    editor
        .set_and_check_fixture(
            "consumer.rb",
            r#"result<hint label="Integer"> = Converters::CONVERT.call(1) # refreshed
"#,
        )
        .await;
    editor
        .set(
            "converters.rb",
            r#"module Converters
  CONVERT = 1
end
"#,
        )
        .await;
    editor
        .set_and_check_fixture(
            "consumer.rb",
            r#"result<hint label=": ?"> = Converters::CONVERT.call(1) # invalidated
"#,
        )
        .await;
}

#[tokio::test]
async fn callable_input_and_capture_edits_replace_results() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "callable_edit.rb",
            r#"prefix = "item"
identity = ->(value) { value }
captured = -> { prefix }
input_result<hint label="String"> = identity.call("ready")
capture_result<hint label="String"> = captured.call
"#,
        )
        .await;
    editor
        .set_and_check_fixture(
            "callable_edit.rb",
            r#"prefix = 1
identity = ->(value) { value }
captured = -> { prefix }
input_result<hint label="Integer"> = identity.call(2)
capture_result<hint label="Integer"> = captured.call
"#,
        )
        .await;
}

#[tokio::test]
async fn callable_constant_parse_failure_clears_stale_cross_file_result() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "callable_source.rb",
            "module CallableSource\n  CONVERT = ->(value) { value.to_s }\nend\n",
        )
        .await;
    editor
        .open_and_check_fixture(
            "callable_consumer.rb",
            "result<hint label=\"String\"> = CallableSource::CONVERT.call(1)\n",
        )
        .await;
    editor
        .set(
            "callable_source.rb",
            "module CallableSource\n  CONVERT = ->(value) {\n",
        )
        .await;
    editor
        .set_and_check_fixture(
            "callable_consumer.rb",
            "result<hint label=\": ?\"> = CallableSource::CONVERT.call(1) # parse failure cleared\n",
        )
        .await;
}

#[tokio::test]
async fn consumer_first_converges_after_callable_constant_is_indexed() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "late_callable_consumer.rb",
            "result<hint label=\": ?\"> = LateCallable::CONVERT.call(1)\n",
        )
        .await;
    editor
        .open(
            "late_callable_source.rb",
            "module LateCallable\n  CONVERT = ->(value) { value.to_s }\nend\n",
        )
        .await;
    editor
        .set_and_check_fixture(
            "late_callable_consumer.rb",
            "result<hint label=\"String\"> = LateCallable::CONVERT.call(1) # reindexed\n",
        )
        .await;
}

#[tokio::test]
async fn conflicting_callable_constant_definitions_are_ambiguous() {
    check_multi_file(&[
        (
            "first_callable.rb",
            "module SharedCallable\n  CONVERT = ->(value) { value.to_s }\nend\n",
        ),
        (
            "second_callable.rb",
            "module SharedCallable\n  CONVERT = ->(value) { value }\nend\n",
        ),
        (
            "shared_callable_consumer.rb",
            "result<hint label=\": ?\"> = SharedCallable::CONVERT.call(1)\n",
        ),
    ])
    .await;
}

#[tokio::test]
async fn callable_body_local_assignment_joins_both_branch_values() {
    check(
        r#"
convert = ->(value) do
  if condition
    result = value
  else
    result = value.to_s
  end
  result
end
output<hint label="(Integer | String)"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_body_local_defined_on_one_branch_includes_nil() {
    check(
        r#"
convert = ->(value) do
  if condition
    result = value
  end
  result
end
output<hint label="(Integer | NilClass)"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn passing_callable_as_an_ordinary_argument_invalidates_all_aliases() {
    check(
        r#"
convert = ->(value) { value.to_s }
alias_convert = convert
consume(convert)
output<hint label=": ?"> = alias_convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn storing_callable_in_a_collection_invalidates_its_local_identity() {
    check(
        r#"
convert = ->(value) { value.to_s }
stored = [convert]
output<hint label=": ?"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn nested_callable_instantiation_boundary_is_accepted() {
    check(
        r#"
c1 = ->(value) { value.to_s }
c2 = ->(value) { c1.call(value) }
c3 = ->(value) { c2.call(value) }
c4 = ->(value) { c3.call(value) }
c5 = ->(value) { c4.call(value) }
c6 = ->(value) { c5.call(value) }
c7 = ->(value) { c6.call(value) }
c8 = ->(value) { c7.call(value) }
output<hint label="String"> = c8.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn nested_callable_instantiation_boundary_plus_one_fails_closed() {
    check(
        r#"
c1 = ->(value) { value.to_s }
c2 = ->(value) { c1.call(value) }
c3 = ->(value) { c2.call(value) }
c4 = ->(value) { c3.call(value) }
c5 = ->(value) { c4.call(value) }
c6 = ->(value) { c5.call(value) }
c7 = ->(value) { c6.call(value) }
c8 = ->(value) { c7.call(value) }
c9 = ->(value) { c8.call(value) }
output<hint label=": ?"> = c9.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_constraint_solve_boundary_is_accepted() {
    check(
        r#"
convert = ->(value) {
  [
    value.to_s, value.to_s, value.to_s, value.to_s,
    value.to_s, value.to_s, value.to_s, value.to_s,
    value.to_s, value.to_s, value.to_s, value.to_s,
    value.to_s, value.to_s, value.to_s, value.to_s
  ]
}
output<hint label="Array<String>"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_constraint_solve_boundary_plus_one_fails_closed() {
    check(
        r#"
convert = ->(value) {
  [
    value.to_s, value.to_s, value.to_s, value.to_s,
    value.to_s, value.to_s, value.to_s, value.to_s,
    value.to_s, value.to_s, value.to_s, value.to_s,
    value.to_s, value.to_s, value.to_s, value.to_s,
    value.to_s
  ]
}
output<hint label=": ?"> = convert.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_capture_boundary_is_accepted() {
    check(
        r#"
a = "a"; b = "b"; c = "c"; d = "d"
e = "e"; f = "f"; g = "g"; h = "h"
collect = -> { [a, b, c, d, e, f, g, h] }
output<hint label="Array<String>"> = collect.call
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_capture_boundary_plus_one_fails_closed() {
    check(
        r#"
a = "a"; b = "b"; c = "c"; d = "d"; e = "e"
f = "f"; g = "g"; h = "h"; i = "i"
collect = -> { [a, b, c, d, e, f, g, h, i] }
output<hint label=": ?"> = collect.call
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_summary_node_boundary_is_accepted() {
    check(
        r#"
collect = -> {
  [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1
  ]
}
output<hint label="Array<Integer>"> = collect.call
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_summary_node_boundary_plus_one_fails_closed() {
    check(
        r#"
collect = -> {
  [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1
  ]
}
output<hint label=": ?"> = collect.call
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_structural_depth_boundary_is_accepted() {
    check(
        r#"
nest = ->(value) { [[[[[[[[value]]]]]]]] }
output<hint label="Array<Array<Array<Array<Array<Array<Array<Array<Integer>>>>>>>>"> = nest.call(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_structural_depth_boundary_plus_one_fails_closed() {
    check(
        r#"
nest = ->(value) { [[[[[[[[[value]]]]]]]]] }
output<hint label=": ?"> = nest.call(1).to_s
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_result_union_boundary_is_accepted() {
    check(
        r#"
choose = -> {
  if a then 1
  elsif b then "s"
  elsif c then :symbol
  elsif d then nil
  elsif e then true
  elsif f then false
  elsif g then 1.0
  else /pattern/
  end
}
output<hint label="(FalseClass | Float | Integer | NilClass | Regexp | String | Symbol | TrueClass)"> = choose.call
"#,
    )
    .await;
}

#[tokio::test]
async fn callable_result_union_boundary_plus_one_fails_closed() {
    check(
        r#"
choose = -> {
  if a then 1
  elsif b then "s"
  elsif c then :symbol
  elsif d then nil
  elsif e then true
  elsif f then false
  elsif g then 1.0
  elsif h then /pattern/
  else { x: 1 }
  end
}
output<hint label=": ?"> = choose.call
"#,
    )
    .await;
}
