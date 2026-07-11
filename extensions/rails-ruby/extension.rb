# frozen_string_literal: true

require "ruby_fast_lsp_extension"

module RailsRuby
  CALLBACKS = [
    "before_validation", "after_validation",
    "before_save", "around_save", "after_save",
    "before_create", "around_create", "after_create",
    "before_update", "around_update", "after_update",
    "before_destroy", "around_destroy", "after_destroy",
    "before_commit", "after_commit", "after_rollback",
    "after_create_commit", "after_update_commit", "after_destroy_commit", "after_save_commit",
    "after_initialize", "after_find", "after_touch"
  ].freeze

  VALIDATIONS = [
    "validate", "validates", "validates_associated",
    "validates_absence_of", "validates_acceptance_of", "validates_confirmation_of",
    "validates_exclusion_of", "validates_format_of", "validates_inclusion_of",
    "validates_length_of", "validates_numericality_of", "validates_presence_of",
    "validates_size_of", "validates_uniqueness_of", "validates_comparison_of"
  ].freeze

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


  (RailsRuby::CALLBACKS + RailsRuby::VALIDATIONS).each do |macro|
    on_call macro do |ctx|
      next [] if ctx.current_namespace.empty?

      ctx.arguments.each_with_object([]) do |argument, patches|
        next if argument.keyword_name

        name = argument.symbol_or_string
        next unless name

        patches << add_reference(
          target: method_reference_target(
            name: name,
            namespace: ctx.current_namespace,
            owner_kind: :instance
          ),
          location: argument.range,
          source: macro_source(ctx)
        )
      end
    end
  end
end
