//! Tests for unresolved-method tracking on expression receivers + chains.
//!
//! Today: `user.unknown` (expression receiver) is silently skipped — type inference
//! resolves the receiver class but no diagnostic is emitted when the method is missing.
//!
//! Fix: emit `unresolved-method` ONLY when the receiver class is fully known and the
//! method does not exist on that class. Downstream chain links after a broken call
//! stay silent — once the first link is flagged, further "unknown receiver" warnings
//! would be redundant noise.
//!
//! Note: `User.new` itself currently warns ("Unresolved method `new` on `User`")
//! because `Class#new` isn't in the user index. Tests scope assertions tightly
//! around the calls under test to avoid colliding with that pre-existing noise.

use crate::indexer::file_processor::FileProcessor;
use crate::test::harness::{check, check_multi_file, FakeEditor};
use ruby_analysis::core::{
    FullyQualifiedName, MethodAvailability, NamespaceKind, RubyConstant, RubyMethod, SourceKind,
};
use ruby_analysis::engine::AnalysisQuery;
use tower_lsp::lsp_types::{NumberOrString, Url};

#[tokio::test]
async fn expr_receiver_known_type_unknown_method_warns() {
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
u.<warn code="unresolved-method">foo</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn expr_receiver_known_type_known_method_no_warn() {
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
<warn none code="unresolved-method">u.name</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn typed_value_constants_use_instance_method_resolution() {
    check(
        r#"
class User
  validates_each :username do |record, attr, value|
    <warn none code="unresolved-method">NAMES.include?("admin")</warn>
    <warn none code="unresolved-method">PATTERN.to_s</warn>
  end

  NAMES = ["admin", "guest"].freeze
  PATTERN = /user/
end
"#,
    )
    .await;
}

#[tokio::test]
async fn method_missing_suppresses_unresolved_method_warn() {
    check(
        r#"
class DynamicRecord
  def method_missing(name, *args)
    "dynamic"
  end
end

record = DynamicRecord.new
<warn none code="unresolved-method">record.virtual_total</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn default_basic_object_stub_method_missing_does_not_hide_missing_calls() {
    let mut editor = FakeEditor::new().await;
    let stub_uri =
        Url::parse("file:///ruby-fast-lsp-stubs/basic_object.rb").expect("stub URI must be valid");
    FileProcessor::new()
        .collect_file_facts_as(
            &stub_uri,
            r#"
class BasicObject
  def method_missing(name, *args)
  end
end

class Object < BasicObject
end
"#,
            editor.server(),
            SourceKind::Stub,
        )
        .expect("default BasicObject stub must index");

    editor
        .open(
            "jruby_import.rb",
            "java_import java.util.concurrent.TimeUnit\n",
        )
        .await;

    let diagnostics = editor.diagnostics("jruby_import.rb").await;
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(NumberOrString::String("unresolved-method".to_string()))
                && diagnostic.message.contains("java_import")
        }),
        "the default raising BasicObject#method_missing stub must not prove that `java_import` exists: {diagnostics:?}"
    );
}

#[tokio::test]
async fn jruby_9_2_runtime_source_outranks_java_import_overlay_navigation() {
    let mut editor = FakeEditor::new().await;
    let overlay_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("support")
        .join("jruby")
        .join("stubs");
    let common_path = overlay_root.join("common").join("runtime.rb");
    let overlay_path = overlay_root.join("9.2").join("runtime.rb");
    let common_uri = Url::from_file_path(&common_path)
        .expect("shared JRuby overlay path must convert to a file URI");
    let overlay_uri = Url::from_file_path(&overlay_path)
        .expect("JRuby 9.2 overlay path must convert to a file URI");
    let common = std::fs::read_to_string(&common_path)
        .expect("shared JRuby overlay must be readable for black-box testing");
    let overlay = std::fs::read_to_string(&overlay_path)
        .expect("JRuby 9.2 overlay must be readable for black-box testing");
    FileProcessor::new()
        .collect_file_facts_as(&common_uri, &common, editor.server(), SourceKind::Stub)
        .expect("shared JRuby overlay must index through the ordinary fact path");
    FileProcessor::new()
        .collect_file_facts_as(&overlay_uri, &overlay, editor.server(), SourceKind::Stub)
        .expect("JRuby 9.2 overlay must index through the ordinary fact path");
    let runtime_uri = Url::parse("file:///ruby-fast-lsp-runtime/jruby/core_ext/object.rb")
        .expect("runtime fixture URI must be valid");
    FileProcessor::new()
        .collect_file_facts_as(
            &runtime_uri,
            "class Object\n  private\n  def java_import(*import_classes)\n    import_classes.flatten.each { |import_class| import_class }\n  end\nend\n",
            editor.server(),
            SourceKind::Stdlib,
        )
        .expect("JRuby runtime implementation source must use the ordinary fact path");

    editor
        .open(
            "jruby_import_supported.rb",
            "java_import java.util.concurrent.TimeUnit\n",
        )
        .await;

    let diagnostics = editor.diagnostics("jruby_import_supported.rb").await;
    assert!(
        diagnostics.iter().all(|diagnostic| {
            diagnostic.code
                != Some(NumberOrString::String("unresolved-method".to_string()))
                || !diagnostic.message.contains("java_import")
        }),
        "JRuby's java_import declaration must suppress only its own unresolved-method diagnostic: {diagnostics:?}"
    );

    let definitions = editor.goto_def_at("jruby_import_supported.rb", 0, 5).await;
    assert_eq!(
        definitions.iter().map(|location| &location.uri).collect::<Vec<_>>(),
        vec![&runtime_uri],
        "goto-definition for java_import must prefer the selected runtime implementation over bundled declaration stubs: {definitions:?}; common overlay: {common_uri}; series overlay: {overlay_uri}"
    );
}

#[tokio::test]
async fn unavailable_runtime_stub_method_emits_actionable_diagnostic() {
    let mut editor = FakeEditor::new().await;
    let stub_uri = Url::parse("file:///ruby-fast-lsp-stubs/jruby-9.2-unavailable.rb")
        .expect("stub URI must be valid");
    FileProcessor::new()
        .collect_file_facts_as(
            &stub_uri,
            r#"
module Process
  # @unavailable JRuby does not implement process forking on the JVM.
  def self.fork
  end
end
"#,
            editor.server(),
            SourceKind::Stub,
        )
        .expect("unavailable runtime stub must index");
    let process = RubyConstant::new("Process").expect("Process must be a valid Ruby constant");
    let fork = FullyQualifiedName::method(
        vec![process],
        RubyMethod::new("fork").expect("fork must be a valid Ruby method"),
    );
    let engine = editor.server().analysis_engine.read();
    let facts = AnalysisQuery::new(&engine).methods_for_fqn(&fork);
    assert!(
        facts.iter().any(|fact| {
            fact.owner
                == FullyQualifiedName::namespace_with_kind(vec![process], NamespaceKind::Singleton)
                && matches!(fact.availability, MethodAvailability::Unavailable { .. })
        }),
        "@unavailable must survive parsing and fact replacement: {facts:?}"
    );
    drop(engine);

    editor.open("jruby_unavailable.rb", "Process.fork\n").await;

    let diagnostics = editor.diagnostics("jruby_unavailable.rb").await;
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code
                == Some(NumberOrString::String(
                    "unsupported-runtime-api".to_string(),
                ))
                && diagnostic.message.contains("process forking")
        }),
        "a resolved but unavailable runtime API must produce its specific diagnostic rather than unresolved-method noise: {diagnostics:?}"
    );
    assert!(
        diagnostics.iter().all(|diagnostic| {
            diagnostic.code
                != Some(NumberOrString::String("unresolved-method".to_string()))
        }),
        "an unavailable method is known and must not also be reported as unresolved: {diagnostics:?}"
    );

    FileProcessor::new()
        .collect_file_facts_as(
            &stub_uri,
            "module Process\n  def self.fork\n  end\nend\n",
            editor.server(),
            SourceKind::Stub,
        )
        .expect("replacing the runtime stub must use the ordinary file lifecycle");
    let diagnostics_after_replacement = editor.diagnostics("jruby_unavailable.rb").await;
    assert!(
        diagnostics_after_replacement.iter().all(|diagnostic| {
            diagnostic.code
                != Some(NumberOrString::String(
                    "unsupported-runtime-api".to_string(),
                ))
        }),
        "replacing the owning stub must remove stale availability metadata: {diagnostics_after_replacement:?}"
    );
}

#[tokio::test]
async fn absent_runtime_overlay_masks_and_restores_compatible_baseline_method() {
    let mut editor = FakeEditor::new().await;
    let baseline_uri = Url::parse("file:///ruby-fast-lsp-stubs/mri-2.5-object-space.rb")
        .expect("baseline stub URI must be valid");
    let overlay_uri = Url::parse("file:///ruby-fast-lsp-stubs/jruby-9.2-absent.rb")
        .expect("overlay stub URI must be valid");
    FileProcessor::new()
        .collect_file_facts_as(
            &baseline_uri,
            "module ObjectSpace\n  def self.dump(object)\n  end\nend\n",
            editor.server(),
            SourceKind::Stub,
        )
        .expect("MRI compatibility baseline must index");
    FileProcessor::new()
        .collect_file_facts_as(
            &overlay_uri,
            r#"
module ObjectSpace
  # @absent JRuby 9.2 does not expose MRI's ObjectSpace.dump API.
  def self.dump(object)
  end
end
"#,
            editor.server(),
            SourceKind::Stub,
        )
        .expect("JRuby absent-method overlay must index");
    editor
        .open("jruby_absent.rb", "ObjectSpace.dump(nil)\n")
        .await;

    let diagnostics = editor.diagnostics("jruby_absent.rb").await;
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.code == Some(NumberOrString::String("unresolved-method".to_string()))
                && diagnostic.message.contains("dump")
        }),
        "an @absent overlay must mask the compatible baseline from method lookup: {diagnostics:?}"
    );
    assert!(
        editor
            .goto_def_at("jruby_absent.rb", 0, 15)
            .await
            .is_empty(),
        "an absent runtime API must not remain navigable through the MRI baseline"
    );

    FileProcessor::new()
        .collect_file_facts_as(&overlay_uri, "", editor.server(), SourceKind::Stub)
        .expect("clearing the overlay file must replace its facts");
    let restored_diagnostics = editor.diagnostics("jruby_absent.rb").await;
    assert!(
        restored_diagnostics.iter().all(|diagnostic| {
            diagnostic.code != Some(NumberOrString::String("unresolved-method".to_string()))
        }),
        "removing the @absent owner must restore the compatible baseline: {restored_diagnostics:?}"
    );
    assert!(
        editor
            .goto_def_at("jruby_absent.rb", 0, 15)
            .await
            .iter()
            .any(|location| location.uri == baseline_uri),
        "baseline navigation must return after the absent overlay is removed"
    );
}

#[tokio::test]
async fn duplicate_inherited_method_defs_do_not_warn() {
    check_multi_file(&[
        (
            "child.rb",
            r#"
class Child < Base
  def save
    <warn none code="unresolved-method">run</warn>
  end
end
"#,
        ),
        (
            "base_a.rb",
            r#"
class Base
  def run
  end
end
"#,
        ),
        (
            "base_b.rb",
            r#"
class Base
  def run
  end
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn chain_first_link_flagged_downstream_silent() {
    // u.foo unresolved on User → flag foo only. .bar's receiver type is
    // unknown after the broken link, so do NOT add additional noise.
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
u.<warn code="unresolved-method">foo</warn>.<warn none code="unresolved-method">bar</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn chain_returning_unknown_stays_silent_downstream() {
    // name's return type is Unknown → upcase has unknown receiver → no warn.
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
<warn none code="unresolved-method">u.name.upcase</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn bare_kernel_method_in_class_does_not_warn_when_object_includes_kernel() {
    check_multi_file(&[
        (
            "scenario_extractor.rb",
            r#"
class ScenarioExtractor
  def to_output
    <warn none code="unresolved-method">puts "ok"</warn>
  end
end
"#,
        ),
        (
            "kernel.rb",
            r#"
module Kernel
  def puts(obj = nil, *args)
  end
end
"#,
        ),
        (
            "object.rb",
            r#"
class Object
  include Kernel
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn top_level_kernel_methods_do_not_warn_when_object_includes_kernel() {
    check_multi_file(&[
        (
            "boot.rb",
            r#"
<warn none code="unresolved-method">require "json"</warn>
"#,
        ),
        (
            "kernel.rb",
            r#"
module Kernel
  def require(name)
  end
end
"#,
        ),
        (
            "object.rb",
            r#"
class Object
  include Kernel
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn instance_kernel_exception_methods_do_not_warn_when_object_includes_kernel() {
    check_multi_file(&[
        (
            "processor.rb",
            r#"
class Processor
  def run
    <warn none code="unresolved-method">raise "bad"</warn>
  end
end
"#,
        ),
        (
            "kernel.rb",
            r#"
module Kernel
  def raise(*args)
  end
end
"#,
        ),
        (
            "object.rb",
            r#"
class Object
  include Kernel
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn module_instance_kernel_methods_do_not_warn_when_object_includes_kernel() {
    check_multi_file(&[
        (
            "feature.rb",
            r#"
module Feature
  def run
    <warn none code="unresolved-method">raise "bad"</warn>
    <warn none code="unresolved-method">__method__</warn>
  end
end
"#,
        ),
        (
            "kernel.rb",
            r#"
module Kernel
  def raise(*args)
  end

  def __method__
  end
end
"#,
        ),
        (
            "object.rb",
            r#"
class Object
  include Kernel
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn bare_kernel_method_in_nested_class_uses_implicit_object_superclass() {
    check_multi_file(&[
        (
            "nested.rb",
            r#"
module Reports
  class ScenarioExtractor
    def to_output
      <warn none code="unresolved-method">puts "ok"</warn>
    end
  end
end
"#,
        ),
        (
            "kernel.rb",
            r#"
module Kernel
  def puts(obj = nil, *args)
  end
end
"#,
        ),
        (
            "object.rb",
            r#"
class Object
  include Kernel
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn array_include_predicate_does_not_warn_when_receiver_type_is_known() {
    check(
        r#"
items = [1, 2, 3]
<warn none code="unresolved-method">items.include?(2)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn visibility_modifier_does_not_warn_as_unresolved_method() {
    check(
        r#"
class BulkAccountActionForm
  <warn none code="unresolved-method">private</warn>

  def bulk_account_action_validation_helper
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn bare_raise_does_not_warn_as_unresolved_method() {
    check_multi_file(&[
        (
            "bulk_account_action_form.rb",
            r#"
class BulkAccountActionForm
  def bulk_account_action_validation_helper
    return <warn none code="unresolved-method">raise GosPosh::Platform::Errors::InvalidInputError.new("Action")</warn>
  end
end
"#,
        ),
        (
            "kernel.rb",
            r#"
module Kernel
  def raise(...)
  end
end
"#,
        ),
        (
            "object.rb",
            r#"
class Object
  include Kernel
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn class_eval_method_does_not_leak_to_lexical_class() {
    check(
        r#"
class MetaTarget
end

class LexicalOwner
  ::MetaTarget.class_eval do
    def patched
      "patched"
    end
  end
end

MetaTarget.new.<warn none code="unresolved-method">patched</warn>
LexicalOwner.new.<warn code="unresolved-method">patched</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn dynamic_definition_blocks_diagnose_against_runtime_receiver() {
    check(
        r#"
class MetaTarget
  def instance_helper
    "instance"
  end

  def self.singleton_helper
    "singleton"
  end

  define_method(:instance_generated) do
    <warn none code="unresolved-method">instance_helper</warn>
  end
end

MetaTarget.define_singleton_method(:singleton_generated) do
  <warn none code="unresolved-method">singleton_helper</warn>
end
"#,
    )
    .await;
}
