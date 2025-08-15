# frozen_string_literal: true

module JSON
  module Ext
    # This is the JSON generator implemented as a C extension. It can be
    # configured to be used by setting
    #
    #  JSON.generator = JSON::Ext::Generator
    #
    # with the method generator= in JSON.
    module Generator
      class State
        # Creates a State object from _opts_, which ought to be Hash to create a
        # new State instance configured by _opts_, something else to create an
        # unconfigured instance. If _opts_ is a State object, it is just returned.
        def self.from_state(opts) end

        def self.generate(p1, p2, p3) end

        def initialize(*args) end

        # This sets whether or not to serialize NaN, Infinity, and -Infinity
        def allow_nan=(enable) end

        # Returns true, if NaN, Infinity, and -Infinity should be generated, otherwise
        # returns false.
        def allow_nan?; end

        # This string is put at the end of a line that holds a JSON array.
        def array_nl; end

        # This string is put at the end of a line that holds a JSON array.
        def array_nl=(array_nl) end

        # This sets whether only ASCII characters should be generated.
        def ascii_only=(enable) end

        # Returns true, if only ASCII characters should be generated. Otherwise
        # returns false.
        def ascii_only?; end

        # This integer returns the current initial length of the buffer.
        def buffer_initial_length; end

        # This sets the initial length of the buffer to +length+, if +length+ > 0,
        # otherwise its value isn't changed.
        def buffer_initial_length=(length) end

        # Returns true, if circular data structures should be checked,
        # otherwise returns false.
        def check_circular?; end

        # This integer returns the current depth of data structure nesting.
        def depth; end

        # This sets the maximum level of data structure nesting in the generated JSON
        # to the integer depth, max_nesting = 0 if no maximum should be checked.
        def depth=(depth) end

        # Returns the string that is used to indent levels in the JSON text.
        def indent; end

        # Sets the string that is used to indent levels in the JSON text.
        def indent=(indent) end

        # Initializes this object from orig if it can be duplicated/cloned and returns
        # it.
        def initialize_copy(orig) end

        # This integer returns the maximum level of data structure nesting in
        # the generated JSON, max_nesting = 0 if no maximum is checked.
        def max_nesting; end

        # This sets the maximum level of data structure nesting in the generated JSON
        # to the integer depth, max_nesting = 0 if no maximum should be checked.
        def max_nesting=(depth) end

        # This string is put at the end of a line that holds a JSON object (or
        # Hash).
        def object_nl; end

        # This string is put at the end of a line that holds a JSON object (or
        # Hash).
        def object_nl=(object_nl) end

        # If this boolean is true, the forward slashes will be escaped in
        # the json output.
        def script_safe; end
        alias script_safe? script_safe
        alias escape_slash script_safe

        # This sets whether or not the forward slashes will be escaped in
        # the json output.
        def script_safe=(enable) end
        alias escape_slash= script_safe=

        # Returns the string that is used to insert a space between the tokens in a JSON
        # string.
        def space; end

        # Sets _space_ to the string that is used to insert a space between the tokens in a JSON
        # string.
        def space=(space) end

        # Returns the string that is used to insert a space before the ':' in JSON objects.
        def space_before; end

        # Sets the string that is used to insert a space before the ':' in JSON objects.
        def space_before=(space_before) end

        # If this boolean is false, types unsupported by the JSON format will
        # be serialized as strings.
        # If this boolean is true, types unsupported by the JSON format will
        # raise a JSON::GeneratorError.
        def strict; end
        alias strict? strict

        # This sets whether or not to serialize types unsupported by the
        # JSON format as strings.
        # If this boolean is false, types unsupported by the JSON format will
        # be serialized as strings.
        # If this boolean is true, types unsupported by the JSON format will
        # raise a JSON::GeneratorError.
        def strict=(enable) end

        private

        def _configure(p1) end

        def _generate(p1, p2) end
      end
    end

    # This is the JSON parser implemented as a C extension. It can be configured
    # to be used by setting
    #
    #  JSON.parser = JSON::Ext::Parser
    #
    # with the method parser= in JSON.
    class Parser
      def self.parse(p1, p2) end

      # Creates a new JSON::Ext::Parser instance for the string _source_.
      #
      # It will be configured by the _opts_ hash. _opts_ can have the following
      # keys:
      #
      # _opts_ can have the following keys:
      # * *max_nesting*: The maximum depth of nesting allowed in the parsed data
      #   structures. Disable depth checking with :max_nesting => false|nil|0, it
      #   defaults to 100.
      # * *allow_nan*: If set to true, allow NaN, Infinity and -Infinity in
      #   defiance of RFC 4627 to be parsed by the Parser. This option defaults to
      #   false.
      # * *symbolize_names*: If set to true, returns symbols for the names
      #   (keys) in a JSON object. Otherwise strings are returned, which is
      #   also the default. It's not possible to use this option in
      #   conjunction with the *create_additions* option.
      # * *create_additions*: If set to false, the Parser doesn't create
      #   additions even if a matching class and create_id was found. This option
      #   defaults to false.
      # * *object_class*: Defaults to Hash. If another type is provided, it will be used
      #   instead of Hash to represent JSON objects. The type must respond to
      #   +new+ without arguments, and return an object that respond to +[]=+.
      # * *array_class*: Defaults to Array If another type is provided, it will be used
      #   instead of Hash to represent JSON arrays. The type must respond to
      #   +new+ without arguments, and return an object that respond to +<<+.
      # * *decimal_class*: Specifies which class to use instead of the default
      #    (Float) when parsing decimal numbers. This class must accept a single
      #    string argument in its constructor.
      def initialize(*args) end

      #  Parses the current JSON text _source_ and returns the complete data
      #  structure as a result.
      #  It raises JSON::ParserError if fail to parse.
      def parse; end

      # Returns a copy of the current _source_ string, that was used to construct
      # this Parser.
      def source; end
    end
  end
end
