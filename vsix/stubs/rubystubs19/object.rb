# frozen_string_literal: true

# Define YAML::Object class
# ---
# Object is the root of Ruby's class hierarchy.  Its methods are available
# to all classes unless explicitly overridden.
#
# Object mixes in the Kernel module, making the built-in kernel functions
# globally accessible. Although the instance methods of Object are defined
# by the Kernel module, we have chosen to document them here for clarity.
#
# In the descriptions of Object's methods, the parameter <i>symbol</i> refers
# to a symbol, which is either a quoted string or a Symbol (such as
# <code>:name</code>).
class Object < BasicObject
  include Kernel

  # ARGF is a stream designed for use in scripts that process files given
  # as command-line arguments or passed in via STDIN.
  #
  # See ARGF (the class) for more details.
  ARGF = _
  # ARGV contains the command line arguments used to run ruby with the
  # first value containing the name of the executable.
  #
  # A library like OptionParser can be used to process command-line
  # arguments.
  ARGV = _
  # DATA is a File that contains the data section of the executed file.
  # To create a data section use <tt>__END__</tt>:
  #
  #   $ cat t.rb
  #   puts DATA.gets
  #   __END__
  #   hello world!
  #
  #   $ ruby t.rb
  #   hello world!
  DATA = _
  # ENV is a Hash-like accessor for environment variables.
  #
  # See ENV (the class) for more details.
  ENV = _
  # An alias of +false+
  FALSE = _
  # An alias of +nil+
  NIL = _
  # The copyright string for ruby
  RUBY_COPYRIGHT = _
  # The full ruby version string, like <tt>ruby -v</tt> prints'
  RUBY_DESCRIPTION = _
  # The engine or interpreter this ruby uses.
  RUBY_ENGINE = _
  # The patchlevel for this ruby.  If this is a development build of ruby
  # the patchlevel will be -1
  RUBY_PATCHLEVEL = _
  # The platform for this ruby
  RUBY_PLATFORM = _
  # The date this ruby was released
  RUBY_RELEASE_DATE = _
  # The SVN revision for this ruby.
  RUBY_REVISION = _
  # The running version of ruby
  RUBY_VERSION = _
  # When a Hash is assigned to +SCRIPT_LINES__+ the contents of files loaded
  # after the assignment will be added as an Array of lines with the file
  # name as the key.
  SCRIPT_LINES__ = _
  # Holds the original stderr
  STDERR = _
  # Holds the original stdin
  STDIN = _
  # Holds the original stdout
  STDOUT = _
  # The Binding of the top level scope
  TOPLEVEL_BINDING = _
  # An alias of +true+
  TRUE = _

  module Syck
    DefaultResolver = _
    GenericResolver = _

    # Convert YAML to bytecode
    def self.compile(p1) end

    private

    # Convert YAML to bytecode
    def compile(p1) end

    # Define YAML::Syck::BadAlias class
    class BadAlias < rb_cObject
      # YAML::Syck::BadAlias.initialize
      def initialize(p1) end

      # YAML::Syck::BadAlias.<=>
      def <=>(other) end
    end

    # Define YAML::Syck::DefaultKey class
    class DefaultKey < rb_cObject
    end

    # Define YAML::DomainType class
    class DomainType < rb_cObject
      # YAML::DomainType.initialize
      def initialize(p1, p2, p3) end
    end

    # Define YAML::Syck::Emitter class
    class Emitter < rb_cObject
      # YAML::Syck::Emitter.reset( options )
      def initialize(p1 = v1) end

      # YAML::Syck::Emitter.emit( object_id ) { |out| ... }
      def emit(p1, &block) end

      # YAML::Syck::Emitter#node_export
      def node_export(p1) end

      # YAML::Syck::Emitter.reset( options )
      def reset(p1 = v1) end

      # YAML::Syck::Emitter#set_resolver
      def set_resolver(p1) end
    end

    class Map < cNode
      # YAML::Syck::Map.initialize
      def initialize(p1, p2, p3) end

      # YAML::Syck::Map.add
      def add(p1, p2) end

      # YAML::Syck::Map.style=
      def style=(p1) end

      # YAML::Syck::Map.value=
      def value=(p1) end
    end

    # Define YAML::Syck::MergeKey class
    class MergeKey < rb_cObject
    end

    # Define YAML::Syck::Node class
    class Node < rb_cObject
      # YAML::Syck::Node.transform
      def transform; end

      # YAML::Syck::Node#type_id=
      def type_id=(p1) end
    end

    # Define YAML::Object class
    class Object < rb_cObject
      # YAML::Object.initialize
      def initialize(p1, p2) end

      # YAML::Object.initialize
      def yaml_initialize(p1, p2) end
    end

    # Define YAML::Syck::Out classes
    class Out < rb_cObject
      # YAML::Syck::Out::initialize
      def initialize(p1) end

      # YAML::Syck::Out::map
      def map(p1, p2 = v2) end

      #    YAML::Syck::Out::scalar
      # syck_out_scalar( self, type_id, str, style )
      #     VALUE self, type_id, str, style;
      def scalar(p1, p2, p3 = v3) end

      # YAML::Syck::Out::seq
      def seq(p1, p2 = v2) end
    end

    # Define YAML::Syck::Parser class
    class Parser < rb_cObject
      # YAML::Syck::Parser.initialize( resolver, options )
      def initialize(p1 = v1) end

      # YAML::Syck::Parser.bufsize => Integer
      def bufsize; end

      # YAML::Syck::Parser.bufsize = Integer
      def bufsize=(p1) end

      # YAML::Syck::Parser.load( IO or String )
      def load(p1, p2 = v2) end

      # YAML::Syck::Parser.load_documents( IO or String ) { |doc| }
      def load_documents(p1, &block) end

      # YAML::Syck::Parser#set_resolver
      def set_resolver(p1) end
    end

    # Define YAML::PrivateType class
    class PrivateType < rb_cObject
      # YAML::PrivateType.initialize
      def initialize(p1, p2) end
    end

    # Define YAML::Syck::Resolver class
    class Resolver < rb_cObject
      # YAML::Syck::Resolver.initialize
      def initialize; end

      # YAML::Syck::Resolver#add_type
      def add_type(p1, p2) end

      # YAML::Syck::Resolver#detect_implicit
      def detect_implicit(p1) end

      # YAML::Syck::Resolver#node_import
      def node_import(p1) end

      # YAML::Syck::Resolver#tagurize
      def tagurize(p1) end

      # YAML::Syck::Resolver#transfer
      def transfer(p1, p2) end

      # YAML::Syck::Resolver#use_types_at
      def use_types_at(p1) end
    end

    # Define YAML::Syck::Scalar, YAML::Syck::Seq, YAML::Syck::Map --
    #     all are the publicly usable variants of YAML::Syck::Node
    class Scalar < cNode
      # YAML::Syck::Scalar.initialize
      def initialize(p1, p2, p3) end

      # YAML::Syck::Scalar.style=
      def style=(p1) end

      # YAML::Syck::Scalar.value=
      def value=(p1) end
    end

    class Seq < cNode
      # YAML::Syck::Seq.initialize
      def initialize(p1, p2, p3) end

      # YAML::Syck::Seq.add
      def add(p1) end

      # YAML::Syck::Seq.style=
      def style=(p1) end

      # YAML::Syck::Seq.value=
      def value=(p1) end
    end
  end
end
