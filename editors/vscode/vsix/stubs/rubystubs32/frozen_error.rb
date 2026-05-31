# frozen_string_literal: true

# Raised when there is an attempt to modify a frozen object.
#
#    [1, 2, 3].freeze << 4
#
# <em>raises the exception:</em>
#
#    FrozenError: can't modify frozen Array
class FrozenError < RuntimeError
  # Construct a new FrozenError exception. If given the <i>receiver</i>
  # parameter may subsequently be examined using the FrozenError#receiver
  # method.
  #
  #    a = [].freeze
  #    raise FrozenError.new("can't modify frozen array", receiver: a)
  def initialize(msg = nil, receiver: nil) end

  # Return the receiver associated with this FrozenError exception.
  def receiver; end
end
