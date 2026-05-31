# frozen_string_literal: true

# The Comparable mixin is used by classes whose objects may be
# ordered. The class must define the <code><=></code> operator,
# which compares the receiver against another object, returning a
# value less than 0, returning 0, or returning a value greater than 0,
# depending on whether the receiver is less than, equal to,
# or greater than the other object. If the other object is not
# comparable then the <code><=></code> operator should return +nil+.
# Comparable uses <code><=></code> to implement the conventional
# comparison operators (<code><</code>, <code><=</code>,
# <code>==</code>, <code>>=</code>, and <code>></code>) and the
# method <code>between?</code>.
#
#    class SizeMatters
#      include Comparable
#      attr :str
#      def <=>(other)
#        str.size <=> other.str.size
#      end
#      def initialize(str)
#        @str = str
#      end
#      def inspect
#        @str
#      end
#    end
#
#    s1 = SizeMatters.new("Z")
#    s2 = SizeMatters.new("YY")
#    s3 = SizeMatters.new("XXX")
#    s4 = SizeMatters.new("WWWW")
#    s5 = SizeMatters.new("VVVVV")
#
#    s1 < s2                       #=> true
#    s4.between?(s1, s3)           #=> false
#    s4.between?(s3, s5)           #=> true
#    [ s3, s2, s5, s4, s1 ].sort   #=> [Z, YY, XXX, WWWW, VVVVV]
#
# == What's Here
#
# \Module \Comparable provides these methods, all of which use method <tt><=></tt>:
#
# - #<: Returns whether +self+ is less than the given object.
# - #<=: Returns whether +self+ is less than or equal to the given object.
# - #==: Returns whether +self+ is equal to the given object.
# - #>: Returns whether +self+ is greater than or equal to the given object.
# - #>=: Returns whether +self+ is greater than the given object.
# - #between?: Returns +true+ if +self+ is between two given objects.
# - #clamp: For given objects +min+ and +max+, or range <tt>(min..max)</tt>, returns:
#
#   - +min+ if <tt>(self <=> min) < 0</tt>.
#   - +max+ if <tt>(self <=> max) > 0</tt>.
#   - +self+ otherwise.
module Comparable
  # Compares two objects based on the receiver's <code><=></code>
  # method, returning true if it returns a value less than 0.
  def <(other) end

  # Compares two objects based on the receiver's <code><=></code>
  # method, returning true if it returns a value less than or equal to 0.
  def <=(other) end

  # Compares two objects based on the receiver's <code><=></code>
  # method, returning true if it returns 0. Also returns true if
  # _obj_ and _other_ are the same object.
  def ==(other) end

  # Compares two objects based on the receiver's <code><=></code>
  # method, returning true if it returns a value greater than 0.
  def >(other) end

  # Compares two objects based on the receiver's <code><=></code>
  # method, returning true if it returns a value greater than or equal to 0.
  def >=(other) end

  # Returns <code>false</code> if _obj_ <code><=></code> _min_ is less
  # than zero or if _obj_ <code><=></code> _max_ is greater than zero,
  # <code>true</code> otherwise.
  #
  #    3.between?(1, 5)               #=> true
  #    6.between?(1, 5)               #=> false
  #    'cat'.between?('ant', 'dog')   #=> true
  #    'gnu'.between?('ant', 'dog')   #=> false
  def between?(min, max) end

  # In <code>(min, max)</code> form, returns _min_ if _obj_
  # <code><=></code> _min_ is less than zero, _max_ if _obj_
  # <code><=></code> _max_ is greater than zero, and _obj_
  # otherwise.
  #
  #     12.clamp(0, 100)         #=> 12
  #     523.clamp(0, 100)        #=> 100
  #     -3.123.clamp(0, 100)     #=> 0
  #
  #     'd'.clamp('a', 'f')      #=> 'd'
  #     'z'.clamp('a', 'f')      #=> 'f'
  #
  # In <code>(range)</code> form, returns _range.begin_ if _obj_
  # <code><=></code> _range.begin_ is less than zero, _range.end_
  # if _obj_ <code><=></code> _range.end_ is greater than zero, and
  # _obj_ otherwise.
  #
  #     12.clamp(0..100)         #=> 12
  #     523.clamp(0..100)        #=> 100
  #     -3.123.clamp(0..100)     #=> 0
  #
  #     'd'.clamp('a'..'f')      #=> 'd'
  #     'z'.clamp('a'..'f')      #=> 'f'
  #
  # If _range.begin_ is +nil+, it is considered smaller than _obj_,
  # and if _range.end_ is +nil+, it is considered greater than
  # _obj_.
  #
  #     -20.clamp(0..)           #=> 0
  #     523.clamp(..100)         #=> 100
  #
  # When _range.end_ is excluded and not +nil+, an exception is
  # raised.
  #
  #     100.clamp(0...100)       # ArgumentError
  def clamp(...) end
end
