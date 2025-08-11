# frozen_string_literal: true

# The top-level number class.
class Numeric
  include Comparable

  #    x.modulo(y) means x-y*(x/y).floor
  #
  # Equivalent to <code>num.divmod(numeric)[1]</code>.
  #
  # See Numeric#divmod.
  def %(other) end
  alias modulo %

  # Unary Plus---Returns the receiver's value.
  def +@; end

  # Unary Minus---Returns the receiver's value, negated.
  def -@; end

  # Returns zero if +number+ equals +other+, otherwise +nil+ is returned if the
  # two values are incomparable.
  def <=>(other) end

  # Returns the absolute value of +num+.
  #
  #    12.abs         #=> 12
  #    (-34.56).abs   #=> 34.56
  #    -34.56.abs     #=> 34.56
  #
  # Numeric#magnitude is an alias of Numeric#abs.
  def abs; end
  alias magnitude abs

  # Returns square of self.
  def abs2; end

  # Returns 0 if the value is positive, pi otherwise.
  def arg; end
  alias angle arg
  alias phase arg

  # Returns the smallest possible Integer that is greater than or equal to
  # +num+.
  #
  # Numeric achieves this by converting itself to a Float then invoking
  # Float#ceil.
  #
  #    1.ceil        #=> 1
  #    1.2.ceil      #=> 2
  #    (-1.2).ceil   #=> -1
  #    (-1.0).ceil   #=> -1
  def ceil; end

  # If a +numeric is the same type as +num+, returns an array containing
  # +numeric+ and +num+. Otherwise, returns an array with both a +numeric+ and
  # +num+ represented as Float objects.
  #
  # This coercion mechanism is used by Ruby to handle mixed-type numeric
  # operations: it is intended to find a compatible common type between the two
  # operands of the operator.
  #
  #    1.coerce(2.5)   #=> [2.5, 1.0]
  #    1.2.coerce(3)   #=> [3.0, 1.2]
  #    1.coerce(2)     #=> [2, 1]
  def coerce(numeric) end

  # Returns self.
  def conjugate; end
  alias conj conjugate

  # Returns the denominator (always positive).
  def denominator; end

  # Uses +/+ to perform division, then converts the result to an integer.
  # +numeric+ does not define the +/+ operator; this is left to subclasses.
  #
  # Equivalent to <code>num.divmod(numeric)[0]</code>.
  #
  # See Numeric#divmod.
  def div(numeric) end

  # Returns an array containing the quotient and modulus obtained by dividing
  # +num+ by +numeric+.
  #
  # If <code>q, r = * x.divmod(y)</code>, then
  #
  #     q = floor(x/y)
  #     x = q*y+r
  #
  # The quotient is rounded toward -infinity, as shown in the following table:
  #
  #    a    |  b  |  a.divmod(b)  |   a/b   | a.modulo(b) | a.remainder(b)
  #   ------+-----+---------------+---------+-------------+---------------
  #    13   |  4  |   3,    1     |   3     |    1        |     1
  #   ------+-----+---------------+---------+-------------+---------------
  #    13   | -4  |  -4,   -3     |  -4     |   -3        |     1
  #   ------+-----+---------------+---------+-------------+---------------
  #   -13   |  4  |  -4,    3     |  -4     |    3        |    -1
  #   ------+-----+---------------+---------+-------------+---------------
  #   -13   | -4  |   3,   -1     |   3     |   -1        |    -1
  #   ------+-----+---------------+---------+-------------+---------------
  #    11.5 |  4  |   2,    3.5   |   2.875 |    3.5      |     3.5
  #   ------+-----+---------------+---------+-------------+---------------
  #    11.5 | -4  |  -3,   -0.5   |  -2.875 |   -0.5      |     3.5
  #   ------+-----+---------------+---------+-------------+---------------
  #   -11.5 |  4  |  -3,    0.5   |  -2.875 |    0.5      |    -3.5
  #   ------+-----+---------------+---------+-------------+---------------
  #   -11.5 | -4  |   2,   -3.5   |   2.875 |   -3.5      |    -3.5
  #
  # Examples
  #
  #    11.divmod(3)         #=> [3, 2]
  #    11.divmod(-3)        #=> [-4, -1]
  #    11.divmod(3.5)       #=> [3, 0.5]
  #    (-11).divmod(3.5)    #=> [-4, 3.0]
  #    (11.5).divmod(3.5)   #=> [3, 1.0]
  def divmod(numeric) end

  # Returns +true+ if +num+ and +numeric+ are the same type and have equal
  # values.
  #
  #    1 == 1.0          #=> true
  #    1.eql?(1.0)       #=> false
  #    (1.0).eql?(1.0)   #=> true
  def eql?(other) end

  # Returns float division.
  def fdiv(numeric) end

  # Returns the largest integer less than or equal to +num+.
  #
  # Numeric implements this by converting an Integer to a Float and invoking
  # Float#floor.
  #
  #    1.floor      #=> 1
  #    (-1).floor   #=> -1
  def floor; end

  # Returns the corresponding imaginary number.
  # Not available for complex numbers.
  def i; end

  # Returns zero.
  def imaginary; end
  alias imag imaginary

  # Numerics are immutable values, which should not be copied.
  #
  # Any attempt to use this method on a Numeric will raise a TypeError.
  def initialize_copy(p1) end

  # Returns +true+ if +num+ is an Integer (including Fixnum and Bignum).
  #
  #     (1.0).integer? #=> false
  #     (1).integer?   #=> true
  def integer?; end

  # Returns +self+ if +num+ is not zero, +nil+ otherwise.
  #
  # This behavior is useful when chaining comparisons:
  #
  #    a = %w( z Bb bB bb BB a aA Aa AA A )
  #    b = a.sort {|a,b| (a.downcase <=> b.downcase).nonzero? || a <=> b }
  #    b   #=> ["A", "a", "AA", "Aa", "aA", "BB", "Bb", "bB", "bb", "z"]
  def nonzero?; end

  # Returns the numerator.
  def numerator; end

  # Returns an array; [num.abs, num.arg].
  def polar; end

  # Returns most exact division (rational for integers, float for floats).
  def quo(*several_variants) end

  # Returns self.
  def real; end

  # Returns +true+ if +num+ is a Real number. (i.e. not Complex).
  def real?; end

  # Returns an array; [num, 0].
  def rectangular; end
  alias rect rectangular

  #    x.remainder(y) means x-y*(x/y).truncate
  #
  # See Numeric#divmod.
  def remainder(numeric) end

  # Rounds +num+ to a given precision in decimal digits (default 0 digits).
  #
  # Precision may be negative.  Returns a floating point number when +ndigits+
  # is more than zero.
  #
  # Numeric implements this by converting itself to a Float and invoking
  # Float#round.
  def round(*ndigits) end

  # Trap attempts to add methods to Numeric objects. Always raises a TypeError.
  #
  # Numerics should be values; singleton_methods should not be added to them.
  def singleton_method_added(p1) end

  # Invokes the given block with the sequence of numbers starting at +num+,
  # incremented by +step+ (defaulted to +1+) on each call.
  #
  # The loop finishes when the value to be passed to the block is greater than
  # +limit+ (if +step+ is positive) or less than +limit+ (if +step+ is
  # negative), where <i>limit</i> is defaulted to infinity.
  #
  # In the recommended keyword argument style, either or both of
  # +step+ and +limit+ (default infinity) can be omitted.  In the
  # fixed position argument style, zero as a step
  # (i.e. num.step(limit, 0)) is not allowed for historical
  # compatibility reasons.
  #
  # If all the arguments are integers, the loop operates using an integer
  # counter.
  #
  # If any of the arguments are floating point numbers, all are converted to floats, and the loop is executed the following expression:
  #
  #     floor(n + n*epsilon)+ 1
  #
  # Where the +n+ is the following:
  #
  #     n = (limit - num)/step
  #
  # Otherwise, the loop starts at +num+, uses either the less-than (<) or
  # greater-than (>) operator to compare the counter against +limit+, and
  # increments itself using the <code>+</code> operator.
  #
  # If no block is given, an Enumerator is returned instead.
  #
  # For example:
  #
  #    p 1.step.take(4)
  #    p 10.step(by: -1).take(4)
  #    3.step(to: 5) { |i| print i, " " }
  #    1.step(10, 2) { |i| print i, " " }
  #    Math::E.step(to: Math::PI, by: 0.2) { |f| print f, " " }
  #
  # Will produce:
  #
  #    [1, 2, 3, 4]
  #    [10, 9, 8, 7]
  #    3 4 5
  #    1 3 5 7 9
  #    2.71828182845905 2.91828182845905 3.11828182845905
  def step(*several_variants) end

  # Returns the value as a complex.
  def to_c; end

  # Invokes the child class's +to_i+ method to convert +num+ to an integer.
  #
  #     1.0.class => Float
  #     1.0.to_int.class => Fixnum
  #     1.0.to_i.class => Fixnum
  def to_int; end

  # Returns +num+ truncated to an Integer.
  #
  # Numeric implements this by converting its value to a Float and invoking
  # Float#truncate.
  def truncate; end

  # Returns +true+ if +num+ has a zero value.
  def zero?; end
end
