# frozen_string_literal: true

# Numeric is the class from which all higher-level numeric classes should inherit.
#
# Numeric allows instantiation of heap-allocated objects. Other core numeric classes such as
# Integer are implemented as immediates, which means that each Integer is a single immutable
# object which is always passed by value.
#
#   a = 1
#   1.object_id == a.object_id   #=> true
#
# There can only ever be one instance of the integer +1+, for example. Ruby ensures this
# by preventing instantiation. If duplication is attempted, the same instance is returned.
#
#   Integer.new(1)                   #=> NoMethodError: undefined method `new' for Integer:Class
#   1.dup                            #=> 1
#   1.object_id == 1.dup.object_id   #=> true
#
# For this reason, Numeric should be used when defining other numeric classes.
#
# Classes which inherit from Numeric must implement +coerce+, which returns a two-member
# Array containing an object that has been coerced into an instance of the new class
# and +self+ (see #coerce).
#
# Inheriting classes should also implement arithmetic operator methods (<code>+</code>,
# <code>-</code>, <code>*</code> and <code>/</code>) and the <code><=></code> operator (see
# Comparable). These methods may rely on +coerce+ to ensure interoperability with
# instances of other numeric classes.
#
#   class Tally < Numeric
#     def initialize(string)
#       @string = string
#     end
#
#     def to_s
#       @string
#     end
#
#     def to_i
#       @string.size
#     end
#
#     def coerce(other)
#       [self.class.new('|' * other.to_i), self]
#     end
#
#     def <=>(other)
#       to_i <=> other.to_i
#     end
#
#     def +(other)
#       self.class.new('|' * (to_i + other.to_i))
#     end
#
#     def -(other)
#       self.class.new('|' * (to_i - other.to_i))
#     end
#
#     def *(other)
#       self.class.new('|' * (to_i * other.to_i))
#     end
#
#     def /(other)
#       self.class.new('|' * (to_i / other.to_i))
#     end
#   end
#
#   tally = Tally.new('||')
#   puts tally * 2            #=> "||||"
#   puts tally > 1            #=> true
class Numeric
  include Comparable

  # <code>x.modulo(y)</code> means <code>x-y*(x/y).floor</code>.
  #
  # Equivalent to <code>num.divmod(numeric)[1]</code>.
  #
  # See Numeric#divmod.
  def %(other) end
  alias modulo %

  # Unary Plus---Returns the receiver.
  def +@; end

  # Unary Minus---Returns the receiver, negated.
  def -@; end

  # Returns zero if +number+ equals +other+, otherwise returns +nil+.
  def <=>(other) end

  # Returns the absolute value of +num+.
  #
  #    12.abs         #=> 12
  #    (-34.56).abs   #=> 34.56
  #    -34.56.abs     #=> 34.56
  #
  # Numeric#magnitude is an alias for Numeric#abs.
  def abs; end
  alias magnitude abs

  # Returns square of self.
  def abs2; end

  # Returns 0 if the value is positive, pi otherwise.
  def arg; end
  alias angle arg
  alias phase arg

  # Returns the smallest number greater than or equal to +num+ with
  # a precision of +ndigits+ decimal digits (default: 0).
  #
  # Numeric implements this by converting its value to a Float and
  # invoking Float#ceil.
  def ceil(*ndigits) end

  # Returns the receiver.  +freeze+ cannot be +false+.
  def clone(freeze: true) end

  # If +numeric+ is the same type as +num+, returns an array
  # <code>[numeric, num]</code>. Otherwise, returns an array with both
  # +numeric+ and +num+ represented as Float objects.
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
  # Numeric does not define the +/+ operator; this is left to subclasses.
  #
  # Equivalent to <code>num.divmod(numeric)[0]</code>.
  #
  # See Numeric#divmod.
  def div(numeric) end

  # Returns an array containing the quotient and modulus obtained by dividing
  # +num+ by +numeric+.
  #
  # If <code>q, r = x.divmod(y)</code>, then
  #
  #     q = floor(x/y)
  #     x = q*y + r
  #
  # The quotient is rounded toward negative infinity, as shown in the
  # following table:
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
  #    11.divmod(3)        #=> [3, 2]
  #    11.divmod(-3)       #=> [-4, -1]
  #    11.divmod(3.5)      #=> [3, 0.5]
  #    (-11).divmod(3.5)   #=> [-4, 3.0]
  #    11.5.divmod(3.5)    #=> [3, 1.0]
  def divmod(numeric) end

  # Returns the receiver.
  def dup; end

  # Returns +true+ if +num+ and +numeric+ are the same type and have equal
  # values.  Contrast this with Numeric#==, which performs type conversions.
  #
  #    1 == 1.0        #=> true
  #    1.eql?(1.0)     #=> false
  #    1.0.eql?(1.0)   #=> true
  def eql?(other) end

  # Returns float division.
  def fdiv(numeric) end

  # Returns +true+ if +num+ is a finite number, otherwise returns +false+.
  def finite?; end

  # Returns the largest number less than or equal to +num+ with
  # a precision of +ndigits+ decimal digits (default: 0).
  #
  # Numeric implements this by converting its value to a Float and
  # invoking Float#floor.
  def floor(*ndigits) end

  # Returns the corresponding imaginary number.
  # Not available for complex numbers.
  #
  #    -42.i  #=> (0-42i)
  #    2.0.i  #=> (0+2.0i)
  def i; end

  # Returns zero.
  def imaginary; end
  alias imag imaginary

  # Returns +nil+, -1, or 1 depending on whether the value is
  # finite, <code>-Infinity</code>, or <code>+Infinity</code>.
  def infinite?; end

  # Returns +true+ if +num+ is an Integer.
  #
  #     1.0.integer?   #=> false
  #     1.integer?     #=> true
  def integer?; end

  # Returns +true+ if +num+ is less than 0.
  def negative?; end

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

  # Returns +true+ if +num+ is greater than 0.
  def positive?; end

  # Returns the most exact division (rational for integers, float for floats).
  def quo(*several_variants) end

  # Returns self.
  def real; end

  # Returns +true+ if +num+ is a real number (i.e. not Complex).
  def real?; end

  # Returns an array; [num, 0].
  def rectangular; end
  alias rect rectangular

  # <code>x.remainder(y)</code> means <code>x-y*(x/y).truncate</code>.
  #
  # See Numeric#divmod.
  def remainder(numeric) end

  # Returns +num+ rounded to the nearest value with
  # a precision of +ndigits+ decimal digits (default: 0).
  #
  # Numeric implements this by converting its value to a Float and
  # invoking Float#round.
  def round(*ndigits) end

  # Invokes the given block with the sequence of numbers starting at +num+,
  # incremented by +step+ (defaulted to +1+) on each call.
  #
  # The loop finishes when the value to be passed to the block is greater than
  # +limit+ (if +step+ is positive) or less than +limit+ (if +step+ is
  # negative), where +limit+ is defaulted to infinity.
  #
  # In the recommended keyword argument style, either or both of
  # +step+ and +limit+ (default infinity) can be omitted.  In the
  # fixed position argument style, zero as a step
  # (i.e. <code>num.step(limit, 0)</code>) is not allowed for historical
  # compatibility reasons.
  #
  # If all the arguments are integers, the loop operates using an integer
  # counter.
  #
  # If any of the arguments are floating point numbers, all are converted
  # to floats, and the loop is executed
  # <i>floor(n + n*Float::EPSILON) + 1</i> times,
  # where <i>n = (limit - num)/step</i>.
  #
  # Otherwise, the loop starts at +num+, uses either the
  # less-than (<code><</code>) or greater-than (<code>></code>) operator
  # to compare the counter against +limit+,
  # and increments itself using the <code>+</code> operator.
  #
  # If no block is given, an Enumerator is returned instead.
  #
  # For example:
  #
  #    p 1.step.take(4)
  #    p 10.step(by: -1).take(4)
  #    3.step(to: 5) {|i| print i, " " }
  #    1.step(10, 2) {|i| print i, " " }
  #    Math::E.step(to: Math::PI, by: 0.2) {|f| print f, " " }
  #
  # Will produce:
  #
  #    [1, 2, 3, 4]
  #    [10, 9, 8, 7]
  #    3 4 5
  #    1 3 5 7 9
  #    2.718281828459045 2.9182818284590453 3.118281828459045
  def step(*several_variants) end

  # Returns the value as a complex.
  def to_c; end

  # Invokes the child class's +to_i+ method to convert +num+ to an integer.
  #
  #     1.0.class          #=> Float
  #     1.0.to_int.class   #=> Integer
  #     1.0.to_i.class     #=> Integer
  def to_int; end

  # Returns +num+ truncated (toward zero) to
  # a precision of +ndigits+ decimal digits (default: 0).
  #
  # Numeric implements this by converting its value to a Float and
  # invoking Float#truncate.
  def truncate(*ndigits) end

  # Returns +true+ if +num+ has a zero value.
  def zero?; end
end
