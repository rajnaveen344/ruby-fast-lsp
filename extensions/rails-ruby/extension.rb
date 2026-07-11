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

  def self.constant_name?(name)
    return false if name.nil? || name.empty?
    first = name[0]
    return false unless first >= "A" && first <= "Z"

    name.each_char do |character|
      next if character >= "A" && character <= "Z"
      next if character >= "a" && character <= "z"
      next if character >= "0" && character <= "9"
      next if character == "_"

      return false
    end
    true
  end

  def self.explicit_target(argument)
    return nil unless argument

    parts = argument.constant_path
    unless parts
      name = argument.symbol_or_string
      return nil unless name

      parts = name.split("::")
      parts.shift if parts.first == ""
    end
    return nil if parts.empty? || parts.any? { |part| !constant_name?(part) }

    parts
  end
end

extension "rails-ruby" do
  ["belongs_to", "has_one", "has_many"].each do |macro|
    on_call macro do |ctx|
      argument = ctx.arguments.first
      name = argument && argument.symbol_or_string
      next [] unless name && !ctx.current_namespace.empty?

      collection = macro == "has_many"
      class_name = ctx.keyword_argument("class_name")
      polymorphic = ctx.keyword_argument("polymorphic")
      target = if polymorphic && polymorphic.boolean_value == true
        nil
      elsif class_name
        RailsRuby.explicit_target(class_name)
      else
        RailsRuby.target_namespace(name, collection)
      end
      target_type = target ? named_type(target.join("::")) : unknown_type
      reader_type = collection ? array_type(target_type) : nilable_type(target_type)
      reader_type = unknown_type if !collection && !target
      source = macro_source(ctx)
      patches = []
      if target
        patches << add_reference(
          target: namespace_reference_target(target),
          location: class_name ? class_name.range : argument.range,
          source: source
        )
      end
      patches << define_method(
          name: name,
          namespace: ctx.current_namespace,
          owner_kind: :instance,
          visibility: :public,
          location: argument.range,
          return_type: reader_type,
          source: source
        )
      patches << define_method(
          name: "#{name}=",
          namespace: ctx.current_namespace,
          owner_kind: :instance,
          visibility: :public,
          location: argument.range,
          params: [{name: "value", kind: :required}],
          return_type: reader_type,
          source: source
        )
      patches
    end
  end
end
