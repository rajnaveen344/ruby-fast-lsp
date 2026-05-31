# frozen_string_literal: true

# <code>Float</code> objects represent inexact real numbers using
# the native architecture's double-precision floating point
# representation.
#
# Floating point has a different arithmetic and is a inexact number.
# So you should know its esoteric system. see following:
#
# - http://docs.sun.com/source/806-3568/ncg_goldberg.html
# - http://wiki.github.com/rdp/ruby_tutorials_core/ruby-talk-faq#floats_imprecise
# - http://en.wikipedia.org/wiki/Floating_point#Accuracy_problems
class Float < Numeric
  DIG = _
  EPSILON = _
  INFINITY = _
  MANT_DIG = _
  MAX = _
  MAX_10_EXP = _
  MAX_EXP = _
  MIN = _
  MIN_10_EXP = _
  MIN_EXP = _
  NAN = _
  RADIX = _
  ROUNDS = _

  # Return the modulo after division of <code>flt</code> by <code>other</code>.
  #
  #    6543.21.modulo(137)      #=> 104.21
  #    6543.21.modulo(137.24)   #=> 92.9299999999996
  def %(other) end
  alias modulo %

  # Returns a new float which is the product of <code>float</code>
  # and <code>other</code>.
  def *(other) end

  #  flt ** other  ->  float
  #
  # Raises <code>float</code> the <code>other</code> power.
  #
  #    2.0**3      #=> 8.0
  def **(other) end

  # Returns a new float which is the sum of <code>float</code>
  # and <code>other</code>.
  def +(other) end

  # Returns a new float which is the difference of <code>float</code>
  # and <code>other</code>.
  def -(other) end

  # Returns float, negated.
  def -@; end

  # Returns a new float which is the result of dividing
  # <code>float</code> by <code>other</code>.
  def /(other) end

  # <code>true</code> if <code>flt</code> is less than <code>real</code>.
  def <(other) end

  # <code>true</code> if <code>flt</code> is less than
  # or equal to <code>real</code>.
  def <=(other) end

  # Returns -1, 0, +1 or nil depending on whether <i>flt</i> is less
  # than, equal to, or greater than <i>real</i>. This is the basis for
  # the tests in <code>Comparable</code>.
  def <=>(other) end

  # Returns <code>true</code> only if <i>obj</i> has the same value
  # as <i>flt</i>. Contrast this with <code>Float#eql?</code>, which
  # requires <i>obj</i> to be a <code>Float</code>.
  #
  #    1.0 == 1   #=> true
  def ==(other) end
  alias === ==

  # <code>true</code> if <code>flt</code> is greater than <code>real</code>.
  def >(other) end

  # <code>true</code> if <code>flt</code> is greater than
  # or equal to <code>real</code>.
  def >=(other) end

  # Returns the absolute value of <i>flt</i>.
  #
  #    (-34.56).abs   #=> 34.56
  #    -34.56.abs     #=> 34.56
  def abs; end
  alias magnitude abs

  # Returns 0 if the value is positive, pi otherwise.
  def arg; end
  alias angle arg
  alias phase arg

  # Returns the smallest <code>Integer</code> greater than or equal to
  # <i>flt</i>.
  #
  #    1.2.ceil      #=> 2
  #    2.0.ceil      #=> 2
  #    (-1.2).ceil   #=> -1
  #    (-2.0).ceil   #=> -2
  def ceil; end

  # Returns an array with both <i>aNumeric</i> and <i>flt</i> represented
  # as <code>Float</code> objects.
  # This is achieved by converting <i>aNumeric</i> to a <code>Float</code>.
  #
  #    1.2.coerce(3)       #=> [3.0, 1.2]
  #    2.5.coerce(1.1)     #=> [1.1, 2.5]
  def coerce(numeric) end

  # Returns the denominator (always positive).  The result is machine
  # dependent.
  #
  # See numerator.
  def denominator; end

  # See <code>Numeric#divmod</code>.
  def divmod(numeric) end

  # Returns <code>true</code> only if <i>obj</i> is a
  # <code>Float</code> with the same value as <i>flt</i>. Contrast this
  # with <code>Float#==</code>, which performs type conversions.
  #
  #    1.0.eql?(1)   #=> false
  def eql?(other) end

  # Returns <code>true</code> if <i>flt</i> is a valid IEEE floating
  # point number (it is not infinite, and <code>nan?</code> is
  # <code>false</code>).
  def finite?; end

  # Returns the largest integer less than or equal to <i>flt</i>.
  #
  #    1.2.floor      #=> 1
  #    2.0.floor      #=> 2
  #    (-1.2).floor   #=> -2
  #    (-2.0).floor   #=> -2
  def floor; end

  # Returns a hash code for this float.
  def hash; end

  # Returns <code>nil</code>, -1, or +1 depending on whether <i>flt</i>
  # is finite, -infinity, or +infinity.
  #
  #    (0.0).infinite?        #=> nil
  #    (-1.0/0.0).infinite?   #=> -1
  #    (+1.0/0.0).infinite?   #=> 1
  def infinite?; end

  # Returns <code>true</code> if <i>flt</i> is an invalid IEEE floating
  # point number.
  #
  #    a = -1.0      #=> -1.0
  #    a.nan?        #=> false
  #    a = 0.0/0.0   #=> NaN
  #    a.nan?        #=> true
  def nan?; end

  # Returns the numerator.  The result is machine dependent.
  #
  # For example:
  #
  #    n = 0.3.numerator    #=> 5404319552844595
  #    d = 0.3.denominator  #=> 18014398509481984
  #    n.fdiv(d)            #=> 0.3
  def numerator; end

  # Returns float / numeric.
  def quo(numeric) end
  alias fdiv quo

  # Returns a simpler approximation of the value (flt-|eps| <= result
  # <= flt+|eps|).  if eps is not given, it will be chosen
  # automatically.
  #
  # For example:
  #
  #    0.3.rationalize          #=> (3/10)
  #    1.333.rationalize        #=> (1333/1000)
  #    1.333.rationalize(0.01)  #=> (4/3)
  def rationalize(*eps) end

  # Rounds <i>flt</i> to a given precision in decimal digits (default 0 digits).
  # Precision may be negative.  Returns a floating point number when ndigits
  # is more than zero.
  #
  #    1.4.round      #=> 1
  #    1.5.round      #=> 2
  #    1.6.round      #=> 2
  #    (-1.5).round   #=> -2
  #
  #    1.234567.round(2)  #=> 1.23
  #    1.234567.round(3)  #=> 1.235
  #    1.234567.round(4)  #=> 1.2346
  #    1.234567.round(5)  #=> 1.23457
  #
  #    34567.89.round(-5) #=> 0
  #    34567.89.round(-4) #=> 30000
  #    34567.89.round(-3) #=> 35000
  #    34567.89.round(-2) #=> 34600
  #    34567.89.round(-1) #=> 34570
  #    34567.89.round(0)  #=> 34568
  #    34567.89.round(1)  #=> 34567.9
  #    34567.89.round(2)  #=> 34567.89
  #    34567.89.round(3)  #=> 34567.89
  def round(*ndigits) end

  # As <code>flt</code> is already a float, returns +self+.
  def to_f; end

  # Returns <i>flt</i> truncated to an <code>Integer</code>.
  def to_i; end
  alias to_int to_i
  alias truncate to_i

  # Returns the value as a rational.
  #
  # NOTE: 0.3.to_r isn't the same as '0.3'.to_r.  The latter is
  # equivalent to '3/10'.to_r, but the former isn't so.
  #
  # For example:
  #
  #    2.0.to_r    #=> (2/1)
  #    2.5.to_r    #=> (5/2)
  #    -0.75.to_r  #=> (-3/4)
  #    0.0.to_r    #=> (0/1)
  def to_r; end

  # Returns a string containing a representation of self. As well as a
  # fixed or exponential form of the number, the call may return
  # ``<code>NaN</code>'', ``<code>Infinity</code>'', and
  # ``<code>-Infinity</code>''.
  def to_s; end

  # Returns <code>true</code> if <i>flt</i> is 0.0.
  def zero?; end
end
