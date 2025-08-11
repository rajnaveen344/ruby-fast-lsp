# frozen_string_literal: true

# Raised when a method is called on a receiver which doesn't have it
# defined and also fails to respond with +method_missing+.
#
#    "hello".to_ary
#
# <em>raises the exception:</em>
#
#    NoMethodError: undefined method `to_ary' for "hello":String
class NoMethodError < NameError
  # Construct a NoMethodError exception for a method of the given name
  # called with the given arguments. The name may be accessed using
  # the <code>#name</code> method on the resulting object, and the
  # arguments using the <code>#args</code> method.
  def initialize(*args) end

  # Return the arguments passed in as the third parameter to
  # the constructor.
  def args; end

  # Return true if the caused method was called as private.
  def private_call?; end
end
