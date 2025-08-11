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

        # Instantiates a new State object, configured by _opts_.
        #
        # _opts_ can have the following keys:
        #
        # * *indent*: a string used to indent levels (default: ''),
        # * *space*: a string that is put after, a : or , delimiter (default: ''),
        # * *space_before*: a string that is put before a : pair delimiter (default: ''),
        # * *object_nl*: a string that is put at the end of a JSON object (default: ''),
        # * *array_nl*: a string that is put at the end of a JSON array (default: ''),
        # * *allow_nan*: true if NaN, Infinity, and -Infinity should be
        #   generated, otherwise an exception is thrown, if these values are
        #   encountered. This options defaults to false.
        # * *ascii_only*: true if only ASCII characters should be generated. This
        #   option defaults to false.
        # * *buffer_initial_length*: sets the initial length of the generator's
        #   internal buffer.
        def initialize(opts = {}) end

        # Returns the value returned by method +name+.
        def [](p1) end

        # Sets the attribute name to value.
        def []=(p1, p2) end

        # Returns true, if NaN, Infinity, and -Infinity should be generated, otherwise
        # returns false.
        def allow_nan?; end

        # This string is put at the end of a line that holds a JSON array.
        def array_nl; end

        # This string is put at the end of a line that holds a JSON array.
        def array_nl=(array_nl) end

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

        # Configure this State instance with the Hash _opts_, and return
        # itself.
        def configure(opts) end
        alias merge configure

        # This integer returns the current depth of data structure nesting.
        def depth; end

        # This sets the maximum level of data structure nesting in the generated JSON
        # to the integer depth, max_nesting = 0 if no maximum should be checked.
        def depth=(depth) end

        # If this boolean is true, the forward slashes will be escaped in
        # the json output.
        def escape_slash; end
        alias escape_slash? escape_slash

        # This sets whether or not the forward slashes will be escaped in
        # the json output.
        def escape_slash=(depth) end

        # Generates a valid JSON document from object +obj+ and returns the
        # result. If no valid JSON document can be created this method raises a
        # GeneratorError exception.
        def generate(obj) end

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

        # Returns the configuration instance variables as a hash, that can be
        # passed to the configure method.
        def to_h; end
        alias to_hash to_h
      end
    end

    # This is the JSON parser implemented as a C extension. It can be configured
    # to be used by setting
    #
    #  JSON.parser = JSON::Ext::Parser
    #
    # with the method parser= in JSON.
    class Parser
      # Creates a new JSON::Ext::Parser instance for the string _source_.
      #
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
      # * *object_class*: Defaults to Hash
      # * *array_class*: Defaults to Array
      def initialize(p1, p2 = {}) end

      #  Parses the current JSON text _source_ and returns the complete data
      #  structure as a result.
      def parse; end

      # Returns a copy of the current _source_ string, that was used to construct
      # this Parser.
      def source; end
    end
  end
end
