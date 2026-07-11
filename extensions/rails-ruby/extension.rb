# frozen_string_literal: true

require "ruby_fast_lsp_extension"

module RailsRuby
  IRREGULAR_SINGULARS = {
    "people" => "person",
    "children" => "child",
    "men" => "man",
    "women" => "woman",
    "mice" => "mouse",
    "geese" => "goose",
    "teeth" => "tooth",
    "feet" => "foot",
    "oxen" => "ox"
  }.freeze
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

  HTTP_ROUTES = ["get", "post", "put", "patch", "delete", "match"].freeze
  ACTIVE_JOB_ENTRY_POINTS = ["perform_later", "perform_now"].freeze

  def self.singularize(name)
    irregular = IRREGULAR_SINGULARS[name]
    return irregular if irregular
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

  def self.method_name?(name)
    return false if name.nil? || name.empty?
    first = name[0]
    return false unless (first >= "a" && first <= "z") || first == "_"

    name.each_char do |character|
      next if character >= "A" && character <= "Z"
      next if character >= "a" && character <= "z"
      next if character >= "0" && character <= "9"
      next if character == "_"

      return false
    end
    true
  end

  def self.controller_namespace(name)
    return nil unless name

    parts = name.split("/")
    return nil if parts.empty? || parts.any? { |part| !method_name?(part) }

    controller = parts.pop
    parts.map { |part| camelize(part) } + ["#{camelize(controller)}Controller"]
  end

  def self.subrange(range, start_offset, length)
    start = range.fetch("start")
    finish = range.fetch("end")
    return nil unless start.fetch("line") == finish.fetch("line")

    {
      "start" => {
        "line" => start.fetch("line"),
        "character" => start.fetch("character") + start_offset
      },
      "end" => {
        "line" => start.fetch("line"),
        "character" => start.fetch("character") + start_offset + length
      }
    }
  end

  def self.route_target(argument)
    value = argument && argument.symbol_or_string
    return nil unless value

    separator = value.index("#")
    return nil unless separator && separator > 0 && separator < value.length - 1

    controller = value[0...separator]
    action = value[(separator + 1)..-1]
    namespace = controller_namespace(controller)
    return nil unless namespace && method_name?(action)

    controller_range = subrange(argument.range, 0, separator)
    action_range = subrange(argument.range, separator + 1, action.length)
    return nil unless controller_range && action_range

    [namespace, action, controller_range, action_range]
  end

  def self.call_value(call, key)
    call[key] || call[key.to_sym]
  end

  def self.constant_receiver(receiver)
    return nil unless receiver

    parts = receiver["Constant"] || receiver[:Constant]
    return nil unless parts.is_a?(Array) && !parts.empty?
    return nil if parts.any? { |part| !constant_name?(part) }
    return nil unless parts.last.end_with?("Job")

    parts
  end

  def self.frame_arguments(call)
    (call_value(call, "arguments") || []).map { |argument| RubyFastLspExtension::Argument.new(argument) }
  end

  def self.route_scope(ctx)
    draw = ctx.enclosing_calls.any? do |call|
      next false unless call_value(call, "method_name") == "draw"

      receiver = call_value(call, "receiver")
      method_call = receiver && (receiver["MethodCall"] || receiver[:MethodCall])
      method_call && (method_call["method_name"] || method_call[:method_name]) == "routes"
    end
    return nil unless draw

    controller_scope = []
    helper_scope = []
    ctx.enclosing_calls.each do |call|
      method_name = call_value(call, "method_name")
      arguments = frame_arguments(call)
      if method_name == "namespace"
        argument = arguments.find { |candidate| !candidate.keyword_name }
        name = argument && argument.symbol_or_string
        return nil unless method_name?(name)

        controller_scope << name
        helper_scope << name
      elsif method_name == "scope"
        module_argument = arguments.find { |candidate| candidate.keyword_name == "module" }
        if module_argument
          modules = module_argument.symbol_or_string.to_s.split("/")
          return nil if modules.empty? || modules.any? { |name| !method_name?(name) }

          controller_scope.concat(modules)
        end
        as_argument = arguments.find { |candidate| candidate.keyword_name == "as" }
        if as_argument
          name = as_argument.symbol_or_string
          return nil unless method_name?(name)

          helper_scope << name
        end
      end
    end
    [controller_scope, helper_scope]
  end

  def self.prefix_helper(namespaces, helper)
    return helper if namespaces.empty?

    "#{namespaces.join("_")}_#{helper}"
  end
end

extension "rails-ruby" do
  RailsRuby::ACTIVE_JOB_ENTRY_POINTS.each do |entry_point|
    on_call entry_point do |ctx|
      job = RailsRuby.constant_receiver(ctx.receiver)
      next [] unless job

      [add_reference(
        target: method_reference_target(
          name: "perform",
          namespace: job,
          owner_kind: :instance
        ),
        location: ctx.message_range,
        source: macro_source(ctx)
      )]
    end
  end

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


  ["resources", "resource"].each do |macro|
    on_call macro do |ctx|
      route_scope = RailsRuby.route_scope(ctx)
      next [] unless route_scope
      controller_scope, helper_scope = route_scope

      argument = ctx.arguments.find { |candidate| !candidate.keyword_name }
      name = argument && argument.symbol_or_string
      next [] unless name && RailsRuby.method_name?(name)

      source = macro_source(ctx)
      controller_argument = ctx.keyword_argument("controller")
      controller_name = if controller_argument
        controller_argument.symbol_or_string
      elsif macro == "resources"
        name
      else
        "#{name}s"
      end
      controller = RailsRuby.controller_namespace(controller_name)
      controller = controller_scope.map { |part| RailsRuby.camelize(part) } + controller if controller
      patches = []
      if controller
        patches << add_reference(
          target: namespace_reference_target(controller),
          location: controller_argument ? controller_argument.range : argument.range,
          source: source
        )
      end

      next patches if ctx.keyword_argument("only") || ctx.keyword_argument("except")

      explicit_name = ctx.keyword_argument("as")
      helper_base = explicit_name ? explicit_name.symbol_or_string : name
      next patches unless RailsRuby.method_name?(helper_base)

      helper_location = explicit_name ? explicit_name.range : argument.range
      helper_names = if macro == "resources"
        member = RailsRuby.singularize(helper_base)
        collection = RailsRuby.prefix_helper(helper_scope, helper_base)
        member = RailsRuby.prefix_helper(helper_scope, member)
        [collection, "new_#{member}", "edit_#{member}", member]
      else
        helper_base = RailsRuby.prefix_helper(helper_scope, helper_base)
        ["new_#{helper_base}", "edit_#{helper_base}", helper_base]
      end
      helper_names.each do |helper|
        ["path", "url"].each do |suffix|
          patches << define_method(
            name: "#{helper}_#{suffix}",
            namespace: ["ApplicationController"],
            owner_kind: :instance,
            visibility: :public,
            location: helper_location,
            params: [
              {name: "args", kind: :rest},
              {name: "kwargs", kind: :keyword_rest}
            ],
            return_type: named_type("String"),
            source: source
          )
        end
      end
      patches
    end
  end


  (RailsRuby::HTTP_ROUTES + ["root"]).each do |macro|
    on_call macro do |ctx|
      route_scope = RailsRuby.route_scope(ctx)
      next [] unless route_scope
      controller_scope, helper_scope = route_scope

      target_argument = ctx.keyword_argument("to")
      target_argument ||= ctx.arguments.find { |argument|
        !argument.keyword_name && argument.symbol_or_string.to_s.include?("#")
      }
      target = RailsRuby.route_target(target_argument)
      source = macro_source(ctx)
      patches = []
      if target
        controller, action, controller_range, action_range = target
        controller = controller_scope.map { |part| RailsRuby.camelize(part) } + controller
        patches << add_reference(
          target: namespace_reference_target(controller),
          location: controller_range,
          source: source
        )
        patches << add_reference(
          target: method_reference_target(
            name: action,
            namespace: controller,
            owner_kind: :instance
          ),
          location: action_range,
          source: source
        )
      end

      explicit_name = ctx.keyword_argument("as")
      helper_base = if explicit_name
        explicit_name.symbol_or_string
      elsif macro == "root"
        "root"
      end
      next patches unless RailsRuby.method_name?(helper_base)
      helper_base = RailsRuby.prefix_helper(helper_scope, helper_base)

      helper_location = explicit_name ? explicit_name.range : ctx.message_range
      ["path", "url"].each do |suffix|
        patches << define_method(
          name: "#{helper_base}_#{suffix}",
          namespace: ["ApplicationController"],
          owner_kind: :instance,
          visibility: :public,
          location: helper_location,
          params: [
            {name: "args", kind: :rest},
            {name: "kwargs", kind: :keyword_rest}
          ],
          return_type: named_type("String"),
          source: source
        )
      end
      patches
    end
  end
end
