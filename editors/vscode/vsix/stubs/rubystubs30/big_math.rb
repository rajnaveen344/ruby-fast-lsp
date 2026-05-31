# frozen_string_literal: true

module BigMath
  # Computes the value of e (the base of natural logarithms) raised to the
  # power of +decimal+, to the specified number of digits of precision.
  #
  # If +decimal+ is infinity, returns Infinity.
  #
  # If +decimal+ is NaN, returns NaN.
  def self.exp(decimal, numeric) end

  # Computes the natural logarithm of +decimal+ to the specified number of
  # digits of precision, +numeric+.
  #
  # If +decimal+ is zero or negative, raises Math::DomainError.
  #
  # If +decimal+ is positive infinity, returns Infinity.
  #
  # If +decimal+ is NaN, returns NaN.
  def self.log(decimal, numeric) end
end
