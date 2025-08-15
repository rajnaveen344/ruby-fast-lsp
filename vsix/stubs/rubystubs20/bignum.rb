# frozen_string_literal: true

# Bignum objects hold integers outside the range of
# Fixnum. Bignum objects are created
# automatically when integer calculations would otherwise overflow a
# Fixnum. When a calculation involving
# Bignum objects returns a result that will fit in a
# Fixnum, the result is automatically converted.
#
# For the purposes of the bitwise operations and <code>[]</code>, a
# Bignum is treated as if it were an infinite-length
# bitstring with 2's complement representation.
#
# While Fixnum values are immediate, Bignum
# objects are not---assignment and parameter passing work with
# references to objects, not the objects themselves.
class Bignum < Integer
  # Returns big modulo other. See Numeric.divmod for more
  # information.
  def %(other) end
  alias modulo %

  # Performs bitwise +and+ between _big_ and _numeric_.
  def &(other) end

  # Multiplies big and other, returning the result.
  def *(other) end

  # Raises _big_ to the _exponent_ power (which may be an integer, float,
  # or anything that will coerce to a number). The result may be
  # a Fixnum, Bignum, or Float
  #
  #   123456789 ** 2      #=> 15241578750190521
  #   123456789 ** 1.2    #=> 5126464716.09932
  #   123456789 ** -2     #=> 6.5610001194102e-17
  def **(other) end

  # Adds big and other, returning the result.
  def +(other) end

  # Subtracts other from big, returning the result.
  def -(other) end

  # Unary minus (returns an integer whose value is 0-big)
  def -@; end

  # Performs division: the class of the resulting object depends on
  # the class of <code>numeric</code> and on the magnitude of the
  # result.
  def /(other) end

  # Returns <code>true</code> if the value of <code>big</code> is
  # less than that of <code>real</code>.
  def <(other) end

  # Shifts big left _numeric_ positions (right if _numeric_ is negative).
  def <<(numeric) end

  # Returns <code>true</code> if the value of <code>big</code> is
  # less than or equal to that of <code>real</code>.
  def <=(other) end

  # Comparison---Returns -1, 0, or +1 depending on whether +big+ is
  # less than, equal to, or greater than +numeric+. This is the
  # basis for the tests in Comparable.
  #
  # +nil+ is returned if the two values are incomparable.
  def <=>(other) end

  # Returns <code>true</code> only if <i>obj</i> has the same value
  # as <i>big</i>. Contrast this with <code>Bignum#eql?</code>, which
  # requires <i>obj</i> to be a <code>Bignum</code>.
  #
  #    68719476736 == 68719476736.0   #=> true
  def ==(other) end
  alias === ==

  # Returns <code>true</code> if the value of <code>big</code> is
  # greater than that of <code>real</code>.
  def >(other) end

  # Returns <code>true</code> if the value of <code>big</code> is
  # greater than or equal to that of <code>real</code>.
  def >=(other) end

  # Shifts big right _numeric_ positions (left if _numeric_ is negative).
  def >>(other) end

  # Bit Reference---Returns the <em>n</em>th bit in the (assumed) binary
  # representation of <i>big</i>, where <i>big</i>[0] is the least
  # significant bit.
  #
  #    a = 9**15
  #    50.downto(0) do |n|
  #      print a[n]
  #    end
  #
  # <em>produces:</em>
  #
  #    000101110110100000111000011110010100111100010111001
  def [](n) end

  # Performs bitwise +exclusive or+ between _big_ and _numeric_.
  def ^(other) end

  # Performs bitwise +or+ between _big_ and _numeric_.
  def |(other) end

  # Inverts the bits in big. As Bignums are conceptually infinite
  # length, the result acts as if it had an infinite number of one
  # bits to the left. In hex representations, this is displayed
  # as two periods to the left of the digits.
  #
  #   sprintf("%X", ~0x1122334455)    #=> "..FEEDDCCBBAA"
  def ~; end

  # Returns the absolute value of <i>big</i>.
  #
  #    -1234567890987654321.abs   #=> 1234567890987654321
  def abs; end
  alias magnitude abs

  # MISSING: documentation
  def coerce(p1) end

  # Performs integer division: returns integer value.
  def div(other) end

  # See <code>Numeric#divmod</code>.
  def divmod(numeric) end

  # Returns <code>true</code> only if <i>obj</i> is a
  # <code>Bignum</code> with the same value as <i>big</i>. Contrast this
  # with <code>Bignum#==</code>, which performs type conversions.
  #
  #    68719476736.eql?(68719476736.0)   #=> false
  def eql?(other) end

  # Returns <code>true</code> if <i>big</i> is an even number.
  def even?; end

  # Returns the floating point result of dividing <i>big</i> by
  # <i>numeric</i>.
  #
  #    -1234567890987654321.fdiv(13731)      #=> -89910996357705.5
  #    -1234567890987654321.fdiv(13731.24)   #=> -89909424858035.7
  def fdiv(numeric) end

  # Compute a hash based on the value of _big_.
  def hash; end

  # Returns <code>true</code> if <i>big</i> is an odd number.
  def odd?; end

  # Returns the remainder after dividing <i>big</i> by <i>numeric</i>.
  #
  #    -1234567890987654321.remainder(13731)      #=> -6966
  #    -1234567890987654321.remainder(13731.24)   #=> -9906.22531493148
  def remainder(numeric) end

  # Returns the number of bytes in the machine representation of
  # <i>big</i>.
  #
  #    (256**10 - 1).size   #=> 12
  #    (256**20 - 1).size   #=> 20
  #    (256**40 - 1).size   #=> 40
  def size; end

  # Converts <i>big</i> to a <code>Float</code>. If <i>big</i> doesn't
  # fit in a <code>Float</code>, the result is infinity.
  def to_f; end

  # Returns a string containing the representation of <i>big</i> radix
  # <i>base</i> (2 through 36).
  #
  #    12345654321.to_s         #=> "12345654321"
  #    12345654321.to_s(2)      #=> "1011011111110110111011110000110001"
  #    12345654321.to_s(8)      #=> "133766736061"
  #    12345654321.to_s(16)     #=> "2dfdbbc31"
  #    78546939656932.to_s(36)  #=> "rubyrules"
  def to_s(base = 10) end
  alias inspect to_s
end
