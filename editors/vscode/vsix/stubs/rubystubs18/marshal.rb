# frozen_string_literal: true

# The marshaling library converts collections of Ruby objects into a
# byte stream, allowing them to be stored outside the currently
# active script. This data may subsequently be read and the original
# objects reconstituted.
# Marshaled data has major and minor version numbers stored along
# with the object information. In normal use, marshaling can only
# load data written with the same major version number and an equal
# or lower minor version number. If Ruby's ``verbose'' flag is set
# (normally using -d, -v, -w, or --verbose) the major and minor
# numbers must match exactly. Marshal versioning is independent of
# Ruby's version numbers. You can extract the version by reading the
# first two bytes of marshaled data.
#
#     str = Marshal.dump("thing")
#     RUBY_VERSION   #=> "1.8.0"
#     str[0]         #=> 4
#     str[1]         #=> 8
#
# Some objects cannot be dumped: if the objects to be dumped include
# bindings, procedure or method objects, instances of class IO, or
# singleton objects, a TypeError will be raised.
# If your class has special serialization needs (for example, if you
# want to serialize in some specific format), or if it contains
# objects that would otherwise not be serializable, you can implement
# your own serialization strategy by defining two methods, _dump and
# _load:
# The instance method _dump should return a String object containing
# all the information necessary to reconstitute objects of this class
# and all referenced objects up to a maximum depth given as an integer
# parameter (a value of -1 implies that you should disable depth checking).
# The class method _load should take a String and return an object of this class.
module Marshal
  MAJOR_VERSION = _
  MINOR_VERSION = _

  # Serializes obj and all descendent objects. If anIO is
  # specified, the serialized data will be written to it, otherwise the
  # data will be returned as a String. If limit is specified, the
  # traversal of subobjects will be limited to that depth. If limit is
  # negative, no checking of depth will be performed.
  #
  #     class Klass
  #       def initialize(str)
  #         @str = str
  #       end
  #       def sayHello
  #         @str
  #       end
  #     end
  #
  # (produces no output)
  #
  #     o = Klass.new("hello\n")
  #     data = Marshal.dump(o)
  #     obj = Marshal.load(data)
  #     obj.sayHello   #=> "hello\n"
  def self.dump(obj, port = _, limit = _) end

  # Returns the result of converting the serialized data in source into a
  # Ruby object (possibly with associated subordinate objects). source
  # may be either an instance of IO or an object that responds to
  # to_str. If proc is specified, it will be passed each object as it
  # is deserialized.
  def self.load(source, *proc) end

  # Returns the result of converting the serialized data in source into a
  # Ruby object (possibly with associated subordinate objects). source
  # may be either an instance of IO or an object that responds to
  # to_str. If proc is specified, it will be passed each object as it
  # is deserialized.
  def self.restore(source, *proc) end

  private

  # Serializes obj and all descendent objects. If anIO is
  # specified, the serialized data will be written to it, otherwise the
  # data will be returned as a String. If limit is specified, the
  # traversal of subobjects will be limited to that depth. If limit is
  # negative, no checking of depth will be performed.
  #
  #     class Klass
  #       def initialize(str)
  #         @str = str
  #       end
  #       def sayHello
  #         @str
  #       end
  #     end
  #
  # (produces no output)
  #
  #     o = Klass.new("hello\n")
  #     data = Marshal.dump(o)
  #     obj = Marshal.load(data)
  #     obj.sayHello   #=> "hello\n"
  def dump(p1, p2 = v2, p3 = v3) end

  # Returns the result of converting the serialized data in source into a
  # Ruby object (possibly with associated subordinate objects). source
  # may be either an instance of IO or an object that responds to
  # to_str. If proc is specified, it will be passed each object as it
  # is deserialized.
  def load(p1, p2 = v2) end
  alias restore load
end
