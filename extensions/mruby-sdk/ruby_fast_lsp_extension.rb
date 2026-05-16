# frozen_string_literal: true

module RubyFastLspExtension
  ABI_VERSION = 1

  module Json
    def self.parse(input)
      Parser.new(input).parse
    end

    def self.generate(value)
      case value
      when Hash
        "{" + value.map { |key, item| generate(key.to_s) + ":" + generate(item) }.join(",") + "}"
      when Array
        "[" + value.map { |item| generate(item) }.join(",") + "]"
      when String
        generate_string(value)
      when Numeric
        value.to_s
      when true
        "true"
      when false
        "false"
      when nil
        "null"
      else
        raise "unsupported JSON value: #{value.class}"
      end
    end

    def self.generate_string(value)
      escaped = value.to_s.gsub("\\", "\\\\")
      escaped = escaped.gsub("\"", "\\\"")
      escaped = escaped.gsub("\n", "\\n")
      escaped = escaped.gsub("\r", "\\r")
      escaped = escaped.gsub("\t", "\\t")
      "\"" + escaped + "\""
    end

    class Parser
      def initialize(input)
        @input = input
        @index = 0
      end

      def parse
        value = parse_value
        skip_ws
        raise "trailing JSON bytes at #{@index}" unless eof?

        value
      end

      private

      def parse_value
        skip_ws
        char = peek
        case char
        when "{"
          parse_object
        when "["
          parse_array
        when "\""
          parse_string
        when "t"
          read_literal("true", true)
        when "f"
          read_literal("false", false)
        when "n"
          read_literal("null", nil)
        else
          parse_number
        end
      end

      def parse_object
        expect("{")
        object = {}
        skip_ws
        return object if consume("}")

        loop do
          key = parse_string
          skip_ws
          expect(":")
          object[key] = parse_value
          skip_ws
          return object if consume("}")
          expect(",")
        end
      end

      def parse_array
        expect("[")
        array = []
        skip_ws
        return array if consume("]")

        loop do
          array << parse_value
          skip_ws
          return array if consume("]")
          expect(",")
        end
      end

      def parse_string
        expect("\"")
        output = String.new
        until eof?
          char = next_char
          return output if char == "\""

          if char == "\\"
            output << escaped_char
          else
            output << char
          end
        end
        raise "unterminated JSON string"
      end

      def escaped_char
        char = next_char
        case char
        when "\"", "\\", "/"
          char
        when "b"
          "\b"
        when "f"
          "\f"
        when "n"
          "\n"
        when "r"
          "\r"
        when "t"
          "\t"
        when "u"
          code = @input[@index, 4]
          @index += 4
          value = code.to_i(16)
          value < 128 ? value.chr : "?"
        else
          raise "invalid JSON escape: #{char}"
        end
      end

      def parse_number
        start = @index
        @index += 1 if peek == "-"
        read_digits
        if peek == "."
          @index += 1
          read_digits
        end
        if peek == "e" || peek == "E"
          @index += 1
          @index += 1 if peek == "+" || peek == "-"
          read_digits
        end
        raw = @input[start...@index]
        raise "invalid JSON number at #{start}" if raw.nil? || raw.empty? || raw == "-"

        raw.include?(".") || raw.include?("e") || raw.include?("E") ? raw.to_f : raw.to_i
      end

      def read_digits
        raise "expected JSON digit at #{@index}" unless digit?(peek)

        @index += 1 while digit?(peek)
      end

      def read_literal(text, value)
        actual = @input[@index, text.length]
        raise "expected JSON literal #{text} at #{@index}" unless actual == text

        @index += text.length
        value
      end

      def expect(char)
        actual = next_char
        raise "expected JSON char #{char}, got #{actual}" unless actual == char
      end

      def consume(char)
        return false unless peek == char

        @index += 1
        true
      end

      def skip_ws
        @index += 1 while [" ", "\n", "\r", "\t"].include?(peek)
      end

      def next_char
        char = peek
        raise "unexpected JSON end" if char.nil?

        @index += 1
        char
      end

      def peek
        @input[@index]
      end

      def eof?
        @index >= @input.length
      end

      def digit?(char)
        char && char >= "0" && char <= "9"
      end
    end
  end

  class Registry
    attr_reader :extensions

    def initialize
      @extensions = {}
    end

    def register(extension)
      @extensions[extension.id] = extension
    end

    def fetch(id)
      ext = @extensions[id]
      raise "unknown extension: #{id}" unless ext

      ext
    end
  end

  class Extension
    attr_reader :id

    def initialize(id)
      @id = id
      @handlers = {}
      @document_symbol_handler = nil
      @code_lens_handler = nil
    end

    def on_call(*names, &block)
      names.each { |name| @handlers[name] = block }
    end

    def on_document_symbols(&block)
      @document_symbol_handler = block
    end

    def on_code_lens(&block)
      @code_lens_handler = block
    end

    def indexed_call_names
      @handlers.keys
    end

    def index_call(raw_ctx)
      ctx = Context.new(raw_ctx)
      handler = @handlers[ctx.method_name]
      return [] unless handler

      result = handler.call(ctx)
      result || []
    end

    def handle_event(raw_event)
      event_name = raw_event["event"] || raw_event[:event]
      case event_name
      when "index.call.enter"
        {
          "index_patches" => index_call(raw_event["call"] || raw_event[:call]),
          "response_patches" => [],
          "command_patches" => []
        }
      when "request.document_symbol"
        {
          "index_patches" => [],
          "response_patches" => document_symbols(raw_event["document"] || raw_event[:document]),
          "command_patches" => []
        }
      when "request.code_lens"
        {
          "index_patches" => [],
          "response_patches" => code_lens(raw_event["document"] || raw_event[:document]),
          "command_patches" => []
        }
      else
        {
          "index_patches" => [],
          "response_patches" => [],
          "command_patches" => []
        }
      end
    end

    def document_symbols(raw_document)
      return [] unless @document_symbol_handler

      result = @document_symbol_handler.call(DocumentContext.new(raw_document))
      result || []
    end

    def code_lens(raw_document)
      return [] unless @code_lens_handler

      result = @code_lens_handler.call(DocumentContext.new(raw_document))
      result || []
    end

    def define_method(name:, namespace:, owner_kind:, location:, source:, visibility: :public, return_type: nil, params: [])
      {
        "DefineMethod" => {
          "name" => name.to_s,
          "namespace" => namespace,
          "owner_kind" => camel(owner_kind),
          "visibility" => camel(visibility),
          "location" => location,
          "params" => params.map { |param| method_param(param) },
          "return_type" => return_type,
          "source" => source
        }
      }
    end

    def method_param(param)
      {
        "name" => (param[:name] || param["name"]).to_s,
        "kind" => camel(param[:kind] || param["kind"])
      }
    end

    def apply_mixin(namespace:, mixin:, kind:, location:, source:, absolute: false, target_kind: :instance)
      {
        "ApplyMixin" => {
          "namespace" => namespace,
          "target_kind" => camel(target_kind),
          "mixin" => mixin,
          "absolute" => absolute,
          "kind" => camel(kind),
          "location" => location,
          "source" => source
        }
      }
    end

    def document_symbol(name:, kind:, range:, selection_range:, source:, detail: nil)
      {
        "DocumentSymbol" => {
          "name" => name.to_s,
          "detail" => detail,
          "kind" => kind.to_s,
          "range" => range,
          "selection_range" => selection_range,
          "source" => source
        }
      }
    end

    def code_lens_patch(title:, command:, range:, arguments:, source:)
      {
        "CodeLens" => {
          "title" => title.to_s,
          "command" => command.to_s,
          "range" => range,
          "arguments" => arguments,
          "source" => source
        }
      }
    end

    def extension_source(macro_name)
      {
        "extension_id" => id,
        "macro_name" => macro_name.to_s
      }
    end

    def macro_source(ctx)
      {
        "extension_id" => id,
        "macro_name" => ctx.method_name
      }
    end

    private

    def camel(value)
      value.to_s.split("_").map { |part| part[0].upcase + part[1..-1] }.join
    end
  end

  class Context
    def initialize(raw)
      @raw = raw
    end

    def method_name
      fetch("method_name")
    end

    def receiver
      fetch("receiver")
    end

    def arguments
      fetch("arguments").map { |arg| Argument.new(arg) }
    end

    def current_namespace
      fetch("current_namespace")
    end

    def namespace_kind
      fetch("namespace_kind")
    end

    def call_range
      fetch("call_range")
    end

    def message_range
      fetch("message_range")
    end

    def resolved_callees
      fetch("resolved_callees") || []
    end

    def enclosing_calls
      fetch("enclosing_calls") || []
    end

    private

    def fetch(key)
      @raw[key] || @raw[key.to_sym]
    end
  end

  class DocumentContext
    def initialize(raw)
      @raw = raw || {}
    end

    def uri
      fetch("uri")
    end

    def text
      fetch("text")
    end

    private

    def fetch(key)
      @raw[key] || @raw[key.to_sym]
    end
  end

  class Argument
    def initialize(raw)
      @raw = raw
    end

    def value
      @raw["value"] || @raw[:value]
    end

    def range
      @raw["range"] || @raw[:range]
    end

    def symbol?
      value.is_a?(Hash) && value.key?("Symbol")
    end

    def string?
      value.is_a?(Hash) && value.key?("String")
    end

    def constant?
      value.is_a?(Hash) && value.key?("Constant")
    end

    def symbol_or_string
      return value["Symbol"] if symbol?
      return value["String"] if string?

      nil
    end

    def constant_path
      return value["Constant"] if constant?

      nil
    end
  end
end

$ruby_fast_lsp_extensions ||= RubyFastLspExtension::Registry.new

def extension(id, &block)
  ext = RubyFastLspExtension::Extension.new(id)
  ext.instance_eval(&block)
  $ruby_fast_lsp_extensions.register(ext)
  ext
end
