# frozen_string_literal: true

# The class of the singleton object +nil+.
#
# Several of its methods act as operators:
#
# - #&
# - #|
# - #===
# - #=~
# - #^
#
# Others act as converters, carrying the concept of _nullity_
# to other classes:
#
# - #rationalize
# - #to_a
# - #to_c
# - #to_h
# - #to_r
# - #to_s
#
# While +nil+ doesn't have an explicitly defined #to_hash method,
# it can be used in <code>**</code> unpacking, not adding any
# keyword arguments.
#
# Another method provides inspection:
#
# - #inspect
#
# Finally, there is this query method:
#
# - #nil?
class NilClass
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

  # Returns +nil+.
  #
  # This method makes it useful to write:
  #
  #   while gets =~ /re/
  #     # ...
  #   end
  def =~(object) end

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

  # Returns string <tt>'nil'</tt>:
  #
  #   nil.inspect # => "nil"
  def inspect; end

  # Returns +true+.
  # For all other objects, method <tt>nil?</tt> returns +false+.
  def nil?; end

  # Returns zero as a Rational:
  #
  #   nil.rationalize # => (0/1)
  #
  # Argument +eps+ is ignored.
  def rationalize(eps = nil) end

  # Returns an empty Array.
  #
  #   nil.to_a # => []
  def to_a; end

  # Returns zero as a Complex:
  #
  #   nil.to_c # => (0+0i)
  def to_c; end

  # Always returns zero.
  #
  #    nil.to_f   #=> 0.0
  def to_f; end

  # Returns an empty Hash.
  #
  #   nil.to_h   #=> {}
  def to_h; end

  # Always returns zero.
  #
  #    nil.to_i   #=> 0
  def to_i; end

  # Returns zero as a Rational:
  #
  #   nil.to_r # => (0/1)
  def to_r; end

  # Returns an empty String:
  #
  #   nil.to_s # => ""
  def to_s; end
end
