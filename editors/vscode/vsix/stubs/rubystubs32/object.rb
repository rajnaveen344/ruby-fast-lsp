# frozen_string_literal: true

# Object is the default root of all Ruby objects.  Object inherits from
# BasicObject which allows creating alternate object hierarchies.  Methods
# on Object are available to all classes unless explicitly overridden.
#
# Object mixes in the Kernel module, making the built-in kernel functions
# globally accessible.  Although the instance methods of Object are defined
# by the Kernel module, we have chosen to document them here for clarity.
#
# When referencing constants in classes inheriting from Object you do not
# need to use the full namespace.  For example, referencing +File+ inside
# +YourClass+ will find the top-level File class.
#
# In the descriptions of Object's methods, the parameter <i>symbol</i> refers
# to a symbol, which is either a quoted string or a Symbol (such as
# <code>:name</code>).
#
# == What's Here
#
# First, what's elsewhere. \Class \Object:
#
# - Inherits from {class BasicObject}[rdoc-ref:BasicObject@What-27s+Here].
# - Includes {module Kernel}[rdoc-ref:Kernel@What-27s+Here].
#
# Here, class \Object provides methods for:
#
# - {Querying}[rdoc-ref:Object@Querying]
# - {Instance Variables}[rdoc-ref:Object@Instance+Variables]
# - {Other}[rdoc-ref:Object@Other]
#
# === Querying
#
# - #!~: Returns +true+ if +self+ does not match the given object,
#   otherwise +false+.
# - #<=>: Returns 0 if +self+ and the given object +object+ are the same
#   object, or if <tt>self == object</tt>; otherwise returns +nil+.
# - #===: Implements case equality, effectively the same as calling #==.
# - #eql?: Implements hash equality, effectively the same as calling #==.
# - #kind_of? (aliased as #is_a?): Returns whether given argument is an ancestor
#   of the singleton class of +self+.
# - #instance_of?: Returns whether +self+ is an instance of the given class.
# - #instance_variable_defined?: Returns whether the given instance variable
#   is defined in +self+.
# - #method: Returns the Method object for the given method in +self+.
# - #methods: Returns an array of symbol names of public and protected methods
#   in +self+.
# - #nil?: Returns +false+. (Only +nil+ responds +true+ to method <tt>nil?</tt>.)
# - #object_id: Returns an integer corresponding to +self+ that is unique
#   for the current process
# - #private_methods: Returns an array of the symbol names
#   of the private methods in +self+.
# - #protected_methods: Returns an array of the symbol names
#   of the protected methods in +self+.
# - #public_method: Returns the Method object for the given public method in +self+.
# - #public_methods: Returns an array of the symbol names
#   of the public methods in +self+.
# - #respond_to?: Returns whether +self+ responds to the given method.
# - #singleton_class: Returns the singleton class of +self+.
# - #singleton_method: Returns the Method object for the given singleton method
#   in +self+.
# - #singleton_methods: Returns an array of the symbol names
#   of the singleton methods in +self+.
#
# - #define_singleton_method: Defines a singleton method in +self+
#   for the given symbol method-name and block or proc.
# - #extend: Includes the given modules in the singleton class of +self+.
# - #public_send: Calls the given public method in +self+ with the given argument.
# - #send: Calls the given method in +self+ with the given argument.
#
# === Instance Variables
#
# - #instance_variable_get: Returns the value of the given instance variable
#   in +self+, or +nil+ if the instance variable is not set.
# - #instance_variable_set: Sets the value of the given instance variable in +self+
#   to the given object.
# - #instance_variables: Returns an array of the symbol names
#   of the instance variables in +self+.
# - #remove_instance_variable: Removes the named instance variable from +self+.
#
# === Other
#
# - #clone:  Returns a shallow copy of +self+, including singleton class
#   and frozen state.
# - #define_singleton_method: Defines a singleton method in +self+
#   for the given symbol method-name and block or proc.
# - #display: Prints +self+ to the given \IO stream or <tt>$stdout</tt>.
# - #dup: Returns a shallow unfrozen copy of +self+.
# - #enum_for (aliased as #to_enum): Returns an Enumerator for +self+
#   using the using the given method, arguments, and block.
# - #extend: Includes the given modules in the singleton class of +self+.
# - #freeze: Prevents further modifications to +self+.
# - #hash: Returns the integer hash value for +self+.
# - #inspect: Returns a human-readable  string representation of +self+.
# - #itself: Returns +self+.
# - #method_missing: Method called when an undefined method is called on +self+.
# - #public_send: Calls the given public method in +self+ with the given argument.
# - #send: Calls the given method in +self+ with the given argument.
# - #to_s: Returns a string representation of +self+.
class Object < BasicObject
  include Kernel

  # ARGF is a stream designed for use in scripts that process files given
  # as command-line arguments or passed in via STDIN.
  #
  # See ARGF (the class) for more details.
  ARGF = _
  # ARGV contains the command line arguments used to run ruby.
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
  # The copyright string for ruby
  RUBY_COPYRIGHT = _
  # The full ruby version string, like <tt>ruby -v</tt> prints
  RUBY_DESCRIPTION = _
  # The engine or interpreter this ruby uses.
  RUBY_ENGINE = _
  # The version of the engine or interpreter this ruby uses.
  RUBY_ENGINE_VERSION = _
  # The patchlevel for this ruby.  If this is a development build of ruby
  # the patchlevel will be -1
  RUBY_PATCHLEVEL = _
  # The platform for this ruby
  RUBY_PLATFORM = _
  # The date this ruby was released
  RUBY_RELEASE_DATE = _
  # The GIT commit hash for this ruby.
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
  WIN32OLE_EVENT = _
  WIN32OLE_METHOD = _
  WIN32OLE_PARAM = _
  WIN32OLE_RECORD = _
  WIN32OLE_TYPE = _
  WIN32OLE_TYPELIB = _
  WIN32OLE_VARIABLE = _
  WIN32OLE_VARIANT = _
end
