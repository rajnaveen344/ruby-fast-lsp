# frozen_string_literal: true

# Holds Integer values that can be represented in a native machine word
# (minus 1 bit).  If any operation on a Fixnum exceeds this range, the value
# is automatically converted to a Bignum.
#
# Fixnum objects have immediate value. This means that when they are assigned
# or passed as parameters, the actual object is passed, rather than a
# reference to that object.
#
# Assignment does not alias Fixnum objects. There is effectively only one
# Fixnum object instance for any given integer value, so, for example, you
# cannot add a singleton method to a Fixnum. Any attempt to add a singleton
# method to a Fixnum object will raise a TypeError.
class Fixnum < Integer
  # Returns +fix+ modulo +other+.
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

  # Raises +fix+ to the power of +numeric+, which may be negative or
  # fractional.
  #
  #   2 ** 3      #=> 8
  #   2 ** -1     #=> (1/2)
  #   2 ** 0.5    #=> 1.4142135623731
  def **(other) end

  # Performs addition: the class of the resulting object depends on the class of
  # +numeric+ and on the magnitude of the result. It may return a Bignum.
  def +(other) end

  # Performs subtraction: the class of the resulting object depends on the class
  # of +numeric+ and on the magnitude of the result. It may return a Bignum.
  def -(other) end

  # Negates +fix+, which may return a Bignum.
  def -@; end

  # Performs division: the class of the resulting object depends on the class of
  # +numeric+ and on the magnitude of the result. It may return a Bignum.
  def /(other) end

  # Returns +true+ if the value of +fix+ is less than that of +real+.
  def <(other) end

  # Shifts +fix+ left +count+ positions, or right if +count+ is negative.
  def <<(count) end

  # Returns +true+ if the value of +fix+ is less than or equal to that of
  # +real+.
  def <=(other) end

  # Comparison---Returns +-1+, +0+, ++1+ or +nil+ depending on whether +fix+ is
  # less than, equal to, or greater than +numeric+.
  #
  # This is the basis for the tests in the Comparable module.
  #
  # +nil+ is returned if the two values are incomparable.
  def <=>(other) end

  # Return +true+ if +fix+ equals +other+ numerically.
  #
  #   1 == 2      #=> false
  #   1 == 1.0    #=> true
  def ==(other) end
  alias === ==

  # Returns +true+ if the value of +fix+ is greater than that of +real+.
  def >(other) end

  # Returns +true+ if the value of +fix+ is greater than or equal to that of
  # +real+.
  def >=(other) end

  # Shifts +fix+ right +count+ positions, or left if +count+ is negative.
  def >>(other) end

  # Bit Reference---Returns the +n+th bit in the binary representation of
  # +fix+, where <code>fix[0]</code> is the least significant bit.
  #
  # For example:
  #
  #    a = 0b11001100101010
  #    30.downto(0) do |n| print a[n] end
  #    #=> 0000000000000000011001100101010
  def [](n) end

  # Bitwise EXCLUSIVE OR.
  def ^(other) end

  # Bitwise OR.
  def |(other) end

  # One's complement: returns a number where each bit is flipped.
  def ~; end

  # Returns the absolute value of +fix+.
  #
  #    -12345.abs   #=> 12345
  #    12345.abs    #=> 12345
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
  #
  # This method can be used to detect overflow in Array#pack as follows.
  #
  #    if n.bit_length < 32
  #      [n].pack("l") # no overflow
  #    else
  #      raise "overflow"
  #    end
  def bit_length; end

  # Performs integer division: returns integer result of dividing +fix+ by
  # +numeric+.
  def div(numeric) end

  # See Numeric#divmod.
  def divmod(numeric) end

  # Returns +true+ if +fix+ is an even number.
  def even?; end

  # Returns the floating point result of dividing +fix+ by +numeric+.
  #
  #    654321.fdiv(13731)      #=> 47.6528293642124
  #    654321.fdiv(13731.24)   #=> 47.6519964693647
  def fdiv(numeric) end

  # Returns +true+ if +fix+ is an odd number.
  def odd?; end

  # Returns the number of bytes in the machine representation of +fix+.
  #
  #    1.size            #=> 4
  #    -1.size           #=> 4
  #    2147483647.size   #=> 4
  def size; end

  # Returns the Integer equal to +int+ + 1.
  #
  #    1.next      #=> 2
  #    (-1).next   #=> 0
  def succ; end

  # Converts +fix+ to a Float.
  def to_f; end

  # Returns a string containing the representation of +fix+ radix +base+
  # (between 2 and 36).
  #
  #    12345.to_s       #=> "12345"
  #    12345.to_s(2)    #=> "11000000111001"
  #    12345.to_s(8)    #=> "30071"
  #    12345.to_s(10)   #=> "12345"
  #    12345.to_s(16)   #=> "3039"
  #    12345.to_s(36)   #=> "9ix"
  def to_s(base = 10) end
  alias inspect to_s

  # Returns +true+ if +fix+ is zero.
  def zero?; end
end
