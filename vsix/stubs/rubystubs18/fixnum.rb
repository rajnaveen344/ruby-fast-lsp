# frozen_string_literal: true

# A <code>Fixnum</code> holds <code>Integer</code> values that can be
# represented in a native machine word (minus 1 bit). If any operation
# on a <code>Fixnum</code> exceeds this range, the value is
# automatically converted to a <code>Bignum</code>.
#
# <code>Fixnum</code> objects have immediate value. This means that
# when they are assigned or passed as parameters, the actual object is
# passed, rather than a reference to that object. Assignment does not
# alias <code>Fixnum</code> objects. There is effectively only one
# <code>Fixnum</code> object instance for any given integer value, so,
# for example, you cannot add a singleton method to a
# <code>Fixnum</code>.
class Fixnum < Integer
  include Precision

  # Convert <code>obj</code> to a Fixnum. Works with numeric parameters.
  # Also works with Symbols, but this is deprecated.
  def self.induced_from(obj) end

  # Returns <code>fix</code> modulo <code>other</code>.
  # See <code>Numeric.divmod</code> for more information.
  def %(other) end
  alias modulo %

  # Bitwise AND.
  def &(other) end

  # Performs multiplication: the class of the resulting object depends on
  # the class of <code>numeric</code> and on the magnitude of the
  # result.
  def *(other) end

  # Raises <code>fix</code> to the <code>other</code> power, which may
  # be negative or fractional.
  #
  #   2 ** 3      #=> 8
  #   2 ** -1     #=> 0.5
  #   2 ** 0.5    #=> 1.4142135623731
  def **(other) end

  # Performs addition: the class of the resulting object depends on
  # the class of <code>numeric</code> and on the magnitude of the
  # result.
  def +(other) end

  # Performs subtraction: the class of the resulting object depends on
  # the class of <code>numeric</code> and on the magnitude of the
  # result.
  def -(other) end

  # Negates <code>fix</code> (which might return a Bignum).
  def -@; end

  # Performs division: the class of the resulting object depends on
  # the class of <code>numeric</code> and on the magnitude of the
  # result.
  def /(other) end
  alias div /

  # Returns <code>true</code> if the value of <code>fix</code> is
  # less than that of <code>other</code>.
  def <(other) end

  # Shifts _fix_ left _count_ positions (right if _count_ is negative).
  def <<(count) end

  # Returns <code>true</code> if the value of <code>fix</code> is
  # less thanor equal to that of <code>other</code>.
  def <=(other) end

  # Comparison---Returns -1, 0, or +1 depending on whether <i>fix</i> is
  # less than, equal to, or greater than <i>numeric</i>. This is the
  # basis for the tests in <code>Comparable</code>.
  def <=>(other) end

  # Return <code>true</code> if <code>fix</code> equals <code>other</code>
  # numerically.
  #
  #   1 == 2      #=> false
  #   1 == 1.0    #=> true
  def ==(other) end

  # Returns <code>true</code> if the value of <code>fix</code> is
  # greater than that of <code>other</code>.
  def >(other) end

  # Returns <code>true</code> if the value of <code>fix</code> is
  # greater than or equal to that of <code>other</code>.
  def >=(other) end

  # Shifts _fix_ right _count_ positions (left if _count_ is negative).
  def >>(other) end

  # Bit Reference---Returns the <em>n</em>th bit in the binary
  # representation of <i>fix</i>, where <i>fix</i>[0] is the least
  # significant bit.
  #
  #    a = 0b11001100101010
  #    30.downto(0) do |n| print a[n] end
  #
  # <em>produces:</em>
  #
  #    0000000000000000011001100101010
  def [](n) end

  # Bitwise EXCLUSIVE OR.
  def ^(other) end

  # Bitwise OR.
  def |(other) end

  # One's complement: returns a number where each bit is flipped.
  def ~; end

  # Returns the absolute value of <i>fix</i>.
  #
  #    -12345.abs   #=> 12345
  #    12345.abs    #=> 12345
  def abs; end

  # See <code>Numeric#divmod</code>.
  def divmod(numeric) end

  # Returns <code>true</code> if <i>fix</i> is an even number.
  def even?; end

  # Returns the name of the object whose symbol id is <i>fix</i>. If
  # there is no symbol in the symbol table with this value, returns
  # <code>nil</code>. <code>id2name</code> has nothing to do with the
  # <code>Object.id</code> method. See also <code>Fixnum#to_sym</code>,
  # <code>String#intern</code>, and class <code>Symbol</code>.
  #
  #    symbol = :@inst_var    #=> :@inst_var
  #    id     = symbol.to_i   #=> 9818
  #    id.id2name             #=> "@inst_var"
  def id2name; end

  # Returns <code>true</code> if <i>fix</i> is an odd number.
  def odd?; end

  # Returns the floating point result of dividing <i>fix</i> by
  # <i>numeric</i>.
  #
  #    654321.quo(13731)      #=> 47.6528293642124
  #    654321.quo(13731.24)   #=> 47.6519964693647
  def quo(numeric) end
  alias fdiv quo

  # Returns the number of <em>bytes</em> in the machine representation
  # of a <code>Fixnum</code>.
  #
  #    1.size            #=> 4
  #    -1.size           #=> 4
  #    2147483647.size   #=> 4
  def size; end

  # Converts <i>fix</i> to a <code>Float</code>.
  def to_f; end

  # Returns a string containing the representation of <i>fix</i> radix
  # <i>base</i> (between 2 and 36).
  #
  #    12345.to_s       #=> "12345"
  #    12345.to_s(2)    #=> "11000000111001"
  #    12345.to_s(8)    #=> "30071"
  #    12345.to_s(10)   #=> "12345"
  #    12345.to_s(16)   #=> "3039"
  #    12345.to_s(36)   #=> "9ix"
  def to_s(base = 10) end

  # Returns the symbol whose integer value is <i>fix</i>. See also
  # <code>Fixnum#id2name</code>.
  #
  #    fred = :fred.to_i
  #    fred.id2name   #=> "fred"
  #    fred.to_sym    #=> :fred
  def to_sym; end

  # Returns <code>true</code> if <i>fix</i> is zero.
  def zero?; end
end
