require_relative "../mruby-sdk/ruby_fast_lsp_extension"
require_relative "extension"

module RubyFastLspExtensionEntrypoint
  def self.ruby_fast_lsp_extension
    $ruby_fast_lsp_extensions.fetch("rspec-ruby")
  end

  def self.indexed_call_names_json
    RubyFastLspExtension::Json.generate(self.ruby_fast_lsp_extension.indexed_call_names)
  end

  def self.index_call_json(input_json)
    ctx = RubyFastLspExtension::Json.parse(input_json)
    RubyFastLspExtension::Json.generate(self.ruby_fast_lsp_extension.index_call(ctx))
  end

  def self.handle_event_json(input_json)
    event = RubyFastLspExtension::Json.parse(input_json)
    RubyFastLspExtension::Json.generate(self.ruby_fast_lsp_extension.handle_event(event))
  end
end
