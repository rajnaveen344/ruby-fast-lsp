# frozen_string_literal: true

# Holds Integer values.  You cannot add a singleton method to an
# Integer. Any attempt to add a singleton method to an Integer object
# will raise a TypeError.
class Integer < Numeric
  # The version of loaded GMP.
  GMP_VERSION = _

  # Returns +int+ modulo +other+.
  #
  # See Numeric#divmod for more information.
  def %(other) end
  alias modulo %

  # Bitwise AND.
  def &(other) end

  # Performs multiplication: the class of the resulting object depends on the
  # class of +numeric+ and on the magnitude of the result. It may return a
  # Bignum.
  def *(other) end

  # Raises +integer+ to the power of +numeric+, which may be negative or
  # fractional.
  # The result may be an Integer, or a Float
  #
  #   2 ** 3      #=> 8
  #   2 ** -1     #=> (1/2)
  #   2 ** 0.5    #=> 1.4142135623731
  #
  #   123456789 ** 2      #=> 15241578750190521
  #   123456789 ** 1.2    #=> 5126464716.09932
  #   123456789 ** -2     #=> (1/15241578750190521)
  def **(other) end

  # Performs addition: the class of the resulting object depends on the class of
  # +numeric+ and on the magnitude of the result. It may return a Bignum.
  def +(other) end

  # Performs subtraction: the class of the resulting object depends on the class
  # of +numeric+ and on the magnitude of the result. It may return a Bignum.
  def -(other) end

  # Negates +int+.
  # (returns an integer whose value is 0-int)
  def -@; end

  # Performs division: the class of the resulting object depends on the class of
  # +numeric+ and on the magnitude of the result. It may return a Bignum.
  def /(other) end

  # Returns +true+ if the value of +int+ is less than that of +real+.
  def <(other) end

  # Shifts +int+ left +count+ positions, or right if +count+ is negative.
  def <<(count) end

  # Returns +true+ if the value of +int+ is less than or equal to that of
  # +real+.
  def <=(other) end

  # Comparison---Returns +-1+, +0+, ++1+ or +nil+ depending on whether +int+ is
  # less than, equal to, or greater than +numeric+.
  #
  # This is the basis for the tests in the Comparable module.
  #
  # +nil+ is returned if the two values are incomparable.
  def <=>(other) end

  def ===(p1) end
  alias == ===

  # Returns +true+ if the value of +int+ is greater than that of +real+.
  def >(other) end

  # Returns +true+ if the value of +int+ is greater than or equal to that of
  # +real+.
  def >=(other) end

  # Shifts +int+ right +count+ positions, or left if +count+ is negative.
  def >>(other) end

  # Bit Reference---Returns the +n+th bit in the binary representation of
  # +int+, where <code>int[0]</code> is the least significant bit.
  #
  # For example:
  #
  #    a = 0b11001100101010
  #    30.downto(0) do |n| print a[n] end
  #    #=> 0000000000000000011001100101010
  #
  #    a = 9**15
  #    50.downto(0) do |n|
  #      print a[n]
  #    end
  #    #=> 000101110110100000111000011110010100111100010111001
  def [](n) end

  # Bitwise EXCLUSIVE OR.
  def ^(other) end

  # Bitwise OR.
  def |(other) end

  # One's complement: returns a number where each bit is flipped.
  #
  # Inverts the bits in an integer. As Integers are conceptually infinite
  # length, the result acts as if it had an infinite number of one
  # bits to the left. In hex representations, this is displayed
  # as two periods to the left of the digits.
  #
  #   sprintf("%X", ~0x1122334455)    #=> "..FEEDDCCBBAA"
  def ~; end

  # Returns the absolute value of +int+.
  #
  #    -12345.abs   #=> 12345
  #    12345.abs    #=> 12345
  #    -1234567890987654321.abs   #=> 1234567890987654321
  def abs; end
  alias magnitude abs

  # Returns the number of bits of the value of <i>int</i>.
  #
  # "the number of bits" means that
  # the bit position of the highest bit which is different to the sign bit.
  # (The bit position of the bit 2**n is n+1.)
  # If there is no such bit (zero or minus one), zero is returned.
  #
  # I.e. This method returns ceil(log2(int < 0 ? -int : int+1)).
  #
  #    (-2**10000-1).bit_length  #=> 10001
  #    (-2**10000).bit_length    #=> 10000
  #    (-2**10000+1).bit_length  #=> 10000
  #    (-2**1000-1).bit_length   #=> 1001
  #    (-2**1000).bit_length     #=> 1000
  #    (-2**1000+1).bit_length   #=> 1000
  #    (-2**12-1).bit_length     #=> 13
  #    (-2**12).bit_length       #=> 12
  #    (-2**12+1).bit_length     #=> 12
  #    -0x101.bit_length         #=> 9
  #    -0x100.bit_length         #=> 8
  #    -0xff.bit_length          #=> 8
  #    -2.bit_length             #=> 1
  #    -1.bit_length             #=> 0
  #    0.bit_length              #=> 0
  #    1.bit_length              #=> 1
  #    0xff.bit_length           #=> 8
  #    0x100.bit_length          #=> 9
  #    (2**12-1).bit_length      #=> 12
  #    (2**12).bit_length        #=> 13
  #    (2**12+1).bit_length      #=> 13
  #    (2**1000-1).bit_length    #=> 1000
  #    (2**1000).bit_length      #=> 1001
  #    (2**1000+1).bit_length    #=> 1001
  #    (2**10000-1).bit_length   #=> 10000
  #    (2**10000).bit_length     #=> 10001
  #    (2**10000+1).bit_length   #=> 10001
  #
  # This method can be used to detect overflow in Array#pack as follows.
  #
  #    if n.bit_length < 32
  #      [n].pack("l") # no overflow
  #    else
  #      raise "overflow"
  #    end
  def bit_length; end

  # Returns the smallest number than or equal to +int+ in decimal
  # digits (default 0 digits).
  #
  # Precision may be negative.  Returns a floating point number when +ndigits+
  # is positive, +self+ for zero, and ceil up for negative.
  #
  #    1.ceil        #=> 1
  #    1.ceil(2)     #=> 1.0
  #    15.ceil(-1)   #=> 20
  def ceil(*ndigits) end

  # Returns a string containing the character represented by the +int+'s value
  # according to +encoding+.
  #
  #    65.chr    #=> "A"
  #    230.chr   #=> "\346"
  #    255.chr(Encoding::UTF_8)   #=> "\303\277"
  def chr(*encoding) end

  # Returns an array with both a +numeric+ and a +big+ represented as Bignum
  # objects.
  #
  # This is achieved by converting +numeric+ to a Bignum.
  #
  # A TypeError is raised if the +numeric+ is not a Fixnum or Bignum type.
  #
  #     (0x3FFFFFFFFFFFFFFF+1).coerce(42)   #=> [42, 4611686018427387904]
  def coerce(numeric) end

  # Returns 1.
  def denominator; end

  # Returns the array including the digits extracted by place-value notation
  # with radix +base+ of +int+.
  #
  # +base+ should be greater than or equal to 2.
  #
  #    12345.digits      #=> [5, 4, 3, 2, 1]
  #    12345.digits(7)   #=> [4, 6, 6, 0, 5]
  #    12345.digits(100) #=> [45, 23, 1]
  #
  #    -12345.digits(7)  #=> Math::DomainError
  def digits(*several_variants) end

  # Performs integer division: returns integer result of dividing +int+ by
  # +numeric+.
  def div(numeric) end

  # See <code>Numeric#divmod</code>.
  def divmod(numeric) end

  # Iterates the given block, passing decreasing values from +int+ down to and
  # including +limit+.
  #
  # If no block is given, an Enumerator is returned instead.
  #
  #    5.downto(1) { |n| print n, ".. " }
  #    print "  Liftoff!\n"
  #    #=> "5.. 4.. 3.. 2.. 1..   Liftoff!"
  def downto(limit) end

  # Returns +true+ if +int+ is an even number.
  def even?; end

  # Returns the floating point result of dividing +integer+ by +numeric+.
  #
  #    654321.fdiv(13731)      #=> 47.6528293642124
  #    654321.fdiv(13731.24)   #=> 47.6519964693647
  #
  #    -1234567890987654321.fdiv(13731)      #=> -89910996357705.5
  #    -1234567890987654321.fdiv(13731.24)   #=> -89909424858035.7
  def fdiv(numeric) end

  # Returns the largest number less than or equal to +int+ in decimal
  # digits (default 0 digits).
  #
  # Precision may be negative.  Returns a floating point number when +ndigits+
  # is positive, +self+ for zero, and floor down for negative.
  #
  #    1.floor        #=> 1
  #    1.floor(2)     #=> 1.0
  #    15.floor(-1)   #=> 10
  def floor(*ndigits) end

  # Returns the greatest common divisor (always positive).  0.gcd(x)
  # and x.gcd(0) return abs(x).
  #
  #    2.gcd(2)                    #=> 2
  #    3.gcd(-7)                   #=> 1
  #    ((1<<31)-1).gcd((1<<61)-1)  #=> 1
  def gcd(int2) end

  # Returns an array; [int.gcd(int2), int.lcm(int2)].
  #
  #    2.gcdlcm(2)                    #=> [2, 2]
  #    3.gcdlcm(-7)                   #=> [1, 21]
  #    ((1<<31)-1).gcdlcm((1<<61)-1)  #=> [1, 4951760154835678088235319297]
  def gcdlcm(int2) end

  # Since +int+ is already an Integer, this always returns +true+.
  def integer?; end

  # Returns the least common multiple (always positive).  0.lcm(x) and
  # x.lcm(0) return zero.
  #
  #    2.lcm(2)                    #=> 2
  #    3.lcm(-7)                   #=> 21
  #    ((1<<31)-1).lcm((1<<61)-1)  #=> 4951760154835678088235319297
  def lcm(int2) end

  # Returns self.
  def numerator; end

  # Returns +true+ if +int+ is an odd number.
  def odd?; end

  # Returns the +int+ itself.
  #
  #    ?a.ord    #=> 97
  #
  # This method is intended for compatibility to character constant in Ruby
  # 1.9.
  #
  # For example, ?a.ord returns 97 both in 1.8 and 1.9.
  def ord; end

  # Returns the Integer equal to +int+ - 1.
  #
  #    1.pred      #=> 0
  #    (-1).pred   #=> -2
  def pred; end

  # Returns the value as a rational.  The optional argument eps is
  # always ignored.
  def rationalize(*eps) end

  # Returns the remainder after dividing <i>big</i> by <i>numeric</i> as:
  #
  #   x.remainder(y) means x-y*(x/y).truncate
  #
  # Examples
  #
  #   5.remainder(3)    #=> 2
  #   -5.remainder(3)   #=> -2
  #   5.remainder(-3)   #=> 2
  #   -5.remainder(-3)  #=> -2
  #
  #   -1234567890987654321.remainder(13731)      #=> -6966
  #   -1234567890987654321.remainder(13731.24)   #=> -9906.22531493148
  #
  # See Numeric#divmod.
  def remainder(numeric) end

  # Rounds +int+ to a given precision in decimal digits (default 0 digits).
  #
  # Precision may be negative.  Returns a floating point number when +ndigits+
  # is positive, +self+ for zero, and round down for negative.
  #
  #    1.round        #=> 1
  #    1.round(2)     #=> 1.0
  #    15.round(-1)   #=> 20
  def round(*ndigits) end

  # Returns the number of bytes in the machine representation of +int+.
  #
  #    1.size            #=> 4
  #    -1.size           #=> 4
  #    2147483647.size   #=> 4
  #    (256**10 - 1).size   #=> 12
  #    (256**20 - 1).size   #=> 20
  #    (256**40 - 1).size   #=> 40
  def size; end

  # Returns the Integer equal to +int+ + 1.
  #
  #    1.next      #=> 2
  #    (-1).next   #=> 0
  #    1.succ      #=> 2
  #    (-1).succ   #=> 0
  def succ; end
  alias next succ

  # Iterates the given block +int+ times, passing in values from zero to
  # <code>int - 1</code>.
  #
  # If no block is given, an Enumerator is returned instead.
  #
  #    5.times do |i|
  #      print i, " "
  #    end
  #    #=> 0 1 2 3 4
  def times; end

  # Converts +int+ to a +Float+.  If +int+ doesn't fit in a +Float+,
  # the result is infinity.
  def to_f; end

  # As +int+ is already an Integer, all these methods simply return the receiver.
  #
  # Synonyms is #to_int
  def to_i; end
  alias to_int to_i

  # Returns the value as a rational.
  #
  #    1.to_r        #=> (1/1)
  #    (1<<64).to_r  #=> (18446744073709551616/1)
  def to_r; end

  # Returns a string containing the representation of +int+ radix +base+
  # (between 2 and 36).
  #
  #    12345.to_s       #=> "12345"
  #    12345.to_s(2)    #=> "11000000111001"
  #    12345.to_s(8)    #=> "30071"
  #    12345.to_s(10)   #=> "12345"
  #    12345.to_s(16)   #=> "3039"
  #    12345.to_s(36)   #=> "9ix"
  #    78546939656932.to_s(36)  #=> "rubyrules"
  def to_s(base = 10) end
  alias inspect to_s

  # Returns the smallest number than or equal to +int+ in decimal
  # digits (default 0 digits).
  #
  # Precision may be negative.  Returns a floating point number when +ndigits+
  # is positive, +self+ for zero, and truncate up for negative.
  #
  #    1.truncate        #=> 1
  #    1.truncate(2)     #=> 1.0
  #    15.truncate(-1)   #=> 10
  def truncate(*ndigits) end

  # Iterates the given block, passing in integer values from +int+ up to and
  # including +limit+.
  #
  # If no block is given, an Enumerator is returned instead.
  #
  # For example:
  #
  #    5.upto(10) { |i| print i, " " }
  #    #=> 5 6 7 8 9 10
  def upto(limit) end
end
