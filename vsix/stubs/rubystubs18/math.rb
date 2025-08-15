# frozen_string_literal: true

# The <code>Math</code> module contains module functions for basic
# trigonometric and transcendental functions. See class
# <code>Float</code> for a list of constants that
# define Ruby's floating point accuracy.
module Math
  E = _
  PI = _

  # Computes the arc cosine of <i>x</i>. Returns 0..PI.
  def self.acos(x) end

  # Computes the inverse hyperbolic cosine of <i>x</i>.
  def self.acosh(x) end

  # Computes the arc sine of <i>x</i>. Returns -{PI/2} .. {PI/2}.
  def self.asin(x) end

  # Computes the inverse hyperbolic sine of <i>x</i>.
  def self.asinh(x) end

  # Computes the arc tangent of <i>x</i>. Returns -{PI/2} .. {PI/2}.
  def self.atan(x) end

  # Computes the arc tangent given <i>y</i> and <i>x</i>. Returns
  # -PI..PI.
  def self.atan2(y, x) end

  # Computes the inverse hyperbolic tangent of <i>x</i>.
  def self.atanh(x) end

  # Computes the cosine of <i>x</i> (expressed in radians). Returns
  # -1..1.
  def self.cos(x) end

  # Computes the hyperbolic cosine of <i>x</i> (expressed in radians).
  def self.cosh(x) end

  #  Calculates the error function of x.
  def self.erf(x) end

  #  Calculates the complementary error function of x.
  def self.erfc(x) end

  # Returns e**x.
  def self.exp(x) end

  # Returns a two-element array containing the normalized fraction (a
  # <code>Float</code>) and exponent (a <code>Fixnum</code>) of
  # <i>numeric</i>.
  #
  #    fraction, exponent = Math.frexp(1234)   #=> [0.6025390625, 11]
  #    fraction * 2**exponent                  #=> 1234.0
  def self.frexp(numeric) end

  # Returns sqrt(x**2 + y**2), the hypotenuse of a right-angled triangle
  # with sides <i>x</i> and <i>y</i>.
  #
  #    Math.hypot(3, 4)   #=> 5.0
  def self.hypot(x, y) end

  # Returns the value of <i>flt</i>*(2**<i>int</i>).
  #
  #    fraction, exponent = Math.frexp(1234)
  #    Math.ldexp(fraction, exponent)   #=> 1234.0
  def self.ldexp(flt, int) end

  # Returns the natural logarithm of <i>numeric</i>.
  def self.log(numeric) end

  # Returns the base 10 logarithm of <i>numeric</i>.
  def self.log10(numeric) end

  # Computes the sine of <i>x</i> (expressed in radians). Returns
  # -1..1.
  def self.sin(x) end

  # Computes the hyperbolic sine of <i>x</i> (expressed in
  # radians).
  def self.sinh(x) end

  # Returns the non-negative square root of <i>numeric</i>.
  def self.sqrt(numeric) end

  # Returns the tangent of <i>x</i> (expressed in radians).
  def self.tan(x) end

  # Computes the hyperbolic tangent of <i>x</i> (expressed in
  # radians).
  def self.tanh; end

  private

  # Computes the arc cosine of <i>x</i>. Returns 0..PI.
  def acos(x) end

  # Computes the inverse hyperbolic cosine of <i>x</i>.
  def acosh(x) end

  # Computes the arc sine of <i>x</i>. Returns -{PI/2} .. {PI/2}.
  def asin(x) end

  # Computes the inverse hyperbolic sine of <i>x</i>.
  def asinh(x) end

  # Computes the arc tangent of <i>x</i>. Returns -{PI/2} .. {PI/2}.
  def atan(x) end

  # Computes the arc tangent given <i>y</i> and <i>x</i>. Returns
  # -PI..PI.
  def atan2(y, x) end

  # Computes the inverse hyperbolic tangent of <i>x</i>.
  def atanh(x) end

  # Computes the cosine of <i>x</i> (expressed in radians). Returns
  # -1..1.
  def cos(x) end

  # Computes the hyperbolic cosine of <i>x</i> (expressed in radians).
  def cosh(x) end

  #  Calculates the error function of x.
  def erf(x) end

  #  Calculates the complementary error function of x.
  def erfc(x) end

  # Returns e**x.
  def exp(x) end

  # Returns a two-element array containing the normalized fraction (a
  # <code>Float</code>) and exponent (a <code>Fixnum</code>) of
  # <i>numeric</i>.
  #
  #    fraction, exponent = Math.frexp(1234)   #=> [0.6025390625, 11]
  #    fraction * 2**exponent                  #=> 1234.0
  def frexp(numeric) end

  # Returns sqrt(x**2 + y**2), the hypotenuse of a right-angled triangle
  # with sides <i>x</i> and <i>y</i>.
  #
  #    Math.hypot(3, 4)   #=> 5.0
  def hypot(x, y) end

  # Returns the value of <i>flt</i>*(2**<i>int</i>).
  #
  #    fraction, exponent = Math.frexp(1234)
  #    Math.ldexp(fraction, exponent)   #=> 1234.0
  def ldexp(flt, int) end

  # Returns the natural logarithm of <i>numeric</i>.
  def log(numeric) end

  # Returns the base 10 logarithm of <i>numeric</i>.
  def log10(numeric) end

  # Computes the sine of <i>x</i> (expressed in radians). Returns
  # -1..1.
  def sin(x) end

  # Computes the hyperbolic sine of <i>x</i> (expressed in
  # radians).
  def sinh(x) end

  # Returns the non-negative square root of <i>numeric</i>.
  def sqrt(numeric) end

  # Returns the tangent of <i>x</i> (expressed in radians).
  def tan(x) end

  # Computes the hyperbolic tangent of <i>x</i> (expressed in
  # radians).
  def tanh; end
end
