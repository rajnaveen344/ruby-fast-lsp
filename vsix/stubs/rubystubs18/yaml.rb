# frozen_string_literal: true

module YAML
  # Define YAML::DomainType class
  class DomainType < rb_cObject
    # YAML::DomainType.initialize
    def initialize(p1, p2, p3) end
  end

  # Define YAML::Object class
  class Object < rb_cObject
    # YAML::Object.initialize
    def initialize(p1, p2) end

    # YAML::Object.initialize
    def yaml_initialize(p1, p2) end
  end

  # Define YAML::PrivateType class
  class PrivateType < rb_cObject
    # YAML::PrivateType.initialize
    def initialize(p1, p2) end
  end

  module Syck
    DefaultResolver = _
    GenericResolver = _
    VERSION = _

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
      # Cloning method for all node types
      def initialize_copy(p1) end

      # YAML::Syck::Node.transform
      def transform; end

      # YAML::Syck::Node#type_id=
      def type_id=(p1) end
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
