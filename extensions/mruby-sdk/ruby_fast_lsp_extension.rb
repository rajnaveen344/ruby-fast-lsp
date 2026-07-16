# frozen_string_literal: true

require "json"

module RubyFastLspExtension
  ABI_VERSION = 1

  module Json
    def self.parse(input)
      ::JSON.parse(input)
    end

    def self.generate(value)
      ::JSON.generate(value)
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
    attr_reader :settings, :project_context

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

    def on_activate(&block)
      @activation_handler = block
    end

    def on_settings_changed(&block)
      @settings_handler = block
    end

    def on_deactivate(&block)
      @deactivation_handler = block
    end

    def on_watched_files_changed(&block)
      @watched_files_handler = block
    end

    def on_process_completed(&block)
      @process_completed_handler = block
    end

    def process_request(request_id:, command:, arguments: [], stdin: nil, workspace_root: nil, timeout_ms: nil)
      {
        "request_id" => request_id,
        "command" => command,
        "arguments" => arguments,
        "stdin" => stdin,
        "workspace_root" => workspace_root,
        "timeout_ms" => timeout_ms
      }
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
      index_call_output(raw_ctx)["index_patches"]
    end

    def index_call_output(raw_ctx)
      ctx = Context.new(raw_ctx)
      handler = @handlers[ctx.method_name]
      return empty_output unless handler

      result = handler.call(ctx)
      normalize_index_output(result)
    end

    def handle_event(raw_event)
      event_name = raw_event["event"] || raw_event[:event]
      case event_name
      when "lifecycle.activate"
        @settings = raw_event["settings"] || raw_event[:settings]
        @project_context = raw_event["project"] || raw_event[:project]
        @activation_handler.call(@settings) if @activation_handler
        empty_output
      when "settings.changed"
        @settings = raw_event["settings"] || raw_event[:settings]
        @settings_handler.call(@settings) if @settings_handler
        empty_output
      when "lifecycle.deactivate"
        @deactivation_handler.call if @deactivation_handler
        empty_output
      when "files.changed"
        files = raw_event["files"] || raw_event[:files] || []
        requests = @watched_files_handler ? (@watched_files_handler.call(files) || []) : []
        empty_output.merge("process_requests" => requests)
      when "process.completed"
        results = raw_event["process_results"] || raw_event[:process_results] || []
        reindex_files = @process_completed_handler ? (@process_completed_handler.call(results) || []) : []
        empty_output.merge("reindex_files" => reindex_files)
      when "index.call.enter"
        index_call_output(raw_event["call"] || raw_event[:call])
      when "request.document_symbol"
        {
          "index_patches" => [],
          "response_patches" => document_symbols(raw_event["document"] || raw_event[:document]),
          "command_patches" => [],
          "process_requests" => []
        }
      when "request.code_lens"
        {
          "index_patches" => [],
          "response_patches" => code_lens(raw_event["document"] || raw_event[:document]),
          "command_patches" => [],
          "process_requests" => []
        }
      else
        {
          "index_patches" => [],
          "response_patches" => [],
          "command_patches" => [],
          "process_requests" => []
        }
      end
    end

    def empty_output
      {
        "index_patches" => [],
        "execution_contexts" => [],
        "response_patches" => [],
        "command_patches" => [],
        "process_requests" => [],
        "reindex_files" => []
      }
    end

    def index_output(index_patches: [], execution_contexts: [])
      empty_output.merge(
        "index_patches" => index_patches,
        "execution_contexts" => execution_contexts
      )
    end

    def normalize_index_output(output)
      return empty_output if output.nil?
      return index_output(index_patches: output) if output.is_a?(Array)

      required = ["index_patches", "execution_contexts"]
      missing = required.reject { |key| output.key?(key) }
      raise "invalid index output; missing #{missing.join(', ')}" unless missing.empty?

      empty_output.merge(output)
    end

    def block_execution_context(call_range:, block_range:, generated_owners:, implicit_receiver:, method_definition_owner:, source:, lexical_scope: :preserve, local_scope: :preserve)
      {
        "call_range" => call_range,
        "block_range" => block_range,
        "generated_owners" => generated_owners,
        "implicit_receiver" => implicit_receiver,
        "method_definition_owner" => method_definition_owner,
        "lexical_scope" => camel(lexical_scope),
        "local_scope" => camel(local_scope),
        "source" => source
      }
    end

    def generated_owner(local_id:, declaration_kind: :class, owner_kind: :instance, parent: nil, scope: :source)
      owner = {
        "local_id" => local_id.to_s,
        "declaration_kind" => camel(declaration_kind),
        "owner_kind" => camel(owner_kind),
        "parent" => parent
      }
      owner["scope"] = camel(scope) unless scope.to_s == "source"
      owner
    end

    def generated_owner_target(local_id, owner_kind: nil)
      target = {"local_id" => local_id.to_s}
      target["owner_kind"] = camel(owner_kind) if owner_kind
      {"GeneratedOwner" => target}
    end

    def project_generated_owner_target(local_id, owner_kind: nil)
      target = {"local_id" => local_id.to_s}
      target["owner_kind"] = camel(owner_kind) if owner_kind
      {"ProjectGeneratedOwner" => target}
    end

    def namespace_execution_target(namespace:, owner_kind: :instance)
      {
        "Namespace" => {
          "namespace" => namespace,
          "owner_kind" => camel(owner_kind)
        }
      }
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

    def define_method(name:, namespace:, owner_kind:, location:, source:, visibility: :public, return_type: nil, return_type_source: nil, params: [], owner_target: nil)
      method = {
        "name" => name.to_s,
        "namespace" => namespace,
        "owner_target" => owner_target,
        "owner_kind" => camel(owner_kind),
        "visibility" => camel(visibility),
        "location" => location,
        "params" => params.map { |param| method_param(param) },
        "return_type" => return_type,
        "source" => source
      }
      method["return_type_source"] = camel(return_type_source) if return_type_source
      {
        "DefineMethod" => {
          **method
        }
      }
    end

    def define_namespace(namespace:, kind:, location:, source:)
      {
        "DefineNamespace" => {
          "namespace" => namespace,
          "kind" => camel(kind),
          "location" => location,
          "source" => source
        }
      }
    end

    def define_constant(name:, namespace:, location:, source:, ruby_type: nil)
      {
        "DefineConstant" => {
          "namespace" => namespace,
          "name" => name.to_s,
          "location" => location,
          "ruby_type" => ruby_type,
          "source" => source
        }
      }
    end

    def named_type(name)
      {"Named" => name.to_s}
    end

    def unknown_type
      "Unknown"
    end

    def array_type(*element_types)
      {"Array" => element_types}
    end

    def hash_type(keys:, values:)
      {"Hash" => {"keys" => keys, "values" => values}}
    end

    def union_type(*types)
      {"Union" => types}
    end

    def nilable_type(ruby_type)
      union_type(ruby_type, named_type("NilClass"))
    end

    def add_reference(target:, location:, source:)
      {
        "AddReference" => {
          "target" => target,
          "location" => location,
          "source" => source
        }
      }
    end

    def namespace_reference_target(namespace)
      {"Namespace" => namespace}
    end

    def constant_reference_target(name:, namespace: [])
      {"Constant" => {"namespace" => namespace, "name" => name.to_s}}
    end

    def method_reference_target(name:, namespace:, owner_kind: :instance)
      {
        "Method" => {
          "namespace" => namespace,
          "owner_kind" => camel(owner_kind),
          "name" => name.to_s
        }
      }
    end

    def method_param(param)
      {
        "name" => (param[:name] || param["name"]).to_s,
        "kind" => camel(param[:kind] || param["kind"])
      }
    end

    def apply_mixin(namespace:, kind:, location:, source:, mixin: nil, mixin_target: nil, absolute: false, target_kind: :instance, owner_target: nil)
      if (!!mixin) == (!!mixin_target)
        raise "apply_mixin requires exactly one of mixin or mixin_target"
      end
      patch = {
        "namespace" => namespace,
        "owner_target" => owner_target,
        "target_kind" => camel(target_kind),
        "mixin" => mixin || [],
        "absolute" => absolute,
        "kind" => camel(kind),
        "location" => location,
        "source" => source
      }
      patch["mixin_target"] = mixin_target if mixin_target
      {"ApplyMixin" => patch}
    end

    def connect_execution_context(template:, application:, location:, source:)
      {
        "ConnectExecutionContext" => {
          "template" => template,
          "application" => application,
          "location" => location,
          "source" => source
        }
      }
    end

    def set_superclass(namespace:, superclass:, location:, source:, absolute: false)
      {
        "SetSuperclass" => {
          "namespace" => namespace,
          "superclass" => superclass,
          "absolute" => absolute,
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

    def keyword_argument(name)
      arguments.find { |argument| argument.keyword_name == name }
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

    def block_range
      fetch("block_range")
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

    def keyword
      @raw["keyword"] || @raw[:keyword]
    end

    def keyword_name
      return nil unless keyword

      keyword["name"] || keyword[:name]
    end

    def keyword_range
      return nil unless keyword

      keyword["range"] || keyword[:range]
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

    def boolean?
      value.is_a?(Hash) && value.key?("Boolean")
    end

    def boolean_value
      return value["Boolean"] if boolean?

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
