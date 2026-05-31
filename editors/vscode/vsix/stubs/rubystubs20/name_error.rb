# frozen_string_literal: true

# Raised when a given name is invalid or undefined.
#
#    puts foo
#
# <em>raises the exception:</em>
#
#    NameError: undefined local variable or method `foo' for main:Object
#
# Since constant names must start with a capital:
#
#    Fixnum.const_set :answer, 42
#
# <em>raises the exception:</em>
#
#    NameError: wrong constant name answer
class NameError < StandardError
  # Construct a new NameError exception. If given the <i>name</i>
  # parameter may subsequently be examined using the <code>NameError.name</code>
  # method.
  def initialize(*args) end

  # Return the name associated with this NameError exception.
  def name; end

  # Produce a nicely-formatted string representing the +NameError+.
  def to_s; end
end
