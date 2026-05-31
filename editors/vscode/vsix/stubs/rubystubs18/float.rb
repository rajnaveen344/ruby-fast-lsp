# frozen_string_literal: true

# <code>Float</code> objects represent real numbers using the native
# architecture's double-precision floating point representation.
class Float < Numeric
  include Precision

  DIG = _
  EPSILON = _
  MANT_DIG = _
  MAX = _
  MAX_10_EXP = _
  MAX_EXP = _
  MIN = _
  MIN_10_EXP = _
  MIN_EXP = _
  RADIX = _
  ROUNDS = _

  # Convert <code>obj</code> to a float.
  def self.induced_from(obj) end

  # Return the modulo after division of <code>flt</code> by <code>other</code>.
  #
  #    6543.21.modulo(137)      #=> 104.21
  #    6543.21.modulo(137.24)   #=> 92.9299999999996
  def %(other) end
  alias modulo %

  # Returns a new float which is the product of <code>float</code>
  # and <code>other</code>.
  def *(other) end

  #  flt ** other   => float
  #
  # Raises <code>float</code> the <code>other</code> power.
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

  # <code>true</code> if <code>flt</code> is less than <code>other</code>.
  def <(other) end

  # <code>true</code> if <code>flt</code> is less than
  # or equal to <code>other</code>.
  def <=(other) end

  # Returns -1, 0, or +1 depending on whether <i>flt</i> is less than,
  # equal to, or greater than <i>numeric</i>. This is the basis for the
  # tests in <code>Comparable</code>.
  def <=>(other) end

  # Returns <code>true</code> only if <i>obj</i> has the same value
  # as <i>flt</i>. Contrast this with <code>Float#eql?</code>, which
  # requires <i>obj</i> to be a <code>Float</code>.
  #
  #    1.0 == 1   #=> true
  def ==(other) end

  # <code>true</code> if <code>flt</code> is greater than <code>other</code>.
  def >(other) end

  # <code>true</code> if <code>flt</code> is greater than
  # or equal to <code>other</code>.
  def >=(other) end

  # Returns the absolute value of <i>flt</i>.
  #
  #    (-34.56).abs   #=> 34.56
  #    -34.56.abs     #=> 34.56
  def abs; end

  # Returns the smallest <code>Integer</code> greater than or equal to
  # <i>flt</i>.
  #
  #    1.2.ceil      #=> 2
  #    2.0.ceil      #=> 2
  #    (-1.2).ceil   #=> -1
  #    (-2.0).ceil   #=> -2
  def ceil; end

  # MISSING: documentation
  def coerce(p1) end

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

  # Rounds <i>flt</i> to the nearest integer. Equivalent to:
  #
  #    def round
  #      return (self+0.5).floor if self > 0.0
  #      return (self-0.5).ceil  if self < 0.0
  #      return 0
  #    end
  #
  #    1.5.round      #=> 2
  #    (-1.5).round   #=> -2
  def round; end

  # As <code>flt</code> is already a float, returns <i>self</i>.
  def to_f; end

  # Returns <i>flt</i> truncated to an <code>Integer</code>.
  def to_i; end
  alias to_int to_i
  alias truncate to_i

  # Returns a string containing a representation of self. As well as a
  # fixed or exponential form of the number, the call may return
  # ``<code>NaN</code>'', ``<code>Infinity</code>'', and
  # ``<code>-Infinity</code>''.
  def to_s; end

  # Returns <code>true</code> if <i>flt</i> is 0.0.
  def zero?; end
end
