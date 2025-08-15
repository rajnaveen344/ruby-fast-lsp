# frozen_string_literal: true

# Raised when a given numerical value is out of range.
#
#    [1, 2, 3].drop(1 << 100)
#
# <em>raises the exception:</em>
#
#    RangeError: bignum too big to convert into `long'
class RangeError < StandardError
end
