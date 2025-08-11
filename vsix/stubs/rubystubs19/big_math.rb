# frozen_string_literal: true

# mathematical functions
module BigMath
  # Computes the value of e (the base of natural logarithms) raised to the
  # power of x, to the specified number of digits of precision.
  #
  # If x is infinite, returns Infinity.
  #
  # If x is NaN, returns NaN.
  def self.exp(x, prec) end

  # Computes the natural logarithm of x to the specified number of digits of
  # precision.
  #
  # If x is zero or negative, raises Math::DomainError.
  #
  # If x is positive infinite, returns Infinity.
  #
  # If x is NaN, returns NaN.
  def self.log(x, prec) end
end
