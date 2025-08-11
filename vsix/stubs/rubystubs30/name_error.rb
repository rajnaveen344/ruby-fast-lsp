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
#    Integer.const_set :answer, 42
#
# <em>raises the exception:</em>
#
#    NameError: wrong constant name answer
class NameError < StandardError
  # Construct a new NameError exception. If given the <i>name</i>
  # parameter may subsequently be examined using the NameError#name
  # method. <i>receiver</i> parameter allows to pass object in
  # context of which the error happened. Example:
  #
  #    [1, 2, 3].method(:rject) # NameError with name "rject" and receiver: Array
  #    [1, 2, 3].singleton_method(:rject) # NameError with name "rject" and receiver: [1, 2, 3]
  def initialize(msg = nil, name = nil, receiver: nil) end

  # Return a list of the local variable names defined where this
  # NameError exception was raised.
  #
  # Internal use only.
  def local_variables; end

  # Return the name associated with this NameError exception.
  def name; end

  # Return the receiver associated with this NameError exception.
  def receiver; end
end
