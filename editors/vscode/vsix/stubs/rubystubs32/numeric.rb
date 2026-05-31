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
#
# == What's Here
#
# First, what's elsewhere. \Class \Numeric:
#
# - Inherits from {class Object}[rdoc-ref:Object@What-27s+Here].
# - Includes {module Comparable}[rdoc-ref:Comparable@What-27s+Here].
#
# Here, class \Numeric provides methods for:
#
# - {Querying}[rdoc-ref:Numeric@Querying]
# - {Comparing}[rdoc-ref:Numeric@Comparing]
# - {Converting}[rdoc-ref:Numeric@Converting]
# - {Other}[rdoc-ref:Numeric@Other]
#
# === Querying
#
# - #finite?: Returns true unless +self+ is infinite or not a number.
# - #infinite?: Returns -1, +nil+ or +1, depending on whether +self+
#   is <tt>-Infinity<tt>, finite, or <tt>+Infinity</tt>.
# - #integer?: Returns whether +self+ is an integer.
# - #negative?: Returns whether +self+ is negative.
# - #nonzero?: Returns whether +self+ is not zero.
# - #positive?: Returns whether +self+ is positive.
# - #real?: Returns whether +self+ is a real value.
# - #zero?: Returns whether +self+ is zero.
#
# === Comparing
#
# - #<=>: Returns:
#
#   - -1 if  +self+ is less than the given value.
#   - 0 if +self+ is equal to the given value.
#   - 1 if +self+ is greater than the given value.
#   - +nil+ if +self+ and the given value are not comparable.
#
# - #eql?: Returns whether +self+ and the given value have the same value and type.
#
# === Converting
#
# - #% (aliased as #modulo): Returns the remainder of +self+ divided by the given value.
# - #-@: Returns the value of +self+, negated.
# - #abs (aliased as #magnitude): Returns the absolute value of +self+.
# - #abs2: Returns the square of +self+.
# - #angle (aliased as #arg and #phase): Returns 0 if +self+ is positive,
#   Math::PI otherwise.
# - #ceil: Returns the smallest number greater than or equal to +self+,
#   to a given precision.
# - #coerce: Returns array <tt>[coerced_self, coerced_other]</tt>
#   for the given other value.
# - #conj (aliased as #conjugate): Returns the complex conjugate of +self+.
# - #denominator: Returns the denominator (always positive)
#   of the Rational representation of +self+.
# - #div: Returns the value of +self+ divided by the given value
#   and converted to an integer.
# - #divmod: Returns array <tt>[quotient, modulus]</tt> resulting
#   from dividing +self+ the given divisor.
# - #fdiv: Returns the Float result of dividing +self+ by the given divisor.
# - #floor: Returns the largest number less than or equal to +self+,
#   to a given precision.
# - #i: Returns the Complex object <tt>Complex(0, self)</tt>.
#   the given value.
# - #imaginary (aliased as #imag): Returns the imaginary part of the +self+.
# - #numerator: Returns the numerator of the Rational representation of +self+;
#   has the same sign as +self+.
# - #polar: Returns the array <tt>[self.abs, self.arg]</tt>.
# - #quo: Returns the value of +self+ divided by the given value.
# - #real: Returns the real part of +self+.
# - #rect (aliased as #rectangular): Returns the array <tt>[self, 0]</tt>.
# - #remainder: Returns <tt>self-arg*(self/arg).truncate</tt> for the given +arg+.
# - #round: Returns the value of +self+ rounded to the nearest value
#   for the given a precision.
# - #to_c: Returns the Complex representation of +self+.
# - #to_int: Returns the Integer representation of +self+, truncating if necessary.
# - #truncate: Returns +self+ truncated (toward zero) to a given precision.
#
# === Other
#
# - #clone: Returns +self+; does not allow freezing.
# - #dup (aliased as #+@): Returns +self+.
# - #step: Invokes the given block with the sequence of specified numbers.
class Numeric
  include Comparable

  # Returns +self+ modulo +other+ as a real number.
  #
  # Of the Core and Standard Library classes,
  # only Rational uses this implementation.
  #
  # For \Rational +r+ and real number +n+, these expressions are equivalent:
  #
  #   r % n
  #   r-n*(r/n).floor
  #   r.divmod(n)[1]
  #
  # See Numeric#divmod.
  #
  # Examples:
  #
  #   r = Rational(1, 2)    # => (1/2)
  #   r2 = Rational(2, 3)   # => (2/3)
  #   r % r2                # => (1/2)
  #   r % 2                 # => (1/2)
  #   r % 2.0               # => 0.5
  #
  #   r = Rational(301,100) # => (301/100)
  #   r2 = Rational(7,5)    # => (7/5)
  #   r % r2                # => (21/100)
  #   r % -r2               # => (-119/100)
  #   (-r) % r2             # => (119/100)
  #   (-r) %-r2             # => (-21/100)
  #
  # Numeric#modulo is an alias for Numeric#%.
  def %(other) end
  alias modulo %

  # Returns +self+.
  def +@; end

  # Unary Minus---Returns the receiver, negated.
  def -@; end

  # Returns zero if +self+ is the same as +other+, +nil+ otherwise.
  #
  # No subclass in the Ruby Core or Standard Library uses this implementation.
  def <=>(other) end

  # Returns the absolute value of +self+.
  #
  #   12.abs        #=> 12
  #   (-34.56).abs  #=> 34.56
  #   -34.56.abs    #=> 34.56
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

  # Returns the smallest number that is greater than or equal to +self+ with
  # a precision of +digits+ decimal digits.
  #
  # \Numeric implements this by converting +self+ to a Float and
  # invoking Float#ceil.
  def ceil(digits = 0) end

  # Returns +self+.
  #
  # Raises an exception if the value for +freeze+ is neither +true+ nor +nil+.
  #
  # Related: Numeric#dup.
  def clone(freeze: true) end

  # Returns a 2-element array containing two numeric elements,
  # formed from the two operands +self+ and +other+,
  # of a common compatible type.
  #
  # Of the Core and Standard Library classes,
  # Integer, Rational, and Complex use this implementation.
  #
  # Examples:
  #
  #   i = 2                    # => 2
  #   i.coerce(3)              # => [3, 2]
  #   i.coerce(3.0)            # => [3.0, 2.0]
  #   i.coerce(Rational(1, 2)) # => [0.5, 2.0]
  #   i.coerce(Complex(3, 4))  # Raises RangeError.
  #
  #   r = Rational(5, 2)       # => (5/2)
  #   r.coerce(2)              # => [(2/1), (5/2)]
  #   r.coerce(2.0)            # => [2.0, 2.5]
  #   r.coerce(Rational(2, 3)) # => [(2/3), (5/2)]
  #   r.coerce(Complex(3, 4))  # => [(3+4i), ((5/2)+0i)]
  #
  #   c = Complex(2, 3)        # => (2+3i)
  #   c.coerce(2)              # => [(2+0i), (2+3i)]
  #   c.coerce(2.0)            # => [(2.0+0i), (2+3i)]
  #   c.coerce(Rational(1, 2)) # => [((1/2)+0i), (2+3i)]
  #   c.coerce(Complex(3, 4))  # => [(3+4i), (2+3i)]
  #
  # Raises an exception if any type conversion fails.
  def coerce(other) end

  # Returns self.
  def conjugate; end
  alias conj conjugate

  # Returns the denominator (always positive).
  def denominator; end

  # Returns the quotient <tt>self/other</tt> as an integer (via +floor+),
  # using method +/+ in the derived class of +self+.
  # (\Numeric itself does not define method +/+.)
  #
  # Of the Core and Standard Library classes,
  # Only Float and Rational use this implementation.
  def div(other) end

  # Returns a 2-element array <tt>[q, r]</tt>, where
  #
  #   q = (self/other).floor                  # Quotient
  #   r = self % other                        # Remainder
  #
  # Of the Core and Standard Library classes,
  # only Rational uses this implementation.
  #
  # Examples:
  #
  #   Rational(11, 1).divmod(4)               # => [2, (3/1)]
  #   Rational(11, 1).divmod(-4)              # => [-3, (-1/1)]
  #   Rational(-11, 1).divmod(4)              # => [-3, (1/1)]
  #   Rational(-11, 1).divmod(-4)             # => [2, (-3/1)]
  #
  #   Rational(12, 1).divmod(4)               # => [3, (0/1)]
  #   Rational(12, 1).divmod(-4)              # => [-3, (0/1)]
  #   Rational(-12, 1).divmod(4)              # => [-3, (0/1)]
  #   Rational(-12, 1).divmod(-4)             # => [3, (0/1)]
  #
  #   Rational(13, 1).divmod(4.0)             # => [3, 1.0]
  #   Rational(13, 1).divmod(Rational(4, 11)) # => [35, (3/11)]
  def divmod(other) end

  # Returns +self+.
  #
  # Related: Numeric#clone.
  def dup; end

  # Returns +true+ if +self+ and +other+ are the same type and have equal values.
  #
  # Of the Core and Standard Library classes,
  # only Integer, Rational, and Complex use this implementation.
  #
  # Examples:
  #
  #   1.eql?(1)              # => true
  #   1.eql?(1.0)            # => false
  #   1.eql?(Rational(1, 1)) # => false
  #   1.eql?(Complex(1, 0))  # => false
  #
  # \Method +eql?+ is different from +==+ in that +eql?+ requires matching types,
  # while +==+ does not.
  def eql?(other) end

  # Returns the quotient <tt>self/other</tt> as a float,
  # using method +/+ in the derived class of +self+.
  # (\Numeric itself does not define method +/+.)
  #
  # Of the Core and Standard Library classes,
  # only BigDecimal uses this implementation.
  def fdiv(other) end

  # Returns +true+ if +num+ is a finite number, otherwise returns +false+.
  def finite?; end

  # Returns the largest number that is less than or equal to +self+ with
  # a precision of +digits+ decimal digits.
  #
  # \Numeric implements this by converting +self+ to a Float and
  # invoking Float#floor.
  def floor(digits = 0) end

  # Returns <tt>Complex(0, self)</tt>:
  #
  #   2.i              # => (0+2i)
  #   -2.i             # => (0-2i)
  #   2.0.i            # => (0+2.0i)
  #   Rational(1, 2).i # => (0+(1/2)*i)
  #   Complex(3, 4).i  # Raises NoMethodError.
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

  # Returns +true+ if +self+ is less than 0, +false+ otherwise.
  def negative?; end

  # Returns +self+ if +self+ is not a zero value, +nil+ otherwise;
  # uses method <tt>zero?</tt> for the evaluation.
  #
  # The returned +self+ allows the method to be chained:
  #
  #   a = %w[z Bb bB bb BB a aA Aa AA A]
  #   a.sort {|a, b| (a.downcase <=> b.downcase).nonzero? || a <=> b }
  #   # => ["A", "a", "AA", "Aa", "aA", "BB", "Bb", "bB", "bb", "z"]
  #
  # Of the Core and Standard Library classes,
  # Integer, Float, Rational, and Complex use this implementation.
  def nonzero?; end

  # Returns the numerator.
  def numerator; end

  # Returns an array; [num.abs, num.arg].
  def polar; end

  # Returns +true+ if +self+ is greater than 0, +false+ otherwise.
  def positive?; end

  # Returns the most exact division (rational for integers, float for floats).
  def quo(...) end

  # Returns self.
  def real; end

  # Returns +true+ if +num+ is a real number (i.e. not Complex).
  def real?; end

  # Returns an array; [num, 0].
  def rectangular; end
  alias rect rectangular

  # Returns the remainder after dividing +self+ by +other+.
  #
  # Of the Core and Standard Library classes,
  # only Float and Rational use this implementation.
  #
  # Examples:
  #
  #   11.0.remainder(4)              # => 3.0
  #   11.0.remainder(-4)             # => 3.0
  #   -11.0.remainder(4)             # => -3.0
  #   -11.0.remainder(-4)            # => -3.0
  #
  #   12.0.remainder(4)              # => 0.0
  #   12.0.remainder(-4)             # => 0.0
  #   -12.0.remainder(4)             # => -0.0
  #   -12.0.remainder(-4)            # => -0.0
  #
  #   13.0.remainder(4.0)            # => 1.0
  #   13.0.remainder(Rational(4, 1)) # => 1.0
  #
  #   Rational(13, 1).remainder(4)   # => (1/1)
  #   Rational(13, 1).remainder(-4)  # => (1/1)
  #   Rational(-13, 1).remainder(4)  # => (-1/1)
  #   Rational(-13, 1).remainder(-4) # => (-1/1)
  def remainder(other) end

  # Returns +self+ rounded to the nearest value with
  # a precision of +digits+ decimal digits.
  #
  # \Numeric implements this by converting +self+ to a Float and
  # invoking Float#round.
  def round(digits = 0) end

  #  Generates a sequence of numbers; with a block given, traverses the sequence.
  #
  #  Of the Core and Standard Library classes,
  #  Integer, Float, and Rational use this implementation.
  #
  #  A quick example:
  #
  #    squares = []
  #    1.step(by: 2, to: 10) {|i| squares.push(i*i) }
  #    squares # => [1, 9, 25, 49, 81]
  #
  #  The generated sequence:
  #
  #  - Begins with +self+.
  #  - Continues at intervals of +step+ (which may not be zero).
  #  - Ends with the last number that is within or equal to +limit+;
  #    that is, less than or equal to +limit+ if +step+ is positive,
  #    greater than or equal to +limit+ if +step+ is negative.
  #    If +limit+ is not given, the sequence is of infinite length.
  #
  #  If a block is given, calls the block with each number in the sequence;
  #  returns +self+.  If no block is given, returns an Enumerator::ArithmeticSequence.
  #
  #  <b>Keyword Arguments</b>
  #
  #  With keyword arguments +by+ and +to+,
  #  their values (or defaults) determine the step and limit:
  #
  #    # Both keywords given.
  #    squares = []
  #    4.step(by: 2, to: 10) {|i| squares.push(i*i) }    # => 4
  #    squares # => [16, 36, 64, 100]
  #    cubes = []
  #    3.step(by: -1.5, to: -3) {|i| cubes.push(i*i*i) } # => 3
  #    cubes   # => [27.0, 3.375, 0.0, -3.375, -27.0]
  #    squares = []
  #    1.2.step(by: 0.2, to: 2.0) {|f| squares.push(f*f) }
  #    squares # => [1.44, 1.9599999999999997, 2.5600000000000005, 3.24, 4.0]
  #
  #    squares = []
  #    Rational(6/5).step(by: 0.2, to: 2.0) {|r| squares.push(r*r) }
  #    squares # => [1.0, 1.44, 1.9599999999999997, 2.5600000000000005, 3.24, 4.0]
  #
  #    # Only keyword to given.
  #    squares = []
  #    4.step(to: 10) {|i| squares.push(i*i) }           # => 4
  #    squares # => [16, 25, 36, 49, 64, 81, 100]
  #    # Only by given.
  #
  #    # Only keyword by given
  #    squares = []
  #    4.step(by:2) {|i| squares.push(i*i); break if i > 10 }
  #    squares # => [16, 36, 64, 100, 144]
  #
  #    # No block given.
  #    e = 3.step(by: -1.5, to: -3) # => (3.step(by: -1.5, to: -3))
  #    e.class                      # => Enumerator::ArithmeticSequence
  #
  #  <b>Positional Arguments</b>
  #
  #  With optional positional arguments +limit+ and +step+,
  #  their values (or defaults) determine the step and limit:
  #
  #    squares = []
  #    4.step(10, 2) {|i| squares.push(i*i) }    # => 4
  #    squares # => [16, 36, 64, 100]
  #    squares = []
  #    4.step(10) {|i| squares.push(i*i) }
  #    squares # => [16, 25, 36, 49, 64, 81, 100]
  #    squares = []
  #    4.step {|i| squares.push(i*i); break if i > 10 }  # => nil
  #    squares # => [16, 25, 36, 49, 64, 81, 100, 121]
  #
  # <b>Implementation Notes</b>
  #
  #  If all the arguments are integers, the loop operates using an integer
  #  counter.
  #
  #  If any of the arguments are floating point numbers, all are converted
  #  to floats, and the loop is executed
  #  <i>floor(n + n*Float::EPSILON) + 1</i> times,
  #  where <i>n = (limit - self)/step</i>.
  def step(...) end

  # Returns the value as a complex.
  def to_c; end

  # Returns +self+ as an integer;
  # converts using method +to_i+ in the derived class.
  #
  # Of the Core and Standard Library classes,
  # only Rational and Complex use this implementation.
  #
  # Examples:
  #
  #   Rational(1, 2).to_int # => 0
  #   Rational(2, 1).to_int # => 2
  #   Complex(2, 0).to_int  # => 2
  #   Complex(2, 1)         # Raises RangeError (non-zero imaginary part)
  def to_int; end

  # Returns +self+ truncated (toward zero) to
  # a precision of +digits+ decimal digits.
  #
  # \Numeric implements this by converting +self+ to a Float and
  # invoking Float#truncate.
  def truncate(digits = 0) end

  # Returns +true+ if +zero+ has a zero value, +false+ otherwise.
  #
  # Of the Core and Standard Library classes,
  # only Rational and Complex use this implementation.
  def zero?; end
end
