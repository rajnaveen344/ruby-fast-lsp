# frozen_string_literal: true

require "ruby_fast_lsp_extension"

module RailsRuby
  def self.singularize(name)
    return name[0...-3] + "y" if name.end_with?("ies") && name.length > 3
    return name[0...-2] if name.end_with?("ses") && name.length > 3
    return name[0...-1] if name.end_with?("s") && !name.end_with?("ss")

    name
  end

  def self.camelize(name)
    result = String.new
    uppercase = true
    name.each_char do |character|
      if character == "_"
        uppercase = true
      else
        result << (uppercase ? character.upcase : character)
        uppercase = false
      end
    end
    result
  end

  def self.target_namespace(name, collection)
    base = collection ? singularize(name) : name
    [camelize(base)]
  end
end

extension "rails-ruby" do
  ["belongs_to", "has_one", "has_many"].each do |macro|
    on_call macro do |ctx|
      argument = ctx.arguments.first
      name = argument && argument.symbol_or_string
      next [] unless name && !ctx.current_namespace.empty?

      collection = macro == "has_many"
      target = RailsRuby.target_namespace(name, collection)
      target_type = named_type(target.join("::"))
      reader_type = collection ? array_type(target_type) : nilable_type(target_type)
      source = macro_source(ctx)
      [
        add_reference(
          target: namespace_reference_target(target),
          location: argument.range,
          source: source
        ),
        define_method(
          name: name,
          namespace: ctx.current_namespace,
          owner_kind: :instance,
          visibility: :public,
          location: argument.range,
          return_type: reader_type,
          source: source
        ),
        define_method(
          name: "#{name}=",
          namespace: ctx.current_namespace,
          owner_kind: :instance,
          visibility: :public,
          location: argument.range,
          params: [{name: "value", kind: :required}],
          return_type: reader_type,
          source: source
        )
      ]
    end
  end
end
