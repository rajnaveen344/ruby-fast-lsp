# frozen_string_literal: true

require "minitest/autorun"
require_relative "../runtime"

class RSpecRubyExtensionTest < Minitest::Test
  RANGE = {
    "start" => { "line" => 2, "character" => 6 },
    "end" => { "line" => 2, "character" => 11 }
  }.freeze
  BLOCK_RANGE = {
    "start" => { "line" => 2, "character" => 12 },
    "end" => { "line" => 8, "character" => 3 }
  }.freeze
  INDEXED_CALL_NAMES = ["shared_context", "shared_examples", "shared_examples_for", "include_context", "include_examples", "it_behaves_like", "it_should_behave_like", "describe", "context", "it", "example", "specify", "before", "after", "around", "let", "let!", "subject", "subject!", "include", "prepend", "extend"].freeze

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

  def root_shared_context_ctx(name = "authenticated")
    root_rspec_ctx("shared_context", [string_arg(name)]).merge(
      "resolved_callees" => [
        {
          "owner" => ["RSpec"],
          "owner_kind" => "Singleton",
          "method" => "shared_context"
        }
      ]
    )
  end

  def root_shared_examples_ctx(name = "auditable")
    root_rspec_ctx("shared_examples", [string_arg(name)]).merge(
      "resolved_callees" => [
        {
          "owner" => ["RSpec"],
          "owner_kind" => "Singleton",
          "method" => "shared_examples"
        }
      ]
    )
  end

  def with_block(context)
    context.merge("block_range" => BLOCK_RANGE)
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

  def string_arg(name)
    {
      "value" => { "String" => name },
      "range" => RANGE
    }
  end

  def test_indexed_call_names
    assert_equal INDEXED_CALL_NAMES, extension.indexed_call_names
  end

  def test_shared_context_declares_project_scoped_execution_owner
    output = extension.index_call_output(with_block(root_shared_context_ctx))
    context = output.fetch("execution_contexts").fetch(0)
    owner = context.fetch("generated_owners").fetch(0)

    assert_equal "shared-context:authenticated", owner.fetch("local_id")
    assert_equal "Project", owner.fetch("scope")
    assert_equal "Module", owner.fetch("declaration_kind")
    assert_equal({
      "ProjectGeneratedOwner" => {
        "local_id" => "shared-context:authenticated",
        "owner_kind" => "Singleton"
      }
    }, context.fetch("implicit_receiver"))
    assert_equal({
      "ProjectGeneratedOwner" => { "local_id" => "shared-context:authenticated" }
    }, context.fetch("method_definition_owner"))
  end

  def test_include_context_mixes_project_scoped_owner_into_example_group
    patches = extension.index_call(ctx("include_context", [string_arg("authenticated")]))
    mixin = patches.fetch(1).fetch("ApplyMixin")

    assert_equal [], mixin.fetch("mixin")
    assert_equal({
      "ProjectGeneratedOwner" => {
        "local_id" => "shared-context:authenticated",
        "owner_kind" => "Instance"
      }
    }, mixin.fetch("mixin_target"))
    assert_equal "Include", mixin.fetch("kind")
  end

  def test_let_inside_shared_context_targets_project_scoped_owner
    shared_call = root_shared_context_ctx.merge(
      "call_range" => RANGE,
      "message_range" => RANGE
    )
    context = ctx("let", [symbol_arg("shared_user")]).merge(
      "enclosing_calls" => [shared_call]
    )

    patches = extension.index_call(context)
    helper = patches.map { |patch| patch.fetch("DefineMethod") }
      .find { |patch| patch.fetch("name") == "shared_user" }
    assert_equal({
      "ProjectGeneratedOwner" => { "local_id" => "shared-context:authenticated" }
    }, helper.fetch("owner_target"))
  end

  def test_shared_examples_declare_project_template_and_runtime_owners
    shared = with_block(root_shared_examples_ctx)
    output = extension.index_call_output(shared)
    context = output.fetch("execution_contexts").fetch(0)
    owner = context.fetch("generated_owners").fetch(0)
    assert_equal "shared-examples:auditable", owner.fetch("local_id")
    assert_equal "Project", owner.fetch("scope")

    example = with_block(ctx("it", [string_arg("works")]).merge("enclosing_calls" => [shared]))
    runtime = extension.index_call_output(example).fetch("execution_contexts").fetch(0)
    runtime_owner = runtime.fetch("generated_owners").fetch(1)
    assert_equal "shared-examples-runtime:auditable", runtime_owner.fetch("local_id")
    assert_equal "Project", runtime_owner.fetch("scope")
    assert_equal({
      "ProjectGeneratedOwner" => {
        "local_id" => "shared-examples:auditable",
        "owner_kind" => "Instance"
      }
    }, runtime_owner.fetch("parent"))
  end

  def test_shared_example_application_connects_template_group_and_runtime
    patches = extension.index_call(ctx("it_behaves_like", [string_arg("auditable")]))
    assert_equal 3, patches.length
    group_include = patches.fetch(1).fetch("ApplyMixin")
    runtime_application = patches.fetch(2).fetch("ConnectExecutionContext")

    assert_equal({
      "ProjectGeneratedOwner" => {
        "local_id" => "shared-examples:auditable",
        "owner_kind" => "Instance"
      }
    }, group_include.fetch("mixin_target"))
    assert_equal({
      "ProjectGeneratedOwner" => {
        "local_id" => "shared-examples-runtime:auditable",
        "owner_kind" => "Singleton"
      }
    }, runtime_application.fetch("template"))
    assert_equal({
      "GeneratedOwner" => {
        "local_id" => "example-group:2:6-2:11",
        "owner_kind" => "Instance"
      }
    }, runtime_application.fetch("application"))
  end

  def test_rspec_describe_uses_manifest_semantic_target
    patches = extension.index_call(root_rspec_ctx("describe", [constant_arg("User")]))

    assert_empty patches
  end

  def test_root_describe_emits_generated_example_group_execution_context
    output = extension.index_call_output(with_block(root_rspec_ctx("describe", [constant_arg("User")])))

    assert_empty output.fetch("index_patches")
    context = output.fetch("execution_contexts").fetch(0)
    owner = context.fetch("generated_owners").fetch(0)
    expected_id = "example-group:2:6-2:11"
    implicit_target = {
      "GeneratedOwner" => { "local_id" => expected_id, "owner_kind" => "Singleton" }
    }
    definition_target = {
      "GeneratedOwner" => { "local_id" => expected_id, "owner_kind" => "Instance" }
    }

    assert_equal RANGE, context.fetch("call_range")
    assert_equal BLOCK_RANGE, context.fetch("block_range")
    assert_equal expected_id, owner.fetch("local_id")
    assert_equal "Class", owner.fetch("declaration_kind")
    assert_equal "Instance", owner.fetch("owner_kind")
    assert_equal({
      "Namespace" => {
        "namespace" => ["RSpec", "Core", "ExampleGroup"],
        "owner_kind" => "Instance"
      }
    }, owner.fetch("parent"))
    assert_equal implicit_target, context.fetch("implicit_receiver")
    assert_equal definition_target, context.fetch("method_definition_owner")
    assert_equal "Preserve", context.fetch("lexical_scope")
    assert_equal "Preserve", context.fetch("local_scope")
  end

  def test_nested_context_inherits_enclosing_generated_owner
    nested_range = {
      "start" => { "line" => 4, "character" => 2 },
      "end" => { "line" => 4, "character" => 18 }
    }
    nested_block = {
      "start" => { "line" => 4, "character" => 19 },
      "end" => { "line" => 7, "character" => 5 }
    }
    nested = ctx("context", [symbol_arg("active")]).merge(
      "call_range" => nested_range,
      "message_range" => nested_range,
      "block_range" => nested_block
    )

    output = extension.index_call_output(nested)
    context = output.fetch("execution_contexts").fetch(0)
    owners = context.fetch("generated_owners")
    outer_id = "example-group:2:6-2:11"
    nested_id = "example-group:4:2-4:18"

    assert_equal [outer_id, nested_id], owners.map { |owner| owner.fetch("local_id") }
    assert_equal({
      "GeneratedOwner" => { "local_id" => outer_id, "owner_kind" => "Instance" }
    }, owners.last.fetch("parent"))
    assert_equal({
      "GeneratedOwner" => { "local_id" => nested_id, "owner_kind" => "Singleton" }
    }, context.fetch("implicit_receiver"))
    patch = output.fetch("index_patches").fetch(0).fetch("DefineMethod")
    assert_equal({ "GeneratedOwner" => { "local_id" => outer_id } }, patch.fetch("owner_target"))
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
    assert_equal "Block", method.fetch("return_type_source")
  end

  def test_let_outside_rspec_scope_is_ignored
    assert_equal [], extension.index_call(outside_ctx("let", [symbol_arg("user")]))
  end

  def test_named_subject_defines_named_helper_method
    patches = extension.index_call(ctx("subject", [symbol_arg("record")]))

    method = patches.map { |patch| patch.fetch("DefineMethod") }.find { |patch| patch.fetch("name") == "record" }
    assert_equal "record", method.fetch("name")
    assert_equal "Block", method.fetch("return_type_source")
  end

  def test_unnamed_subject_defines_subject_method
    patches = extension.index_call(ctx("subject", []))

    assert_equal 1, patches.length
    method = patches.map { |patch| patch.fetch("DefineMethod") }.find { |patch| patch.fetch("name") == "subject" }
    assert_equal "subject", method.fetch("name")
    assert_equal RANGE, method.fetch("location")
    assert_equal "Block", method.fetch("return_type_source")
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
    assert_equal INDEXED_CALL_NAMES, RubyFastLspExtension::Json.parse(RubyFastLspExtensionEntrypoint.indexed_call_names_json)

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

  def test_watched_file_event_updates_private_extension_state
    received = nil
    extension.on_watched_files_changed { |files| received = files }
    files = [{
      "workspace_root" => "/repo",
      "path" => "config/routes.rb",
      "uri" => "file:///repo/config/routes.rb",
      "kind" => "Changed"
    }]

    output = extension.handle_event("event" => "files.changed", "files" => files)

    assert_equal files, received
    assert_equal [], output.fetch("index_patches")
    assert_equal [], output.fetch("response_patches")
    assert_equal [], output.fetch("command_patches")
  end

  def test_process_request_and_completion_callbacks
    received = nil
    extension.on_watched_files_changed do |_files|
      [extension.process_request(
        request_id: "routes",
        command: "bundle",
        arguments: ["exec", "rails", "routes"],
        timeout_ms: 2_000
      )]
    end
    extension.on_process_completed do |results|
      received = results
      [{"workspace_root" => "/repo", "path" => "app/models/user.rb"}]
    end

    output = extension.handle_event("event" => "files.changed", "files" => [])
    request = output.fetch("process_requests").fetch(0)
    assert_equal "routes", request.fetch("request_id")
    assert_equal "bundle", request.fetch("command")
    assert_equal ["exec", "rails", "routes"], request.fetch("arguments")

    results = [{"request_id" => "routes", "status" => "Exited", "exit_code" => 0}]
    completion = extension.handle_event("event" => "process.completed", "process_results" => results)
    assert_equal results, received
    assert_equal [], completion.fetch("process_requests")
    assert_equal(
      [{"workspace_root" => "/repo", "path" => "app/models/user.rb"}],
      completion.fetch("reindex_files")
    )
  end
end
