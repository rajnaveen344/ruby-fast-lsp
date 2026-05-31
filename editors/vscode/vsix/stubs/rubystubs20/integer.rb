# frozen_string_literal: true

# <code>Integer</code> is the basis for the two concrete classes that
# hold whole numbers, <code>Bignum</code> and <code>Fixnum</code>.
class Integer < Numeric
  # Returns a string containing the character represented by the
  # receiver's value according to +encoding+.
  #
  #    65.chr    #=> "A"
  #    230.chr   #=> "\346"
  #    255.chr(Encoding::UTF_8)   #=> "\303\277"
  def chr(*encoding) end

  # Returns 1.
  def denominator; end

  # Iterates <em>block</em>, passing decreasing values from <i>int</i>
  # down to and including <i>limit</i>.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    5.downto(1) { |n| print n, ".. " }
  #    print "  Liftoff!\n"
  #
  # <em>produces:</em>
  #
  #    5.. 4.. 3.. 2.. 1..   Liftoff!
  def downto(limit) end

  # Returns <code>true</code> if <i>int</i> is an even number.
  def even?; end

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

  # Always returns <code>true</code>.
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

  # Returns <code>true</code> if <i>int</i> is an odd number.
  def odd?; end

  # Returns the int itself.
  #
  #    ?a.ord    #=> 97
  #
  # This method is intended for compatibility to
  # character constant in Ruby 1.9.
  # For example, ?a.ord returns 97 both in 1.8 and 1.9.
  def ord; end

  # Returns the <code>Integer</code> equal to <i>int</i> - 1.
  #
  #    1.pred      #=> 0
  #    (-1).pred   #=> -2
  def pred; end

  # Returns the value as a rational.  The optional argument eps is
  # always ignored.
  def rationalize(*eps) end

  # Rounds <i>flt</i> to a given precision in decimal digits (default 0 digits).
  # Precision may be negative.  Returns a floating point number when +ndigits+
  # is positive, +self+ for zero, and round down for negative.
  #
  #    1.round        #=> 1
  #    1.round(2)     #=> 1.0
  #    15.round(-1)   #=> 20
  def round(*ndigits) end

  # Returns the <code>Integer</code> equal to <i>int</i> + 1.
  #
  #    1.next      #=> 2
  #    (-1).next   #=> 0
  def succ; end
  alias next succ

  # Iterates block <i>int</i> times, passing in values from zero to
  # <i>int</i> - 1.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    5.times do |i|
  #      print i, " "
  #    end
  #
  # <em>produces:</em>
  #
  #    0 1 2 3 4
  def times; end

  # As <i>int</i> is already an <code>Integer</code>, all these
  # methods simply return the receiver.
  def to_i; end
  alias to_int to_i
  alias floor to_i
  alias ceil to_i
  alias truncate to_i

  # Returns the value as a rational.
  #
  #    1.to_r        #=> (1/1)
  #    (1<<64).to_r  #=> (18446744073709551616/1)
  def to_r; end

  # Iterates <em>block</em>, passing in integer values from <i>int</i>
  # up to and including <i>limit</i>.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    5.upto(10) { |i| print i, " " }
  #
  # <em>produces:</em>
  #
  #    5 6 7 8 9 10
  def upto(limit) end
end
