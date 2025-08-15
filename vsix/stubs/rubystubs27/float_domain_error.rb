# frozen_string_literal: true

# Raised when attempting to convert special float values (in particular
# +Infinity+ or +NaN+) to numerical classes which don't support them.
#
#    Float::INFINITY.to_r   #=> FloatDomainError: Infinity
class FloatDomainError < RangeError
end
