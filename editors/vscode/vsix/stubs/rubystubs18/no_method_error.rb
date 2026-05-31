# frozen_string_literal: true

class NoMethodError < NameError
  # Construct a NoMethodError exception for a method of the given name
  # called with the given arguments. The name may be accessed using
  # the <code>#name</code> method on the resulting object, and the
  # arguments using the <code>#args</code> method.
  def initialize(*args) end

  # Return the arguments passed in as the third parameter to
  # the constructor.
  def args; end
end
