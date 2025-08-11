# frozen_string_literal: true

# BigDecimal provides arbitrary-precision floating point decimal arithmetic.
#
# == Introduction
#
# Ruby provides built-in support for arbitrary precision integer arithmetic.
#
# For example:
#
#      42**13  #=>   1265437718438866624512
#
# BigDecimal provides similar support for very large or very accurate floating
# point numbers.
#
# Decimal arithmetic is also useful for general calculation, because it
# provides the correct answers people expect--whereas normal binary floating
# point arithmetic often introduces subtle errors because of the conversion
# between base 10 and base 2.
#
# For example, try:
#
#   sum = 0
#   10_000.times do
#     sum = sum + 0.0001
#   end
#   print sum #=> 0.9999999999999062
#
# and contrast with the output from:
#
#   require 'bigdecimal'
#
#   sum = BigDecimal("0")
#   10_000.times do
#     sum = sum + BigDecimal("0.0001")
#   end
#   print sum #=> 0.1E1
#
# Similarly:
#
#      (BigDecimal("1.2") - BigDecimal("1.0")) == BigDecimal("0.2") #=> true
#
#      (1.2 - 1.0) == 0.2 #=> false
#
# == A Note About Precision
#
# For a calculation using a \BigDecimal and another +value+,
# the precision of the result depends on the type of +value+:
#
# - If +value+ is a \Float,
#   the precision is Float::DIG + 1.
# - If +value+ is a \Rational, the precision is larger than Float::DIG + 1.
# - If +value+ is a \BigDecimal, the precision is +value+'s precision in the
#   internal representation, which is platform-dependent.
# - If +value+ is other object, the precision is determined by the result of +BigDecimal(value)+.
#
# == Special features of accurate decimal arithmetic
#
# Because BigDecimal is more accurate than normal binary floating point
# arithmetic, it requires some special values.
#
# === Infinity
#
# BigDecimal sometimes needs to return infinity, for example if you divide
# a value by zero.
#
#      BigDecimal("1.0") / BigDecimal("0.0")  #=> Infinity
#      BigDecimal("-1.0") / BigDecimal("0.0")  #=> -Infinity
#
# You can represent infinite numbers to BigDecimal using the strings
# <code>'Infinity'</code>, <code>'+Infinity'</code> and
# <code>'-Infinity'</code> (case-sensitive)
#
# === Not a Number
#
# When a computation results in an undefined value, the special value +NaN+
# (for 'not a number') is returned.
#
# Example:
#
#      BigDecimal("0.0") / BigDecimal("0.0") #=> NaN
#
# You can also create undefined values.
#
# NaN is never considered to be the same as any other value, even NaN itself:
#
#      n = BigDecimal('NaN')
#      n == 0.0 #=> false
#      n == n #=> false
#
# === Positive and negative zero
#
# If a computation results in a value which is too small to be represented as
# a BigDecimal within the currently specified limits of precision, zero must
# be returned.
#
# If the value which is too small to be represented is negative, a BigDecimal
# value of negative zero is returned.
#
#      BigDecimal("1.0") / BigDecimal("-Infinity") #=> -0.0
#
# If the value is positive, a value of positive zero is returned.
#
#      BigDecimal("1.0") / BigDecimal("Infinity") #=> 0.0
#
# (See BigDecimal.mode for how to specify limits of precision.)
#
# Note that +-0.0+ and +0.0+ are considered to be the same for the purposes of
# comparison.
#
# Note also that in mathematics, there is no particular concept of negative
# or positive zero; true mathematical zero has no sign.
#
# == bigdecimal/util
#
# When you require +bigdecimal/util+, the #to_d method will be
# available on BigDecimal and the native Integer, Float, Rational,
# and String classes:
#
#      require 'bigdecimal/util'
#
#      42.to_d         # => 0.42e2
#      0.5.to_d        # => 0.5e0
#      (2/3r).to_d(3)  # => 0.667e0
#      "0.5".to_d      # => 0.5e0
#
# == License
#
# Copyright (C) 2002 by Shigeo Kobayashi <shigeo@tinyforest.gr.jp>.
#
# BigDecimal is released under the Ruby and 2-clause BSD licenses.
# See LICENSE.txt for details.
#
# Maintained by mrkn <mrkn@mrkn.jp> and ruby-core members.
#
# Documented by zzak <zachary@zacharyscott.net>, mathew <meta@pobox.com>, and
# many other contributors.
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
  # Special value constants
  INFINITY = _
  NAN = _
  # Round towards +Infinity. See BigDecimal.mode.
  ROUND_CEILING = _
  # Indicates that values should be rounded towards zero. See
  # BigDecimal.mode.
  ROUND_DOWN = _
  # Round towards -Infinity. See BigDecimal.mode.
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
  # The version of bigdecimal library
  VERSION = _

  # Internal method used to provide marshalling support. See the Marshal module.
  def self._load(p1) end

  #  Returns the number of digits a Float object is allowed to have;
  #  the result is system-dependent:
  #
  #    BigDecimal.double_fig # => 16
  def self.double_fig; end

  def self.interpret_loosely(p1) end

  #  Limit the number of significant digits in newly created BigDecimal
  #  numbers to the specified value. Rounding is performed as necessary,
  #  as specified by BigDecimal.mode.
  #
  #  A limit of 0, the default, means no upper limit.
  #
  #  The limit specified by this method takes less priority over any limit
  #  specified to instance methods such as ceil, floor, truncate, or round.
  def self.limit(digits = 0) end

  # Returns an integer representing the mode settings
  # for exception handling and rounding.
  #
  # These modes control exception handling:
  #
  # - \BigDecimal::EXCEPTION_NaN.
  # - \BigDecimal::EXCEPTION_INFINITY.
  # - \BigDecimal::EXCEPTION_UNDERFLOW.
  # - \BigDecimal::EXCEPTION_OVERFLOW.
  # - \BigDecimal::EXCEPTION_ZERODIVIDE.
  # - \BigDecimal::EXCEPTION_ALL.
  #
  # Values for +setting+ for exception handling:
  #
  # - +true+: sets the given +mode+ to +true+.
  # - +false+: sets the given +mode+ to +false+.
  # - +nil+: does not modify the mode settings.
  #
  # You can use method BigDecimal.save_exception_mode
  # to temporarily change, and then automatically restore, exception modes.
  #
  # For clarity, some examples below begin by setting all
  # exception modes to +false+.
  #
  # This mode controls the way rounding is to be performed:
  #
  # - \BigDecimal::ROUND_MODE
  #
  # You can use method BigDecimal.save_rounding_mode
  # to temporarily change, and then automatically restore, the rounding mode.
  #
  # <b>NaNs</b>
  #
  # Mode \BigDecimal::EXCEPTION_NaN controls behavior
  # when a \BigDecimal NaN is created.
  #
  # Settings:
  #
  # - +false+ (default): Returns <tt>BigDecimal('NaN')</tt>.
  # - +true+: Raises FloatDomainError.
  #
  # Examples:
  #
  #   BigDecimal.mode(BigDecimal::EXCEPTION_ALL, false) # => 0
  #   BigDecimal('NaN')                                 # => NaN
  #   BigDecimal.mode(BigDecimal::EXCEPTION_NaN, true)  # => 2
  #   BigDecimal('NaN') # Raises FloatDomainError
  #
  # <b>Infinities</b>
  #
  # Mode \BigDecimal::EXCEPTION_INFINITY controls behavior
  # when a \BigDecimal Infinity or -Infinity is created.
  # Settings:
  #
  # - +false+ (default): Returns <tt>BigDecimal('Infinity')</tt>
  #   or <tt>BigDecimal('-Infinity')</tt>.
  # - +true+: Raises FloatDomainError.
  #
  # Examples:
  #
  #   BigDecimal.mode(BigDecimal::EXCEPTION_ALL, false)     # => 0
  #   BigDecimal('Infinity')                                # => Infinity
  #   BigDecimal('-Infinity')                               # => -Infinity
  #   BigDecimal.mode(BigDecimal::EXCEPTION_INFINITY, true) # => 1
  #   BigDecimal('Infinity')  # Raises FloatDomainError
  #   BigDecimal('-Infinity') # Raises FloatDomainError
  #
  # <b>Underflow</b>
  #
  # Mode \BigDecimal::EXCEPTION_UNDERFLOW controls behavior
  # when a \BigDecimal underflow occurs.
  # Settings:
  #
  # - +false+ (default): Returns <tt>BigDecimal('0')</tt>
  #   or <tt>BigDecimal('-Infinity')</tt>.
  # - +true+: Raises FloatDomainError.
  #
  # Examples:
  #
  #   BigDecimal.mode(BigDecimal::EXCEPTION_ALL, false)      # => 0
  #   def flow_under
  #     x = BigDecimal('0.1')
  #     100.times { x *= x }
  #   end
  #   flow_under                                             # => 100
  #   BigDecimal.mode(BigDecimal::EXCEPTION_UNDERFLOW, true) # => 4
  #   flow_under # Raises FloatDomainError
  #
  # <b>Overflow</b>
  #
  # Mode \BigDecimal::EXCEPTION_OVERFLOW controls behavior
  # when a \BigDecimal overflow occurs.
  # Settings:
  #
  # - +false+ (default): Returns <tt>BigDecimal('Infinity')</tt>
  #   or <tt>BigDecimal('-Infinity')</tt>.
  # - +true+: Raises FloatDomainError.
  #
  # Examples:
  #
  #   BigDecimal.mode(BigDecimal::EXCEPTION_ALL, false)     # => 0
  #   def flow_over
  #     x = BigDecimal('10')
  #     100.times { x *= x }
  #   end
  #   flow_over                                             # => 100
  #   BigDecimal.mode(BigDecimal::EXCEPTION_OVERFLOW, true) # => 1
  #   flow_over # Raises FloatDomainError
  #
  # <b>Zero Division</b>
  #
  # Mode \BigDecimal::EXCEPTION_ZERODIVIDE controls behavior
  # when a zero-division occurs.
  # Settings:
  #
  # - +false+ (default): Returns <tt>BigDecimal('Infinity')</tt>
  #   or <tt>BigDecimal('-Infinity')</tt>.
  # - +true+: Raises FloatDomainError.
  #
  # Examples:
  #
  #   BigDecimal.mode(BigDecimal::EXCEPTION_ALL, false)       # => 0
  #   one = BigDecimal('1')
  #   zero = BigDecimal('0')
  #   one / zero                                              # => Infinity
  #   BigDecimal.mode(BigDecimal::EXCEPTION_ZERODIVIDE, true) # => 16
  #   one / zero # Raises FloatDomainError
  #
  # <b>All Exceptions</b>
  #
  # Mode \BigDecimal::EXCEPTION_ALL controls all of the above:
  #
  #   BigDecimal.mode(BigDecimal::EXCEPTION_ALL, false) # => 0
  #   BigDecimal.mode(BigDecimal::EXCEPTION_ALL, true)  # => 23
  #
  # <b>Rounding</b>
  #
  # Mode \BigDecimal::ROUND_MODE controls the way rounding is to be performed;
  # its +setting+ values are:
  #
  # - +ROUND_UP+: Round away from zero.
  #   Aliased as +:up+.
  # - +ROUND_DOWN+: Round toward zero.
  #   Aliased as +:down+ and +:truncate+.
  # - +ROUND_HALF_UP+: Round toward the nearest neighbor;
  #   if the neighbors are equidistant, round away from zero.
  #   Aliased as +:half_up+ and +:default+.
  # - +ROUND_HALF_DOWN+: Round toward the nearest neighbor;
  #   if the neighbors are equidistant, round toward zero.
  #   Aliased as +:half_down+.
  # - +ROUND_HALF_EVEN+ (Banker's rounding): Round toward the nearest neighbor;
  #   if the neighbors are equidistant, round toward the even neighbor.
  #   Aliased as +:half_even+ and +:banker+.
  # - +ROUND_CEILING+: Round toward positive infinity.
  #   Aliased as +:ceiling+ and +:ceil+.
  # - +ROUND_FLOOR+: Round toward negative infinity.
  #   Aliased as +:floor:+.
  def self.mode(mode, setting = nil) end

  # Execute the provided block, but preserve the exception mode
  #
  #     BigDecimal.save_exception_mode do
  #       BigDecimal.mode(BigDecimal::EXCEPTION_OVERFLOW, false)
  #       BigDecimal.mode(BigDecimal::EXCEPTION_NaN, false)
  #
  #       BigDecimal(BigDecimal('Infinity'))
  #       BigDecimal(BigDecimal('-Infinity'))
  #       BigDecimal(BigDecimal('NaN'))
  #     end
  #
  # For use with the BigDecimal::EXCEPTION_*
  #
  # See BigDecimal.mode
  def self.save_exception_mode; end

  # Execute the provided block, but preserve the precision limit
  #
  #      BigDecimal.limit(100)
  #      puts BigDecimal.limit
  #      BigDecimal.save_limit do
  #          BigDecimal.limit(200)
  #          puts BigDecimal.limit
  #      end
  #      puts BigDecimal.limit
  def self.save_limit; end

  # Execute the provided block, but preserve the rounding mode
  #
  #     BigDecimal.save_rounding_mode do
  #       BigDecimal.mode(BigDecimal::ROUND_MODE, :up)
  #       puts BigDecimal.mode(BigDecimal::ROUND_MODE)
  #     end
  #
  # For use with the BigDecimal::ROUND_*
  #
  # See BigDecimal.mode
  def self.save_rounding_mode; end

  # Returns the modulus from dividing by b.
  #
  # See BigDecimal#divmod.
  def %(other) end
  alias modulo %

  def *(other) end

  # Returns the \BigDecimal value of +self+ raised to power +other+:
  #
  #   b = BigDecimal('3.14')
  #   b ** 2              # => 0.98596e1
  #   b ** 2.0            # => 0.98596e1
  #   b ** Rational(2, 1) # => 0.98596e1
  #
  # Related: BigDecimal#power.
  def **(other) end

  # Returns the \BigDecimal sum of +self+ and +value+:
  #
  #   b = BigDecimal('111111.111') # => 0.111111111e6
  #   b + 2                        # => 0.111113111e6
  #   b + 2.0                      # => 0.111113111e6
  #   b + Rational(2, 1)           # => 0.111113111e6
  #   b + Complex(2, 0)            # => (0.111113111e6+0i)
  #
  # See the {Note About Precision}[BigDecimal.html#class-BigDecimal-label-A+Note+About+Precision].
  def +(other) end

  # Returns +self+:
  #
  #    +BigDecimal(5)  # => 0.5e1
  #    +BigDecimal(-5) # => -0.5e1
  def +@; end

  #  Returns the \BigDecimal difference of +self+ and +value+:
  #
  #    b = BigDecimal('333333.333') # => 0.333333333e6
  #    b - 2                        # => 0.333331333e6
  #    b - 2.0                      # => 0.333331333e6
  #    b - Rational(2, 1)           # => 0.333331333e6
  #    b - Complex(2, 0)            # => (0.333331333e6+0i)
  #
  #  See the {Note About Precision}[BigDecimal.html#class-BigDecimal-label-A+Note+About+Precision].
  def -(other) end

  # Returns the \BigDecimal negation of self:
  #
  #   b0 = BigDecimal('1.5')
  #   b1 = -b0 # => -0.15e1
  #   b2 = -b1 # => 0.15e1
  def -@; end

  # Divide by the specified value.
  #
  # The result precision will be the precision of the larger operand,
  # but its minimum is 2*Float::DIG.
  #
  # See BigDecimal#div.
  # See BigDecimal#quo.
  def /(other) end

  # Returns +true+ if +self+ is less than +other+, +false+ otherwise:
  #
  #   b = BigDecimal('1.5') # => 0.15e1
  #   b < 2                 # => true
  #   b < 2.0               # => true
  #   b < Rational(2, 1)    # => true
  #   b < 1.5               # => false
  #
  # Raises an exception if the comparison cannot be made.
  def <(other) end

  # Returns +true+ if +self+ is less or equal to than +other+, +false+ otherwise:
  #
  #   b = BigDecimal('1.5') # => 0.15e1
  #   b <= 2                # => true
  #   b <= 2.0              # => true
  #   b <= Rational(2, 1)   # => true
  #   b <= 1.5              # => true
  #   b < 1                 # => false
  #
  # Raises an exception if the comparison cannot be made.
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
  #   BigDecimal('1.0') == 1.0  #=> true
  def ==(other) end
  alias === ==
  alias eql? ==

  # Returns +true+ if +self+ is greater than +other+, +false+ otherwise:
  #
  #   b = BigDecimal('1.5')
  #   b > 1              # => true
  #   b > 1.0            # => true
  #   b > Rational(1, 1) # => true
  #   b > 2              # => false
  #
  # Raises an exception if the comparison cannot be made.
  def >(other) end

  # Returns +true+ if +self+ is greater than or equal to +other+, +false+ otherwise:
  #
  #   b = BigDecimal('1.5')
  #   b >= 1              # => true
  #   b >= 1.0            # => true
  #   b >= Rational(1, 1) # => true
  #   b >= 1.5            # => true
  #   b > 2               # => false
  #
  # Raises an exception if the comparison cannot be made.
  def >=(other) end

  # Returns a string representing the marshalling of +self+.
  # See module Marshal.
  #
  #   inf = BigDecimal('Infinity') # => Infinity
  #   dumped = inf._dump           # => "9:Infinity"
  #   BigDecimal._load(dumped)     # => Infinity
  def _dump; end

  # Returns the \BigDecimal absolute value of +self+:
  #
  #   BigDecimal('5').abs  # => 0.5e1
  #   BigDecimal('-3').abs # => 0.3e1
  def abs; end

  # Returns the \BigDecimal sum of +self+ and +value+
  # with a precision of +ndigits+ decimal digits.
  #
  # When +ndigits+ is less than the number of significant digits
  # in the sum, the sum is rounded to that number of digits,
  # according to the current rounding mode; see BigDecimal.mode.
  #
  # Examples:
  #
  #   # Set the rounding mode.
  #   BigDecimal.mode(BigDecimal::ROUND_MODE, :half_up)
  #   b = BigDecimal('111111.111')
  #   b.add(1, 0)               # => 0.111112111e6
  #   b.add(1, 3)               # => 0.111e6
  #   b.add(1, 6)               # => 0.111112e6
  #   b.add(1, 15)              # => 0.111112111e6
  #   b.add(1.0, 15)            # => 0.111112111e6
  #   b.add(Rational(1, 1), 15) # => 0.111112111e6
  def add(value, ndigits) end

  # Return the smallest integer greater than or equal to the value, as a BigDecimal.
  #
  #      BigDecimal('3.14159').ceil #=> 4
  #      BigDecimal('-9.1').ceil #=> -9
  #
  # If n is specified and positive, the fractional part of the result has no
  # more than that many digits.
  #
  # If n is specified and negative, at least that
  # many digits to the left of the decimal point will be 0 in the result.
  #
  #      BigDecimal('3.14159').ceil(3) #=> 3.142
  #      BigDecimal('13345.234').ceil(-2) #=> 13400.0
  def ceil(n = 0) end

  def clone; end
  alias dup clone

  # The coerce method provides support for Ruby type coercion. It is not
  # enabled by default.
  #
  # This means that binary operations like + * / or - can often be performed
  # on a BigDecimal and an object of another type, if the other object can
  # be coerced into a BigDecimal value.
  #
  # e.g.
  #   a = BigDecimal("1.0")
  #   b = a / 2.0 #=> 0.5
  #
  # Note that coercing a String to a BigDecimal is not supported by default;
  # it requires a special compile-time option when building Ruby.
  def coerce(p1) end

  # Divide by the specified value.
  #
  # digits:: If specified and less than the number of significant digits of the
  #          result, the result is rounded to that number of digits, according
  #          to BigDecimal.mode.
  #
  #          If digits is 0, the result is the same as for the / operator
  #          or #quo.
  #
  #          If digits is not specified, the result is an integer,
  #          by analogy with Float#div; see also BigDecimal#divmod.
  #
  # See BigDecimal#/.
  # See BigDecimal#quo.
  #
  # Examples:
  #
  #   a = BigDecimal("4")
  #   b = BigDecimal("3")
  #
  #   a.div(b, 3)  # => 0.133e1
  #
  #   a.div(b, 0)  # => 0.1333333333333333333e1
  #   a / b        # => 0.1333333333333333333e1
  #   a.quo(b)     # => 0.1333333333333333333e1
  #
  #   a.div(b)     # => 1
  def div(...) end

  # Divides by the specified value, and returns the quotient and modulus
  # as BigDecimal numbers. The quotient is rounded towards negative infinity.
  #
  # For example:
  #
  #   require 'bigdecimal'
  #
  #   a = BigDecimal("42")
  #   b = BigDecimal("9")
  #
  #   q, m = a.divmod(b)
  #
  #   c = q * b + m
  #
  #   a == c  #=> true
  #
  # The quotient q is (a/b).floor, and the modulus is the amount that must be
  # added to q * b to get a.
  def divmod(value) end

  # Returns the exponent of the BigDecimal number, as an Integer.
  #
  # If the number can be represented as 0.xxxxxx*10**n where xxxxxx is a string
  # of digits with no leading zeros, then n is the exponent.
  def exponent; end

  # Returns True if the value is finite (not NaN or infinite).
  def finite?; end

  # Return the integer part of the number, as a BigDecimal.
  def fix; end

  # Return the largest integer less than or equal to the value, as a BigDecimal.
  #
  #      BigDecimal('3.14159').floor #=> 3
  #      BigDecimal('-9.1').floor #=> -10
  #
  # If n is specified and positive, the fractional part of the result has no
  # more than that many digits.
  #
  # If n is specified and negative, at least that
  # many digits to the left of the decimal point will be 0 in the result.
  #
  #      BigDecimal('3.14159').floor(3) #=> 3.141
  #      BigDecimal('13345.234').floor(-2) #=> 13300.0
  def floor(n = 0) end

  # Return the fractional part of the number, as a BigDecimal.
  def frac; end

  # Returns the integer hash value for +self+.
  #
  # Two instances of \BigDecimal have the same hash value if and only if
  # they have equal:
  #
  # - Sign.
  # - Fractional part.
  # - Exponent.
  def hash; end

  # Returns nil, -1, or +1 depending on whether the value is finite,
  # -Infinity, or +Infinity.
  def infinite?; end

  # Returns a string representation of self.
  #
  #   BigDecimal("1234.5678").inspect
  #     #=> "0.12345678e4"
  def inspect; end

  # Returns the \BigDecimal product of +self+ and +value+
  # with a precision of +ndigits+ decimal digits.
  #
  # When +ndigits+ is less than the number of significant digits
  # in the sum, the sum is rounded to that number of digits,
  # according to the current rounding mode; see BigDecimal.mode.
  #
  # Examples:
  #
  #   # Set the rounding mode.
  #   BigDecimal.mode(BigDecimal::ROUND_MODE, :half_up)
  #   b = BigDecimal('555555.555')
  #   b.mult(3, 0)              # => 0.1666666665e7
  #   b.mult(3, 3)              # => 0.167e7
  #   b.mult(3, 6)              # => 0.166667e7
  #   b.mult(3, 15)             # => 0.1666666665e7
  #   b.mult(3.0, 0)            # => 0.1666666665e7
  #   b.mult(Rational(3, 1), 0) # => 0.1666666665e7
  #   b.mult(Complex(3, 0), 0)  # => (0.1666666665e7+0.0i)
  def mult(other, ndigits) end

  # Returns the number of decimal significant digits in +self+.
  #
  #   BigDecimal("0").n_significant_digits         # => 0
  #   BigDecimal("1").n_significant_digits         # => 1
  #   BigDecimal("1.1").n_significant_digits       # => 2
  #   BigDecimal("3.1415").n_significant_digits    # => 5
  #   BigDecimal("-1e20").n_significant_digits     # => 1
  #   BigDecimal("1e-20").n_significant_digits     # => 1
  #   BigDecimal("Infinity").n_significant_digits  # => 0
  #   BigDecimal("-Infinity").n_significant_digits # => 0
  #   BigDecimal("NaN").n_significant_digits       # => 0
  def n_significant_digits; end

  # Returns True if the value is Not a Number.
  def nan?; end

  # Returns self if the value is non-zero, nil otherwise.
  def nonzero?; end

  # Returns the value raised to the power of n.
  #
  # Note that n must be an Integer.
  #
  # Also available as the operator **.
  def power(...) end

  # Returns the number of decimal digits in +self+:
  #
  #   BigDecimal("0").precision         # => 0
  #   BigDecimal("1").precision         # => 1
  #   BigDecimal("1.1").precision       # => 2
  #   BigDecimal("3.1415").precision    # => 5
  #   BigDecimal("-1e20").precision     # => 21
  #   BigDecimal("1e-20").precision     # => 20
  #   BigDecimal("Infinity").precision  # => 0
  #   BigDecimal("-Infinity").precision # => 0
  #   BigDecimal("NaN").precision       # => 0
  def precision; end

  # Returns a 2-length array; the first item is the result of
  # BigDecimal#precision and the second one is of BigDecimal#scale.
  #
  # See BigDecimal#precision.
  # See BigDecimal#scale.
  def precision_scale; end

  # Returns an Array of two Integer values that represent platform-dependent
  # internal storage properties.
  #
  # This method is deprecated and will be removed in the future.
  # Instead, use BigDecimal#n_significant_digits for obtaining the number of
  # significant digits in scientific notation, and BigDecimal#precision for
  # obtaining the number of digits in decimal notation.
  def precs; end

  # Divide by the specified value.
  #
  # digits:: If specified and less than the number of significant digits of
  #          the result, the result is rounded to the given number of digits,
  #          according to the rounding mode indicated by BigDecimal.mode.
  #
  #          If digits is 0 or omitted, the result is the same as for the
  #          / operator.
  #
  # See BigDecimal#/.
  # See BigDecimal#div.
  def quo(...) end

  # Returns the remainder from dividing by the value.
  #
  # x.remainder(y) means x-y*(x/y).truncate
  def remainder(value) end

  # Round to the nearest integer (by default), returning the result as a
  # BigDecimal if n is specified, or as an Integer if it isn't.
  #
  #      BigDecimal('3.14159').round #=> 3
  #      BigDecimal('8.7').round #=> 9
  #      BigDecimal('-9.9').round #=> -10
  #
  #      BigDecimal('3.14159').round(2).class.name #=> "BigDecimal"
  #      BigDecimal('3.14159').round.class.name #=> "Integer"
  #
  # If n is specified and positive, the fractional part of the result has no
  # more than that many digits.
  #
  # If n is specified and negative, at least that many digits to the left of the
  # decimal point will be 0 in the result, and return value will be an Integer.
  #
  #      BigDecimal('3.14159').round(3) #=> 3.142
  #      BigDecimal('13345.234').round(-2) #=> 13300
  #
  # The value of the optional mode argument can be used to determine how
  # rounding is performed; see BigDecimal.mode.
  def round(n = 0, mode = BigDecimal::ROUND_HALF_UP) end

  # Returns the number of decimal digits following the decimal digits in +self+.
  #
  #   BigDecimal("0").scale         # => 0
  #   BigDecimal("1").scale         # => 1
  #   BigDecimal("1.1").scale       # => 1
  #   BigDecimal("3.1415").scale    # => 4
  #   BigDecimal("-1e20").precision # => 0
  #   BigDecimal("1e-20").precision # => 20
  #   BigDecimal("Infinity").scale  # => 0
  #   BigDecimal("-Infinity").scale # => 0
  #   BigDecimal("NaN").scale       # => 0
  def scale; end

  # Returns the sign of the value.
  #
  # Returns a positive value if > 0, a negative value if < 0.
  # It behaves the same with zeros -
  # it returns a positive value for a positive zero (BigDecimal('0')) and
  # a negative value for a negative zero (BigDecimal('-0')).
  #
  # The specific value returned indicates the type and sign of the BigDecimal,
  # as follows:
  #
  # BigDecimal::SIGN_NaN:: value is Not a Number
  # BigDecimal::SIGN_POSITIVE_ZERO:: value is +0
  # BigDecimal::SIGN_NEGATIVE_ZERO:: value is -0
  # BigDecimal::SIGN_POSITIVE_INFINITE:: value is +Infinity
  # BigDecimal::SIGN_NEGATIVE_INFINITE:: value is -Infinity
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
  # Result has at least n significant digits.
  def sqrt(n) end

  # Subtract the specified value.
  #
  # e.g.
  #   c = a.sub(b,n)
  #
  # digits:: If specified and less than the number of significant digits of the
  #          result, the result is rounded to that number of digits, according
  #          to BigDecimal.mode.
  def sub(value, digits) end

  # Returns a new Float object having approximately the same value as the
  # BigDecimal number. Normal accuracy limits and built-in errors of binary
  # Float arithmetic apply.
  def to_f; end

  # Returns the value as an Integer.
  #
  # If the BigDecimal is infinity or NaN, raises FloatDomainError.
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
  #   BigDecimal('-123.45678901234567890').to_s('5F')
  #     #=> '-123.45678 90123 45678 9'
  #
  #   BigDecimal('123.45678901234567890').to_s('+8F')
  #     #=> '+123.45678901 23456789'
  #
  #   BigDecimal('123.45678901234567890').to_s(' F')
  #     #=> ' 123.4567890123456789'
  def to_s(s) end

  # Truncate to the nearest integer (by default), returning the result as a
  # BigDecimal.
  #
  #      BigDecimal('3.14159').truncate #=> 3
  #      BigDecimal('8.7').truncate #=> 8
  #      BigDecimal('-9.9').truncate #=> -9
  #
  # If n is specified and positive, the fractional part of the result has no
  # more than that many digits.
  #
  # If n is specified and negative, at least that many digits to the left of the
  # decimal point will be 0 in the result.
  #
  #      BigDecimal('3.14159').truncate(3) #=> 3.141
  #      BigDecimal('13345.234').truncate(-2) #=> 13300.0
  def truncate(n) end

  # Returns True if the value is zero.
  def zero?; end
end
