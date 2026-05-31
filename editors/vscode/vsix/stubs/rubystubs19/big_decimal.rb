# frozen_string_literal: true

# BigDecimal provides arbitrary-precision floating point decimal arithmetic.
#
# Copyright (C) 2002 by Shigeo Kobayashi <shigeo@tinyforest.gr.jp>.
# You may distribute under the terms of either the GNU General Public
# License or the Artistic License, as specified in the README file
# of the BigDecimal distribution.
#
# Documented by mathew <meta@pobox.com>.
#
# = Introduction
#
# Ruby provides built-in support for arbitrary precision integer arithmetic.
# For example:
#
# 42**13   ->   1265437718438866624512
#
# BigDecimal provides similar support for very large or very accurate floating
# point numbers.
#
# Decimal arithmetic is also useful for general calculation, because it
# provides the correct answers people expect--whereas normal binary floating
# point arithmetic often introduces subtle errors because of the conversion
# between base 10 and base 2. For example, try:
#
#   sum = 0
#   for i in (1..10000)
#     sum = sum + 0.0001
#   end
#   print sum
#
# and contrast with the output from:
#
#   require 'bigdecimal'
#
#   sum = BigDecimal.new("0")
#   for i in (1..10000)
#     sum = sum + BigDecimal.new("0.0001")
#   end
#   print sum
#
# Similarly:
#
# (BigDecimal.new("1.2") - BigDecimal("1.0")) == BigDecimal("0.2") -> true
#
# (1.2 - 1.0) == 0.2 -> false
#
# = Special features of accurate decimal arithmetic
#
# Because BigDecimal is more accurate than normal binary floating point
# arithmetic, it requires some special values.
#
# == Infinity
#
# BigDecimal sometimes needs to return infinity, for example if you divide
# a value by zero.
#
# BigDecimal.new("1.0") / BigDecimal.new("0.0")  -> infinity
#
# BigDecimal.new("-1.0") / BigDecimal.new("0.0")  -> -infinity
#
# You can represent infinite numbers to BigDecimal using the strings
# 'Infinity', '+Infinity' and '-Infinity' (case-sensitive)
#
# == Not a Number
#
# When a computation results in an undefined value, the special value NaN
# (for 'not a number') is returned.
#
# Example:
#
# BigDecimal.new("0.0") / BigDecimal.new("0.0") -> NaN
#
# You can also create undefined values.  NaN is never considered to be the
# same as any other value, even NaN itself:
#
# n = BigDecimal.new('NaN')
#
# n == 0.0 -> nil
#
# n == n -> nil
#
# == Positive and negative zero
#
# If a computation results in a value which is too small to be represented as
# a BigDecimal within the currently specified limits of precision, zero must
# be returned.
#
# If the value which is too small to be represented is negative, a BigDecimal
# value of negative zero is returned. If the value is positive, a value of
# positive zero is returned.
#
# BigDecimal.new("1.0") / BigDecimal.new("-Infinity") -> -0.0
#
# BigDecimal.new("1.0") / BigDecimal.new("Infinity") -> 0.0
#
# (See BigDecimal.mode for how to specify limits of precision.)
#
# Note that -0.0 and 0.0 are considered to be the same for the purposes of
# comparison.
#
# Note also that in mathematics, there is no particular concept of negative
# or positive zero; true mathematical zero has no sign.
class BigDecimal < Numeric
  # Base value used in internal calculations.  On a 32 bit system, BASE
  # is 10000, indicating that calculation is done in groups of 4 digits.
  # (If it were larger, BASE**2 wouldn't fit in 32 bits, so you couldn't
  # guarantee that two groups could always be multiplied together without
  # overflow.)
  BASE = _
  # Determines whether overflow, underflow or zero divide result in
  # an exception being thrown. See BigDecimal.mode.
  EXCEPTION_ALL = _
  # Determines what happens when the result of a computation is
  # infinity.  See BigDecimal.mode.
  EXCEPTION_INFINITY = _
  # Determines what happens when the result of a computation is not a
  # number (NaN). See BigDecimal.mode.
  EXCEPTION_NaN = _
  # Determines what happens when the result of a computation is an
  # overflow (a result too large to be represented). See BigDecimal.mode.
  EXCEPTION_OVERFLOW = _
  # Determines what happens when the result of a computation is an
  # underflow (a result too small to be represented). See BigDecimal.mode.
  EXCEPTION_UNDERFLOW = _
  # Determines what happens when a division by zero is performed.
  # See BigDecimal.mode.
  EXCEPTION_ZERODIVIDE = _
  INFINITY = _
  NAN = _
  # Round towards +infinity. See BigDecimal.mode.
  ROUND_CEILING = _
  # Indicates that values should be rounded towards zero. See
  # BigDecimal.mode.
  ROUND_DOWN = _
  # Round towards -infinity. See BigDecimal.mode.
  ROUND_FLOOR = _
  # Indicates that digits >= 6 should be rounded up, others rounded down.
  # See BigDecimal.mode.
  ROUND_HALF_DOWN = _
  # Round towards the even neighbor. See BigDecimal.mode.
  ROUND_HALF_EVEN = _
  # Indicates that digits >= 5 should be rounded up, others rounded down.
  # See BigDecimal.mode.
  ROUND_HALF_UP = _
  # Determines what happens when a result must be rounded in order to
  # fit in the appropriate number of significant digits. See
  # BigDecimal.mode.
  ROUND_MODE = _
  # Indicates that values should be rounded away from zero. See
  # BigDecimal.mode.
  ROUND_UP = _
  # Indicates that a value is negative and finite. See BigDecimal.sign.
  SIGN_NEGATIVE_FINITE = _
  # Indicates that a value is negative and infinite. See BigDecimal.sign.
  SIGN_NEGATIVE_INFINITE = _
  # Indicates that a value is -0. See BigDecimal.sign.
  SIGN_NEGATIVE_ZERO = _
  # Indicates that a value is not a number. See BigDecimal.sign.
  SIGN_NaN = _
  # Indicates that a value is positive and finite. See BigDecimal.sign.
  SIGN_POSITIVE_FINITE = _
  # Indicates that a value is positive and infinite. See BigDecimal.sign.
  SIGN_POSITIVE_INFINITE = _
  # Indicates that a value is +0. See BigDecimal.sign.
  SIGN_POSITIVE_ZERO = _

  # Internal method used to provide marshalling support. See the Marshal module.
  def self._load(p1) end

  # The BigDecimal.double_fig class method returns the number of digits a
  # Float number is allowed to have. The result depends upon the CPU and OS
  # in use.
  def self.double_fig; end

  #  Limit the number of significant digits in newly created BigDecimal
  #  numbers to the specified value. Rounding is performed as necessary,
  #  as specified by BigDecimal.mode.
  #
  #  A limit of 0, the default, means no upper limit.
  #
  #  The limit specified by this method takes less priority over any limit
  #  specified to instance methods such as ceil, floor, truncate, or round.
  def self.limit(digits = 0) end

  # Controls handling of arithmetic exceptions and rounding. If no value
  # is supplied, the current value is returned.
  #
  # Six values of the mode parameter control the handling of arithmetic
  # exceptions:
  #
  # BigDecimal::EXCEPTION_NaN
  # BigDecimal::EXCEPTION_INFINITY
  # BigDecimal::EXCEPTION_UNDERFLOW
  # BigDecimal::EXCEPTION_OVERFLOW
  # BigDecimal::EXCEPTION_ZERODIVIDE
  # BigDecimal::EXCEPTION_ALL
  #
  # For each mode parameter above, if the value set is false, computation
  # continues after an arithmetic exception of the appropriate type.
  # When computation continues, results are as follows:
  #
  # EXCEPTION_NaN:: NaN
  # EXCEPTION_INFINITY:: +infinity or -infinity
  # EXCEPTION_UNDERFLOW:: 0
  # EXCEPTION_OVERFLOW:: +infinity or -infinity
  # EXCEPTION_ZERODIVIDE:: +infinity or -infinity
  #
  # One value of the mode parameter controls the rounding of numeric values:
  # BigDecimal::ROUND_MODE. The values it can take are:
  #
  # ROUND_UP, :up:: round away from zero
  # ROUND_DOWN, :down, :truncate:: round towards zero (truncate)
  # ROUND_HALF_UP, :half_up, :default:: round towards the nearest neighbor, unless both neighbors are equidistant, in which case round away from zero. (default)
  # ROUND_HALF_DOWN, :half_down:: round towards the nearest neighbor, unless both neighbors are equidistant, in which case round towards zero.
  # ROUND_HALF_EVEN, :half_even, :banker:: round towards the nearest neighbor, unless both neighbors are equidistant, in which case round towards the even neighbor (Banker's rounding)
  # ROUND_CEILING, :ceiling, :ceil:: round towards positive infinity (ceil)
  # ROUND_FLOOR, :floor:: round towards negative infinity (floor)
  def self.mode(mode, value) end

  def self.save_exception_mode; end

  def self.save_limit; end

  def self.save_rounding_mode; end

  # Returns the BigDecimal version number.
  #
  # Ruby 1.8.0 returns 1.0.0.
  # Ruby 1.8.1 thru 1.8.3 return 1.0.1.
  def self.ver; end

  # Create a new BigDecimal object.
  #
  # initial:: The initial value, as an Integer, a Float, a Rational,
  #           a BigDecimal, or a String.
  #           If it is a String, spaces are ignored and unrecognized characters
  #           terminate the value.
  #
  # digits:: The number of significant digits, as a Fixnum. If omitted or 0,
  #          the number of significant digits is determined from the initial
  #          value.
  #
  # The actual number of significant digits used in computation is usually
  # larger than the specified number.
  def initialize(initial, digits) end

  # Returns the modulus from dividing by b. See divmod.
  def %(other) end
  alias modulo %

  #  Multiply by the specified value.
  #
  #  e.g.
  #    c = a.mult(b,n)
  #    c = a * b
  #
  #  digits:: If specified and less than the number of significant digits of the result, the result is rounded to that number of digits, according to BigDecimal.mode.
  def *(other) end

  # It is a synonym of big_decimal.power(exp).
  def **(other) end

  #  Add the specified value.
  #
  #  e.g.
  #    c = a.add(b,n)
  #    c = a + b
  #
  #  digits:: If specified and less than the number of significant digits of the result, the result is rounded to that number of digits, according to BigDecimal.mode.
  def +(other) end

  def +@; end

  #  Subtract the specified value.
  #
  #  e.g.
  #    c = a.sub(b,n)
  #    c = a - b
  #
  #  digits:: If specified and less than the number of significant digits of the result, the result is rounded to that number of digits, according to BigDecimal.mode.
  def -(other) end

  def -@; end

  #  Divide by the specified value.
  #
  #  e.g.
  #    c = a.div(b,n)
  #
  #  digits:: If specified and less than the number of significant digits of the result, the result is rounded to that number of digits, according to BigDecimal.mode.
  #
  #  If digits is 0, the result is the same as the / operator. If not, the
  #  result is an integer BigDecimal, by analogy with Float#div.
  #
  #  The alias quo is provided since div(value, 0) is the same as computing
  #  the quotient; see divmod.
  def /(other) end
  alias quo /

  # Returns true if a is less than b. Values may be coerced to perform the
  # comparison (see ==, coerce).
  def <(other) end

  # Returns true if a is less than or equal to b. Values may be coerced to
  # perform the comparison (see ==, coerce).
  def <=(other) end

  # The comparison operator.
  # a <=> b is 0 if a == b, 1 if a > b, -1 if a < b.
  def <=>(other) end

  # Tests for value equality; returns true if the values are equal.
  #
  # The == and === operators and the eql? method have the same implementation
  # for BigDecimal.
  #
  # Values may be coerced to perform the comparison:
  #
  # BigDecimal.new('1.0') == 1.0  -> true
  def ==(other) end
  alias === ==
  alias eql? ==

  # Returns true if a is greater than b.  Values may be coerced to
  # perform the comparison (see ==, coerce).
  def >(other) end

  # Returns true if a is greater than or equal to b. Values may be coerced to
  # perform the comparison (see ==, coerce)
  def >=(other) end

  def _dump(p1 = v1) end

  # Returns the absolute value.
  #
  # BigDecimal('5').abs -> 5
  #
  # BigDecimal('-3').abs -> 3
  def abs; end

  def add(p1, p2) end

  # Return the smallest integer greater than or equal to the value, as a BigDecimal.
  #
  # BigDecimal('3.14159').ceil -> 4
  #
  # BigDecimal('-9.1').ceil -> -9
  #
  # If n is specified and positive, the fractional part of the result has no
  # more than that many digits.
  #
  # If n is specified and negative, at least that
  # many digits to the left of the decimal point will be 0 in the result.
  #
  # BigDecimal('3.14159').ceil(3) -> 3.142
  #
  # BigDecimal('13345.234').ceil(-2) -> 13400.0
  def ceil(n = 0) end

  # The coerce method provides support for Ruby type coercion. It is not
  # enabled by default.
  #
  # This means that binary operations like + * / or - can often be performed
  # on a BigDecimal and an object of another type, if the other object can
  # be coerced into a BigDecimal value.
  #
  # e.g.
  # a = BigDecimal.new("1.0")
  # b = a / 2.0  -> 0.5
  #
  # Note that coercing a String to a BigDecimal is not supported by default;
  # it requires a special compile-time option when building Ruby.
  def coerce(p1) end

  def div(p1, p2 = v2) end

  # Divides by the specified value, and returns the quotient and modulus
  # as BigDecimal numbers. The quotient is rounded towards negative infinity.
  #
  # For example:
  #
  # require 'bigdecimal'
  #
  # a = BigDecimal.new("42")
  # b = BigDecimal.new("9")
  #
  # q,m = a.divmod(b)
  #
  # c = q * b + m
  #
  # a == c  -> true
  #
  # The quotient q is (a/b).floor, and the modulus is the amount that must be
  # added to q * b to get a.
  def divmod(p1) end

  # Returns the exponent of the BigDecimal number, as an Integer.
  #
  # If the number can be represented as 0.xxxxxx*10**n where xxxxxx is a string
  # of digits with no leading zeros, then n is the exponent.
  def exponent; end

  # Returns True if the value is finite (not NaN or infinite)
  def finite?; end

  # Return the integer part of the number.
  def fix; end

  # Return the largest integer less than or equal to the value, as a BigDecimal.
  #
  # BigDecimal('3.14159').floor -> 3
  #
  # BigDecimal('-9.1').floor -> -10
  #
  # If n is specified and positive, the fractional part of the result has no
  # more than that many digits.
  #
  # If n is specified and negative, at least that
  # many digits to the left of the decimal point will be 0 in the result.
  #
  # BigDecimal('3.14159').floor(3) -> 3.141
  #
  # BigDecimal('13345.234').floor(-2) -> 13300.0
  def floor(n = 0) end

  # Return the fractional part of the number.
  def frac; end

  def hash; end

  # Returns nil, -1, or +1 depending on whether the value is finite,
  # -infinity, or +infinity.
  def infinite?; end

  # Returns debugging information about the value as a string of comma-separated
  # values in angle brackets with a leading #:
  #
  # BigDecimal.new("1234.5678").inspect ->
  # "#<BigDecimal:b7ea1130,'0.12345678E4',8(12)>"
  #
  # The first part is the address, the second is the value as a string, and
  # the final part ss(mm) is the current number of significant digits and the
  # maximum number of significant digits, respectively.
  def inspect; end

  def mult(p1, p2) end

  # Returns True if the value is Not a Number
  def nan?; end

  # Returns self if the value is non-zero, nil otherwise.
  def nonzero?; end

  # Returns the value raised to the power of n. Note that n must be an Integer.
  #
  # Also available as the operator **
  def power(*several_variants) end

  # Returns an Array of two Integer values.
  #
  # The first value is the current number of significant digits in the
  # BigDecimal. The second value is the maximum number of significant digits
  # for the BigDecimal.
  def precs; end

  # Returns the remainder from dividing by the value.
  #
  # x.remainder(y) means x-y*(x/y).truncate
  def remainder(p1) end

  # Round to the nearest 1 (by default), returning the result as a BigDecimal.
  #
  # BigDecimal('3.14159').round -> 3
  #
  # BigDecimal('8.7').round -> 9
  #
  # If n is specified and positive, the fractional part of the result has no
  # more than that many digits.
  #
  # If n is specified and negative, at least that many digits to the left of the
  # decimal point will be 0 in the result.
  #
  # BigDecimal('3.14159').round(3) -> 3.142
  #
  # BigDecimal('13345.234').round(-2) -> 13300.0
  #
  # The value of the optional mode argument can be used to determine how
  # rounding is performed; see BigDecimal.mode.
  def round(n = 0, mode = BigDecimal::ROUND_HALF_UP) end

  # Returns the sign of the value.
  #
  # Returns a positive value if > 0, a negative value if < 0, and a
  # zero if == 0.
  #
  # The specific value returned indicates the type and sign of the BigDecimal,
  # as follows:
  #
  # BigDecimal::SIGN_NaN:: value is Not a Number
  # BigDecimal::SIGN_POSITIVE_ZERO:: value is +0
  # BigDecimal::SIGN_NEGATIVE_ZERO:: value is -0
  # BigDecimal::SIGN_POSITIVE_INFINITE:: value is +infinity
  # BigDecimal::SIGN_NEGATIVE_INFINITE:: value is -infinity
  # BigDecimal::SIGN_POSITIVE_FINITE:: value is positive
  # BigDecimal::SIGN_NEGATIVE_FINITE:: value is negative
  def sign; end

  # Splits a BigDecimal number into four parts, returned as an array of values.
  #
  # The first value represents the sign of the BigDecimal, and is -1 or 1, or 0
  # if the BigDecimal is Not a Number.
  #
  # The second value is a string representing the significant digits of the
  # BigDecimal, with no leading zeros.
  #
  # The third value is the base used for arithmetic (currently always 10) as an
  # Integer.
  #
  # The fourth value is an Integer exponent.
  #
  # If the BigDecimal can be represented as 0.xxxxxx*10**n, then xxxxxx is the
  # string of significant digits with no leading zeros, and n is the exponent.
  #
  # From these values, you can translate a BigDecimal to a float as follows:
  #
  #   sign, significant_digits, base, exponent = a.split
  #   f = sign * "0.#{significant_digits}".to_f * (base ** exponent)
  #
  # (Note that the to_f method is provided as a more convenient way to translate
  # a BigDecimal to a Float.)
  def split; end

  # Returns the square root of the value.
  #
  # If n is specified, returns at least that many significant digits.
  def sqrt(n) end

  def sub(p1, p2) end

  # Returns a new Float object having approximately the same value as the
  # BigDecimal number. Normal accuracy limits and built-in errors of binary
  # Float arithmetic apply.
  def to_f; end

  # Returns the value as an integer (Fixnum or Bignum).
  #
  # If the BigNumber is infinity or NaN, raises FloatDomainError.
  def to_i; end
  alias to_int to_i

  # Converts a BigDecimal to a Rational.
  def to_r; end

  # Converts the value to a string.
  #
  # The default format looks like  0.xxxxEnn.
  #
  # The optional parameter s consists of either an integer; or an optional '+'
  # or ' ', followed by an optional number, followed by an optional 'E' or 'F'.
  #
  # If there is a '+' at the start of s, positive values are returned with
  # a leading '+'.
  #
  # A space at the start of s returns positive values with a leading space.
  #
  # If s contains a number, a space is inserted after each group of that many
  # fractional digits.
  #
  # If s ends with an 'E', engineering notation (0.xxxxEnn) is used.
  #
  # If s ends with an 'F', conventional floating point notation is used.
  #
  # Examples:
  #
  # BigDecimal.new('-123.45678901234567890').to_s('5F') -> '-123.45678 90123 45678 9'
  #
  # BigDecimal.new('123.45678901234567890').to_s('+8F') -> '+123.45678901 23456789'
  #
  # BigDecimal.new('123.45678901234567890').to_s(' F') -> ' 123.4567890123456789'
  def to_s(s) end

  # Truncate to the nearest 1, returning the result as a BigDecimal.
  #
  # BigDecimal('3.14159').truncate -> 3
  #
  # BigDecimal('8.7').truncate -> 8
  #
  # If n is specified and positive, the fractional part of the result has no
  # more than that many digits.
  #
  # If n is specified and negative, at least that many digits to the left of the
  # decimal point will be 0 in the result.
  #
  # BigDecimal('3.14159').truncate(3) -> 3.141
  #
  # BigDecimal('13345.234').truncate(-2) -> 13300.0
  def truncate(n) end

  # Returns True if the value is zero.
  def zero?; end
end
