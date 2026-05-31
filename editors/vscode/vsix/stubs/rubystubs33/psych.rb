# frozen_string_literal: true

module Psych
  # Returns the version of libyaml being used
  def self.libyaml_version; end

  class ClassLoader
    private

    # Convert +path+ string to a class
    def path2class(path) end
  end

  class Emitter < Handler
    # Create a new Psych::Emitter that writes to +io+.
    def initialize(io, options = Psych::Emitter::OPTIONS) end

    # Emit an alias with +anchor+.
    #
    # See Psych::Handler#alias
    def alias(anchor) end

    # Get the output style, canonical or not.
    def canonical; end

    # Set the output style to canonical, or not.
    def canonical=(p1) end

    # End a document emission with an +implicit+ ending.
    #
    # See Psych::Handler#end_document
    def end_document(implicit) end

    # Emit the end of a mapping.
    #
    # See Psych::Handler#end_mapping
    def end_mapping; end

    # End sequence emission.
    #
    # See Psych::Handler#end_sequence
    def end_sequence; end

    # End a stream emission
    #
    # See Psych::Handler#end_stream
    def end_stream; end

    # Get the indentation level.
    def indentation; end

    # Set the indentation level to +level+.  The level must be less than 10 and
    # greater than 1.
    def indentation=(level) end

    # Get the preferred line width.
    def line_width; end

    # Set the preferred line with to +width+.
    def line_width=(width) end

    # Emit a scalar with +value+, +anchor+, +tag+, and a +plain+ or +quoted+
    # string type with +style+.
    #
    # See Psych::Handler#scalar
    def scalar(value, anchor, tag, plain, quoted, style) end

    # Start a document emission with YAML +version+, +tags+, and an +implicit+
    # start.
    #
    # See Psych::Handler#start_document
    def start_document(version, tags, implicit) end

    # Start emitting a YAML map with +anchor+, +tag+, an +implicit+ start
    # and end, and +style+.
    #
    # See Psych::Handler#start_mapping
    def start_mapping(anchor, tag, implicit, style) end

    # Start emitting a sequence with +anchor+, a +tag+, +implicit+ sequence
    # start and end, along with +style+.
    #
    # See Psych::Handler#start_sequence
    def start_sequence(anchor, tag, implicit, style) end

    # Start a stream emission with +encoding+
    #
    # See Psych::Handler#start_stream
    def start_stream(encoding) end
  end

  class Handler
  end

  class Parser
    # Let the parser choose the encoding
    ANY = _
    # UTF-16-BE Encoding with BOM
    UTF16BE = _
    # UTF-16-LE Encoding with BOM
    UTF16LE = _
    # UTF-8 Encoding
    UTF8 = _

    # Returns a Psych::Parser::Mark object that contains line, column, and index
    # information.
    def mark; end

    private

    def _native_parse(p1, p2, p3) end
  end

  module Visitors
    class ToRuby < Visitor
      private

      # Create an exception with class +klass+ and +message+
      def build_exception(klass, message) end
    end

    class Visitor
    end

    class YAMLTree < Visitor
    end
  end
end
