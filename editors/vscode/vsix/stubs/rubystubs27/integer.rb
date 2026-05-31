# frozen_string_literal: true

# Holds Integer values.  You cannot add a singleton method to an
# Integer object, any attempt to do so will raise a TypeError.
class Integer < Numeric
  # The version of loaded GMP.
  GMP_VERSION = _

  # Returns the integer square root of the non-negative integer +n+,
  # i.e. the largest non-negative integer less than or equal to the
  # square root of +n+.
  #
  #   Integer.sqrt(0)        #=> 0
  #   Integer.sqrt(1)        #=> 1
  #   Integer.sqrt(24)       #=> 4
  #   Integer.sqrt(25)       #=> 5
  #   Integer.sqrt(10**400)  #=> 10**200
  #
  # Equivalent to <code>Math.sqrt(n).floor</code>, except that
  # the result of the latter code may differ from the true value
  # due to the limited precision of floating point arithmetic.
  #
  #   Integer.sqrt(10**46)     #=> 100000000000000000000000
  #   Math.sqrt(10**46).floor  #=>  99999999999999991611392 (!)
  #
  # If +n+ is not an Integer, it is converted to an Integer first.
  # If +n+ is negative, a Math::DomainError is raised.
  def self.sqrt(n) end

  # Returns +int+ modulo +other+.
  #
  # See Numeric#divmod for more information.
  def %(other) end
  alias modulo %

  # Bitwise AND.
  def &(other) end

  # Performs multiplication: the class of the resulting object depends on
  # the class of +numeric+.
  def *(other) end

  # Raises +int+ to the power of +numeric+, which may be negative or
  # fractional.
  # The result may be an Integer, a Float, a Rational, or a complex number.
  #
  #    2 ** 3        #=> 8
  #    2 ** -1       #=> (1/2)
  #    2 ** 0.5      #=> 1.4142135623730951
  #    (-1) ** 0.5   #=> (0.0+1.0i)
  #
  #    123456789 ** 2     #=> 15241578750190521
  #    123456789 ** 1.2   #=> 5126464716.0993185
  #    123456789 ** -2    #=> (1/15241578750190521)
  def **(other) end

  # Performs addition: the class of the resulting object depends on
  # the class of +numeric+.
  def +(other) end

  # Performs subtraction: the class of the resulting object depends on
  # the class of +numeric+.
  def -(other) end

  # Returns +int+, negated.
  def -@; end

  # Performs division: the class of the resulting object depends on
  # the class of +numeric+.
  def /(other) end

  # Returns +true+ if the value of +int+ is less than that of +real+.
  def <(other) end

  # Returns +int+ shifted left +count+ positions, or right if +count+
  # is negative.
  def <<(count) end

  # Returns +true+ if the value of +int+ is less than or equal to that of
  # +real+.
  def <=(other) end

  # Comparison---Returns -1, 0, or +1 depending on whether +int+ is
  # less than, equal to, or greater than +numeric+.
  #
  # This is the basis for the tests in the Comparable module.
  #
  # +nil+ is returned if the two values are incomparable.
  def <=>(other) end

  # Returns +true+ if +int+ equals +other+ numerically.
  # Contrast this with Integer#eql?, which requires +other+ to be an Integer.
  #
  #    1 == 2     #=> false
  #    1 == 1.0   #=> true
  def ===(p1) end
  alias == ===

  # Returns +true+ if the value of +int+ is greater than that of +real+.
  def >(other) end

  # Returns +true+ if the value of +int+ is greater than or equal to that of
  # +real+.
  def >=(other) end

  # Returns +int+ shifted right +count+ positions, or left if +count+
  # is negative.
  def >>(other) end

  # Bit Reference---Returns the <code>n</code>th bit in the
  # binary representation of +int+, where <code>int[0]</code>
  # is the least significant bit.
  #
  #    a = 0b11001100101010
  #    30.downto(0) {|n| print a[n] }
  #    #=> 0000000000000000011001100101010
  #
  #    a = 9**15
  #    50.downto(0) {|n| print a[n] }
  #    #=> 000101110110100000111000011110010100111100010111001
  #
  # In principle, <code>n[i]</code> is equivalent to <code>(n >> i) & 1</code>.
  # Thus, any negative index always returns zero:
  #
  #    p 255[-1] #=> 0
  #
  # Range operations <code>n[i, len]</code> and <code>n[i..j]</code>
  # are naturally extended.
  #
  # * <code>n[i, len]</code> equals to <code>(n >> i) & ((1 << len) - 1)</code>.
  # * <code>n[i..j]</code> equals to <code>(n >> i) & ((1 << (j - i + 1)) - 1)</code>.
  # * <code>n[i...j]</code> equals to <code>(n >> i) & ((1 << (j - i)) - 1)</code>.
  # * <code>n[i..]</code> equals to <code>(n >> i)</code>.
  # * <code>n[..j]</code> is zero if <code>n & ((1 << (j + 1)) - 1)</code> is zero.  Otherwise, raises an ArgumentError.
  # * <code>n[...j]</code> is zero if <code>n & ((1 << j) - 1)</code> is zero.  Otherwise, raises an ArgumentError.
  #
  # Note that range operation may exhaust memory.
  # For example, <code>-1[0, 1000000000000]</code> will raise NoMemoryError.
  def [](...) end

  # Bitwise EXCLUSIVE OR.
  def ^(other) end

  # Bitwise OR.
  def |(other) end

  # One's complement: returns a number where each bit is flipped.
  #
  # Inverts the bits in an Integer. As integers are conceptually of
  # infinite length, the result acts as if it had an infinite number of
  # one bits to the left. In hex representations, this is displayed
  # as two periods to the left of the digits.
  #
  #   sprintf("%X", ~0x1122334455)    #=> "..FEEDDCCBBAA"
  def ~; end

  # Returns the absolute value of +int+.
  #
  #    (-12345).abs   #=> 12345
  #    -12345.abs     #=> 12345
  #    12345.abs      #=> 12345
  #
  # Integer#magnitude is an alias for Integer#abs.
  def abs; end
  alias magnitude abs

  # Returns +true+ if all bits of <code>+int+ & +mask+</code> are 1.
  def allbits?(mask) end

  # Returns +true+ if any bits of <code>+int+ & +mask+</code> are 1.
  def anybits?(mask) end

  # Returns the number of bits of the value of +int+.
  #
  # "Number of bits" means the bit position of the highest bit
  # which is different from the sign bit
  # (where the least significant bit has bit position 1).
  # If there is no such bit (zero or minus one), zero is returned.
  #
  # I.e. this method returns <i>ceil(log2(int < 0 ? -int : int+1))</i>.
  #
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
  #
  # This method can be used to detect overflow in Array#pack as follows:
  #
  #    if n.bit_length < 32
  #      [n].pack("l") # no overflow
  #    else
  #      raise "overflow"
  #    end
  def bit_length; end

  # Returns the smallest number greater than or equal to +int+ with
  # a precision of +ndigits+ decimal digits (default: 0).
  #
  # When the precision is negative, the returned value is an integer
  # with at least <code>ndigits.abs</code> trailing zeros.
  #
  # Returns +self+ when +ndigits+ is zero or positive.
  #
  #    1.ceil           #=> 1
  #    1.ceil(2)        #=> 1
  #    18.ceil(-1)      #=> 20
  #    (-18).ceil(-1)   #=> -10
  def ceil(*ndigits) end

  # Returns a string containing the character represented by the +int+'s value
  # according to +encoding+.
  #
  #    65.chr    #=> "A"
  #    230.chr   #=> "\xE6"
  #    255.chr(Encoding::UTF_8)   #=> "\u00FF"
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

  # Returns the digits of +int+'s place-value representation
  # with radix +base+ (default: 10).
  # The digits are returned as an array with the least significant digit
  # as the first array element.
  #
  # +base+ must be greater than or equal to 2.
  #
  #    12345.digits      #=> [5, 4, 3, 2, 1]
  #    12345.digits(7)   #=> [4, 6, 6, 0, 5]
  #    12345.digits(100) #=> [45, 23, 1]
  #
  #    -12345.digits(7)  #=> Math::DomainError
  def digits(...) end

  # Performs integer division: returns the integer result of dividing +int+
  # by +numeric+.
  def div(numeric) end

  # See Numeric#divmod.
  def divmod(numeric) end

  # Iterates the given block, passing in decreasing values from +int+ down to
  # and including +limit+.
  #
  # If no block is given, an Enumerator is returned instead.
  #
  #    5.downto(1) { |n| print n, ".. " }
  #    puts "Liftoff!"
  #    #=> "5.. 4.. 3.. 2.. 1.. Liftoff!"
  def downto(limit) end

  # Returns +true+ if +int+ is an even number.
  def even?; end

  # Returns the floating point result of dividing +int+ by +numeric+.
  #
  #    654321.fdiv(13731)      #=> 47.652829364212366
  #    654321.fdiv(13731.24)   #=> 47.65199646936475
  #    -654321.fdiv(13731)     #=> -47.652829364212366
  def fdiv(numeric) end

  # Returns the largest number less than or equal to +int+ with
  # a precision of +ndigits+ decimal digits (default: 0).
  #
  # When the precision is negative, the returned value is an integer
  # with at least <code>ndigits.abs</code> trailing zeros.
  #
  # Returns +self+ when +ndigits+ is zero or positive.
  #
  #    1.floor           #=> 1
  #    1.floor(2)        #=> 1
  #    18.floor(-1)      #=> 10
  #    (-18).floor(-1)   #=> -20
  def floor(*ndigits) end

  # Returns the greatest common divisor of the two integers.
  # The result is always positive. 0.gcd(x) and x.gcd(0) return x.abs.
  #
  #    36.gcd(60)                  #=> 12
  #    2.gcd(2)                    #=> 2
  #    3.gcd(-7)                   #=> 1
  #    ((1<<31)-1).gcd((1<<61)-1)  #=> 1
  def gcd(other_int) end

  # Returns an array with the greatest common divisor and
  # the least common multiple of the two integers, [gcd, lcm].
  #
  #    36.gcdlcm(60)                  #=> [12, 180]
  #    2.gcdlcm(2)                    #=> [2, 2]
  #    3.gcdlcm(-7)                   #=> [1, 21]
  #    ((1<<31)-1).gcdlcm((1<<61)-1)  #=> [1, 4951760154835678088235319297]
  def gcdlcm(other_int) end

  # Since +int+ is already an Integer, this always returns +true+.
  def integer?; end

  # Returns the least common multiple of the two integers.
  # The result is always positive. 0.lcm(x) and x.lcm(0) return zero.
  #
  #    36.lcm(60)                  #=> 180
  #    2.lcm(2)                    #=> 2
  #    3.lcm(-7)                   #=> 21
  #    ((1<<31)-1).lcm((1<<61)-1)  #=> 4951760154835678088235319297
  def lcm(other_int) end

  # Returns +true+ if no bits of <code>+int+ & +mask+</code> are 1.
  def nobits?(mask) end

  # Returns self.
  def numerator; end

  # Returns +true+ if +int+ is an odd number.
  def odd?; end

  # Returns the +int+ itself.
  #
  #    97.ord   #=> 97
  #
  # This method is intended for compatibility to character literals
  # in Ruby 1.9.
  #
  # For example, <code>?a.ord</code> returns 97 both in 1.8 and 1.9.
  def ord; end

  # Returns (modular) exponentiation as:
  #
  #   a.pow(b)     #=> same as a**b
  #   a.pow(b, m)  #=> same as (a**b) % m, but avoids huge temporary values
  def pow(...) end

  # Returns the predecessor of +int+,
  # i.e. the Integer equal to <code>int-1</code>.
  #
  #    1.pred      #=> 0
  #    (-1).pred   #=> -2
  def pred; end

  # Returns the value as a rational.  The optional argument +eps+ is
  # always ignored.
  def rationalize(*eps) end

  # Returns the remainder after dividing +int+ by +numeric+.
  #
  # <code>x.remainder(y)</code> means <code>x-y*(x/y).truncate</code>.
  #
  #    5.remainder(3)     #=> 2
  #    -5.remainder(3)    #=> -2
  #    5.remainder(-3)    #=> 2
  #    -5.remainder(-3)   #=> -2
  #    5.remainder(1.5)   #=> 0.5
  #
  # See Numeric#divmod.
  def remainder(numeric) end

  # Returns +int+ rounded to the nearest value with
  # a precision of +ndigits+ decimal digits (default: 0).
  #
  # When the precision is negative, the returned value is an integer
  # with at least <code>ndigits.abs</code> trailing zeros.
  #
  # Returns +self+ when +ndigits+ is zero or positive.
  #
  #    1.round           #=> 1
  #    1.round(2)        #=> 1
  #    15.round(-1)      #=> 20
  #    (-15).round(-1)   #=> -20
  #
  # The optional +half+ keyword argument is available
  # similar to Float#round.
  #
  #    25.round(-1, half: :up)      #=> 30
  #    25.round(-1, half: :down)    #=> 20
  #    25.round(-1, half: :even)    #=> 20
  #    35.round(-1, half: :up)      #=> 40
  #    35.round(-1, half: :down)    #=> 30
  #    35.round(-1, half: :even)    #=> 40
  #    (-25).round(-1, half: :up)   #=> -30
  #    (-25).round(-1, half: :down) #=> -20
  #    (-25).round(-1, half: :even) #=> -20
  def round(p1 = v1, p2 = {}) end

  # Returns the number of bytes in the machine representation of +int+
  # (machine dependent).
  #
  #    1.size               #=> 8
  #    -1.size              #=> 8
  #    2147483647.size      #=> 8
  #    (256**10 - 1).size   #=> 10
  #    (256**20 - 1).size   #=> 20
  #    (256**40 - 1).size   #=> 40
  def size; end

  # Returns the successor of +int+,
  # i.e. the Integer equal to <code>int+1</code>.
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
  #    5.times {|i| print i, " " }   #=> 0 1 2 3 4
  def times; end

  # Converts +int+ to a Float.  If +int+ doesn't fit in a Float,
  # the result is infinity.
  def to_f; end

  # Since +int+ is already an Integer, returns +self+.
  #
  # #to_int is an alias for #to_i.
  def to_i; end
  alias to_int to_i

  # Returns the value as a rational.
  #
  #    1.to_r        #=> (1/1)
  #    (1<<64).to_r  #=> (18446744073709551616/1)
  def to_r; end

  # Returns a string containing the place-value representation of +int+
  # with radix +base+ (between 2 and 36).
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

  # Returns +int+ truncated (toward zero) to
  # a precision of +ndigits+ decimal digits (default: 0).
  #
  # When the precision is negative, the returned value is an integer
  # with at least <code>ndigits.abs</code> trailing zeros.
  #
  # Returns +self+ when +ndigits+ is zero or positive.
  #
  #    1.truncate           #=> 1
  #    1.truncate(2)        #=> 1
  #    18.truncate(-1)      #=> 10
  #    (-18).truncate(-1)   #=> -10
  def truncate(*ndigits) end

  # Iterates the given block, passing in integer values from +int+ up to and
  # including +limit+.
  #
  # If no block is given, an Enumerator is returned instead.
  #
  #    5.upto(10) {|i| print i, " " }   #=> 5 6 7 8 9 10
  def upto(limit) end
end
