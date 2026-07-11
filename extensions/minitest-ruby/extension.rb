# frozen_string_literal: true

require "ruby_fast_lsp_extension"

module MinitestRuby
  def self.test_file?(uri)
    path = uri.to_s
    path.end_with?("_test.rb") || path.include?("/test/")
  end

  def self.nodes(document)
    return [] unless test_file?(document.uri)

    output = []
    document.text.to_s.split("\n").each_with_index do |line, index|
      node = node_for_line(line, index)
      output << node if node
    end
    output
  end

  def self.node_for_line(line, index)
    stripped = line.lstrip
    indent = line.length - stripped.length
    if stripped.start_with?("class ")
      name = token(stripped[6..-1].to_s)
      return nil unless name && name.split("::").last.end_with?("Test")

      return node("Class", name, nil, line, index, indent, 6, name.length)
    end
    if stripped.start_with?("def test_")
      name = method_token(stripped[4..-1].to_s)
      return nil unless name

      return node("Method", name, name, line, index, indent, 4, name.length)
    end
    return nil unless stripped.start_with?("test ") || stripped.start_with?("test(")

    rest = stripped[4..-1].to_s.strip
    rest = rest[1..-1].to_s.strip if rest.start_with?("(")
    description = quoted(rest)
    return nil unless description && !description.empty?

    node("Method", description, "test_: #{description}", line, index, indent, 0, 4)
  end

  def self.token(text)
    chars = []
    text.each_char do |character|
      break if [" ", "<", "(", ";"].include?(character)

      chars << character
    end
    value = chars.join
    value.empty? ? nil : value
  end

  def self.method_token(text)
    chars = []
    text.each_char do |character|
      break if [" ", "(", "=", ";"].include?(character)

      chars << character
    end
    value = chars.join
    value.start_with?("test_") && value.length > 5 ? value : nil
  end

  def self.quoted(text)
    quote = text[0, 1]
    return nil unless quote == "\"" || quote == "'"

    chars = []
    escaped = false
    text[1..-1].to_s.each_char do |character|
      if escaped
        chars << character
        escaped = false
      elsif character == "\\"
        escaped = true
      elsif character == quote
        return chars.join
      else
        chars << character
      end
    end
    nil
  end

  def self.node(kind, label, test_name, line, index, indent, selection_offset, selection_length)
    {
      "kind" => kind,
      "label" => label,
      "test_name" => test_name,
      "range" => source_range(index, indent, line.length),
      "selection_range" => source_range(
        index,
        indent + selection_offset,
        indent + selection_offset + selection_length
      )
    }
  end

  def self.source_range(line, start_char, end_char)
    {
      "start" => {"line" => line, "character" => start_char},
      "end" => {"line" => line, "character" => end_char}
    }
  end
end

extension "minitest-ruby" do
  on_document_symbols do |document|
    MinitestRuby.nodes(document).filter_map do |node|
      next if node["kind"] == "Class"

      document_symbol(
        name: node["label"],
        kind: node["kind"],
        range: node["range"],
        selection_range: node["selection_range"],
        source: extension_source("minitest")
      )
    end
  end

  on_code_lens do |document|
    MinitestRuby.nodes(document).each_with_object([]) do |node, lenses|
      line = (node["range"]["start"]["line"] + 1).to_s
      arguments = [document.uri.to_s, line, node["test_name"].to_s]
      lenses << code_lens_patch(
        title: "Run Minitest",
        command: "ruby-fast-lsp.minitest.run",
        range: node["selection_range"],
        arguments: arguments,
        source: extension_source("minitest")
      )
      lenses << code_lens_patch(
        title: "Debug Minitest",
        command: "ruby-fast-lsp.minitest.debug",
        range: node["selection_range"],
        arguments: arguments,
        source: extension_source("minitest")
      )
    end
  end
end
