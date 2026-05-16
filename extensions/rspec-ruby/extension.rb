# frozen_string_literal: true

require "ruby_fast_lsp_extension"

module RSpecRuby
  CALLS = ["RSpec.describe", "describe", "context", "it", "example", "specify", "shared_examples", "shared_context"]
  EXAMPLES = ["it", "example", "specify"]

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
        owner == ["RSpec"] && ["describe", "context", "shared_examples", "shared_context"].include?(method)
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

  def self.dsl_params(method_name)
    case method_name
    when "let", "let!"
      [{ name: "name", kind: :required }, { name: "block", kind: :block }]
    when "subject", "subject!"
      [{ name: "name", kind: :optional }, { name: "block", kind: :block }]
    when "describe", "context", "it", "example", "specify", "before", "after", "around"
      [{ name: "args", kind: :rest }, { name: "block", kind: :block }]
    else
      raise "unknown RSpec DSL method for params: #{method_name}"
    end
  end
end

extension "rspec-ruby" do
  on_call "describe" do |ctx|
    if RSpecRuby.rspec_root_describe?(ctx)
      next [
        define_method(
          name: ctx.method_name,
          namespace: ["RSpec"],
          owner_kind: :singleton,
          location: ctx.message_range,
          params: RSpecRuby.dsl_params(ctx.method_name),
          source: macro_source(ctx)
        )
      ]
    end

    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    [
      define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      )
    ]
  end

  on_call "context", "it", "example", "specify", "before", "after", "around" do |ctx|
    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    [
      define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      )
    ]
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
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      ),
      define_method(
        name: name,
        namespace: ctx.current_namespace,
        owner_kind: :instance,
        location: arg.range,
        source: macro_source(ctx)
      )
    ]
  end

  on_call "subject", "subject!" do |ctx|
    next [] unless RSpecRuby.inside_rspec_scope?(ctx)

    arg = ctx.arguments.first
    name = arg && arg.symbol_or_string
    location = arg ? arg.range : ctx.call_range

    [
      define_method(
        name: ctx.method_name,
        namespace: ctx.current_namespace,
        owner_kind: ctx.namespace_kind,
        location: ctx.message_range,
        params: RSpecRuby.dsl_params(ctx.method_name),
        source: macro_source(ctx)
      ),
      define_method(
        name: name || "subject",
        namespace: ctx.current_namespace,
        owner_kind: :instance,
        location: location,
        source: macro_source(ctx)
      )
    ]
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
