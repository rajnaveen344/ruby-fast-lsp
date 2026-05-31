# frozen_string_literal: true

# Raised when there is an attempt to modify a frozen object.
#
#    [1, 2, 3].freeze << 4
#
# <em>raises the exception:</em>
#
#    FrozenError: can't modify frozen Array
class FrozenError < RuntimeError
end
