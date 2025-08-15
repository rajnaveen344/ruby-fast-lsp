# frozen_string_literal: true

# The class of the singleton object <code>nil</code>.
class NilClass
  # And---Returns <code>false</code>. <i>obj</i> is always
  # evaluated as it is the argument to a method call---there is no
  # short-circuit evaluation in this case.
  def &(other) end

  # Case Equality -- For class Object, effectively the same as calling
  # <code>#==</code>, but typically overridden by descendants to provide
  # meaningful semantics in +case+ statements.
  def ===(other) end

  # Exclusive Or---If <i>obj</i> is <code>nil</code> or
  # <code>false</code>, returns <code>false</code>; otherwise, returns
  # <code>true</code>.
  def ^(other) end

  # Or---Returns <code>false</code> if <i>obj</i> is
  # <code>nil</code> or <code>false</code>; <code>true</code> otherwise.
  def |(other) end

  # Always returns the string "nil".
  def inspect; end

  # Only the object <i>nil</i> responds <code>true</code> to <code>nil?</code>.
  def nil?; end

  # Returns zero as a rational.  The optional argument +eps+ is always
  # ignored.
  def rationalize(*eps) end

  # Always returns an empty array.
  #
  #    nil.to_a   #=> []
  def to_a; end

  # Returns zero as a complex.
  def to_c; end

  # Always returns zero.
  #
  #    nil.to_f   #=> 0.0
  def to_f; end

  # Always returns an empty hash.
  #
  #    nil.to_h   #=> {}
  def to_h; end

  # Always returns zero.
  #
  #    nil.to_i   #=> 0
  def to_i; end

  # Returns zero as a rational.
  def to_r; end

  # Always returns the empty string.
  def to_s; end
end
