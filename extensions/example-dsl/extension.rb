# frozen_string_literal: true

require "ruby_fast_lsp_extension"

module ExampleDsl
  def self.field_nodes(document)
    nodes = []
    document.text.to_s.split("\n").each_with_index do |line, line_number|
      stripped = line.lstrip
      next unless stripped.start_with?("field :")

      name = String.new
      stripped[7..-1].to_s.each_char do |character|
        valid = (character >= "a" && character <= "z") ||
          (character >= "A" && character <= "Z") ||
          (character >= "0" && character <= "9") ||
          ["_", "!", "?", "="].include?(character)
        break unless valid

        name << character
      end
      next if name.nil? || name.empty?

      indent = line.length - stripped.length
      nodes << {
        "name" => name,
        "range" => source_range(line_number, indent, line.length),
        "selection_range" => source_range(line_number, indent + 7, indent + 7 + name.length)
      }
    end
    nodes
  end

  def self.source_range(line, start_character, end_character)
    {
      "start" => {"line" => line, "character" => start_character},
      "end" => {"line" => line, "character" => end_character}
    }
  end
end

extension "example-dsl" do
  on_call "field" do |ctx|
    argument = ctx.arguments.first
    name = argument && argument.symbol_or_string
    next [] unless name

    [
      define_method(
        name: name,
        namespace: ctx.current_namespace,
        owner_kind: :instance,
        location: argument.range,
        return_type: {"Named" => "String"},
        source: macro_source(ctx)
      )
    ]
  end

  on_document_symbols do |document|
    ExampleDsl.field_nodes(document).map do |node|
      document_symbol(
        name: "field #{node.fetch("name")}",
        detail: "Example DSL field",
        kind: "Field",
        range: node.fetch("range"),
        selection_range: node.fetch("selection_range"),
        source: extension_source("document.symbol")
      )
    end
  end

  on_code_lens do |document|
    ExampleDsl.field_nodes(document).map do |node|
      code_lens_patch(
        title: "Inspect field",
        command: "ruby-fast-lsp.example.inspectField",
        range: node.fetch("range"),
        arguments: [node.fetch("name")],
        source: extension_source("code.lens")
      )
    end
  end
end
