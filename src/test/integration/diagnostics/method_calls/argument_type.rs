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

/// A signature file declares parameter types as documentation does. The
/// definition, when there is one, still decides the call shape.
#[tokio::test]
async fn signature_file_parameters_are_checked() {
    check_project(&[
        (
            "sig/desk.rbs",
            r#"class Crate
end

class Desk
  def label: (String name) -> String
end

class Shelf
  def initialize: (String name) -> void
  def self.build: (String name) -> Shelf
  def label: (String name) -> String
  def maybe: (String? name) -> void
  def either: (String name) -> void
            | (Integer name) -> void
  def flag: (bool on) -> void
  def level: (1 | 2 level) -> void
  def pair: ([String, Integer] pair) -> void
  def row: ({ id: Integer } row) -> void
  def opt: (?String name) -> void
  def kw: (name: String, ?count: Integer) -> void
  def list: (Array[String] names) -> void
  def crate: (Crate item) -> void
end
"#,
        ),
        (
            "desk.rb",
            r#"class Desk
  def label(name)
    name
  end
end

desk = Desk.new
desk.label(<warn code="argument-type-mismatch">42</warn>)
<warn none code="argument-type-mismatch">desk.label("ok")</warn>
desk.<warn code="wrong-arity">label</warn>("a", "b")

shelf = Shelf.new(<warn code="argument-type-mismatch">1</warn>)
Shelf.build(<warn code="argument-type-mismatch">1</warn>)
shelf.label(<warn code="argument-type-mismatch">42</warn>)
<warn none code="argument-type-mismatch">shelf.label("ok")</warn>
shelf.maybe(<warn code="argument-type-mismatch">42</warn>)
<warn none code="argument-type-mismatch">shelf.maybe("ok")</warn>
<warn none code="argument-type-mismatch">shelf.maybe(nil)</warn>
shelf.either(<warn code="argument-type-mismatch">:sym</warn>)
<warn none code="argument-type-mismatch">shelf.either(1)</warn>
shelf.flag(<warn code="argument-type-mismatch">1</warn>)
<warn none code="argument-type-mismatch">shelf.flag(true)</warn>
shelf.level(<warn code="argument-type-mismatch">"x"</warn>)
<warn none code="argument-type-mismatch">shelf.level(3)</warn>
shelf.pair(<warn code="argument-type-mismatch">1</warn>)
shelf.row(<warn code="argument-type-mismatch">1</warn>)
shelf.opt(<warn code="argument-type-mismatch">1</warn>)
shelf.kw(name: <warn code="argument-type-mismatch">1</warn>)
shelf.kw(name: "a", count: <warn code="argument-type-mismatch">"b"</warn>)
shelf.list(<warn code="argument-type-mismatch">1</warn>)
<warn none code="argument-type-mismatch">shelf.list([1])</warn>
shelf.crate(<warn code="argument-type-mismatch">1</warn>)
<warn none code="argument-type-mismatch">shelf.crate(Crate.new)</warn>
"#,
        ),
    ])
    .await;
}

/// Signature types this check cannot hold a call to stay silent, and
/// overloads that take different argument lists settle neither the argument
/// types nor the argument count.
#[tokio::test]
async fn unmodelled_signature_parameters_stay_silent() {
    check_project(&[
        (
            "sig/bin.rbs",
            r#"class Crate
end

class Bin
  def shapes: (String name) -> void
            | (String name, Integer count) -> void
  def generic: [T] (T name) -> T
  def shaped: (_ToS name) -> void
  def alias_int: (int count) -> void
  def alias_string: (string name) -> void
  def flagish: (boolish on) -> void
  def any: (untyped value) -> void
  def me: (self other) -> void
  def inst: (instance other) -> void
  def order: (:asc | :desc order) -> void
  def callable: (^(Integer) -> String fn) -> void
  def rest: (*String names) -> void
  def klass: (singleton(Crate) type) -> void
  def top: (top value) -> void
end
"#,
        ),
        (
            "bin.rb",
            r#"bin = Bin.new
<warn none>bin.shapes(1)</warn>
<warn none>bin.shapes("a", "b")</warn>
<warn none>bin.generic(1)</warn>
<warn none>bin.shaped(1)</warn>
<warn none>bin.alias_int("s")</warn>
<warn none>bin.alias_string(1)</warn>
<warn none>bin.flagish(1)</warn>
<warn none>bin.any(1)</warn>
<warn none>bin.me(1)</warn>
<warn none>bin.inst(1)</warn>
<warn none>bin.order("asc")</warn>
<warn none>bin.order(:other)</warn>
<warn none>bin.callable(1)</warn>
<warn none>bin.rest(1)</warn>
<warn none>bin.klass(1)</warn>
<warn none>bin.top(1)</warn>
"#,
        ),
    ])
    .await;
}

/// Documentation and a signature that name different types for one
/// parameter leave no single contract, so calls stay silent. When they
/// agree the call is checked.
#[tokio::test]
async fn documentation_and_signature_must_agree() {
    check_project(&[
        (
            "sig/desk.rbs",
            "class Desk\n  def label: (Integer name) -> void\n  def tag: (String name) -> void\nend\n",
        ),
        (
            "desk.rb",
            r#"class Desk
  # @param name [String]
  def label(name)
  end

  # @param name [String]
  def tag(name)
  end
end

desk = Desk.new
<warn none code="argument-type-mismatch">desk.label("x")</warn>
<warn none code="argument-type-mismatch">desk.label(1)</warn>
desk.tag(<warn code="argument-type-mismatch">1</warn>)
"#,
        ),
    ])
    .await;
}

/// Calls to Ruby's own methods are not compared against the bundled core
/// signatures.
#[tokio::test]
async fn built_in_methods_are_not_compared() {
    check_project(&[(
        "main.rb",
        r#"<warn none code="argument-type-mismatch">"abc".center("x")</warn>
<warn none code="argument-type-mismatch">[1, 2].first("x")</warn>
<warn none code="argument-type-mismatch">1.fdiv("x")</warn>
"#,
    )])
    .await;
}

/// A local typed by a project method depends on facts another file's edit
/// can change, so it is never used as proof: the call stays silent before and
/// after the edit, whichever file is opened first.
#[tokio::test]
async fn locals_typed_by_project_methods_stay_silent_across_edits() {
    // Project classes only: this editor has no core classes, so a core type
    // could never warn here. The last line of each caller is the control.
    let greet = "class Word; end\nclass Count; end\n\n# @param name [Word]\ndef greet(name)\n  name\nend\n\n";
    let control = "greet(Count.new)\n";
    let cases: Vec<(&str, Box<dyn Fn(&str) -> String>, String)> = vec![
        (
            "bare",
            Box::new(|v: &str| format!("def helper_count\n  {v}\nend\n")),
            format!("{greet}def run\n  n = helper_count\n  greet(n)\nend\n{control}"),
        ),
        (
            "implicit_self",
            Box::new(|v: &str| format!("class Shop\n  def count\n    {v}\n  end\nend\n")),
            format!("{greet}class Shop\n  def run\n    n = count\n    greet(n)\n  end\nend\n{control}"),
        ),
        (
            "explicit_self",
            Box::new(|v: &str| format!("class Shop\n  def count\n    {v}\n  end\nend\n")),
            format!("{greet}class Shop\n  def run\n    n = self.count\n    greet(n)\n  end\nend\n{control}"),
        ),
        (
            "doc_receiver",
            Box::new(|v: &str| format!("class Shop\n  def count\n    {v}\n  end\nend\n")),
            format!(
                "{greet}# @param shop [Shop]\ndef run(shop)\n  n = shop.count\n  greet(n)\nend\n{control}"
            ),
        ),
        (
            "chain",
            Box::new(|v: &str| format!("def helper_count\n  {v}\nend\n")),
            format!("{greet}def run\n  s = helper_count\n  n = s.itself\n  greet(n)\nend\n{control}"),
        ),
        (
            "top_level",
            Box::new(|v: &str| format!("def helper_count\n  {v}\nend\n")),
            format!("{greet}n = helper_count\ngreet(n)\n{control}"),
        ),
    ];
    for (name, callee, caller) in cases {
        for caller_first in [false, true] {
            let mut editor = FakeEditor::new().await;
            if caller_first {
                editor.open("caller.rb", &caller).await;
                editor.open("lib.rb", &callee("Count.new")).await;
            } else {
                editor.open("lib.rb", &callee("Count.new")).await;
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
            let mut seen = Vec::new();
            seen.push(count(editor.diagnostics("caller.rb").await));
            for v in ["Word.new", "Count.new", "Word.new", "Count.new"] {
                editor.set("lib.rb", &callee(v)).await;
                seen.push(count(editor.diagnostics("caller.rb").await));
            }
            assert_eq!(seen, [1; 5], "{name} caller_first={caller_first}");
        }
    }
}

/// A type that reaches a local through another file's method can lag an
/// edit two files away, so such a local is never used as proof.
#[tokio::test]
async fn local_typed_through_two_other_files_never_warns() {
    let caller = "class Word; end\nclass Count; end\n\n# @param name [Word]\ndef greet(name)\n  name\nend\n\n# @param shop [Shop]\ndef run(shop)\n  n = shop.count\n  greet(n)\nend\n";
    let lib = "class Shop\n  def count\n    Maker.new.make\n  end\nend\n";
    let maker = |v: &str| format!("class Maker\n  def make\n    {v}\n  end\nend\n");
    for order in 0..3 {
        let files = [
            ("caller.rb", caller.to_string()),
            ("lib.rb", lib.to_string()),
            ("maker.rb", maker("Count.new")),
        ];
        let mut editor = FakeEditor::new().await;
        for step in 0..3 {
            let (name, text) = &files[(step + order) % 3];
            editor.open(name, text).await;
        }
        for value in ["Word.new", "Count.new", "Word.new"] {
            editor.set("maker.rb", &maker(value)).await;
            let warnings = editor
                .diagnostics("caller.rb")
                .await
                .into_iter()
                .filter(|diagnostic| {
                    diagnostic.code
                        == Some(tower_lsp::lsp_types::NumberOrString::String(
                            CODE.to_string(),
                        ))
                })
                .count();
            assert_eq!(warnings, 0, "order={order} value={value}");
        }
    }
}

/// The warning follows the signature file on disk: created, retyped,
/// emptied, deleted, then created again.
#[tokio::test]
async fn signature_edits_update_the_callers_warning() {
    use tower_lsp::lsp_types::FileChangeType;

    let code = "class Word; end\nclass Count; end\n\nclass Desk\n  def put(item)\n    item\n  end\nend\n\nDesk.new.put(Count.new)\n";
    let signature = |item: &str| format!("class Desk\n  def put: ({item} item) -> untyped\nend\n");
    let temp = tempfile::TempDir::new().expect("create project root");
    let root = dunce::canonicalize(temp.path()).expect("canonical project root");
    let file = |name: &str| root.join(name).to_string_lossy().into_owned();
    let mut editor = FakeEditor::new().await;
    editor.add_workspace(&root.to_string_lossy());
    std::fs::create_dir_all(root.join("sig")).expect("create sig");
    let main = file("main.rb");
    let path = file("sig/desk.rbs");
    std::fs::write(&main, code).expect("write code");
    editor.open(&main, code).await;

    let mut seen = Vec::new();
    for (text, change) in [
        (Some(signature("Word")), FileChangeType::CREATED),
        (Some(signature("Count")), FileChangeType::CHANGED),
        (Some(signature("Word")), FileChangeType::CHANGED),
        (Some(String::new()), FileChangeType::CHANGED),
        (Some(signature("Word")), FileChangeType::CHANGED),
        (None, FileChangeType::DELETED),
        (Some(signature("Word")), FileChangeType::CREATED),
    ] {
        match text {
            Some(text) => std::fs::write(&path, text).expect("write signature"),
            None => std::fs::remove_file(&path).expect("delete signature"),
        }
        editor.watched_file_changed(&path, change).await;
        let warnings = editor
            .diagnostics(&main)
            .await
            .into_iter()
            .filter(|diagnostic| {
                diagnostic.code
                    == Some(tower_lsp::lsp_types::NumberOrString::String(
                        CODE.to_string(),
                    ))
            })
            .count();
        seen.push(warnings);
    }
    assert_eq!(seen, [1, 0, 1, 0, 1, 0, 1]);
}

async fn mismatch_count(editor: &FakeEditor, file: &str) -> usize {
    editor
        .diagnostics(file)
        .await
        .into_iter()
        .filter(|diagnostic| {
            diagnostic.code
                == Some(tower_lsp::lsp_types::NumberOrString::String(
                    CODE.to_string(),
                ))
        })
        .count()
}

/// The argument's type comes from a method whose own return is decided two
/// files away. Each edit there must reach the caller in the same step,
/// whichever order the editor refreshes the open files in.
#[tokio::test]
async fn call_argument_through_two_other_files_tracks_every_edit() {
    let caller = "class Word; end\nclass Count; end\n\n# @param name [Word]\ndef greet(name)\n  name\nend\n\n# @param shop [Shop]\ndef run(shop)\n  greet(shop.count)\nend\n";
    let middle = "class Shop\n  def count\n    Maker.new.make\n  end\nend\n";
    let source = |value: &str| format!("class Maker\n  def make\n    {value}\n  end\nend\n");
    // The caller sorts before, between, and after the files it reads.
    for names in [
        ["a_caller.rb", "m_middle.rb", "z_source.rb"],
        ["m_caller.rb", "a_middle.rb", "z_source.rb"],
        ["z_caller.rb", "m_middle.rb", "a_source.rb"],
        ["m_caller.rb", "z_middle.rb", "a_source.rb"],
    ] {
        for first in 0..3 {
            let files = [
                (names[0], caller.to_string()),
                (names[1], middle.to_string()),
                (names[2], source("Count.new")),
            ];
            let mut editor = FakeEditor::new().await;
            for offset in 0..3 {
                let (name, text) = &files[(first + offset) % 3];
                editor.open(name, text).await;
            }
            let mut seen = vec![mismatch_count(&editor, names[0]).await];
            for value in ["Word.new", "Count.new", "Word.new", "Count.new"] {
                editor.set(names[2], &source(value)).await;
                seen.push(mismatch_count(&editor, names[0]).await);
            }
            assert_eq!(seen, [1, 0, 1, 0, 1], "names {names:?}, first {first}");
        }
    }
}

/// `Const.new` proves an instance of `Const` only while the class keeps the
/// language constructor.
#[tokio::test]
async fn class_replacing_new_is_never_flagged() {
    let caller = "class Word; end\n\n# @param name [Word]\ndef greet(name)\n  name\nend\n\ndef run\n  n = Count.new\n  greet(n)\nend\ngreet(Count.new)\n";
    let plain = "class Count\nend\n";
    let replaced = "class Count\n  def self.new\n    Word.allocate\n  end\nend\n";
    for caller_first in [false, true] {
        let mut editor = FakeEditor::new().await;
        if caller_first {
            editor.open("caller.rb", caller).await;
            editor.open("lib.rb", plain).await;
        } else {
            editor.open("lib.rb", plain).await;
            editor.open("caller.rb", caller).await;
        }
        let mut seen = vec![mismatch_count(&editor, "caller.rb").await > 0];
        for text in [replaced, plain, replaced] {
            editor.set("lib.rb", text).await;
            seen.push(mismatch_count(&editor, "caller.rb").await > 0);
        }
        assert_eq!(
            seen,
            [true, false, true, false],
            "caller first {caller_first}"
        );
    }
}

/// A receiver built by a replaced `new` is not assumed to be an instance, so
/// calls on it are never reported as unresolved. `initialize` keeps the
/// language constructor, for the class and for its subclasses.
#[tokio::test]
async fn receiver_from_replaced_new_is_never_unresolved() {
    const WORD: &str = "class Word\n  def shout; end\nend\n";
    let caller =
        "def run\n  n = Count.new\n  n.shout\n  n.tally\nend\nCount.new.shout\nCount.new.tally\n";
    let typed = vec![2, 5];
    let unknown = vec![];
    let cases = [
        ("plain", "class Count\n  def tally; end\nend\n", &typed),
        (
            "initialize only",
            "class Count\n  def initialize(size = 1); end\n  def tally; end\nend\n",
            &typed,
        ),
        (
            "subclass of an initialize base",
            "class Base\n  def initialize; end\nend\nclass Count < Base\n  def tally; end\nend\n",
            &typed,
        ),
        (
            "replaced",
            "class Count\n  def tally; end\n  def self.new\n    Word.allocate\n  end\nend\n",
            &unknown,
        ),
        (
            "initialize and replaced",
            "class Count\n  def initialize; end\n  def tally; end\n  def self.new\n    Word.allocate\n  end\nend\n",
            &unknown,
        ),
        (
            "subclass of a replaced base",
            "class Base\n  def self.new\n    Word.allocate\n  end\nend\nclass Count < Base\n  def tally; end\nend\n",
            &unknown,
        ),
        (
            "replaced in the singleton class",
            "class Count\n  def tally; end\n  class << self\n    def new\n      Word.allocate\n    end\n  end\nend\n",
            &unknown,
        ),
    ];
    for (name, count, expected) in cases {
        let mut editor = FakeEditor::new().await;
        editor.open("lib.rb", &format!("{WORD}{count}")).await;
        editor.open("caller.rb", caller).await;
        let lines: Vec<u32> = editor
            .diagnostics("caller.rb")
            .await
            .into_iter()
            .map(|diagnostic| diagnostic.range.start.line)
            .collect();
        assert_eq!(&lines, expected, "{name}");
    }
}

/// A parameter documented in this file is a declared type, so passing it on
/// to a differently documented parameter is a contradiction between two
/// declarations. A value derived from it by a call is not compared.
#[tokio::test]
async fn documented_parameter_passed_on_is_checked() {
    check_project(&[(
        "main.rb",
        r#"
# @param label [Symbol]
def find(label)
  label
end

# @param name [String]
# @param key [Symbol]
def forward(name, key)
  find(<warn code="argument-type-mismatch">name</warn>)
  <warn none code="argument-type-mismatch">find(key)</warn>
  find(
    :first,
  )
  find(
    <warn code="argument-type-mismatch">name</warn>
  )
end

# @param name [String]
def reassigned(name, other)
  name = other
  <warn none code="argument-type-mismatch">find(name)</warn>
end

# @param key [Symbol]
def derived(key)
  key = key.to_s
  <warn none code="argument-type-mismatch">find(key)</warn>
end
"#,
    )])
    .await;
}
