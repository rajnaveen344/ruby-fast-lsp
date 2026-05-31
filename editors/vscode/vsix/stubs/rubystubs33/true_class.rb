# frozen_string_literal: true

# The class of the singleton object +true+.
#
# Several of its methods act as operators:
#
# - #&
# - #|
# - #===
# - #^
#
# One other method:
#
# - #to_s and its alias #inspect.
class TrueClass
  # Returns +false+ if +object+ is +false+ or +nil+, +true+ otherwise:
  #
  # true & Object.new # => true
  # true & false      # => false
  # true & nil        # => false
  def &(other) end

  # Returns +true+ or +false+.
  #
  # Like Object#==, if +object+ is an instance of Object
  # (and not an instance of one of its many subclasses).
  #
  # This method is commonly overridden by those subclasses,
  # to provide meaningful semantics in +case+ statements.
  def ===(other) end

  # Returns +true+ if +object+ is +false+ or +nil+, +false+ otherwise:
  #
  #   true ^ Object.new # => false
  #   true ^ false      # => true
  #   true ^ nil        # => true
  def ^(other) end

  # Returns +true+:
  #
  #   true | Object.new # => true
  #   true | false      # => true
  #   true | nil        # => true
  #
  # Argument +object+ is evaluated.
  # This is different from +true+ with the short-circuit operator,
  # whose operand is evaluated only if necessary:
  #
  #   true | raise # => Raises RuntimeError.
  #   true || raise # => true
  def |(other) end

  # Returns string <tt>'true'</tt>:
  #
  #   true.to_s # => "true"
  #
  # TrueClass#inspect is an alias for TrueClass#to_s.
  def to_s; end
  alias inspect to_s
end
