# frozen_string_literal: true

# Object is the default root of all Ruby objects.  Object inherits from
# BasicObject which allows creating alternate object hierarchies.  Methods
# on object are available to all classes unless explicitly overridden.
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
end
