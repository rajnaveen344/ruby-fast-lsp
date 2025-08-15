# frozen_string_literal: true

# \Module \Math provides methods for basic trigonometric,
# logarithmic, and transcendental functions, and for extracting roots.
#
# You can write its constants and method calls thus:
#
#   Math::PI      # => 3.141592653589793
#   Math::E       # => 2.718281828459045
#   Math.sin(0.0) # => 0.0
#   Math.cos(0.0) # => 1.0
#
# If you include module \Math, you can write simpler forms:
#
#   include Math
#   PI       # => 3.141592653589793
#   E        # => 2.718281828459045
#   sin(0.0) # => 0.0
#   cos(0.0) # => 1.0
#
# For simplicity, the examples here assume:
#
#   include Math
#   INFINITY = Float::INFINITY
#
# The domains and ranges for the methods
# are denoted by open or closed intervals,
# using, respectively, parentheses or square brackets:
#
# - An open interval does not include the endpoints:
#
#     (-INFINITY, INFINITY)
#
# - A closed interval includes the endpoints:
#
#     [-1.0, 1.0]
#
# - A half-open interval includes one endpoint, but not the other:
#
#    [1.0, INFINITY)
#
# Many values returned by \Math methods are numerical approximations.
# This is because many such values are, in mathematics,
# of infinite precision, while in numerical computation
# the precision is finite.
#
# Thus, in mathematics, <i>cos(π/2)</i> is exactly zero,
# but in our computation <tt>cos(PI/2)</tt> is a number very close to zero:
#
#   cos(PI/2) # => 6.123031769111886e-17
#
# For very large and very small returned values,
# we have added formatted numbers for clarity:
#
#   tan(PI/2)  # => 1.633123935319537e+16   # 16331239353195370.0
#   tan(PI)    # => -1.2246467991473532e-16 # -0.0000000000000001
#
# See class Float for the constants
# that affect Ruby's floating-point arithmetic.
#
# === What's Here
#
# ==== Trigonometric Functions
#
# - ::cos: Returns the cosine of the given argument.
# - ::sin: Returns the sine of the given argument.
# - ::tan: Returns the tangent of the given argument.
#
# ==== Inverse Trigonometric Functions
#
# - ::acos: Returns the arc cosine of the given argument.
# - ::asin: Returns the arc sine of the given argument.
# - ::atan: Returns the arc tangent of the given argument.
# - ::atan2: Returns the arg tangent of two given arguments.
#
# ==== Hyperbolic Trigonometric Functions
#
# - ::cosh: Returns the hyperbolic cosine of the given argument.
# - ::sinh: Returns the hyperbolic sine of the given argument.
# - ::tanh: Returns the hyperbolic tangent of the given argument.
#
# ==== Inverse Hyperbolic Trigonometric Functions
#
# - ::acosh: Returns the inverse hyperbolic cosine of the given argument.
# - ::asinh: Returns the inverse hyperbolic sine of the given argument.
# - ::atanh: Returns the inverse hyperbolic tangent of the given argument.
#
# ==== Exponentiation and Logarithmic Functions
#
# - ::exp: Returns the value of a given value raised to a given power.
# - ::log: Returns the logarithm of a given value in a given base.
# - ::log10: Returns the base 10 logarithm of the given argument.
# - ::log2: Returns the base 2 logarithm of the given argument.
#
# ==== Fraction and Exponent Functions
#
# - ::frexp: Returns the fraction and exponent of the given argument.
# - ::ldexp: Returns the value for a given fraction and exponent.
#
# ==== Root Functions
#
# - ::cbrt: Returns the cube root of the given argument.
# - ::sqrt: Returns the square root of the given argument.
#
# ==== Error Functions
#
# - ::erf: Returns the value of the Gauss error function for the given argument.
# - ::erfc: Returns the value of the complementary error function
#   for the given argument.
#
# ==== Gamma Functions
#
# - ::gamma: Returns the value of the gamma function for the given argument.
# - ::lgamma: Returns the value of the logarithmic gamma function
#   for the given argument.
#
# ==== Hypotenuse Function
#
# - ::hypot: Returns <tt>sqrt(a**2 + b**2)</tt> for the given +a+ and +b+.
module Math
  # Definition of the mathematical constant E for Euler's number (e) as a Float number.
  E = _
  # Definition of the mathematical constant PI as a Float number.
  PI = _

  # Returns the {arc cosine}[https://en.wikipedia.org/wiki/Inverse_trigonometric_functions] of +x+.
  #
  # - Domain: <tt>[-1, 1]</tt>.
  # - Range: <tt>[0, PI]</tt>.
  #
  # Examples:
  #
  #   acos(-1.0) # => 3.141592653589793  # PI
  #   acos(0.0)  # => 1.5707963267948966 # PI/2
  #   acos(1.0)  # => 0.0
  def self.acos(x) end

  # Returns the {inverse hyperbolic cosine}[https://en.wikipedia.org/wiki/Inverse_hyperbolic_functions] of +x+.
  #
  # - Domain: <tt>[1, INFINITY]</tt>.
  # - Range: <tt>[0, INFINITY]</tt>.
  #
  # Examples:
  #
  #   acosh(1.0)      # => 0.0
  #   acosh(INFINITY) # => Infinity
  def self.acosh(x) end

  # Returns the {arc sine}[https://en.wikipedia.org/wiki/Inverse_trigonometric_functions] of +x+.
  #
  # - Domain: <tt>[-1, -1]</tt>.
  # - Range: <tt>[-PI/2, PI/2]</tt>.
  #
  # Examples:
  #
  #   asin(-1.0) # => -1.5707963267948966 # -PI/2
  #   asin(0.0)  # => 0.0
  #   asin(1.0)  # => 1.5707963267948966  # PI/2
  def self.asin(x) end

  # Returns the {inverse hyperbolic sine}[https://en.wikipedia.org/wiki/Inverse_hyperbolic_functions] of +x+.
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   asinh(-INFINITY) # => -Infinity
  #   asinh(0.0)       # => 0.0
  #   asinh(INFINITY)  # => Infinity
  def self.asinh(x) end

  # Returns the {arc tangent}[https://en.wikipedia.org/wiki/Inverse_trigonometric_functions] of +x+.
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-PI/2, PI/2]  </tt>.
  #
  # Examples:
  #
  #   atan(-INFINITY) # => -1.5707963267948966 # -PI2
  #   atan(-PI)       # => -1.2626272556789115
  #   atan(-PI/2)     # => -1.0038848218538872
  #   atan(0.0)       # => 0.0
  #   atan(PI/2)      # => 1.0038848218538872
  #   atan(PI)        # => 1.2626272556789115
  #   atan(INFINITY)  # => 1.5707963267948966  # PI/2
  def self.atan(x) end

  # Returns the {arc tangent}[https://en.wikipedia.org/wiki/Atan2] of +y+ and +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain of +y+: <tt>[-INFINITY, INFINITY]</tt>.
  # - Domain of +x+: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-PI, PI]</tt>.
  #
  # Examples:
  #
  #   atan2(-1.0, -1.0) # => -2.356194490192345  # -3*PI/4
  #   atan2(-1.0, 0.0)  # => -1.5707963267948966 # -PI/2
  #   atan2(-1.0, 1.0)  # => -0.7853981633974483 # -PI/4
  #   atan2(0.0, -1.0)  # => 3.141592653589793   # PI
  def self.atan2(y, x) end

  # Returns the {inverse hyperbolic tangent}[https://en.wikipedia.org/wiki/Inverse_hyperbolic_functions] of +x+.
  #
  # - Domain: <tt>[-1, 1]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   atanh(-1.0) # => -Infinity
  #   atanh(0.0)  # => 0.0
  #   atanh(1.0)  # => Infinity
  def self.atanh(x) end

  # Returns the {cube root}[https://en.wikipedia.org/wiki/Cube_root] of +x+.
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   cbrt(-INFINITY) # => -Infinity
  #   cbrt(-27.0)     # => -3.0
  #   cbrt(-8.0)      # => -2.0
  #   cbrt(-2.0)      # => -1.2599210498948732
  #   cbrt(1.0)       # => 1.0
  #   cbrt(0.0)       # => 0.0
  #   cbrt(1.0)       # => 1.0
  #   cbrt(2.0)       # => 1.2599210498948732
  #   cbrt(8.0)       # => 2.0
  #   cbrt(27.0)      # => 3.0
  #   cbrt(INFINITY)  # => Infinity
  def self.cbrt(x) end

  # Returns the
  # {cosine}[https://en.wikipedia.org/wiki/Sine_and_cosine] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>(-INFINITY, INFINITY)</tt>.
  # - Range: <tt>[-1.0, 1.0]</tt>.
  #
  # Examples:
  #
  #   cos(-PI)   # => -1.0
  #   cos(-PI/2) # => 6.123031769111886e-17 # 0.0000000000000001
  #   cos(0.0)   # => 1.0
  #   cos(PI/2)  # => 6.123031769111886e-17 # 0.0000000000000001
  #   cos(PI)    # => -1.0
  def self.cos(x) end

  # Returns the {hyperbolic cosine}[https://en.wikipedia.org/wiki/Hyperbolic_functions] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[1, INFINITY]</tt>.
  #
  # Examples:
  #
  #   cosh(-INFINITY) # => Infinity
  #   cosh(0.0)       # => 1.0
  #   cosh(INFINITY)  # => Infinity
  def self.cosh(x) end

  #  Returns the value of the {Gauss error function}[https://en.wikipedia.org/wiki/Error_function] for +x+.
  #
  #  - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  #  - Range: <tt>[-1, 1]</tt>.
  #
  #  Examples:
  #
  #    erf(-INFINITY) # => -1.0
  #    erf(0.0)       # => 0.0
  #    erf(INFINITY)  # => 1.0
  #
  #  Related: Math.erfc.
  def self.erf(x) end

  #  Returns the value of the {complementary error function}[https://en.wikipedia.org/wiki/Error_function#Complementary_error_function] for +x+.
  #
  #  - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  #  - Range: <tt>[0, 2]</tt>.
  #
  #  Examples:
  #
  #    erfc(-INFINITY) # => 2.0
  #    erfc(0.0)       # => 1.0
  #    erfc(INFINITY)  # => 0.0
  #
  #  Related: Math.erf.
  def self.erfc(x) end

  # Returns +e+ raised to the +x+ power.
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[0, INFINITY]</tt>.
  #
  # Examples:
  #
  #   exp(-INFINITY) # => 0.0
  #   exp(-1.0)      # => 0.36787944117144233 # 1.0/E
  #   exp(0.0)       # => 1.0
  #   exp(0.5)       # => 1.6487212707001282  # sqrt(E)
  #   exp(1.0)       # => 2.718281828459045   # E
  #   exp(2.0)       # => 7.38905609893065    # E**2
  #   exp(INFINITY)  # => Infinity
  def self.exp(x) end

  # Returns a 2-element array containing the normalized signed float +fraction+
  # and integer +exponent+ of +x+ such that:
  #
  #   x = fraction * 2**exponent
  #
  # See {IEEE 754 double-precision binary floating-point format: binary64}[https://en.wikipedia.org/wiki/Double-precision_floating-point_format#IEEE_754_double-precision_binary_floating-point_format:_binary64].
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   frexp(-INFINITY) # => [-Infinity, -1]
  #   frexp(-2.0)      # => [-0.5, 2]
  #   frexp(-1.0)      # => [-0.5, 1]
  #   frexp(0.0)       # => [0.0, 0]
  #   frexp(1.0)       # => [0.5, 1]
  #   frexp(2.0)       # => [0.5, 2]
  #   frexp(INFINITY)  # => [Infinity, -1]
  #
  # Related: Math.ldexp (inverse of Math.frexp).
  def self.frexp(x) end

  #  Returns the value of the {gamma function}[https://en.wikipedia.org/wiki/Gamma_function] for +x+.
  #
  #  - Domain: <tt>(-INFINITY, INFINITY]</tt> excluding negative integers.
  #  - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  #  Examples:
  #
  #    gamma(-2.5)      # => -0.9453087204829431
  #    gamma(-1.5)      # => 2.3632718012073513
  #    gamma(-0.5)      # => -3.5449077018110375
  #    gamma(0.0)      # => Infinity
  #    gamma(1.0)      # => 1.0
  #    gamma(2.0)      # => 1.0
  #    gamma(3.0)      # => 2.0
  #    gamma(4.0)      # => 6.0
  #    gamma(5.0)      # => 24.0
  #
  #  Related: Math.lgamma.
  def self.gamma(x) end

  # Returns <tt>sqrt(a**2 + b**2)</tt>,
  # which is the length of the longest side +c+ (the hypotenuse)
  # of the right triangle whose other sides have lengths +a+ and +b+.
  #
  # - Domain of +a+: <tt>[-INFINITY, INFINITY]</tt>.
  # - Domain of +ab: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[0, INFINITY]</tt>.
  #
  # Examples:
  #
  #   hypot(0.0, 1.0)       # => 1.0
  #   hypot(1.0, 1.0)       # => 1.4142135623730951 # sqrt(2.0)
  #   hypot(3.0, 4.0)       # => 5.0
  #   hypot(5.0, 12.0)      # => 13.0
  #   hypot(1.0, sqrt(3.0)) # => 1.9999999999999998 # Near 2.0
  #
  # Note that if either argument is +INFINITY+ or <tt>-INFINITY</tt>,
  # the result is +Infinity+.
  def self.hypot(a, b) end

  # Returns the value of <tt>fraction * 2**exponent</tt>.
  #
  # - Domain of +fraction+: <tt>[0.0, 1.0)</tt>.
  # - Domain of +exponent+: <tt>[0, 1024]</tt>
  #   (larger values are equivalent to 1024).
  #
  # See {IEEE 754 double-precision binary floating-point format: binary64}[https://en.wikipedia.org/wiki/Double-precision_floating-point_format#IEEE_754_double-precision_binary_floating-point_format:_binary64].
  #
  # Examples:
  #
  #   ldexp(-INFINITY, -1) # => -Infinity
  #   ldexp(-0.5, 2)       # => -2.0
  #   ldexp(-0.5, 1)       # => -1.0
  #   ldexp(0.0, 0)        # => 0.0
  #   ldexp(-0.5, 1)       # => 1.0
  #   ldexp(-0.5, 2)       # => 2.0
  #   ldexp(INFINITY, -1)  # => Infinity
  #
  # Related: Math.frexp (inverse of Math.ldexp).
  def self.ldexp(fraction, exponent) end

  #  Returns a 2-element array equivalent to:
  #
  #    [Math.log(Math.gamma(x).abs), Math.gamma(x) < 0 ? -1 : 1]
  #
  #  See {logarithmic gamma function}[https://en.wikipedia.org/wiki/Gamma_function#The_log-gamma_function].
  #
  #  - Domain: <tt>(-INFINITY, INFINITY]</tt>.
  #  - Range of first element: <tt>(-INFINITY, INFINITY]</tt>.
  #  - Second element is -1 or 1.
  #
  #  Examples:
  #
  #    lgamma(-4.0) # => [Infinity, -1]
  #    lgamma(-3.0) # => [Infinity, -1]
  #    lgamma(-2.0) # => [Infinity, -1]
  #    lgamma(-1.0) # => [Infinity, -1]
  #    lgamma(0.0)  # => [Infinity, 1]
  #
  #    lgamma(1.0)  # => [0.0, 1]
  #    lgamma(2.0)  # => [0.0, 1]
  #    lgamma(3.0)  # => [0.6931471805599436, 1]
  #    lgamma(4.0)  # => [1.7917594692280545, 1]
  #
  #    lgamma(-2.5) # => [-0.05624371649767279, -1]
  #    lgamma(-1.5) # => [0.8600470153764797, 1]
  #    lgamma(-0.5) # => [1.265512123484647, -1]
  #    lgamma(0.5)  # => [0.5723649429247004, 1]
  #    lgamma(1.5)  # => [-0.12078223763524676, 1]
  #    lgamma(2.5)      # => [0.2846828704729205, 1]
  #
  #  Related: Math.gamma.
  def self.lgamma(x) end

  # Returns the base +base+ {logarithm}[https://en.wikipedia.org/wiki/Logarithm] of +x+.
  #
  # - Domain: <tt>[0, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY)]</tt>.
  #
  # Examples:
  #
  #   log(0.0)        # => -Infinity
  #   log(1.0)        # => 0.0
  #   log(E)          # => 1.0
  #   log(INFINITY)   # => Infinity
  #
  #   log(0.0, 2.0)   # => -Infinity
  #   log(1.0, 2.0)   # => 0.0
  #   log(2.0, 2.0)   # => 1.0
  #
  #   log(0.0, 10.0)  # => -Infinity
  #   log(1.0, 10.0)  # => 0.0
  #   log(10.0, 10.0) # => 1.0
  def self.log(x, base = Math::E) end

  # Returns the base 10 {logarithm}[https://en.wikipedia.org/wiki/Logarithm] of +x+.
  #
  # - Domain: <tt>[0, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   log10(0.0)      # => -Infinity
  #   log10(1.0)      # => 0.0
  #   log10(10.0)     # => 1.0
  #   log10(INFINITY) # => Infinity
  def self.log10(x) end

  # Returns the base 2 {logarithm}[https://en.wikipedia.org/wiki/Logarithm] of +x+.
  #
  # - Domain: <tt>[0, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   log2(0.0)      # => -Infinity
  #   log2(1.0)      # => 0.0
  #   log2(2.0)      # => 1.0
  #   log2(INFINITY) # => Infinity
  def self.log2(x) end

  # Returns the
  # {sine}[https://en.wikipedia.org/wiki/Sine_and_cosine] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>(-INFINITY, INFINITY)</tt>.
  # - Range: <tt>[-1.0, 1.0]</tt>.
  #
  # Examples:
  #
  #   sin(-PI)   # => -1.2246063538223773e-16 # -0.0000000000000001
  #   sin(-PI/2) # => -1.0
  #   sin(0.0)   # => 0.0
  #   sin(PI/2)  # => 1.0
  #   sin(PI)    # => 1.2246063538223773e-16  # 0.0000000000000001
  def self.sin(x) end

  # Returns the {hyperbolic sine}[https://en.wikipedia.org/wiki/Hyperbolic_functions] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   sinh(-INFINITY) # => -Infinity
  #   sinh(0.0)       # => 0.0
  #   sinh(INFINITY)  # => Infinity
  def self.sinh(x) end

  # Returns the principal (non-negative) {square root}[https://en.wikipedia.org/wiki/Square_root] of +x+.
  #
  # - Domain: <tt>[0, INFINITY]</tt>.
  # - Range: <tt>[0, INFINITY]</tt>.
  #
  # Examples:
  #
  #   sqrt(0.0)      # => 0.0
  #   sqrt(0.5)      # => 0.7071067811865476
  #   sqrt(1.0)      # => 1.0
  #   sqrt(2.0)      # => 1.4142135623730951
  #   sqrt(4.0)      # => 2.0
  #   sqrt(9.0)      # => 3.0
  #   sqrt(INFINITY) # => Infinity
  def self.sqrt(x) end

  # Returns the
  # {tangent}[https://en.wikipedia.org/wiki/Trigonometric_functions] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>(-INFINITY, INFINITY)</tt>.
  # - Range: <tt>(-INFINITY, INFINITY)</tt>.
  #
  # Examples:
  #
  #   tan(-PI)   # => 1.2246467991473532e-16  # -0.0000000000000001
  #   tan(-PI/2) # => -1.633123935319537e+16  # -16331239353195370.0
  #   tan(0.0)   # => 0.0
  #   tan(PI/2)  # => 1.633123935319537e+16   # 16331239353195370.0
  #   tan(PI)    # => -1.2246467991473532e-16 # -0.0000000000000001
  def self.tan(x) end

  # Returns the {hyperbolic tangent}[https://en.wikipedia.org/wiki/Hyperbolic_functions] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-1, 1]</tt>.
  #
  # Examples:
  #
  #   tanh(-INFINITY) # => -1.0
  #   tanh(0.0)       # => 0.0
  #   tanh(INFINITY)  # => 1.0
  def self.tanh(x) end

  private

  # Returns the {arc cosine}[https://en.wikipedia.org/wiki/Inverse_trigonometric_functions] of +x+.
  #
  # - Domain: <tt>[-1, 1]</tt>.
  # - Range: <tt>[0, PI]</tt>.
  #
  # Examples:
  #
  #   acos(-1.0) # => 3.141592653589793  # PI
  #   acos(0.0)  # => 1.5707963267948966 # PI/2
  #   acos(1.0)  # => 0.0
  def acos(x) end

  # Returns the {inverse hyperbolic cosine}[https://en.wikipedia.org/wiki/Inverse_hyperbolic_functions] of +x+.
  #
  # - Domain: <tt>[1, INFINITY]</tt>.
  # - Range: <tt>[0, INFINITY]</tt>.
  #
  # Examples:
  #
  #   acosh(1.0)      # => 0.0
  #   acosh(INFINITY) # => Infinity
  def acosh(x) end

  # Returns the {arc sine}[https://en.wikipedia.org/wiki/Inverse_trigonometric_functions] of +x+.
  #
  # - Domain: <tt>[-1, -1]</tt>.
  # - Range: <tt>[-PI/2, PI/2]</tt>.
  #
  # Examples:
  #
  #   asin(-1.0) # => -1.5707963267948966 # -PI/2
  #   asin(0.0)  # => 0.0
  #   asin(1.0)  # => 1.5707963267948966  # PI/2
  def asin(x) end

  # Returns the {inverse hyperbolic sine}[https://en.wikipedia.org/wiki/Inverse_hyperbolic_functions] of +x+.
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   asinh(-INFINITY) # => -Infinity
  #   asinh(0.0)       # => 0.0
  #   asinh(INFINITY)  # => Infinity
  def asinh(x) end

  # Returns the {arc tangent}[https://en.wikipedia.org/wiki/Inverse_trigonometric_functions] of +x+.
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-PI/2, PI/2]  </tt>.
  #
  # Examples:
  #
  #   atan(-INFINITY) # => -1.5707963267948966 # -PI2
  #   atan(-PI)       # => -1.2626272556789115
  #   atan(-PI/2)     # => -1.0038848218538872
  #   atan(0.0)       # => 0.0
  #   atan(PI/2)      # => 1.0038848218538872
  #   atan(PI)        # => 1.2626272556789115
  #   atan(INFINITY)  # => 1.5707963267948966  # PI/2
  def atan(x) end

  # Returns the {arc tangent}[https://en.wikipedia.org/wiki/Atan2] of +y+ and +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain of +y+: <tt>[-INFINITY, INFINITY]</tt>.
  # - Domain of +x+: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-PI, PI]</tt>.
  #
  # Examples:
  #
  #   atan2(-1.0, -1.0) # => -2.356194490192345  # -3*PI/4
  #   atan2(-1.0, 0.0)  # => -1.5707963267948966 # -PI/2
  #   atan2(-1.0, 1.0)  # => -0.7853981633974483 # -PI/4
  #   atan2(0.0, -1.0)  # => 3.141592653589793   # PI
  def atan2(y, x) end

  # Returns the {inverse hyperbolic tangent}[https://en.wikipedia.org/wiki/Inverse_hyperbolic_functions] of +x+.
  #
  # - Domain: <tt>[-1, 1]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   atanh(-1.0) # => -Infinity
  #   atanh(0.0)  # => 0.0
  #   atanh(1.0)  # => Infinity
  def atanh(x) end

  # Returns the {cube root}[https://en.wikipedia.org/wiki/Cube_root] of +x+.
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   cbrt(-INFINITY) # => -Infinity
  #   cbrt(-27.0)     # => -3.0
  #   cbrt(-8.0)      # => -2.0
  #   cbrt(-2.0)      # => -1.2599210498948732
  #   cbrt(1.0)       # => 1.0
  #   cbrt(0.0)       # => 0.0
  #   cbrt(1.0)       # => 1.0
  #   cbrt(2.0)       # => 1.2599210498948732
  #   cbrt(8.0)       # => 2.0
  #   cbrt(27.0)      # => 3.0
  #   cbrt(INFINITY)  # => Infinity
  def cbrt(x) end

  # Returns the
  # {cosine}[https://en.wikipedia.org/wiki/Sine_and_cosine] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>(-INFINITY, INFINITY)</tt>.
  # - Range: <tt>[-1.0, 1.0]</tt>.
  #
  # Examples:
  #
  #   cos(-PI)   # => -1.0
  #   cos(-PI/2) # => 6.123031769111886e-17 # 0.0000000000000001
  #   cos(0.0)   # => 1.0
  #   cos(PI/2)  # => 6.123031769111886e-17 # 0.0000000000000001
  #   cos(PI)    # => -1.0
  def cos(x) end

  # Returns the {hyperbolic cosine}[https://en.wikipedia.org/wiki/Hyperbolic_functions] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[1, INFINITY]</tt>.
  #
  # Examples:
  #
  #   cosh(-INFINITY) # => Infinity
  #   cosh(0.0)       # => 1.0
  #   cosh(INFINITY)  # => Infinity
  def cosh(x) end

  #  Returns the value of the {Gauss error function}[https://en.wikipedia.org/wiki/Error_function] for +x+.
  #
  #  - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  #  - Range: <tt>[-1, 1]</tt>.
  #
  #  Examples:
  #
  #    erf(-INFINITY) # => -1.0
  #    erf(0.0)       # => 0.0
  #    erf(INFINITY)  # => 1.0
  #
  #  Related: Math.erfc.
  def erf(x) end

  #  Returns the value of the {complementary error function}[https://en.wikipedia.org/wiki/Error_function#Complementary_error_function] for +x+.
  #
  #  - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  #  - Range: <tt>[0, 2]</tt>.
  #
  #  Examples:
  #
  #    erfc(-INFINITY) # => 2.0
  #    erfc(0.0)       # => 1.0
  #    erfc(INFINITY)  # => 0.0
  #
  #  Related: Math.erf.
  def erfc(x) end

  # Returns +e+ raised to the +x+ power.
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[0, INFINITY]</tt>.
  #
  # Examples:
  #
  #   exp(-INFINITY) # => 0.0
  #   exp(-1.0)      # => 0.36787944117144233 # 1.0/E
  #   exp(0.0)       # => 1.0
  #   exp(0.5)       # => 1.6487212707001282  # sqrt(E)
  #   exp(1.0)       # => 2.718281828459045   # E
  #   exp(2.0)       # => 7.38905609893065    # E**2
  #   exp(INFINITY)  # => Infinity
  def exp(x) end

  # Returns a 2-element array containing the normalized signed float +fraction+
  # and integer +exponent+ of +x+ such that:
  #
  #   x = fraction * 2**exponent
  #
  # See {IEEE 754 double-precision binary floating-point format: binary64}[https://en.wikipedia.org/wiki/Double-precision_floating-point_format#IEEE_754_double-precision_binary_floating-point_format:_binary64].
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   frexp(-INFINITY) # => [-Infinity, -1]
  #   frexp(-2.0)      # => [-0.5, 2]
  #   frexp(-1.0)      # => [-0.5, 1]
  #   frexp(0.0)       # => [0.0, 0]
  #   frexp(1.0)       # => [0.5, 1]
  #   frexp(2.0)       # => [0.5, 2]
  #   frexp(INFINITY)  # => [Infinity, -1]
  #
  # Related: Math.ldexp (inverse of Math.frexp).
  def frexp(x) end

  #  Returns the value of the {gamma function}[https://en.wikipedia.org/wiki/Gamma_function] for +x+.
  #
  #  - Domain: <tt>(-INFINITY, INFINITY]</tt> excluding negative integers.
  #  - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  #  Examples:
  #
  #    gamma(-2.5)      # => -0.9453087204829431
  #    gamma(-1.5)      # => 2.3632718012073513
  #    gamma(-0.5)      # => -3.5449077018110375
  #    gamma(0.0)      # => Infinity
  #    gamma(1.0)      # => 1.0
  #    gamma(2.0)      # => 1.0
  #    gamma(3.0)      # => 2.0
  #    gamma(4.0)      # => 6.0
  #    gamma(5.0)      # => 24.0
  #
  #  Related: Math.lgamma.
  def gamma(x) end

  # Returns <tt>sqrt(a**2 + b**2)</tt>,
  # which is the length of the longest side +c+ (the hypotenuse)
  # of the right triangle whose other sides have lengths +a+ and +b+.
  #
  # - Domain of +a+: <tt>[-INFINITY, INFINITY]</tt>.
  # - Domain of +ab: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[0, INFINITY]</tt>.
  #
  # Examples:
  #
  #   hypot(0.0, 1.0)       # => 1.0
  #   hypot(1.0, 1.0)       # => 1.4142135623730951 # sqrt(2.0)
  #   hypot(3.0, 4.0)       # => 5.0
  #   hypot(5.0, 12.0)      # => 13.0
  #   hypot(1.0, sqrt(3.0)) # => 1.9999999999999998 # Near 2.0
  #
  # Note that if either argument is +INFINITY+ or <tt>-INFINITY</tt>,
  # the result is +Infinity+.
  def hypot(a, b) end

  # Returns the value of <tt>fraction * 2**exponent</tt>.
  #
  # - Domain of +fraction+: <tt>[0.0, 1.0)</tt>.
  # - Domain of +exponent+: <tt>[0, 1024]</tt>
  #   (larger values are equivalent to 1024).
  #
  # See {IEEE 754 double-precision binary floating-point format: binary64}[https://en.wikipedia.org/wiki/Double-precision_floating-point_format#IEEE_754_double-precision_binary_floating-point_format:_binary64].
  #
  # Examples:
  #
  #   ldexp(-INFINITY, -1) # => -Infinity
  #   ldexp(-0.5, 2)       # => -2.0
  #   ldexp(-0.5, 1)       # => -1.0
  #   ldexp(0.0, 0)        # => 0.0
  #   ldexp(-0.5, 1)       # => 1.0
  #   ldexp(-0.5, 2)       # => 2.0
  #   ldexp(INFINITY, -1)  # => Infinity
  #
  # Related: Math.frexp (inverse of Math.ldexp).
  def ldexp(fraction, exponent) end

  #  Returns a 2-element array equivalent to:
  #
  #    [Math.log(Math.gamma(x).abs), Math.gamma(x) < 0 ? -1 : 1]
  #
  #  See {logarithmic gamma function}[https://en.wikipedia.org/wiki/Gamma_function#The_log-gamma_function].
  #
  #  - Domain: <tt>(-INFINITY, INFINITY]</tt>.
  #  - Range of first element: <tt>(-INFINITY, INFINITY]</tt>.
  #  - Second element is -1 or 1.
  #
  #  Examples:
  #
  #    lgamma(-4.0) # => [Infinity, -1]
  #    lgamma(-3.0) # => [Infinity, -1]
  #    lgamma(-2.0) # => [Infinity, -1]
  #    lgamma(-1.0) # => [Infinity, -1]
  #    lgamma(0.0)  # => [Infinity, 1]
  #
  #    lgamma(1.0)  # => [0.0, 1]
  #    lgamma(2.0)  # => [0.0, 1]
  #    lgamma(3.0)  # => [0.6931471805599436, 1]
  #    lgamma(4.0)  # => [1.7917594692280545, 1]
  #
  #    lgamma(-2.5) # => [-0.05624371649767279, -1]
  #    lgamma(-1.5) # => [0.8600470153764797, 1]
  #    lgamma(-0.5) # => [1.265512123484647, -1]
  #    lgamma(0.5)  # => [0.5723649429247004, 1]
  #    lgamma(1.5)  # => [-0.12078223763524676, 1]
  #    lgamma(2.5)      # => [0.2846828704729205, 1]
  #
  #  Related: Math.gamma.
  def lgamma(x) end

  # Returns the base +base+ {logarithm}[https://en.wikipedia.org/wiki/Logarithm] of +x+.
  #
  # - Domain: <tt>[0, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY)]</tt>.
  #
  # Examples:
  #
  #   log(0.0)        # => -Infinity
  #   log(1.0)        # => 0.0
  #   log(E)          # => 1.0
  #   log(INFINITY)   # => Infinity
  #
  #   log(0.0, 2.0)   # => -Infinity
  #   log(1.0, 2.0)   # => 0.0
  #   log(2.0, 2.0)   # => 1.0
  #
  #   log(0.0, 10.0)  # => -Infinity
  #   log(1.0, 10.0)  # => 0.0
  #   log(10.0, 10.0) # => 1.0
  def log(x, base = Math::E) end

  # Returns the base 10 {logarithm}[https://en.wikipedia.org/wiki/Logarithm] of +x+.
  #
  # - Domain: <tt>[0, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   log10(0.0)      # => -Infinity
  #   log10(1.0)      # => 0.0
  #   log10(10.0)     # => 1.0
  #   log10(INFINITY) # => Infinity
  def log10(x) end

  # Returns the base 2 {logarithm}[https://en.wikipedia.org/wiki/Logarithm] of +x+.
  #
  # - Domain: <tt>[0, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   log2(0.0)      # => -Infinity
  #   log2(1.0)      # => 0.0
  #   log2(2.0)      # => 1.0
  #   log2(INFINITY) # => Infinity
  def log2(x) end

  # Returns the
  # {sine}[https://en.wikipedia.org/wiki/Sine_and_cosine] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>(-INFINITY, INFINITY)</tt>.
  # - Range: <tt>[-1.0, 1.0]</tt>.
  #
  # Examples:
  #
  #   sin(-PI)   # => -1.2246063538223773e-16 # -0.0000000000000001
  #   sin(-PI/2) # => -1.0
  #   sin(0.0)   # => 0.0
  #   sin(PI/2)  # => 1.0
  #   sin(PI)    # => 1.2246063538223773e-16  # 0.0000000000000001
  def sin(x) end

  # Returns the {hyperbolic sine}[https://en.wikipedia.org/wiki/Hyperbolic_functions] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-INFINITY, INFINITY]</tt>.
  #
  # Examples:
  #
  #   sinh(-INFINITY) # => -Infinity
  #   sinh(0.0)       # => 0.0
  #   sinh(INFINITY)  # => Infinity
  def sinh(x) end

  # Returns the principal (non-negative) {square root}[https://en.wikipedia.org/wiki/Square_root] of +x+.
  #
  # - Domain: <tt>[0, INFINITY]</tt>.
  # - Range: <tt>[0, INFINITY]</tt>.
  #
  # Examples:
  #
  #   sqrt(0.0)      # => 0.0
  #   sqrt(0.5)      # => 0.7071067811865476
  #   sqrt(1.0)      # => 1.0
  #   sqrt(2.0)      # => 1.4142135623730951
  #   sqrt(4.0)      # => 2.0
  #   sqrt(9.0)      # => 3.0
  #   sqrt(INFINITY) # => Infinity
  def sqrt(x) end

  # Returns the
  # {tangent}[https://en.wikipedia.org/wiki/Trigonometric_functions] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>(-INFINITY, INFINITY)</tt>.
  # - Range: <tt>(-INFINITY, INFINITY)</tt>.
  #
  # Examples:
  #
  #   tan(-PI)   # => 1.2246467991473532e-16  # -0.0000000000000001
  #   tan(-PI/2) # => -1.633123935319537e+16  # -16331239353195370.0
  #   tan(0.0)   # => 0.0
  #   tan(PI/2)  # => 1.633123935319537e+16   # 16331239353195370.0
  #   tan(PI)    # => -1.2246467991473532e-16 # -0.0000000000000001
  def tan(x) end

  # Returns the {hyperbolic tangent}[https://en.wikipedia.org/wiki/Hyperbolic_functions] of +x+
  # in {radians}[https://en.wikipedia.org/wiki/Trigonometric_functions#Radians_versus_degrees].
  #
  # - Domain: <tt>[-INFINITY, INFINITY]</tt>.
  # - Range: <tt>[-1, 1]</tt>.
  #
  # Examples:
  #
  #   tanh(-INFINITY) # => -1.0
  #   tanh(0.0)       # => 0.0
  #   tanh(INFINITY)  # => 1.0
  def tanh(x) end

  # Raised when a mathematical function is evaluated outside of its
  # domain of definition.
  #
  # For example, since +cos+ returns values in the range -1..1,
  # its inverse function +acos+ is only defined on that interval:
  #
  #    Math.acos(42)
  #
  # <em>produces:</em>
  #
  #    Math::DomainError: Numerical argument is out of domain - "acos"
  class DomainError < StandardError
  end
end
