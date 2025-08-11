# frozen_string_literal: true

# The global value <code>false</code> is the only instance of class
# FalseClass and represents a logically false value in
# boolean expressions. The class provides operators allowing
# <code>false</code> to participate correctly in logical expressions.
class FalseClass
  # Returns +false+:
  #
  #   false & true       # => false
  #   false & Object.new # => false
  #
  # Argument +object+ is evaluated:
  #
  #   false & raise # Raises RuntimeError.
  def &(other) end

  # Returns +true+ or +false+.
  #
  # Like Object#==, if +object+ is an instance of Object
  # (and not an instance of one of its many subclasses).
  #
  # This method is commonly overridden by those subclasses,
  # to provide meaningful semantics in +case+ statements.
  def ===(other) end

  # Returns +false+ if +object+ is +nil+ or +false+, +true+ otherwise:
  #
  #   nil ^ nil        # => false
  #   nil ^ false      # => false
  #   nil ^ Object.new # => true
  def ^(other) end

  # Returns +false+ if +object+ is +nil+ or +false+, +true+ otherwise:
  #
  #   nil | nil        # => false
  #   nil | false      # => false
  #   nil | Object.new # => true
  def |(other) end

  # The string representation of <code>false</code> is "false".
  def to_s; end
  alias inspect to_s
end
