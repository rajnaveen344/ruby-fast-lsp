# frozen_string_literal: true

require "minitest/autorun"
require_relative "../runtime"

class RSpecRubyExtensionTest < Minitest::Test
  RANGE = {
    "start" => { "line" => 2, "character" => 6 },
    "end" => { "line" => 2, "character" => 11 }
  }.freeze

  def extension
    $ruby_fast_lsp_extensions.fetch("rspec-ruby")
  end

  def ctx(method_name, args)
    {
      "method_name" => method_name,
      "receiver" => "None",
      "arguments" => args,
      "current_namespace" => ["User"],
      "namespace_kind" => "Singleton",
      "call_range" => RANGE,
      "message_range" => RANGE,
      "resolved_callees" => [],
      "enclosing_calls" => [rspec_describe_call]
    }
  end

  def outside_ctx(method_name, args)
    ctx(method_name, args).merge("enclosing_calls" => [])
  end

  def root_rspec_ctx(method_name, args)
    ctx(method_name, args).merge(
      "receiver" => { "Constant" => ["RSpec"] },
      "resolved_callees" => [
        {
          "owner" => ["RSpec"],
          "owner_kind" => "Singleton",
          "method" => "describe"
        }
      ],
      "current_namespace" => [],
      "namespace_kind" => "Singleton",
      "enclosing_calls" => []
    )
  end

  def rspec_describe_call
    {
      "method_name" => "describe",
      "receiver" => { "Constant" => ["RSpec"] },
      "resolved_callees" => [
        {
          "owner" => ["RSpec"],
          "owner_kind" => "Singleton",
          "method" => "describe"
        }
      ],
      "call_range" => RANGE,
      "message_range" => RANGE
    }
  end

  def symbol_arg(name)
    {
      "value" => { "Symbol" => name },
      "range" => RANGE
    }
  end

  def constant_arg(*parts)
    {
      "value" => { "Constant" => parts },
      "range" => RANGE
    }
  end

  def test_indexed_call_names
    assert_equal ["describe", "context", "it", "example", "specify", "before", "after", "around", "let", "let!", "subject", "subject!", "include", "prepend", "extend"], extension.indexed_call_names
  end

  def test_rspec_describe_defines_root_dsl_method
    patches = extension.index_call(root_rspec_ctx("describe", [constant_arg("User")]))

    method = patches.first.fetch("DefineMethod")
    assert_equal "describe", method.fetch("name")
    assert_equal ["RSpec"], method.fetch("namespace")
    assert_equal "Singleton", method.fetch("owner_kind")
    assert_equal [
      { "name" => "args", "kind" => "Rest" },
      { "name" => "block", "kind" => "Block" }
    ], method.fetch("params")
  end

  def test_context_defines_nested_dsl_method
    patches = extension.index_call(ctx("context", [symbol_arg("active")]))

    method = patches.first.fetch("DefineMethod")
    assert_equal "context", method.fetch("name")
    assert_equal ["User"], method.fetch("namespace")
    assert_equal "Singleton", method.fetch("owner_kind")
    assert_equal [
      { "name" => "args", "kind" => "Rest" },
      { "name" => "block", "kind" => "Block" }
    ], method.fetch("params")
  end

  def test_let_defines_helper_method
    patches = extension.index_call(ctx("let", [symbol_arg("user")]))

    assert_equal 2, patches.length
    macro = patches.map { |patch| patch.fetch("DefineMethod") }.find { |patch| patch.fetch("name") == "let" }
    assert_equal [
      { "name" => "name", "kind" => "Required" },
      { "name" => "block", "kind" => "Block" }
    ], macro.fetch("params")
    method = patches.map { |patch| patch.fetch("DefineMethod") }.find { |patch| patch.fetch("name") == "user" }
    assert_equal "user", method.fetch("name")
    assert_equal ["User"], method.fetch("namespace")
    assert_equal "Instance", method.fetch("owner_kind")
    assert_equal "Public", method.fetch("visibility")
    assert_equal "rspec-ruby", method.fetch("source").fetch("extension_id")
    assert_equal "let", method.fetch("source").fetch("macro_name")
  end

  def test_let_outside_rspec_scope_is_ignored
    assert_equal [], extension.index_call(outside_ctx("let", [symbol_arg("user")]))
  end

  def test_named_subject_defines_named_helper_method
    patches = extension.index_call(ctx("subject", [symbol_arg("record")]))

    method = patches.map { |patch| patch.fetch("DefineMethod") }.find { |patch| patch.fetch("name") == "record" }
    assert_equal "record", method.fetch("name")
  end

  def test_unnamed_subject_defines_subject_method
    patches = extension.index_call(ctx("subject", []))

    method = patches.map { |patch| patch.fetch("DefineMethod") }.find { |patch| patch.fetch("name") == "subject" }
    assert_equal "subject", method.fetch("name")
    assert_equal RANGE, method.fetch("location")
  end

  def test_bang_subject_defines_subject_method
    patches = extension.index_call(ctx("subject!", []))

    method = patches.map { |patch| patch.fetch("DefineMethod") }.find { |patch| patch.fetch("name") == "subject" }
    assert_equal "subject", method.fetch("name")
    assert_equal "subject!", method.fetch("source").fetch("macro_name")
  end

  def test_include_applies_mixin
    patches = extension.index_call(ctx("include", [constant_arg("SpecHelpers")]))

    assert_equal 2, patches.length
    mixin = patches.first.fetch("ApplyMixin")
    assert_equal ["User"], mixin.fetch("namespace")
    assert_equal "Singleton", mixin.fetch("target_kind")
    assert_equal ["SpecHelpers"], mixin.fetch("mixin")
    assert_equal false, mixin.fetch("absolute")
    assert_equal "Include", mixin.fetch("kind")
    assert_equal RANGE, mixin.fetch("location")
    assert_equal "rspec-ruby", mixin.fetch("source").fetch("extension_id")
    assert_equal "include", mixin.fetch("source").fetch("macro_name")
    assert_equal "Instance", patches.last.fetch("ApplyMixin").fetch("target_kind")
  end

  def test_extend_applies_mixin
    patches = extension.index_call(ctx("extend", [constant_arg("SpecHelpers")]))

    assert_equal 1, patches.length
    mixin = patches.first.fetch("ApplyMixin")
    assert_equal "Singleton", mixin.fetch("target_kind")
    assert_equal "Include", mixin.fetch("kind")
  end

  def test_prepend_applies_mixin
    patches = extension.index_call(ctx("prepend", [constant_arg("SpecHelpers")]))

    assert_equal 2, patches.length
    mixin = patches.first.fetch("ApplyMixin")
    assert_equal "Singleton", mixin.fetch("target_kind")
    assert_equal "Prepend", mixin.fetch("kind")
  end

  def test_json_runtime_entrypoints
    assert_equal ["describe", "context", "it", "example", "specify", "before", "after", "around", "let", "let!", "subject", "subject!", "include", "prepend", "extend"], RubyFastLspExtension::Json.parse(RubyFastLspExtensionEntrypoint.indexed_call_names_json)

    input = RubyFastLspExtension::Json.generate(ctx("let", [symbol_arg("user")]))
    patches = RubyFastLspExtension::Json.parse(RubyFastLspExtensionEntrypoint.index_call_json(input))
    method = patches.map { |patch| patch.fetch("DefineMethod") }.find { |patch| patch.fetch("name") == "user" }
    assert_equal "user", method.fetch("name")
  end

  def test_event_runtime_entrypoint
    event = {
      "event" => "index.call.enter",
      "call" => ctx("let", [symbol_arg("user")]),
      "document" => nil
    }

    output = RubyFastLspExtension::Json.parse(RubyFastLspExtensionEntrypoint.handle_event_json(RubyFastLspExtension::Json.generate(event)))
    method = output.fetch("index_patches").map { |patch| patch.fetch("DefineMethod") }.find { |patch| patch.fetch("name") == "user" }
    assert_equal "user", method.fetch("name")
    assert_equal [], output.fetch("response_patches")
    assert_equal [], output.fetch("command_patches")
  end

  def test_document_symbol_event
    event = {
      "event" => "request.document_symbol",
      "call" => nil,
      "document" => {
        "uri" => "file:///repo/spec/user_spec.rb",
        "text" => "RSpec.describe User do\n  context \"active\" do\n    it \"returns name\" do\n    end\n  end\nend\n"
      }
    }

    output = RubyFastLspExtension::Json.parse(RubyFastLspExtensionEntrypoint.handle_event_json(RubyFastLspExtension::Json.generate(event)))
    names = output.fetch("response_patches").map { |patch| patch.fetch("DocumentSymbol").fetch("name") }
    assert_equal ["describe User", "context active", "it returns name"], names
  end

  def test_code_lens_event
    event = {
      "event" => "request.code_lens",
      "call" => nil,
      "document" => {
        "uri" => "file:///repo/spec/user_spec.rb",
        "text" => "RSpec.describe User do\n  it \"returns name\" do\n  end\nend\n"
      }
    }

    output = RubyFastLspExtension::Json.parse(RubyFastLspExtensionEntrypoint.handle_event_json(RubyFastLspExtension::Json.generate(event)))
    titles = output.fetch("response_patches").map { |patch| patch.fetch("CodeLens").fetch("title") }
    assert_equal ["Run RSpec", "Debug RSpec", "Run RSpec", "Debug RSpec"], titles
  end
end
