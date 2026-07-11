# frozen_string_literal: true

require "minitest/autorun"
require_relative "../runtime"

class RailsRubyTest < Minitest::Test
  RANGE = {
    "start" => {"line" => 1, "character" => 13},
    "end" => {"line" => 1, "character" => 20}
  }.freeze

  def extension
    RubyFastLspExtensionEntrypoint.ruby_fast_lsp_extension
  end

  def context(method_name, association, keywords = [])
    {
      "method_name" => method_name,
      "receiver" => "None",
      "arguments" => [
        {"value" => {"Symbol" => association}, "range" => RANGE},
        *keywords
      ],
      "current_namespace" => ["User"],
      "namespace_kind" => "Instance",
      "call_range" => RANGE,
      "message_range" => RANGE,
      "resolved_callees" => [],
      "enclosing_calls" => []
    }
  end

  def test_belongs_to_emits_reference_reader_and_writer
    patches = extension.index_call(context("belongs_to", "account"))
    reference = patches.fetch(0).fetch("AddReference")
    assert_equal({"Namespace" => ["Account"]}, reference.fetch("target"))

    reader = patches.fetch(1).fetch("DefineMethod")
    assert_equal "account", reader.fetch("name")
    assert_equal ["User"], reader.fetch("namespace")
    assert_equal "Public", reader.fetch("visibility")
    assert_equal(
      {"Union" => [{"Named" => "Account"}, {"Named" => "NilClass"}]},
      reader.fetch("return_type")
    )

    writer = patches.fetch(2).fetch("DefineMethod")
    assert_equal "account=", writer.fetch("name")
    assert_equal [{"name" => "value", "kind" => "Required"}], writer.fetch("params")
  end

  def test_has_many_singularizes_collection_type
    patches = extension.index_call(context("has_many", "companies"))
    reader = patches.fetch(1).fetch("DefineMethod")
    assert_equal "companies", reader.fetch("name")
    assert_equal({"Array" => [{"Named" => "Company"}]}, reader.fetch("return_type"))
  end

  def test_has_one_emits_a_nilable_singular_reader
    patches = extension.index_call(context("has_one", "profile"))
    reader = patches.fetch(1).fetch("DefineMethod")
    assert_equal "profile", reader.fetch("name")
    assert_equal(
      {"Union" => [{"Named" => "Profile"}, {"Named" => "NilClass"}]},
      reader.fetch("return_type")
    )
  end

  def test_class_name_overrides_conventional_target
    keywords = [{
      "keyword" => {"name" => "class_name", "range" => RANGE},
      "value" => {"String" => "Billing::Account"},
      "range" => RANGE
    }]
    patches = extension.index_call(context("belongs_to", "account", keywords))

    reference = patches.fetch(0).fetch("AddReference")
    assert_equal({"Namespace" => ["Billing", "Account"]}, reference.fetch("target"))
    reader = patches.fetch(1).fetch("DefineMethod")
    assert_equal(
      {"Union" => [{"Named" => "Billing::Account"}, {"Named" => "NilClass"}]},
      reader.fetch("return_type")
    )
  end

  def test_polymorphic_association_does_not_guess_a_target_class
    keywords = [{
      "keyword" => {"name" => "polymorphic", "range" => RANGE},
      "value" => {"Boolean" => true},
      "range" => RANGE
    }]
    patches = extension.index_call(context("belongs_to", "subject", keywords))

    assert_equal 2, patches.length
    assert patches.all? { |patch| !patch.key?("AddReference") }
    reader = patches.fetch(0).fetch("DefineMethod")
    assert_equal "Unknown", reader.fetch("return_type")
  end

  def test_callback_symbol_references_an_instance_method
    patches = extension.index_call(context("before_save", "normalize_account"))

    assert_equal 1, patches.length
    reference = patches.fetch(0).fetch("AddReference")
    assert_equal(
      {
        "Method" => {
          "namespace" => ["User"],
          "owner_kind" => "Instance",
          "name" => "normalize_account"
        }
      },
      reference.fetch("target")
    )
  end

  def test_custom_validation_symbol_references_a_private_method
    patches = extension.index_call(context("validate", "account_is_active"))

    reference = patches.fetch(0).fetch("AddReference")
    assert_equal "account_is_active", reference.dig("target", "Method", "name")
  end

  def test_attribute_validation_symbol_references_its_reader
    patches = extension.index_call(context("validates_presence_of", "account"))

    reference = patches.fetch(0).fetch("AddReference")
    assert_equal "account", reference.dig("target", "Method", "name")
  end
end
