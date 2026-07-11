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

  def context(method_name, association, keywords = [], enclosing_calls = [])
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
      "enclosing_calls" => enclosing_calls
    }
  end

  def routes_draw_frame
    {
      "method_name" => "draw",
      "receiver" => {"MethodCall" => {"method_name" => "routes"}},
      "arguments" => [],
      "resolved_callees" => [],
      "call_range" => RANGE,
      "message_range" => RANGE
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

  def test_resources_generates_rest_helpers_and_controller_reference
    patches = extension.index_call(context("resources", "users", [], [routes_draw_frame]))

    reference = patches.fetch(0).fetch("AddReference")
    assert_equal({"Namespace" => ["UsersController"]}, reference.fetch("target"))
    methods = patches.drop(1).map { |patch| patch.fetch("DefineMethod") }
    assert_equal(
      %w[users_path users_url new_user_path new_user_url edit_user_path edit_user_url user_path user_url],
      methods.map { |method| method.fetch("name") }
    )
    assert methods.all? { |method| method.fetch("namespace") == ["ApplicationController"] }
    assert methods.all? { |method| method.fetch("return_type") == {"Named" => "String"} }
  end

  def test_named_route_generates_helpers_and_controller_action_references
    route_range = {
      "start" => {"line" => 2, "character" => 17},
      "end" => {"line" => 2, "character" => 27}
    }
    name_range = {
      "start" => {"line" => 2, "character" => 33},
      "end" => {"line" => 2, "character" => 40}
    }
    route_context = context("get", "/account", [
      {
        "keyword" => {"name" => "to", "range" => route_range},
        "value" => {"String" => "users#show"},
        "range" => route_range
      },
      {
        "keyword" => {"name" => "as", "range" => name_range},
        "value" => {"Symbol" => "account"},
        "range" => name_range
      }
    ], [routes_draw_frame])
    patches = extension.index_call(route_context)

    assert_equal({"Namespace" => ["UsersController"]}, patches.fetch(0).dig("AddReference", "target"))
    assert_equal(
      {
        "Method" => {
          "namespace" => ["UsersController"],
          "owner_kind" => "Instance",
          "name" => "show"
        }
      },
      patches.fetch(1).dig("AddReference", "target")
    )
    assert_equal %w[account_path account_url], patches.drop(2).map { |patch| patch.dig("DefineMethod", "name") }
  end


  def test_nested_namespace_prefixes_helpers_and_controller_target
    namespace_frame = {
      "method_name" => "namespace",
      "receiver" => "None",
      "arguments" => [{"value" => {"Symbol" => "admin"}, "range" => RANGE}],
      "resolved_callees" => [],
      "call_range" => RANGE,
      "message_range" => RANGE
    }
    patches = extension.index_call(context("resources", "users", [], [routes_draw_frame, namespace_frame]))

    assert_equal(
      {"Namespace" => ["Admin", "UsersController"]},
      patches.fetch(0).dig("AddReference", "target")
    )
    assert_equal(
      %w[admin_users_path admin_users_url new_admin_user_path new_admin_user_url edit_admin_user_path edit_admin_user_url admin_user_path admin_user_url],
      patches.drop(1).map { |patch| patch.dig("DefineMethod", "name") }
    )
  end

  def test_scope_module_and_as_prefix_controller_and_helpers
    scope_frame = {
      "method_name" => "scope",
      "receiver" => "None",
      "arguments" => [
        {
          "keyword" => {"name" => "module", "range" => RANGE},
          "value" => {"Symbol" => "admin"},
          "range" => RANGE
        },
        {
          "keyword" => {"name" => "as", "range" => RANGE},
          "value" => {"Symbol" => "admin"},
          "range" => RANGE
        }
      ],
      "resolved_callees" => [],
      "call_range" => RANGE,
      "message_range" => RANGE
    }
    patches = extension.index_call(context("resources", "users", [], [routes_draw_frame, scope_frame]))

    assert_equal(
      {"Namespace" => ["Admin", "UsersController"]},
      patches.fetch(0).dig("AddReference", "target")
    )
    assert_equal "admin_users_path", patches.fetch(1).dig("DefineMethod", "name")
  end

  def test_resources_uses_common_irregular_singular_route_name
    patches = extension.index_call(context("resources", "people", [], [routes_draw_frame]))
    names = patches.drop(1).map { |patch| patch.dig("DefineMethod", "name") }

    assert_includes names, "person_path"
    assert_includes names, "new_person_path"
    refute_includes names, "new_people_path"
  end

  def test_namespaced_irregular_resource_singularizes_before_prefix
    namespace_frame = {
      "method_name" => "namespace",
      "receiver" => "None",
      "arguments" => [{"value" => {"Symbol" => "admin"}, "range" => RANGE}],
      "resolved_callees" => [],
      "call_range" => RANGE,
      "message_range" => RANGE
    }
    patches = extension.index_call(context("resources", "people", [], [routes_draw_frame, namespace_frame]))
    names = patches.drop(1).map { |patch| patch.dig("DefineMethod", "name") }

    assert_includes names, "new_admin_person_path"
    refute_includes names, "new_admin_people_path"
  end

  def test_resources_outside_routes_draw_are_ignored
    unrelated_draw = routes_draw_frame.merge(
      "receiver" => {"MethodCall" => {"method_name" => "canvas"}}
    )

    assert_empty extension.index_call(context("resources", "users", [], [unrelated_draw]))
  end

  def test_active_job_enqueue_entry_points_reference_the_perform_method
    %w[perform_later perform_now].each do |entry_point|
      job_context = context(entry_point, "user")
      job_context["receiver"] = {"Constant" => ["Billing", "EmailJob"]}
      job_context["current_namespace"] = []

      patches = extension.index_call(job_context)

      assert_equal 1, patches.length
      reference = patches.fetch(0).fetch("AddReference")
      assert_equal(
        {
          "Method" => {
            "namespace" => ["Billing", "EmailJob"],
            "owner_kind" => "Instance",
            "name" => "perform"
          }
        },
        reference.fetch("target")
      )
      assert_equal RANGE, reference.fetch("location")
    end
  end

  def test_active_job_entry_point_requires_a_constant_job_receiver
    job_context = context("perform_later", "user")
    job_context["receiver"] = {"LocalVariable" => "job_class"}
    job_context["current_namespace"] = []

    assert_empty extension.index_call(job_context)
  end
end
