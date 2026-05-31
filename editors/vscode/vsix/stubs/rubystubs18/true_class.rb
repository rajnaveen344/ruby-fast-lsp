# frozen_string_literal: true

# The global value <code>true</code> is the only instance of class
# <code>TrueClass</code> and represents a logically true value in
# boolean expressions. The class provides operators allowing
# <code>true</code> to be used in logical expressions.
class TrueClass
  # And---Returns <code>false</code> if <i>obj</i> is
  # <code>nil</code> or <code>false</code>, <code>true</code> otherwise.
  def &(other) end

  # Exclusive Or---Returns <code>true</code> if <i>obj</i> is
  # <code>nil</code> or <code>false</code>, <code>false</code>
  # otherwise.
  def ^(other) end

  # Or---Returns <code>true</code>. As <i>anObject</i> is an argument to
  # a method call, it is always evaluated; there is no short-circuit
  # evaluation in this case.
  #
  #    true |  puts("or")
  #    true || puts("logical or")
  #
  # <em>produces:</em>
  #
  #    or
  def |(other) end

  # The string representation of <code>true</code> is "true".
  def to_s; end
end
