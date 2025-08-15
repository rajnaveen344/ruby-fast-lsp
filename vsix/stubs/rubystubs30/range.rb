# frozen_string_literal: true

# A Range represents an interval---a set of values with a
# beginning and an end. Ranges may be constructed using the
# <em>s</em><code>..</code><em>e</em> and
# <em>s</em><code>...</code><em>e</em> literals, or with
# Range::new. Ranges constructed using <code>..</code>
# run from the beginning to the end inclusively. Those created using
# <code>...</code> exclude the end value. When used as an iterator,
# ranges return each value in the sequence.
#
#    (-1..-5).to_a      #=> []
#    (-5..-1).to_a      #=> [-5, -4, -3, -2, -1]
#    ('a'..'e').to_a    #=> ["a", "b", "c", "d", "e"]
#    ('a'...'e').to_a   #=> ["a", "b", "c", "d"]
#
# == Beginless/Endless Ranges
#
# A "beginless range" and "endless range" represents a semi-infinite
# range.  Literal notation for a beginless range is:
#
#    (..1)
#    # or
#    (...1)
#
# Literal notation for an endless range is:
#
#    (1..)
#    # or similarly
#    (1...)
#
# Which is equivalent to
#
#    (1..nil)  # or similarly (1...nil)
#    Range.new(1, nil) # or Range.new(1, nil, true)
#
# Beginless/endless ranges are useful, for example, for idiomatic
# slicing of arrays:
#
#   [1, 2, 3, 4, 5][...2]   # => [1, 2]
#   [1, 2, 3, 4, 5][2...]   # => [3, 4, 5]
#
# Some implementation details:
#
# * +begin+ of beginless range and +end+ of endless range are +nil+;
# * +each+ of beginless range raises an exception;
# * +each+ of endless range enumerates infinite sequence (may be
#   useful in combination with Enumerable#take_while or similar
#   methods);
# * <code>(1..)</code> and <code>(1...)</code> are not equal,
#   although technically representing the same sequence.
#
# == Custom Objects in Ranges
#
# Ranges can be constructed using any objects that can be compared
# using the <code><=></code> operator.
# Methods that treat the range as a sequence (#each and methods inherited
# from Enumerable) expect the begin object to implement a
# <code>succ</code> method to return the next object in sequence.
# The #step and #include? methods require the begin
# object to implement <code>succ</code> or to be numeric.
#
# In the <code>Xs</code> class below both <code><=></code> and
# <code>succ</code> are implemented so <code>Xs</code> can be used
# to construct ranges. Note that the Comparable module is included
# so the <code>==</code> method is defined in terms of <code><=></code>.
#
#    class Xs                # represent a string of 'x's
#      include Comparable
#      attr :length
#      def initialize(n)
#        @length = n
#      end
#      def succ
#        Xs.new(@length + 1)
#      end
#      def <=>(other)
#        @length <=> other.length
#      end
#      def to_s
#        sprintf "%2d #{inspect}", @length
#      end
#      def inspect
#        'x' * @length
#      end
#    end
#
# An example of using <code>Xs</code> to construct a range:
#
#    r = Xs.new(3)..Xs.new(6)   #=> xxx..xxxxxx
#    r.to_a                     #=> [xxx, xxxx, xxxxx, xxxxxx]
#    r.member?(Xs.new(5))       #=> true
class Range
  include Enumerable

  # Constructs a range using the given +begin+ and +end+. If the +exclude_end+
  # parameter is omitted or is <code>false</code>, the range will include
  # the end object; otherwise, it will be excluded.
  def initialize(p1, p2, p3 = v3) end

  # Iterates over the range, passing each <code>n</code>th element to the block.
  # If begin and end are numeric, +n+ is added for each iteration.
  # Otherwise #step invokes #succ to iterate through range elements.
  #
  # If no block is given, an enumerator is returned instead.
  # Especially, the enumerator is an Enumerator::ArithmeticSequence
  # if begin and end of the range are numeric.
  #
  #   range = Xs.new(1)..Xs.new(10)
  #   range.step(2) {|x| puts x}
  #   puts
  #   range.step(3) {|x| puts x}
  #
  # <em>produces:</em>
  #
  #    1 x
  #    3 xxx
  #    5 xxxxx
  #    7 xxxxxxx
  #    9 xxxxxxxxx
  #
  #    1 x
  #    4 xxxx
  #    7 xxxxxxx
  #   10 xxxxxxxxxx
  #
  # See Range for the definition of class Xs.
  def %(other) end

  # Returns <code>true</code> only if +obj+ is a Range, has equivalent
  # begin and end items (by comparing them with <code>==</code>), and has
  # the same #exclude_end? setting as the range.
  #
  #   (0..2) == (0..2)            #=> true
  #   (0..2) == Range.new(0,2)    #=> true
  #   (0..2) == (0...2)           #=> false
  def ==(other) end

  # Returns <code>true</code> if +obj+ is between begin and end of range,
  # <code>false</code> otherwise (same as #cover?). Conveniently,
  # <code>===</code> is the comparison operator used by <code>case</code>
  # statements.
  #
  #    case 79
  #    when 1..50   then   puts "low"
  #    when 51..75  then   puts "medium"
  #    when 76..100 then   puts "high"
  #    end
  #    # Prints "high"
  #
  #    case "2.6.5"
  #    when ..."2.4" then puts "EOL"
  #    when "2.4"..."2.5" then puts "maintenance"
  #    when "2.5"..."2.7" then puts "stable"
  #    when "2.7".. then puts "upcoming"
  #    end
  #    # Prints "stable"
  def ===(obj) end

  # Returns the object that defines the beginning of the range.
  #
  #     (1..10).begin   #=> 1
  def begin; end

  # By using binary search, finds a value in range which meets the given
  # condition in O(log n) where n is the size of the range.
  #
  # You can use this method in two use cases: a find-minimum mode and
  # a find-any mode.  In either case, the elements of the range must be
  # monotone (or sorted) with respect to the block.
  #
  # In find-minimum mode (this is a good choice for typical use case),
  # the block must return true or false, and there must be a value x
  # so that:
  #
  # - the block returns false for any value which is less than x, and
  # - the block returns true for any value which is greater than or
  #   equal to x.
  #
  # If x is within the range, this method returns the value x.
  # Otherwise, it returns nil.
  #
  #    ary = [0, 4, 7, 10, 12]
  #    (0...ary.size).bsearch {|i| ary[i] >= 4 } #=> 1
  #    (0...ary.size).bsearch {|i| ary[i] >= 6 } #=> 2
  #    (0...ary.size).bsearch {|i| ary[i] >= 8 } #=> 3
  #    (0...ary.size).bsearch {|i| ary[i] >= 100 } #=> nil
  #
  #    (0.0...Float::INFINITY).bsearch {|x| Math.log(x) >= 0 } #=> 1.0
  #
  # In find-any mode (this behaves like libc's bsearch(3)), the block
  # must return a number, and there must be two values x and y (x <= y)
  # so that:
  #
  # - the block returns a positive number for v if v < x,
  # - the block returns zero for v if x <= v < y, and
  # - the block returns a negative number for v if y <= v.
  #
  # This method returns any value which is within the intersection of
  # the given range and x...y (if any).  If there is no value that
  # satisfies the condition, it returns nil.
  #
  #    ary = [0, 100, 100, 100, 200]
  #    (0..4).bsearch {|i| 100 - ary[i] } #=> 1, 2 or 3
  #    (0..4).bsearch {|i| 300 - ary[i] } #=> nil
  #    (0..4).bsearch {|i|  50 - ary[i] } #=> nil
  #
  # You must not mix the two modes at a time; the block must always
  # return either true/false, or always return a number.  It is
  # undefined which value is actually picked up at each iteration.
  def bsearch; end

  # Identical to Enumerable#count, except it returns Infinity for endless
  # ranges.
  def count(...) end

  # Returns <code>true</code> if +obj+ is between the begin and end of
  # the range.
  #
  # This tests <code>begin <= obj <= end</code> when #exclude_end? is +false+
  # and <code>begin <= obj < end</code> when #exclude_end? is +true+.
  #
  # If called with a Range argument, returns <code>true</code> when the
  # given range is covered by the receiver,
  # by comparing the begin and end values. If the argument can be treated as
  # a sequence, this method treats it that way. In the specific case of
  # <code>(a..b).cover?(c...d)</code> with <code>a <= c && b < d</code>,
  # the end of the sequence must be calculated, which may exhibit poor
  # performance if <code>c</code> is non-numeric.
  # Returns <code>false</code> if the begin value of the
  # range is larger than the end value. Also returns +false+ if one of the
  # internal calls to <code><=></code> returns +nil+ (indicating the objects
  # are not comparable).
  #
  #    ("a".."z").cover?("c")  #=> true
  #    ("a".."z").cover?("5")  #=> false
  #    ("a".."z").cover?("cc") #=> true
  #    ("a".."z").cover?(1)    #=> false
  #    (1..5).cover?(2..3)     #=> true
  #    (1..5).cover?(0..6)     #=> false
  #    (1..5).cover?(1...6)    #=> true
  def cover?(...) end

  # Iterates over the elements of range, passing each in turn to the
  # block.
  #
  # The +each+ method can only be used if the begin object of the range
  # supports the +succ+ method.  A TypeError is raised if the object
  # does not have +succ+ method defined (like Float).
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    (10..15).each {|n| print n, ' ' }
  #    # prints: 10 11 12 13 14 15
  #
  #    (2.5..5).each {|n| print n, ' ' }
  #    # raises: TypeError: can't iterate from Float
  def each; end

  # Returns the object that defines the end of the range.
  #
  #    (1..10).end    #=> 10
  #    (1...10).end   #=> 10
  def end; end

  # Returns <code>true</code> only if +obj+ is a Range, has equivalent
  # begin and end items (by comparing them with <code>eql?</code>),
  # and has the same #exclude_end? setting as the range.
  #
  #   (0..2).eql?(0..2)            #=> true
  #   (0..2).eql?(Range.new(0,2))  #=> true
  #   (0..2).eql?(0...2)           #=> false
  def eql?(other) end

  # Returns <code>true</code> if the range excludes its end value.
  #
  #    (1..5).exclude_end?     #=> false
  #    (1...5).exclude_end?    #=> true
  def exclude_end?; end

  # Returns the first object in the range, or an array of the first +n+
  # elements.
  #
  #   (10..20).first     #=> 10
  #   (10..20).first(3)  #=> [10, 11, 12]
  def first(...) end

  # Compute a hash-code for this range. Two ranges with equal
  # begin and end points (using <code>eql?</code>), and the same
  # #exclude_end? value will generate the same hash-code.
  #
  # See also Object#hash.
  def hash; end

  # Convert this range object to a printable form (using #inspect to
  # convert the begin and end objects).
  def inspect; end

  # Returns the last object in the range,
  # or an array of the last +n+ elements.
  #
  # Note that with no arguments +last+ will return the object that defines
  # the end of the range even if #exclude_end? is +true+.
  #
  #   (10..20).last      #=> 20
  #   (10...20).last     #=> 20
  #   (10..20).last(3)   #=> [18, 19, 20]
  #   (10...20).last(3)  #=> [17, 18, 19]
  def last(...) end

  # Returns the maximum value in the range, or an array of maximum
  # values in the range if given an \Integer argument.
  #
  # For inclusive ranges with an end, the maximum value of the range
  # is the same as the end of the range.
  #
  # If an argument or block is given, or +self+ is an exclusive,
  # non-numeric range, calls Enumerable#max (via +super+) with the
  # argument and/or block to get the maximum values, unless +self+ is
  # a beginless range, in which case it raises a RangeError.
  #
  # If +self+ is an exclusive, integer range (both start and end of the
  # range are integers), and no arguments or block are provided, returns
  # last value in the range (1 before the end).  Otherwise, if +self+ is
  # an exclusive, numeric range, raises a TypeError.
  #
  # Returns +nil+ if the begin value of the range larger than the
  # end value. Returns +nil+ if the begin value of an exclusive
  # range is equal to the end value.  Raises a RangeError if called on
  # an endless range.
  #
  # Examples:
  #   (10..20).max                        #=> 20
  #   (10..20).max(2)                     #=> [20, 19]
  #   (10...20).max                       #=> 19
  #   (10...20).max(2)                    #=> [19, 18]
  #   (10...20).max{|x, y| -x <=> -y }    #=> 10
  #   (10...20).max(2){|x, y| -x <=> -y } #=> [10, 11]
  def max(...) end

  # Returns <code>true</code> if +obj+ is an element of
  # the range, <code>false</code> otherwise.
  #
  #    ("a".."z").include?("g")   #=> true
  #    ("a".."z").include?("A")   #=> false
  #    ("a".."z").include?("cc")  #=> false
  #
  # If you need to ensure +obj+ is between +begin+ and +end+, use #cover?
  #
  #    ("a".."z").cover?("cc")  #=> true
  #
  # If begin and end are numeric, #include? behaves like #cover?
  #
  #    (1..3).include?(1.5) # => true
  def member?(obj) end
  alias include? member?

  # Returns the minimum value in the range. Returns +nil+ if the begin
  # value of the range is larger than the end value. Returns +nil+ if
  # the begin value of an exclusive range is equal to the end value.
  #
  # Can be given an optional block to override the default comparison
  # method <code>a <=> b</code>.
  #
  #   (10..20).min    #=> 10
  def min(...) end

  # Returns a two element array which contains the minimum and the
  # maximum value in the range.
  #
  # Can be given an optional block to override the default comparison
  # method <code>a <=> b</code>.
  def minmax; end

  # Returns the number of elements in the range. Both the begin and the end of
  # the Range must be Numeric, otherwise nil is returned.
  #
  #   (10..20).size    #=> 11
  #   ('a'..'z').size  #=> nil
  #   (-Float::INFINITY..Float::INFINITY).size #=> Infinity
  def size; end

  # Iterates over the range, passing each <code>n</code>th element to the block.
  # If begin and end are numeric, +n+ is added for each iteration.
  # Otherwise #step invokes #succ to iterate through range elements.
  #
  # If no block is given, an enumerator is returned instead.
  # Especially, the enumerator is an Enumerator::ArithmeticSequence
  # if begin and end of the range are numeric.
  #
  #   range = Xs.new(1)..Xs.new(10)
  #   range.step(2) {|x| puts x}
  #   puts
  #   range.step(3) {|x| puts x}
  #
  # <em>produces:</em>
  #
  #    1 x
  #    3 xxx
  #    5 xxxxx
  #    7 xxxxxxx
  #    9 xxxxxxxxx
  #
  #    1 x
  #    4 xxxx
  #    7 xxxxxxx
  #   10 xxxxxxxxxx
  #
  # See Range for the definition of class Xs.
  def step(n = 1) end

  # Returns an array containing the items in the range.
  #
  #   (1..7).to_a  #=> [1, 2, 3, 4, 5, 6, 7]
  #   (1..).to_a   #=> RangeError: cannot convert endless range to an array
  def to_a; end
  alias entries to_a

  # Convert this range object to a printable form (using #to_s to convert the
  # begin and end objects).
  def to_s; end
end
