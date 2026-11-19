//! Tests for the `argument-type-mismatch` diagnostic.
//!
//! A documented (YARD) parameter type is the contract. The call is
//! flagged only when the argument's type is proven and provably outside that
//! contract; every unproven or partially modelled case stays silent.

use crate::test::harness::{check_project, FakeEditor};

const CODE: &str = "argument-type-mismatch";

#[tokio::test]
async fn integer_literal_into_string_parameter_warns() {
    check_project(&[(
        "main.rb",
        r#"
# @param name [String]
def greet(name)
  name
end

greet(<warn code="argument-type-mismatch" message="expects `String`">42</warn>)
"#,
    )])
    .await;
}

#[tokio::test]
async fn keyword_argument_into_integer_parameter_warns() {
    check_project(&[(
        "main.rb",
        r#"
# @param name [String]
# @param age [Integer]
def greet(name:, age: 0)
  name
end

greet(name: "x", age: <warn code="argument-type-mismatch">"old"</warn>)
"#,
    )])
    .await;
}

#[tokio::test]
async fn second_positional_argument_is_matched_to_its_parameter() {
    check_project(&[(
        "main.rb",
        r#"
# @param name [String]
# @param age [Integer]
def greet(name, age)
  name
end

greet(<warn none code="argument-type-mismatch">"x"</warn>, <warn code="argument-type-mismatch">:old</warn>)
"#,
    )])
    .await;
}

#[tokio::test]
async fn unrelated_user_class_into_class_parameter_warns() {
    check_project(&[(
        "main.rb",
        r#"
class Widget; end
class Gadget; end

class Shop
  # @param widget [Widget]
  def sell(widget)
    widget
  end
end

Shop.new.sell(<warn code="argument-type-mismatch">Gadget.new</warn>)
"#,
    )])
    .await;
}

#[tokio::test]
async fn typed_local_argument_warns() {
    check_project(&[(
        "main.rb",
        r#"
# @param name [String]
def greet(name)
  name
end

count = 3
greet(<warn code="argument-type-mismatch">count</warn>)
"#,
    )])
    .await;
}

#[tokio::test]
async fn union_argument_warns_only_when_every_member_mismatches() {
    check_project(&[(
        "main.rb",
        r#"
# @param name [String]
def greet(name)
  name
end

def pick(flag)
  both = flag ? 1 : :one
  greet(<warn code="argument-type-mismatch">both</warn>)
  some = flag ? 1 : "one"
  greet(<warn none code="argument-type-mismatch">some</warn>)
end
"#,
    )])
    .await;
}

#[tokio::test]
async fn matching_and_wider_declarations_stay_silent() {
    check_project(&[(
        "main.rb",
        r#"
class Base; end
class Child < Base; end

module Named; end
class Tagged
  include Named
end

# @param base [Base]
# @param named [Named]
# @param number [Numeric]
# @param either [String, Symbol]
# @param anything [Object]
def take(base, named, number, either, anything)
end

<warn none code="argument-type-mismatch">take(Child.new, Tagged.new, 3, :sym, 4.5)</warn>
"#,
    )])
    .await;
}

#[tokio::test]
async fn unproven_and_unmodelled_cases_stay_silent() {
    check_project(&[(
        "main.rb",
        r#"
# @param name [String]
def greet(name)
  name
end

# @param reader [#read]
# @param ghost [Phantom]
# @param names [Array<String>]
def open(reader, ghost, names)
end

def plain(name)
end

def caller_side(mystery, list)
  <warn none code="argument-type-mismatch">greet(mystery)</warn>
  <warn none code="argument-type-mismatch">greet(*list)</warn>
  <warn none code="argument-type-mismatch">greet(nil)</warn>
  <warn none code="argument-type-mismatch">open(1, 2, [3])</warn>
  <warn none code="argument-type-mismatch">plain(1)</warn>
end
"#,
    )])
    .await;
}

#[tokio::test]
async fn numeric_kinds_are_not_flagged_against_each_other() {
    check_project(&[(
        "main.rb",
        r#"
# @param ratio [Float]
def scale(ratio)
end

<warn none code="argument-type-mismatch">scale(2)</warn>
"#,
    )])
    .await;
}

#[tokio::test]
async fn rest_parameters_skip_the_check() {
    check_project(&[(
        "main.rb",
        r#"
# @param name [String]
def greet(name, *rest)
end

<warn none code="argument-type-mismatch">greet(1, 2)</warn>
"#,
    )])
    .await;
}

/// The declaration lives in another file. Editing only that file's doc tag
/// must add and remove the warning in the caller's file.
#[tokio::test]
async fn editing_the_declaration_updates_the_callers_warning() {
    let mut editor = FakeEditor::new().await;
    let callee = |tag: &str| {
        format!(
            "class Widget; end\nclass Gadget; end\n\nclass Shop\n  # @param item [{tag}]\n  def sell(item)\n    item\n  end\nend\n"
        )
    };
    editor.open("shop.rb", &callee("Gadget")).await;
    editor
        .open("caller.rb", "Shop.new.sell(Gadget.new)\n")
        .await;
    let mismatches = |diagnostics: Vec<tower_lsp::lsp_types::Diagnostic>| {
        diagnostics
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.code
                    == Some(tower_lsp::lsp_types::NumberOrString::String(
                        CODE.to_string(),
                    ))
            })
            .count()
    };
    assert_eq!(mismatches(editor.diagnostics("caller.rb").await), 0);

    editor.set("shop.rb", &callee("Widget")).await;
    assert_eq!(mismatches(editor.diagnostics("caller.rb").await), 1);

    editor.set("shop.rb", &callee("Gadget")).await;
    assert_eq!(mismatches(editor.diagnostics("caller.rb").await), 0);
}

/// A call passed as an argument is typed by its own result, never by the
/// receiver that starts at the same offset.
#[tokio::test]
async fn nested_call_argument_uses_the_calls_own_type() {
    check_project(&[(
        "main.rb",
        r#"
# @param name [String]
def greet(name)
  name
end

def pick(flag, mystery)
  n = flag ? 1 : 2
  <warn none code="argument-type-mismatch">greet(n.to_s)</warn>
  <warn none code="argument-type-mismatch">greet(n.undefined_thing)</warn>
  <warn none code="argument-type-mismatch">greet(mystery.size)</warn>
  greet(<warn code="argument-type-mismatch">n.abs</warn>)
end
"#,
    )])
    .await;
}

#[tokio::test]
async fn locals_reassigned_out_of_line_stay_silent() {
    check_project(&[(
        "main.rb",
        r#"
# @param name [String]
def greet(name)
  name
end

def in_block
  x = 1
  [0].each { x = "s" }
  <warn none code="argument-type-mismatch">greet(x)</warn>
end

def in_lambda
  x = 1
  f = -> { x = "s" }
  f.call
  <warn none code="argument-type-mismatch">greet(x)</warn>
end

def in_loop(flag)
  x = 1
  while flag
    <warn none code="argument-type-mismatch">greet(x)</warn>
    x = "s"
  end
end

def compound(other)
  x = nil
  x ||= other
  <warn none code="argument-type-mismatch">greet(x)</warn>
  y = 1
  y += other
  <warn none code="argument-type-mismatch">greet(y)</warn>
end
"#,
    )])
    .await;
}

#[tokio::test]
async fn constructor_argument_is_checked_against_initialize() {
    check_project(&[(
        "main.rb",
        r#"
class Thing
  # @param label [String]
  def initialize(label)
    @label = label
  end
end

Thing.new(<warn code="argument-type-mismatch">42</warn>)
<warn none code="argument-type-mismatch">Thing.new("ok")</warn>
"#,
    )])
    .await;
}

#[tokio::test]
async fn yard_overloads_with_differing_types_stay_silent() {
    check_project(&[(
        "main.rb",
        r#"
# @overload fetch(key)
#   @param key [String]
# @overload fetch(key)
#   @param key [Integer]
def fetch(key)
  key
end

<warn none code="argument-type-mismatch">fetch(1)</warn>
<warn none code="argument-type-mismatch">fetch(:sym)</warn>
"#,
    )])
    .await;
}

/// Signature-file parameters are not compared yet: every call stays silent,
/// including overloaded, optional, generic, and interface parameters.
#[tokio::test]
async fn signature_file_parameters_are_not_checked_yet() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = crate::test::harness::fixture_uri("/sig/desk.rbs");
    crate::loader::file_processor::FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            r#"class Desk
  def label: (String name) -> String
  def maybe: (String? name) -> void
  def either: (String name) -> void
            | (Integer name) -> void
  def generic: [T] (T name) -> T
  def shaped: (_ToS name) -> void
end

class Shelf
  def label: (String name) -> String
  def maybe: (String? name) -> void
  def either: (String name) -> void
            | (Integer name) -> void
  def generic: [T] (T name) -> T
  def shaped: (_ToS name) -> void
end
"#,
            editor.server(),
        )
        .expect("signature parameter contracts must enter the shared engine");
    editor
        .open_and_check_fixture(
            "desk.rb",
            r#"class Desk
  def label(name)
    name
  end

  def maybe(name)
  end

  def either(name)
  end

  def generic(name)
    name
  end

  def shaped(name)
  end
end

desk = Desk.new
<warn none code="argument-type-mismatch">desk.label(42)</warn>
<warn none code="argument-type-mismatch">desk.label("ok")</warn>
<warn none code="argument-type-mismatch">desk.maybe("ok")</warn>
<warn none code="argument-type-mismatch">desk.either(1)</warn>
<warn none code="argument-type-mismatch">desk.either(:sym)</warn>
<warn none code="argument-type-mismatch">desk.generic(1)</warn>
<warn none code="argument-type-mismatch">desk.shaped(1)</warn>

shelf = Shelf.new
<warn none code="argument-type-mismatch">shelf.label(42)</warn>
<warn none code="argument-type-mismatch">shelf.label("ok")</warn>
<warn none code="argument-type-mismatch">shelf.maybe("ok")</warn>
<warn none code="argument-type-mismatch">shelf.maybe(nil)</warn>
<warn none code="argument-type-mismatch">shelf.either(1)</warn>
<warn none code="argument-type-mismatch">shelf.either(:sym)</warn>
<warn none code="argument-type-mismatch">shelf.generic(1)</warn>
<warn none code="argument-type-mismatch">shelf.shaped(1)</warn>
"#,
        )
        .await;
}

/// A local typed by a project method depends on facts another file's edit
/// can change, so it is never used as proof: the call stays silent before and
/// after the edit, whichever file is opened first.
#[tokio::test]
async fn locals_typed_by_project_methods_stay_silent_across_edits() {
    let greet = "# @param name [String]\ndef greet(name)\n  name\nend\n\n";
    let cases: Vec<(&str, Box<dyn Fn(&str) -> String>, String)> = vec![
        (
            "bare",
            Box::new(|v: &str| format!("def helper_count\n  {v}\nend\n")),
            format!("{greet}def run\n  n = helper_count\n  greet(n)\nend\n"),
        ),
        (
            "implicit_self",
            Box::new(|v: &str| format!("class Shop\n  def count\n    {v}\n  end\nend\n")),
            format!("{greet}class Shop\n  def run\n    n = count\n    greet(n)\n  end\nend\n"),
        ),
        (
            "explicit_self",
            Box::new(|v: &str| format!("class Shop\n  def count\n    {v}\n  end\nend\n")),
            format!("{greet}class Shop\n  def run\n    n = self.count\n    greet(n)\n  end\nend\n"),
        ),
        (
            "doc_receiver",
            Box::new(|v: &str| format!("class Shop\n  def count\n    {v}\n  end\nend\n")),
            format!(
                "{greet}# @param shop [Shop]\ndef run(shop)\n  n = shop.count\n  greet(n)\nend\n"
            ),
        ),
        (
            "chain",
            Box::new(|v: &str| format!("def helper_count\n  {v}\nend\n")),
            format!("{greet}def run\n  s = helper_count\n  n = s.itself\n  greet(n)\nend\n"),
        ),
        (
            "top_level",
            Box::new(|v: &str| format!("def helper_count\n  {v}\nend\n")),
            format!("{greet}n = helper_count\ngreet(n)\n"),
        ),
    ];
    for (name, callee, caller) in cases {
        for caller_first in [false, true] {
            let mut editor = FakeEditor::new().await;
            if caller_first {
                editor.open("caller.rb", &caller).await;
                editor.open("lib.rb", &callee("1")).await;
            } else {
                editor.open("lib.rb", &callee("1")).await;
                editor.open("caller.rb", &caller).await;
            }
            let count = |d: Vec<tower_lsp::lsp_types::Diagnostic>| {
                d.into_iter()
                    .filter(|x| {
                        x.code
                            == Some(tower_lsp::lsp_types::NumberOrString::String(
                                CODE.to_string(),
                            ))
                    })
                    .count()
            };
            let before = count(editor.diagnostics("caller.rb").await);
            editor.set("lib.rb", &callee("\"s\"")).await;
            let after = count(editor.diagnostics("caller.rb").await);
            assert_eq!(
                (before, after),
                (0, 0),
                "{name} caller_first={caller_first}"
            );
        }
    }
}
