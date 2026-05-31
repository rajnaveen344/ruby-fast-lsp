# frozen_string_literal: true

# <code>Integer</code> is the basis for the two concrete classes that
# hold whole numbers, <code>Bignum</code> and <code>Fixnum</code>.
class Integer < Numeric
  include Precision

  # Convert <code>obj</code> to an Integer.
  def self.induced_from(obj) end

  # Returns a string containing the ASCII character represented by the
  # receiver's value.
  #
  #    65.chr    #=> "A"
  #    ?a.chr    #=> "a"
  #    230.chr   #=> "\346"
  def chr; end

  # Iterates <em>block</em>, passing decreasing values from <i>int</i>
  # down to and including <i>limit</i>.
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

  # Always returns <code>true</code>.
  def integer?; end

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

  # Returns the <code>Integer</code> equal to <i>int</i> + 1.
  #
  #    1.next      #=> 2
  #    (-1).next   #=> 0
  def succ; end
  alias next succ

  # Iterates block <i>int</i> times, passing in values from zero to
  # <i>int</i> - 1.
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
  alias round to_i
  alias truncate to_i

  # Iterates <em>block</em>, passing in integer values from <i>int</i>
  # up to and including <i>limit</i>.
  #
  #    5.upto(10) { |i| print i, " " }
  #
  # <em>produces:</em>
  #
  #    5 6 7 8 9 10
  def upto(limit) end
end
