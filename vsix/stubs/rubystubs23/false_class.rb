# frozen_string_literal: true

# The global value <code>false</code> is the only instance of class
# <code>FalseClass</code> and represents a logically false value in
# boolean expressions. The class provides operators allowing
# <code>false</code> to participate correctly in logical expressions.
class FalseClass
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

  # 'nuf said...
  def to_s; end
  alias inspect to_s
end
