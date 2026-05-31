# frozen_string_literal: true

# The class of the singleton object <code>nil</code>.
class NilClass
  # And---Returns <code>false</code>. <i>obj</i> is always
  # evaluated as it is the argument to a method call---there is no
  # short-circuit evaluation in this case.
  def &(other) end

  # Exclusive Or---If <i>obj</i> is <code>nil</code> or
  # <code>false</code>, returns <code>false</code>; otherwise, returns
  # <code>true</code>.
  def ^(other) end

  # Or---Returns <code>false</code> if <i>obj</i> is
  # <code>nil</code> or <code>false</code>; <code>true</code> otherwise.
  def |(other) end

  # Always returns the string "nil".
  def inspect; end

  # call_seq:
  #   nil.nil?               => true
  #
  # Only the object <i>nil</i> responds <code>true</code> to <code>nil?</code>.
  def nil?; end

  # Always returns an empty array.
  #
  #    nil.to_a   #=> []
  def to_a; end

  # Always returns zero.
  #
  #    nil.to_f   #=> 0.0
  def to_f; end

  # Always returns zero.
  #
  #    nil.to_i   #=> 0
  def to_i; end

  # Always returns the empty string.
  #
  #    nil.to_s   #=> ""
  def to_s; end
end
