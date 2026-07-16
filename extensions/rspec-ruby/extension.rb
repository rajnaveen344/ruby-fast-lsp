# frozen_string_literal: true

require "ruby_fast_lsp_extension"

module RSpecRuby
  CALLS = ["RSpec.describe", "describe", "context", "it", "example", "specify", "shared_examples", "shared_examples_for", "shared_context", "include_examples", "it_behaves_like", "it_should_behave_like"]
  EXAMPLES = ["it", "example", "specify"]
  SHARED_EXAMPLE_DECLARATIONS = ["shared_examples", "shared_examples_for"]
  SHARED_EXAMPLE_APPLICATIONS = ["include_examples", "it_behaves_like", "it_should_behave_like"]

  def self.nodes(document)
    output = []
    document.text.to_s.split("\n").each_with_index do |line, index|
      node = node_for_line(line, index)
      output << node if node
    end
    output
  end

  def self.node_for_line(line, index)
    stripped = line.lstrip
    call = matching_call(stripped)
    return nil unless call

    keyword = call == "RSpec.describe" ? "describe" : call
    indent = line.length - stripped.length
    rest = stripped[call.length..-1].to_s.strip
    label = label_for(keyword, rest)
    range = source_range(index, indent, line.length)
    {
      "keyword" => keyword,
      "label" => label,
      "kind" => EXAMPLES.include?(keyword) ? "Method" : "Namespace",
      "range" => range,
      "selection_range" => source_range(index, indent, indent + call.length)
    }
  end

  def self.matching_call(stripped)
    CALLS.each do |call|
      next unless stripped[0, call.length] == call

      char = stripped[call.length, 1]
      return call if char.nil? || [" ", "(", "\"", "'"].include?(char)
    end
    nil
  end

  def self.label_for(keyword, rest)
    value = first_arg(rest)
    value && value.length > 0 ? "#{keyword} #{value}" : keyword
  end

  def self.first_arg(rest)
    text = rest
    text = text[1..-1].to_s.strip if text[0, 1] == "("
    return quoted(text, "\"") if text[0, 1] == "\""
    return quoted(text, "'") if text[0, 1] == "'"

    token = []
    text.each_char do |char|
      break if [" ", ",", ")", "{"].include?(char)

      token << char
    end
    token.join
  end

  def self.quoted(text, quote)
    chars = []
    escaped = false
    text[1..-1].to_s.each_char do |char|
      if escaped
        chars << char
        escaped = false
      elsif char == "\\"
        escaped = true
      elsif char == quote
        break
      else
        chars << char
      end
    end
    chars.join
  end

  def self.source_range(line, start_char, end_char)
    {
      "start" => { "line" => line, "character" => start_char },
      "end" => { "line" => line, "character" => end_char }
    }
  end

  def self.inside_rspec_scope?(ctx)
    ctx.enclosing_calls.any? do |call|
      callees = call["resolved_callees"] || call[:resolved_callees] || []
      callees.any? do |callee|
        owner = callee["owner"] || callee[:owner]
        method = callee["method"] || callee[:method]
        owner == ["RSpec"] && ["describe", "context", "shared_examples", "shared_examples_for", "shared_context"].include?(method)
      end
    end
  end

  def self.rspec_root_describe?(ctx)
    receiver = ctx.receiver
    receiver == { "Constant" => ["RSpec"] } &&
      ctx.method_name == "describe" &&
      ctx.resolved_callees.any? do |callee|
        owner = callee["owner"] || callee[:owner]
        owner == ["RSpec"]
      end
  end

  def self.rspec_root_shared_context?(ctx)
    receiver = ctx.receiver
    receiver == { "Constant" => ["RSpec"] } &&
      ctx.method_name == "shared_context" &&
      ctx.resolved_callees.any? do |callee|
        owner = callee["owner"] || callee[:owner]
        method = callee["method"] || callee[:method]
        owner == ["RSpec"] && method == "shared_context"
      end
  end

  def self.rspec_root_shared_examples?(ctx)
    receiver = ctx.receiver
    receiver == { "Constant" => ["RSpec"] } &&
      SHARED_EXAMPLE_DECLARATIONS.include?(ctx.method_name) &&
      ctx.resolved_callees.any? do |callee|
        owner = callee["owner"] || callee[:owner]
        method = callee["method"] || callee[:method]
        owner == ["RSpec"] && SHARED_EXAMPLE_DECLARATIONS.include?(method)
      end
  end

  def self.dsl_params(method_name)
    case method_name
    when "let", "let!"
      [{ name: "name", kind: :required }, { name: "block", kind: :block }]
    when "subject", "subject!"
      [{ name: "name", kind: :optional }, { name: "block", kind: :block }]
    when "describe", "context", "shared_context", "include_context", "shared_examples", "shared_examples_for", "include_examples", "it_behaves_like", "it_should_behave_like", "it", "example", "specify", "before", "after", "around"
      [{ name: "args", kind: :rest }, { name: "block", kind: :block }]
    else
      raise "unknown RSpec DSL method for params: #{method_name}"
    end
  end

  def self.group_call?(call)
    method = call["method_name"] || call[:method_name]
    receiver = call["receiver"] || call[:receiver]
    return false unless ["describe", "context"].include?(method)

    return true if receiver == "None"

    callees = call["resolved_callees"] || call[:resolved_callees] || []
    callees.any? do |callee|
      (callee["owner"] || callee[:owner]) == ["RSpec"]
    end
  end

  def self.shared_context_call?(call)
    method = call["method_name"] || call[:method_name]
    receiver = call["receiver"] || call[:receiver]
    return false unless method == "shared_context"
    return false unless receiver == { "Constant" => ["RSpec"] }

    callees = call["resolved_callees"] || call[:resolved_callees] || []
    callees.any? do |callee|
      owner = callee["owner"] || callee[:owner]
      resolved_method = callee["method"] || callee[:method]
      owner == ["RSpec"] && resolved_method == "shared_context"
    end
  end

  def self.shared_examples_call?(call)
    method = call["method_name"] || call[:method_name]
    receiver = call["receiver"] || call[:receiver]
    return false unless SHARED_EXAMPLE_DECLARATIONS.include?(method)
    return false unless receiver == { "Constant" => ["RSpec"] }

    callees = call["resolved_callees"] || call[:resolved_callees] || []
    callees.any? do |callee|
      owner = callee["owner"] || callee[:owner]
      resolved_method = callee["method"] || callee[:method]
      owner == ["RSpec"] && SHARED_EXAMPLE_DECLARATIONS.include?(resolved_method)
    end
  end

  def self.shared_context_name(arguments)
    argument = arguments.first
    return nil unless argument
    return argument.symbol_or_string if argument.respond_to?(:symbol_or_string)

    value = argument["value"] || argument[:value] || {}
    value["Symbol"] || value[:Symbol] || value["String"] || value[:String]
  end

  def self.shared_context_owner_id(name)
    "shared-context:#{name}"
  end

  def self.shared_context_target(builder, name, owner_kind: nil)
    builder.project_generated_owner_target(
      shared_context_owner_id(name),
      owner_kind: owner_kind
    )
  end

  def self.shared_examples_owner_id(name)
    "shared-examples:#{name}"
  end

  def self.shared_examples_runtime_owner_id(name)
    "shared-examples-runtime:#{name}"
  end

  def self.shared_examples_target(builder, name, owner_kind: nil)
    builder.project_generated_owner_target(
      shared_examples_owner_id(name),
      owner_kind: owner_kind
    )
  end

  def self.shared_examples_runtime_target(builder, name, owner_kind: nil)
    builder.project_generated_owner_target(
      shared_examples_runtime_owner_id(name),
      owner_kind: owner_kind
    )
  end

  def self.group_owner_id(range)
    start = range["start"] || range[:start]
    finish = range["end"] || range[:end]
    "example-group:#{start['line'] || start[:line]}:#{start['character'] || start[:character]}-#{finish['line'] || finish[:line]}:#{finish['character'] || finish[:character]}"
  end

  def self.current_group_target(ctx, builder, owner_kind: nil)
    call = ctx.enclosing_calls.reverse.find do |enclosing|
      group_call?(enclosing) || shared_context_call?(enclosing) || shared_examples_call?(enclosing)
    end
    return nil unless call

    if shared_context_call?(call)
      name = shared_context_name(call["arguments"] || call[:arguments] || [])
      return name && shared_context_target(builder, name, owner_kind: owner_kind)
    end
    if shared_examples_call?(call)
      name = shared_context_name(call["arguments"] || call[:arguments] || [])
      return name && shared_examples_target(builder, name, owner_kind: owner_kind)
    end

    range = call["call_range"] || call[:call_range]
    builder.generated_owner_target(group_owner_id(range), owner_kind: owner_kind)
  end

  def self.enclosing_shared_examples_name(ctx)
    call = ctx.enclosing_calls.reverse.find { |enclosing| shared_examples_call?(enclosing) }
    return nil unless call

    shared_context_name(call["arguments"] || call[:arguments] || [])
  end

  def self.runtime_owner_id(method_name, range)
    "runtime-#{method_name}:#{group_owner_id(range).sub('example-group:', '')}"
  end

  def self.shared_runtime_owner_id(group_range)
    "group-runtime:#{group_owner_id(group_range)}"
  end
end

extension "rspec-ruby" do
  on_call "shared_context" do |ctx|
    next [] unless RSpecRuby.rspec_root_shared_context?(ctx)

    name = RSpecRuby.shared_context_name(ctx.arguments)
    next [] unless name && ctx.block_range

    local_id = RSpecRuby.shared_context_owner_id(name)
    target = RSpecRuby.shared_context_target(self, name)
    index_output(
      index_patches: [],
      execution_contexts: [
        block_execution_context(
          call_range: ctx.call_range,
          block_range: ctx.block_range,
          generated_owners: [
            generated_owner(
              local_id: local_id,
              scope: :project,
              declaration_kind: :module,
              owner_kind: :instance
            )
          ],
          implicit_receiver: RSpecRuby.shared_context_target(self, name, owner_kind: :singleton),
          method_definition_owner: target,
          source: macro_source(ctx)
        )
      ]
    )
  end

  on_call "shared_examples", "shared_examples_for" do |ctx|
    next [] unless RSpecRuby.rspec_root_shared_examples?(ctx)

    name = RSpecRuby.shared_context_name(ctx.arguments)
    next [] unless name && ctx.block_range

    owner_id = RSpecRuby.shared_examples_owner_id(name)
    owner_target = RSpecRuby.shared_examples_target(self, name)
    index_output(
      index_patches: [],
      execution_contexts: [
        block_execution_context(
          call_range: ctx.call_range,
          block_range: ctx.block_range,
          generated_owners: [
            generated_owner(
              local_id: owner_id,
              scope: :project,
              declaration_kind: :module,
              owner_kind: :instance
            )
          ],
          implicit_receiver: RSpecRuby.shared_examples_target(self, name, owner_kind: :singleton),
          method_definition_owner: owner_target,
          source: macro_source(ctx)
        )
      ]
    )
  end

  on_call "include_context" do |ctx|
    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    argument = ctx.arguments.first
    name = argument && argument.symbol_or_string
    next [] unless name

    [
      define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_target: RSpecRuby.current_group_target(ctx, self),
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      ),
      apply_mixin(
        namespace: ctx.current_namespace,
        owner_target: RSpecRuby.current_group_target(ctx, self),
        target_kind: :instance,
        mixin_target: RSpecRuby.shared_context_target(self, name, owner_kind: :instance),
        kind: :include,
        location: argument.range,
        source: macro_source(ctx)
      )
    ]
  end

  on_call "include_examples", "it_behaves_like", "it_should_behave_like" do |ctx|
    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    argument = ctx.arguments.first
    name = argument && argument.symbol_or_string
    group_target = RSpecRuby.current_group_target(ctx, self, owner_kind: :instance)
    next [] unless name && group_target

    [
      define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_target: group_target,
        owner_kind: :instance,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      ),
      apply_mixin(
        namespace: ctx.current_namespace,
        owner_target: group_target,
        target_kind: :instance,
        mixin_target: RSpecRuby.shared_examples_target(self, name, owner_kind: :instance),
        kind: :include,
        location: argument.range,
        source: macro_source(ctx)
      ),
      connect_execution_context(
        template: RSpecRuby.shared_examples_runtime_target(self, name, owner_kind: :singleton),
        application: group_target,
        location: argument.range,
        source: macro_source(ctx)
      )
    ]
  end

  on_call "describe" do |ctx|
    root = RSpecRuby.rspec_root_describe?(ctx)
    nested = RSpecRuby.inside_rspec_scope?(ctx)
    next [] unless root || nested

    patches = root ? [] : [
      define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_target: RSpecRuby.current_group_target(ctx, self),
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      )
    ]
    next patches unless ctx.block_range

    owners = []
    parent = namespace_execution_target(
      namespace: ["RSpec", "Core", "ExampleGroup"],
      owner_kind: :instance
    )
    ctx.enclosing_calls.select { |call| RSpecRuby.group_call?(call) }.each do |call|
      range = call["call_range"] || call[:call_range]
      local_id = RSpecRuby.group_owner_id(range)
      owners << generated_owner(local_id: local_id, parent: parent)
      parent = generated_owner_target(local_id, owner_kind: :instance)
    end
    current_id = RSpecRuby.group_owner_id(ctx.call_range)
    owners << generated_owner(local_id: current_id, parent: parent)
    implicit_target = generated_owner_target(current_id, owner_kind: :singleton)
    definition_target = generated_owner_target(current_id, owner_kind: :instance)
    index_output(
      index_patches: patches,
      execution_contexts: [
        block_execution_context(
          call_range: ctx.call_range,
          block_range: ctx.block_range,
          generated_owners: owners,
          implicit_receiver: implicit_target,
          method_definition_owner: definition_target,
          source: macro_source(ctx)
        )
      ]
    )
  end

  on_call "context" do |ctx|
    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    patches = [
      define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_target: RSpecRuby.current_group_target(ctx, self),
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      )
    ]
    next patches unless ctx.block_range

    owners = []
    parent = namespace_execution_target(
      namespace: ["RSpec", "Core", "ExampleGroup"],
      owner_kind: :instance
    )
    ctx.enclosing_calls.select { |call| RSpecRuby.group_call?(call) }.each do |call|
      range = call["call_range"] || call[:call_range]
      local_id = RSpecRuby.group_owner_id(range)
      owners << generated_owner(local_id: local_id, parent: parent)
      parent = generated_owner_target(local_id, owner_kind: :instance)
    end
    current_id = RSpecRuby.group_owner_id(ctx.call_range)
    owners << generated_owner(local_id: current_id, parent: parent)
    implicit_target = generated_owner_target(current_id, owner_kind: :singleton)
    definition_target = generated_owner_target(current_id, owner_kind: :instance)
    index_output(
      index_patches: patches,
      execution_contexts: [
        block_execution_context(
          call_range: ctx.call_range,
          block_range: ctx.block_range,
          generated_owners: owners,
          implicit_receiver: implicit_target,
          method_definition_owner: definition_target,
          source: macro_source(ctx)
        )
      ]
    )
  end

  on_call "it", "example", "specify", "before", "after", "around" do |ctx|
    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    patches = [
      define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_target: RSpecRuby.current_group_target(ctx, self),
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      )
    ]
    next patches unless ctx.block_range

    shared_examples_name = RSpecRuby.enclosing_shared_examples_name(ctx)
    if shared_examples_name
      owner_target = RSpecRuby.shared_examples_target(
        self,
        shared_examples_name,
        owner_kind: :instance
      )
      runtime_id = RSpecRuby.shared_examples_runtime_owner_id(shared_examples_name)
      runtime_target = RSpecRuby.shared_examples_runtime_target(
        self,
        shared_examples_name,
        owner_kind: :singleton
      )
      next index_output(
        index_patches: patches,
        execution_contexts: [
          block_execution_context(
            call_range: ctx.call_range,
            block_range: ctx.block_range,
            generated_owners: [
              generated_owner(
                local_id: RSpecRuby.shared_examples_owner_id(shared_examples_name),
                scope: :project,
                declaration_kind: :module,
                owner_kind: :instance
              ),
              generated_owner(
                local_id: runtime_id,
                scope: :project,
                declaration_kind: :class,
                owner_kind: :singleton,
                parent: owner_target
              )
            ],
            implicit_receiver: runtime_target,
            method_definition_owner: runtime_target,
            source: macro_source(ctx)
          )
        ]
      )
    end

    owners = []
    parent = namespace_execution_target(
      namespace: ["RSpec", "Core", "ExampleGroup"],
      owner_kind: :instance
    )
    ctx.enclosing_calls.select { |call| RSpecRuby.group_call?(call) }.each do |call|
      range = call["call_range"] || call[:call_range]
      local_id = RSpecRuby.group_owner_id(range)
      owners << generated_owner(local_id: local_id, parent: parent)
      parent = generated_owner_target(local_id, owner_kind: :instance)
    end
    next patches if owners.empty?

    group_call = ctx.enclosing_calls.reverse.find { |call| RSpecRuby.group_call?(call) }
    next patches unless group_call

    group_range = group_call["call_range"] || group_call[:call_range]
    shared_runtime_id = RSpecRuby.shared_runtime_owner_id(group_range)
    owners << generated_owner(
      local_id: shared_runtime_id,
      owner_kind: :singleton,
      parent: parent
    )
    shared_runtime_target = generated_owner_target(shared_runtime_id, owner_kind: :singleton)
    if ["before", "after", "around"].include?(ctx.method_name)
      runtime_target = shared_runtime_target
    else
      runtime_id = RSpecRuby.runtime_owner_id(ctx.method_name, ctx.call_range)
      owners << generated_owner(
        local_id: runtime_id,
        owner_kind: :singleton,
        parent: shared_runtime_target
      )
      runtime_target = generated_owner_target(runtime_id, owner_kind: :singleton)
    end
    index_output(
      index_patches: patches,
      execution_contexts: [
        block_execution_context(
          call_range: ctx.call_range,
          block_range: ctx.block_range,
          generated_owners: owners,
          implicit_receiver: runtime_target,
          method_definition_owner: runtime_target,
          source: macro_source(ctx)
        )
      ]
    )
  end

  on_call "let", "let!" do |ctx|
    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    arg = ctx.arguments.first
    name = arg && arg.symbol_or_string
    next [] unless name

    [
      define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_target: RSpecRuby.current_group_target(ctx, self),
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      ),
      define_method(
        name: name,
        namespace: ctx.current_namespace,
        owner_target: RSpecRuby.current_group_target(ctx, self),
        owner_kind: :instance,
        location: arg.range,
        return_type_source: :block,
        source: macro_source(ctx)
      )
    ]
  end

  on_call "subject", "subject!" do |ctx|
    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    arg = ctx.arguments.first
    name = arg && arg.symbol_or_string
    location = arg ? arg.range : ctx.call_range

    patches = []
    if name || ctx.method_name != "subject"
      patches << define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_target: RSpecRuby.current_group_target(ctx, self),
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      )
    end
    patches << define_method(
      name: name || "subject",
      namespace: ctx.current_namespace,
      owner_target: RSpecRuby.current_group_target(ctx, self),
      owner_kind: :instance,
      location: location,
      return_type_source: :block,
      source: macro_source(ctx)
    )
    patches
  end

  on_call "include", "prepend", "extend" do |ctx|
    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    ctx.arguments.map do |arg|
      mixin = arg.constant_path
      next nil unless mixin

      singleton_kind = ctx.method_name == "extend" ? "include" : ctx.method_name
      patches = [
        apply_mixin(
          namespace: ctx.current_namespace,
          owner_target: RSpecRuby.current_group_target(ctx, self),
          target_kind: :singleton,
          mixin: mixin,
          absolute: false,
          kind: singleton_kind,
          location: arg.range,
          source: macro_source(ctx)
        )
      ]
      unless ctx.method_name == "extend"
        patches << apply_mixin(
          namespace: ctx.current_namespace,
          owner_target: RSpecRuby.current_group_target(ctx, self),
          target_kind: :instance,
          mixin: mixin,
          absolute: false,
          kind: ctx.method_name,
          location: arg.range,
          source: macro_source(ctx)
        )
      end
      patches
    end.compact.flatten
  end

  on_document_symbols do |document|
    RSpecRuby.nodes(document).map do |node|
      document_symbol(
        name: node["label"],
        kind: node["kind"],
        range: node["range"],
        selection_range: node["selection_range"],
        source: extension_source(node["keyword"])
      )
    end
  end

  on_code_lens do |document|
    lenses = []
    RSpecRuby.nodes(document).each do |node|
      line = (node["range"]["start"]["line"] + 1).to_s
      target = document.uri.to_s + ":" + line
      lenses << code_lens_patch(
        title: "Run RSpec",
        command: "ruby-fast-lsp.rspec.run",
        range: node["selection_range"],
        arguments: [document.uri.to_s, line, target],
        source: extension_source(node["keyword"])
      )
      lenses << code_lens_patch(
        title: "Debug RSpec",
        command: "ruby-fast-lsp.rspec.debug",
        range: node["selection_range"],
        arguments: [document.uri.to_s, line, target],
        source: extension_source(node["keyword"])
      )
    end
    lenses
  end
end
