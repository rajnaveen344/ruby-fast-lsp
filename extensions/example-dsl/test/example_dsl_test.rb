# frozen_string_literal: true

require "minitest/autorun"
require_relative "../runtime"

class ExampleDslTest < Minitest::Test
  def extension
    RubyFastLspExtensionEntrypoint.ruby_fast_lsp_extension
  end

  def test_public_sdk_produces_method_symbol_and_lens_patches
    range = {
      "start" => {"line" => 1, "character" => 9},
      "end" => {"line" => 1, "character" => 13}
    }
    call = {
      "method_name" => "field",
      "receiver" => "None",
      "arguments" => [{"value" => {"Symbol" => "name"}, "range" => range}],
      "current_namespace" => ["ExampleModel"],
      "namespace_kind" => "Instance",
      "call_range" => range,
      "message_range" => range,
      "resolved_callees" => [],
      "enclosing_calls" => []
    }
    patches = extension.index_call(call)
    namespace_patch = patches.fetch(0).fetch("DefineNamespace")
    assert_equal ["GeneratedRecord"], namespace_patch.fetch("namespace")
    assert_equal "Class", namespace_patch.fetch("kind")

    constant_patch = patches.fetch(1).fetch("DefineConstant")
    assert_equal "DEFAULT_NAME", constant_patch.fetch("name")
    assert_equal ["GeneratedRecord"], constant_patch.fetch("namespace")
    assert_equal({"Named" => "String"}, constant_patch.fetch("ruby_type"))

    method_patch = patches.fetch(2).fetch("DefineMethod")
    assert_equal "name", method_patch.fetch("name")
    assert_equal ["ExampleModel"], method_patch.fetch("namespace")
    assert_equal "Private", method_patch.fetch("visibility")

    document = {
      "uri" => "file:///app/example_model.rb",
      "text" => "class ExampleModel\n  field :name\nend\n"
    }
    symbols = extension.handle_event(
      "event" => "request.document_symbol",
      "document" => document
    ).fetch("response_patches")
    assert_equal "field name", symbols.fetch(0).fetch("DocumentSymbol").fetch("name")

    lenses = extension.handle_event(
      "event" => "request.code_lens",
      "document" => document
    ).fetch("response_patches")
    assert_equal "Inspect field", lenses.fetch(0).fetch("CodeLens").fetch("title")
  end
end
