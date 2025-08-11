# frozen_string_literal: true

# An \Array is an ordered, integer-indexed collection of objects,
# called _elements_.  Any object may be an \Array element.
#
# == \Array Indexes
#
# \Array indexing starts at 0, as in C or Java.
#
# A positive index is an offset from the first element:
# - Index 0 indicates the first element.
# - Index 1 indicates the second element.
# - ...
#
# A negative index is an offset, backwards, from the end of the array:
# - Index -1 indicates the last element.
# - Index -2 indicates the next-to-last element.
# - ...
#
# A non-negative index is <i>in range</i> if it is smaller than
# the size of the array.  For a 3-element array:
# - Indexes 0 through 2 are in range.
# - Index 3 is out of range.
#
# A negative index is <i>in range</i> if its absolute value is
# not larger than the size of the array.  For a 3-element array:
# - Indexes -1 through -3 are in range.
# - Index -4 is out of range.
#
# == Creating Arrays
#
# A new array can be created by using the literal constructor
# <code>[]</code>.  Arrays can contain different types of objects.  For
# example, the array below contains an Integer, a String and a Float:
#
#    ary = [1, "two", 3.0] #=> [1, "two", 3.0]
#
# An array can also be created by explicitly calling Array.new with zero, one
# (the initial size of the Array) or two arguments (the initial size and a
# default object).
#
#    ary = Array.new    #=> []
#    Array.new(3)       #=> [nil, nil, nil]
#    Array.new(3, true) #=> [true, true, true]
#
# Note that the second argument populates the array with references to the
# same object.  Therefore, it is only recommended in cases when you need to
# instantiate arrays with natively immutable objects such as Symbols,
# numbers, true or false.
#
# To create an array with separate objects a block can be passed instead.
# This method is safe to use with mutable objects such as hashes, strings or
# other arrays:
#
#    Array.new(4) {Hash.new}    #=> [{}, {}, {}, {}]
#    Array.new(4) {|i| i.to_s } #=> ["0", "1", "2", "3"]
#
# This is also a quick way to build up multi-dimensional arrays:
#
#    empty_table = Array.new(3) {Array.new(3)}
#    #=> [[nil, nil, nil], [nil, nil, nil], [nil, nil, nil]]
#
# An array can also be created by using the Array() method, provided by
# Kernel, which tries to call #to_ary, then #to_a on its argument.
#
#     Array({:a => "a", :b => "b"}) #=> [[:a, "a"], [:b, "b"]]
#
# == Example Usage
#
# In addition to the methods it mixes in through the Enumerable module, the
# Array class has proprietary methods for accessing, searching and otherwise
# manipulating arrays.
#
# Some of the more common ones are illustrated below.
#
# == Accessing Elements
#
# Elements in an array can be retrieved using the Array#[] method.  It can
# take a single integer argument (a numeric index), a pair of arguments
# (start and length) or a range. Negative indices start counting from the end,
# with -1 being the last element.
#
#    arr = [1, 2, 3, 4, 5, 6]
#    arr[2]    #=> 3
#    arr[100]  #=> nil
#    arr[-3]   #=> 4
#    arr[2, 3] #=> [3, 4, 5]
#    arr[1..4] #=> [2, 3, 4, 5]
#    arr[1..-3] #=> [2, 3, 4]
#
# Another way to access a particular array element is by using the #at method
#
#    arr.at(0) #=> 1
#
# The #slice method works in an identical manner to Array#[].
#
# To raise an error for indices outside of the array bounds or else to
# provide a default value when that happens, you can use #fetch.
#
#    arr = ['a', 'b', 'c', 'd', 'e', 'f']
#    arr.fetch(100) #=> IndexError: index 100 outside of array bounds: -6...6
#    arr.fetch(100, "oops") #=> "oops"
#
# The special methods #first and #last will return the first and last
# elements of an array, respectively.
#
#    arr.first #=> 1
#    arr.last  #=> 6
#
# To return the first +n+ elements of an array, use #take
#
#    arr.take(3) #=> [1, 2, 3]
#
# #drop does the opposite of #take, by returning the elements after +n+
# elements have been dropped:
#
#    arr.drop(3) #=> [4, 5, 6]
#
# == Obtaining Information about an Array
#
# Arrays keep track of their own length at all times.  To query an array
# about the number of elements it contains, use #length, #count or #size.
#
#   browsers = ['Chrome', 'Firefox', 'Safari', 'Opera', 'IE']
#   browsers.length #=> 5
#   browsers.count #=> 5
#
# To check whether an array contains any elements at all
#
#   browsers.empty? #=> false
#
# To check whether a particular item is included in the array
#
#   browsers.include?('Konqueror') #=> false
#
# == Adding Items to Arrays
#
# Items can be added to the end of an array by using either #push or #<<
#
#   arr = [1, 2, 3, 4]
#   arr.push(5) #=> [1, 2, 3, 4, 5]
#   arr << 6    #=> [1, 2, 3, 4, 5, 6]
#
# #unshift will add a new item to the beginning of an array.
#
#    arr.unshift(0) #=> [0, 1, 2, 3, 4, 5, 6]
#
# With #insert you can add a new element to an array at any position.
#
#    arr.insert(3, 'apple')  #=> [0, 1, 2, 'apple', 3, 4, 5, 6]
#
# Using the #insert method, you can also insert multiple values at once:
#
#    arr.insert(3, 'orange', 'pear', 'grapefruit')
#    #=> [0, 1, 2, "orange", "pear", "grapefruit", "apple", 3, 4, 5, 6]
#
# == Removing Items from an Array
#
# The method #pop removes the last element in an array and returns it:
#
#    arr =  [1, 2, 3, 4, 5, 6]
#    arr.pop #=> 6
#    arr #=> [1, 2, 3, 4, 5]
#
# To retrieve and at the same time remove the first item, use #shift:
#
#    arr.shift #=> 1
#    arr #=> [2, 3, 4, 5]
#
# To delete an element at a particular index:
#
#    arr.delete_at(2) #=> 4
#    arr #=> [2, 3, 5]
#
# To delete a particular element anywhere in an array, use #delete:
#
#    arr = [1, 2, 2, 3]
#    arr.delete(2) #=> 2
#    arr #=> [1,3]
#
# A useful method if you need to remove +nil+ values from an array is
# #compact:
#
#    arr = ['foo', 0, nil, 'bar', 7, 'baz', nil]
#    arr.compact  #=> ['foo', 0, 'bar', 7, 'baz']
#    arr          #=> ['foo', 0, nil, 'bar', 7, 'baz', nil]
#    arr.compact! #=> ['foo', 0, 'bar', 7, 'baz']
#    arr          #=> ['foo', 0, 'bar', 7, 'baz']
#
# Another common need is to remove duplicate elements from an array.
#
# It has the non-destructive #uniq, and destructive method #uniq!
#
#    arr = [2, 5, 6, 556, 6, 6, 8, 9, 0, 123, 556]
#    arr.uniq #=> [2, 5, 6, 556, 8, 9, 0, 123]
#
# == Iterating over Arrays
#
# Like all classes that include the Enumerable module, Array has an each
# method, which defines what elements should be iterated over and how.  In
# case of Array's #each, all elements in the Array instance are yielded to
# the supplied block in sequence.
#
# Note that this operation leaves the array unchanged.
#
#    arr = [1, 2, 3, 4, 5]
#    arr.each {|a| print a -= 10, " "}
#    # prints: -9 -8 -7 -6 -5
#    #=> [1, 2, 3, 4, 5]
#
# Another sometimes useful iterator is #reverse_each which will iterate over
# the elements in the array in reverse order.
#
#    words = %w[first second third fourth fifth sixth]
#    str = ""
#    words.reverse_each {|word| str += "#{word} "}
#    p str #=> "sixth fifth fourth third second first "
#
# The #map method can be used to create a new array based on the original
# array, but with the values modified by the supplied block:
#
#    arr.map {|a| 2*a}     #=> [2, 4, 6, 8, 10]
#    arr                   #=> [1, 2, 3, 4, 5]
#    arr.map! {|a| a**2}   #=> [1, 4, 9, 16, 25]
#    arr                   #=> [1, 4, 9, 16, 25]
#
# == Selecting Items from an Array
#
# Elements can be selected from an array according to criteria defined in a
# block.  The selection can happen in a destructive or a non-destructive
# manner.  While the destructive operations will modify the array they were
# called on, the non-destructive methods usually return a new array with the
# selected elements, but leave the original array unchanged.
#
# === Non-destructive Selection
#
#    arr = [1, 2, 3, 4, 5, 6]
#    arr.select {|a| a > 3}       #=> [4, 5, 6]
#    arr.reject {|a| a < 3}       #=> [3, 4, 5, 6]
#    arr.drop_while {|a| a < 4}   #=> [4, 5, 6]
#    arr                          #=> [1, 2, 3, 4, 5, 6]
#
# === Destructive Selection
#
# #select! and #reject! are the corresponding destructive methods to #select
# and #reject
#
# Similar to #select vs. #reject, #delete_if and #keep_if have the exact
# opposite result when supplied with the same block:
#
#    arr.delete_if {|a| a < 4}   #=> [4, 5, 6]
#    arr                         #=> [4, 5, 6]
#
#    arr = [1, 2, 3, 4, 5, 6]
#    arr.keep_if {|a| a < 4}   #=> [1, 2, 3]
#    arr                       #=> [1, 2, 3]
# ---
# for pack.c
class Array
  include Enumerable

  # Returns a new array populated with the given objects.
  #
  #   Array.[]( 1, 'a', /^A/)  # => [1, "a", /^A/]
  #   Array[ 1, 'a', /^A/ ]    # => [1, "a", /^A/]
  #   [ 1, 'a', /^A/ ]         # => [1, "a", /^A/]
  def self.[](*args) end

  # If +object+ is an \Array object, returns +object+.
  #
  # Otherwise if +object+ responds to <tt>:to_ary</tt>,
  # calls <tt>object.to_ary</tt> and returns the result.
  #
  # Returns +nil+ if +object+ does not respond to <tt>:to_ary</tt>
  #
  # Raises an exception unless <tt>object.to_ary</tt> returns an \Array object.
  def self.try_convert(object) end

  # Returns a new \Array.
  #
  # With no block and no arguments, returns a new empty \Array object.
  #
  # With no block and a single \Array argument +array+,
  # returns a new \Array formed from +array+:
  #   a = Array.new([:foo, 'bar', 2])
  #   a.class # => Array
  #   a # => [:foo, "bar", 2]
  #
  # With no block and a single \Integer argument +size+,
  # returns a new \Array of the given size
  # whose elements are all +nil+:
  #   a = Array.new(3)
  #   a # => [nil, nil, nil]
  #
  # With no block and arguments +size+ and +default_value+,
  # returns an \Array of the given size;
  # each element is that same +default_value+:
  #   a = Array.new(3, 'x')
  #   a # => ['x', 'x', 'x']
  #
  # With a block and argument +size+,
  # returns an \Array of the given size;
  # the block is called with each successive integer +index+;
  # the element for that +index+ is the return value from the block:
  #   a = Array.new(3) {|index| "Element #{index}" }
  #   a # => ["Element 0", "Element 1", "Element 2"]
  #
  # Raises ArgumentError if +size+ is negative.
  #
  # With a block and no argument,
  # or a single argument +0+,
  # ignores the block and returns a new empty \Array.
  def initialize(...) end

  # Returns a new \Array containing each element found in both +array+ and \Array +other_array+;
  # duplicates are omitted; items are compared using <tt>eql?</tt>:
  #   [0, 1, 2, 3] & [1, 2] # => [1, 2]
  #   [0, 1, 0, 1] & [0, 1] # => [0, 1]
  #
  # Preserves order from +array+:
  #   [0, 1, 2] & [3, 2, 1, 0] # => [0, 1, 2]
  #
  # Related: Array#intersection.
  def &(other) end

  # When non-negative argument \Integer +n+ is given,
  # returns a new \Array built by concatenating the +n+ copies of +self+:
  #   a = ['x', 'y']
  #   a * 3 # => ["x", "y", "x", "y", "x", "y"]
  #
  # When \String argument +string_separator+ is given,
  # equivalent to <tt>array.join(string_separator)</tt>:
  #   [0, [0, 1], {foo: 0}] * ', ' # => "0, 0, 1, {:foo=>0}"
  def *(...) end

  # Returns a new \Array containing all elements of +array+
  # followed by all elements of +other_array+:
  #   a = [0, 1] + [2, 3]
  #   a # => [0, 1, 2, 3]
  #
  # Related: #concat.
  def +(other) end

  # Returns a new \Array containing only those elements from +array+
  # that are not found in \Array +other_array+;
  # items are compared using <tt>eql?</tt>;
  # the order from +array+ is preserved:
  #   [0, 1, 1, 2, 1, 1, 3, 1, 1] - [1] # => [0, 2, 3]
  #   [0, 1, 2, 3] - [3, 0] # => [1, 2]
  #   [0, 1, 2] - [4] # => [0, 1, 2]
  #
  # Related: Array#difference.
  def -(other) end

  # Appends +object+ to +self+; returns +self+:
  #   a = [:foo, 'bar', 2]
  #   a << :baz # => [:foo, "bar", 2, :baz]
  #
  # Appends +object+ as one element, even if it is another \Array:
  #   a = [:foo, 'bar', 2]
  #   a1 = a << [3, 4]
  #   a1 # => [:foo, "bar", 2, [3, 4]]
  def <<(object) end

  # Returns -1, 0, or 1 as +self+ is less than, equal to, or greater than +other_array+.
  # For each index +i+ in +self+, evaluates <tt>result = self[i] <=> other_array[i]</tt>.
  #
  # Returns -1 if any result is -1:
  #   [0, 1, 2] <=> [0, 1, 3] # => -1
  #
  # Returns 1 if any result is 1:
  #   [0, 1, 2] <=> [0, 1, 1] # => 1
  #
  # When all results are zero:
  # - Returns -1 if +array+ is smaller than +other_array+:
  #     [0, 1, 2] <=> [0, 1, 2, 3] # => -1
  # - Returns 1 if +array+ is larger than +other_array+:
  #     [0, 1, 2] <=> [0, 1] # => 1
  # - Returns 0 if +array+ and +other_array+ are the same size:
  #     [0, 1, 2] <=> [0, 1, 2] # => 0
  def <=>(other) end

  # Returns +true+ if both <tt>array.size == other_array.size</tt>
  # and for each index +i+ in +array+, <tt>array[i] == other_array[i]</tt>:
  #   a0 = [:foo, 'bar', 2]
  #   a1 = [:foo, 'bar', 2.0]
  #   a1 == a0 # => true
  #   [] == [] # => true
  #
  # Otherwise, returns +false+.
  #
  # This method is different from method Array#eql?,
  # which compares elements using <tt>Object#eql?</tt>.
  def ==(other) end

  # Returns elements from +self+; does not modify +self+.
  #
  # When a single \Integer argument +index+ is given, returns the element at offset +index+:
  #   a = [:foo, 'bar', 2]
  #   a[0] # => :foo
  #   a[2] # => 2
  #   a # => [:foo, "bar", 2]
  #
  # If +index+ is negative, counts relative to the end of +self+:
  #   a = [:foo, 'bar', 2]
  #   a[-1] # => 2
  #   a[-2] # => "bar"
  #
  # If +index+ is out of range, returns +nil+.
  #
  # When two \Integer arguments +start+ and +length+ are given,
  # returns a new \Array of size +length+ containing successive elements beginning at offset +start+:
  #   a = [:foo, 'bar', 2]
  #   a[0, 2] # => [:foo, "bar"]
  #   a[1, 2] # => ["bar", 2]
  #
  # If <tt>start + length</tt> is greater than <tt>self.length</tt>,
  # returns all elements from offset +start+ to the end:
  #   a = [:foo, 'bar', 2]
  #   a[0, 4] # => [:foo, "bar", 2]
  #   a[1, 3] # => ["bar", 2]
  #   a[2, 2] # => [2]
  #
  # If <tt>start == self.size</tt> and <tt>length >= 0</tt>,
  # returns a new empty \Array.
  #
  # If +length+ is negative, returns +nil+.
  #
  # When a single \Range argument +range+ is given,
  # treats <tt>range.min</tt> as +start+ above
  # and <tt>range.size</tt> as +length+ above:
  #   a = [:foo, 'bar', 2]
  #   a[0..1] # => [:foo, "bar"]
  #   a[1..2] # => ["bar", 2]
  #
  # Special case: If <tt>range.start == a.size</tt>, returns a new empty \Array.
  #
  # If <tt>range.end</tt> is negative, calculates the end index from the end:
  #   a = [:foo, 'bar', 2]
  #   a[0..-1] # => [:foo, "bar", 2]
  #   a[0..-2] # => [:foo, "bar"]
  #   a[0..-3] # => [:foo]
  #
  # If <tt>range.start</tt> is negative, calculates the start index from the end:
  #   a = [:foo, 'bar', 2]
  #   a[-1..2] # => [2]
  #   a[-2..2] # => ["bar", 2]
  #   a[-3..2] # => [:foo, "bar", 2]
  #
  # If <tt>range.start</tt> is larger than the array size, returns +nil+.
  #   a = [:foo, 'bar', 2]
  #   a[4..1] # => nil
  #   a[4..0] # => nil
  #   a[4..-1] # => nil
  #
  # When a single Enumerator::ArithmeticSequence argument +aseq+ is given,
  # returns an Array of elements corresponding to the indexes produced by
  # the sequence.
  #   a = ['--', 'data1', '--', 'data2', '--', 'data3']
  #   a[(1..).step(2)] # => ["data1", "data2", "data3"]
  #
  # Unlike slicing with range, if the start or the end of the arithmetic sequence
  # is larger than array size, throws RangeError.
  #   a = ['--', 'data1', '--', 'data2', '--', 'data3']
  #   a[(1..11).step(2)]
  #   # RangeError (((1..11).step(2)) out of range)
  #   a[(7..).step(2)]
  #   # RangeError (((7..).step(2)) out of range)
  #
  # If given a single argument, and its type is not one of the listed, tries to
  # convert it to Integer, and raises if it is impossible:
  #   a = [:foo, 'bar', 2]
  #   # Raises TypeError (no implicit conversion of Symbol into Integer):
  #   a[:foo]
  #
  # Array#slice is an alias for Array#[].
  def [](...) end
  alias slice []

  # Assigns elements in +self+; returns the given +object+.
  #
  # When \Integer argument +index+ is given, assigns +object+ to an element in +self+.
  #
  # If +index+ is non-negative, assigns +object+ the element at offset +index+:
  #   a = [:foo, 'bar', 2]
  #   a[0] = 'foo' # => "foo"
  #   a # => ["foo", "bar", 2]
  #
  # If +index+ is greater than <tt>self.length</tt>, extends the array:
  #   a = [:foo, 'bar', 2]
  #   a[7] = 'foo' # => "foo"
  #   a # => [:foo, "bar", 2, nil, nil, nil, nil, "foo"]
  #
  # If +index+ is negative, counts backwards from the end of the array:
  #   a = [:foo, 'bar', 2]
  #   a[-1] = 'two' # => "two"
  #   a # => [:foo, "bar", "two"]
  #
  # When \Integer arguments +start+ and +length+ are given and +object+ is not an \Array,
  # removes <tt>length - 1</tt> elements beginning at offset +start+,
  # and assigns +object+ at offset +start+:
  #   a = [:foo, 'bar', 2]
  #   a[0, 2] = 'foo' # => "foo"
  #   a # => ["foo", 2]
  #
  # If +start+ is negative, counts backwards from the end of the array:
  #   a = [:foo, 'bar', 2]
  #   a[-2, 2] = 'foo' # => "foo"
  #   a # => [:foo, "foo"]
  #
  # If +start+ is non-negative and outside the array (<tt> >= self.size</tt>),
  # extends the array with +nil+, assigns +object+ at offset +start+,
  # and ignores +length+:
  #   a = [:foo, 'bar', 2]
  #   a[6, 50] = 'foo' # => "foo"
  #   a # => [:foo, "bar", 2, nil, nil, nil, "foo"]
  #
  # If +length+ is zero, shifts elements at and following offset +start+
  # and assigns +object+ at offset +start+:
  #   a = [:foo, 'bar', 2]
  #   a[1, 0] = 'foo' # => "foo"
  #   a # => [:foo, "foo", "bar", 2]
  #
  # If +length+ is too large for the existing array, does not extend the array:
  #   a = [:foo, 'bar', 2]
  #   a[1, 5] = 'foo' # => "foo"
  #   a # => [:foo, "foo"]
  #
  # When \Range argument +range+ is given and +object+ is an \Array,
  # removes <tt>length - 1</tt> elements beginning at offset +start+,
  # and assigns +object+ at offset +start+:
  #   a = [:foo, 'bar', 2]
  #   a[0..1] = 'foo' # => "foo"
  #   a # => ["foo", 2]
  #
  # if <tt>range.begin</tt> is negative, counts backwards from the end of the array:
  #   a = [:foo, 'bar', 2]
  #   a[-2..2] = 'foo' # => "foo"
  #   a # => [:foo, "foo"]
  #
  # If the array length is less than <tt>range.begin</tt>,
  # assigns +object+ at offset <tt>range.begin</tt>, and ignores +length+:
  #   a = [:foo, 'bar', 2]
  #   a[6..50] = 'foo' # => "foo"
  #   a # => [:foo, "bar", 2, nil, nil, nil, "foo"]
  #
  # If <tt>range.end</tt> is zero, shifts elements at and following offset +start+
  # and assigns +object+ at offset +start+:
  #   a = [:foo, 'bar', 2]
  #   a[1..0] = 'foo' # => "foo"
  #   a # => [:foo, "foo", "bar", 2]
  #
  # If <tt>range.end</tt> is negative, assigns +object+ at offset +start+,
  # retains <tt>range.end.abs -1</tt> elements past that, and removes those beyond:
  #   a = [:foo, 'bar', 2]
  #   a[1..-1] = 'foo' # => "foo"
  #   a # => [:foo, "foo"]
  #   a = [:foo, 'bar', 2]
  #   a[1..-2] = 'foo' # => "foo"
  #   a # => [:foo, "foo", 2]
  #   a = [:foo, 'bar', 2]
  #   a[1..-3] = 'foo' # => "foo"
  #   a # => [:foo, "foo", "bar", 2]
  #   a = [:foo, 'bar', 2]
  #
  # If <tt>range.end</tt> is too large for the existing array,
  # replaces array elements, but does not extend the array with +nil+ values:
  #   a = [:foo, 'bar', 2]
  #   a[1..5] = 'foo' # => "foo"
  #   a # => [:foo, "foo"]
  def []=(...) end

  # Returns the union of +array+ and \Array +other_array+;
  # duplicates are removed; order is preserved;
  # items are compared using <tt>eql?</tt>:
  #   [0, 1] | [2, 3] # => [0, 1, 2, 3]
  #   [0, 1, 1] | [2, 2, 3] # => [0, 1, 2, 3]
  #   [0, 1, 2] | [3, 2, 1, 0] # => [0, 1, 2, 3]
  #
  # Related: Array#union.
  def |(other) end

  # Returns +true+ if all elements of +self+ meet a given criterion.
  #
  # With no block given and no argument, returns +true+ if +self+ contains only truthy elements,
  # +false+ otherwise:
  #   [0, 1, :foo].all? # => true
  #   [0, nil, 2].all? # => false
  #   [].all? # => true
  #
  # With a block given and no argument, calls the block with each element in +self+;
  # returns +true+ if the block returns only truthy values, +false+ otherwise:
  #   [0, 1, 2].all? { |element| element < 3 } # => true
  #   [0, 1, 2].all? { |element| element < 2 } # => false
  #
  # If argument +obj+ is given, returns +true+ if <tt>obj.===</tt> every element, +false+ otherwise:
  #   ['food', 'fool', 'foot'].all?(/foo/) # => true
  #   ['food', 'drink'].all?(/bar/) # => false
  #   [].all?(/foo/) # => true
  #   [0, 0, 0].all?(0) # => true
  #   [0, 1, 2].all?(1) # => false
  #
  # Related: Enumerable#all?
  def all?(...) end

  # Returns +true+ if any element of +self+ meets a given criterion.
  #
  # With no block given and no argument, returns +true+ if +self+ has any truthy element,
  # +false+ otherwise:
  #   [nil, 0, false].any? # => true
  #   [nil, false].any? # => false
  #   [].any? # => false
  #
  # With a block given and no argument, calls the block with each element in +self+;
  # returns +true+ if the block returns any truthy value, +false+ otherwise:
  #   [0, 1, 2].any? {|element| element > 1 } # => true
  #   [0, 1, 2].any? {|element| element > 2 } # => false
  #
  # If argument +obj+ is given, returns +true+ if +obj+.<tt>===</tt> any element,
  # +false+ otherwise:
  #   ['food', 'drink'].any?(/foo/) # => true
  #   ['food', 'drink'].any?(/bar/) # => false
  #   [].any?(/foo/) # => false
  #   [0, 1, 2].any?(1) # => true
  #   [0, 1, 2].any?(3) # => false
  #
  # Related: Enumerable#any?
  def any?(...) end

  # Returns the first element in +self+ that is an \Array
  # whose first element <tt>==</tt> +obj+:
  #   a = [{foo: 0}, [2, 4], [4, 5, 6], [4, 5]]
  #   a.assoc(4) # => [4, 5, 6]
  #
  # Returns +nil+ if no such element is found.
  #
  # Related: #rassoc.
  def assoc(obj) end

  # Returns the element at \Integer offset +index+; does not modify +self+.
  #   a = [:foo, 'bar', 2]
  #   a.at(0) # => :foo
  #   a.at(2) # => 2
  def at(index) end

  # Returns an element from +self+ selected by a binary search.
  # +self+ should be sorted, but this is not checked.
  #
  # By using binary search, finds a value from this array which meets
  # the given condition in <tt>O(log n)</tt> where +n+ is the size of the array.
  #
  # There are two search modes:
  # - <b>Find-minimum mode</b>: the block should return +true+ or +false+.
  # - <b>Find-any mode</b>: the block should return a numeric value.
  #
  # The block should not mix the modes by and sometimes returning +true+ or +false+
  # and sometimes returning a numeric value, but this is not checked.
  #
  # <b>Find-Minimum Mode</b>
  #
  # In find-minimum mode, the block always returns +true+ or +false+.
  # The further requirement (though not checked) is that
  # there are no indexes +i+ and +j+ such that:
  # - <tt>0 <= i < j <= self.size</tt>.
  # - The block returns +true+ for <tt>self[i]</tt> and +false+ for <tt>self[j]</tt>.
  #
  # In find-minimum mode, method bsearch returns the first element for which the block returns true.
  #
  # Examples:
  #   a = [0, 4, 7, 10, 12]
  #   a.bsearch {|x| x >= 4 } # => 4
  #   a.bsearch {|x| x >= 6 } # => 7
  #   a.bsearch {|x| x >= -1 } # => 0
  #   a.bsearch {|x| x >= 100 } # => nil
  #
  # Less formally: the block is such that all +false+-evaluating elements
  # precede all +true+-evaluating elements.
  #
  # These make sense as blocks in find-minimum mode:
  #   a = [0, 4, 7, 10, 12]
  #   a.map {|x| x >= 4 } # => [false, true, true, true, true]
  #   a.map {|x| x >= 6 } # => [false, false, true, true, true]
  #   a.map {|x| x >= -1 } # => [true, true, true, true, true]
  #   a.map {|x| x >= 100 } # => [false, false, false, false, false]
  #
  # This would not make sense:
  #   a = [0, 4, 7, 10, 12]
  #   a.map {|x| x == 7 } # => [false, false, true, false, false]
  #
  # <b>Find-Any Mode</b>
  #
  # In find-any mode, the block always returns a numeric value.
  # The further requirement (though not checked) is that
  # there are no indexes +i+ and +j+ such that:
  # - <tt>0 <= i < j <= self.size</tt>.
  # - The block returns a negative value for <tt>self[i]</tt>
  #   and a positive value for <tt>self[j]</tt>.
  # - The block returns a negative value for <tt>self[i]</tt> and zero <tt>self[j]</tt>.
  # - The block returns zero for <tt>self[i]</tt> and a positive value for <tt>self[j]</tt>.
  #
  # In find-any mode, method bsearch returns some element
  # for which the block returns zero, or +nil+ if no such element is found.
  #
  # Examples:
  #   a = [0, 4, 7, 10, 12]
  #   a.bsearch {|element| 7 <=> element } # => 7
  #   a.bsearch {|element| -1 <=> element } # => nil
  #   a.bsearch {|element| 5 <=> element } # => nil
  #   a.bsearch {|element| 15 <=> element } # => nil
  #
  # Less formally: the block is such that:
  # - All positive-evaluating elements precede all zero-evaluating elements.
  # - All positive-evaluating elements precede all negative-evaluating elements.
  # - All zero-evaluating elements precede all negative-evaluating elements.
  #
  # These make sense as blocks in find-any mode:
  #   a = [0, 4, 7, 10, 12]
  #   a.map {|element| 7 <=> element } # => [1, 1, 0, -1, -1]
  #   a.map {|element| -1 <=> element } # => [-1, -1, -1, -1, -1]
  #   a.map {|element| 5 <=> element } # => [1, 1, -1, -1, -1]
  #   a.map {|element| 15 <=> element } # => [1, 1, 1, 1, 1]
  #
  # This would not make sense:
  #   a = [0, 4, 7, 10, 12]
  #   a.map {|element| element <=> 7 } # => [-1, -1, 0, 1, 1]
  #
  # Returns an enumerator if no block given:
  #   a = [0, 4, 7, 10, 12]
  #   a.bsearch # => #<Enumerator: [0, 4, 7, 10, 12]:bsearch>
  def bsearch; end

  # Searches +self+ as described at method #bsearch,
  # but returns the _index_ of the found element instead of the element itself.
  def bsearch_index; end

  # Removes all elements from +self+:
  #   a = [:foo, 'bar', 2]
  #   a.clear # => []
  def clear; end

  # Calls the block, if given, with each element of +self+;
  # returns a new \Array whose elements are the return values from the block:
  #   a = [:foo, 'bar', 2]
  #   a1 = a.map {|element| element.class }
  #   a1 # => [Symbol, String, Integer]
  #
  # Returns a new \Enumerator if no block given:
  #   a = [:foo, 'bar', 2]
  #   a1 = a.map
  #   a1 # => #<Enumerator: [:foo, "bar", 2]:map>
  #
  # Array#collect is an alias for Array#map.
  def collect; end
  alias map collect

  # Calls the block, if given, with each element;
  # replaces the element with the block's return value:
  #   a = [:foo, 'bar', 2]
  #   a.map! { |element| element.class } # => [Symbol, String, Integer]
  #
  # Returns a new \Enumerator if no block given:
  #   a = [:foo, 'bar', 2]
  #   a1 = a.map!
  #   a1 # => #<Enumerator: [:foo, "bar", 2]:map!>
  #
  # Array#collect! is an alias for Array#map!.
  def collect!; end
  alias map! collect!

  # Calls the block, if given, with combinations of elements of +self+;
  # returns +self+. The order of combinations is indeterminate.
  #
  # When a block and an in-range positive \Integer argument +n+ (<tt>0 < n <= self.size</tt>)
  # are given, calls the block with all +n+-tuple combinations of +self+.
  #
  # Example:
  #   a = [0, 1, 2]
  #   a.combination(2) {|combination| p combination }
  # Output:
  #   [0, 1]
  #   [0, 2]
  #   [1, 2]
  #
  # Another example:
  #   a = [0, 1, 2]
  #   a.combination(3) {|combination| p combination }
  # Output:
  #   [0, 1, 2]
  #
  # When +n+ is zero, calls the block once with a new empty \Array:
  #   a = [0, 1, 2]
  #   a1 = a.combination(0) {|combination| p combination }
  # Output:
  #   []
  #
  # When +n+ is out of range (negative or larger than <tt>self.size</tt>),
  # does not call the block:
  #   a = [0, 1, 2]
  #   a.combination(-1) {|combination| fail 'Cannot happen' }
  #   a.combination(4) {|combination| fail 'Cannot happen' }
  #
  # Returns a new \Enumerator if no block given:
  #   a = [0, 1, 2]
  #   a.combination(2) # => #<Enumerator: [0, 1, 2]:combination(2)>
  def combination(n) end

  # Returns a new \Array containing all non-+nil+ elements from +self+:
  #   a = [nil, 0, nil, 1, nil, 2, nil]
  #   a.compact # => [0, 1, 2]
  def compact; end

  # Removes all +nil+ elements from +self+.
  #
  # Returns +self+ if any elements removed, otherwise +nil+.
  def compact!; end

  # Adds to +array+ all elements from each \Array in +other_arrays+; returns +self+:
  #   a = [0, 1]
  #   a.concat([2, 3], [4, 5]) # => [0, 1, 2, 3, 4, 5]
  def concat(*other_arrays) end

  # Returns a count of specified elements.
  #
  # With no argument and no block, returns the count of all elements:
  #   [0, 1, 2].count # => 3
  #   [].count # => 0
  #
  # With argument +obj+, returns the count of elements <tt>eql?</tt> to +obj+:
  #   [0, 1, 2, 0].count(0) # => 2
  #   [0, 1, 2].count(3) # => 0
  #
  # With no argument and a block given, calls the block with each element;
  # returns the count of elements for which the block returns a truthy value:
  #   [0, 1, 2, 3].count {|element| element > 1} # => 2
  #
  # With argument +obj+ and a block given, issues a warning, ignores the block,
  # and returns the count of elements <tt>eql?</tt> to +obj+:
  def count(...) end

  # When called with positive \Integer argument +count+ and a block,
  # calls the block with each element, then does so again,
  # until it has done so +count+ times; returns +nil+:
  #   output = []
  #   [0, 1].cycle(2) {|element| output.push(element) } # => nil
  #   output # => [0, 1, 0, 1]
  #
  # If +count+ is zero or negative, does not call the block:
  #   [0, 1].cycle(0) {|element| fail 'Cannot happen' } # => nil
  #   [0, 1].cycle(-1) {|element| fail 'Cannot happen' } # => nil
  #
  # When a block is given, and argument is omitted or +nil+, cycles forever:
  #   # Prints 0 and 1 forever.
  #   [0, 1].cycle {|element| puts element }
  #   [0, 1].cycle(nil) {|element| puts element }
  #
  # When no block is given, returns a new \Enumerator:
  #
  #   [0, 1].cycle(2) # => #<Enumerator: [0, 1]:cycle(2)>
  #   [0, 1].cycle # => # => #<Enumerator: [0, 1]:cycle>
  #   [0, 1].cycle.first(5) # => [0, 1, 0, 1, 0]
  def cycle(...) end

  def deconstruct; end

  # Removes zero or more elements from +self+; returns +self+.
  #
  # When no block is given,
  # removes from +self+ each element +ele+ such that <tt>ele == obj</tt>;
  # returns the last deleted element:
  #   s1 = 'bar'; s2 = 'bar'
  #   a = [:foo, s1, 2, s2]
  #   a.delete('bar') # => "bar"
  #   a # => [:foo, 2]
  #
  # Returns +nil+ if no elements removed.
  #
  # When a block is given,
  # removes from +self+ each element +ele+ such that <tt>ele == obj</tt>.
  #
  # If any such elements are found, ignores the block
  # and returns the last deleted element:
  #   s1 = 'bar'; s2 = 'bar'
  #   a = [:foo, s1, 2, s2]
  #   deleted_obj = a.delete('bar') {|obj| fail 'Cannot happen' }
  #   a # => [:foo, 2]
  #
  # If no such elements are found, returns the block's return value:
  #   a = [:foo, 'bar', 2]
  #   a.delete(:nosuch) {|obj| "#{obj} not found" } # => "nosuch not found"
  def delete(obj) end

  # Deletes an element from +self+, per the given \Integer +index+.
  #
  # When +index+ is non-negative, deletes the element at offset +index+:
  #   a = [:foo, 'bar', 2]
  #   a.delete_at(1) # => "bar"
  #   a # => [:foo, 2]
  #
  # If index is too large, returns +nil+.
  #
  # When +index+ is negative, counts backward from the end of the array:
  #   a = [:foo, 'bar', 2]
  #   a.delete_at(-2) # => "bar"
  #   a # => [:foo, 2]
  #
  # If +index+ is too small (far from zero), returns nil.
  def delete_at(index) end

  # Removes each element in +self+ for which the block returns a truthy value;
  # returns +self+:
  #   a = [:foo, 'bar', 2, 'bat']
  #   a.delete_if {|element| element.to_s.start_with?('b') } # => [:foo, 2]
  #
  # Returns a new \Enumerator if no block given:
  #   a = [:foo, 'bar', 2]
  #   a.delete_if # => #<Enumerator: [:foo, "bar", 2]:delete_if>
  def delete_if; end

  # Returns a new \Array containing only those elements from +self+
  # that are not found in any of the Arrays +other_arrays+;
  # items are compared using <tt>eql?</tt>;  order from +self+ is preserved:
  #   [0, 1, 1, 2, 1, 1, 3, 1, 1].difference([1]) # => [0, 2, 3]
  #   [0, 1, 2, 3].difference([3, 0], [1, 3]) # => [2]
  #   [0, 1, 2].difference([4]) # => [0, 1, 2]
  #
  # Returns a copy of +self+ if no arguments given.
  #
  # Related: Array#-.
  def difference(*other_arrays) end

  # Finds and returns the object in nested objects
  # that is specified by +index+ and +identifiers+.
  # The nested objects may be instances of various classes.
  # See {Dig Methods}[rdoc-ref:doc/dig_methods.rdoc].
  #
  # Examples:
  #   a = [:foo, [:bar, :baz, [:bat, :bam]]]
  #   a.dig(1) # => [:bar, :baz, [:bat, :bam]]
  #   a.dig(1, 2) # => [:bat, :bam]
  #   a.dig(1, 2, 0) # => :bat
  #   a.dig(1, 2, 3) # => nil
  def dig(index, *identifiers) end

  # Returns a new \Array containing all but the first +n+ element of +self+,
  # where +n+ is a non-negative \Integer;
  # does not modify +self+.
  #
  # Examples:
  #   a = [0, 1, 2, 3, 4, 5]
  #   a.drop(0) # => [0, 1, 2, 3, 4, 5]
  #   a.drop(1) # => [1, 2, 3, 4, 5]
  #   a.drop(2) # => [2, 3, 4, 5]
  def drop(n) end

  # Returns a new \Array containing zero or more trailing elements of +self+;
  # does not modify +self+.
  #
  # With a block given, calls the block with each successive element of +self+;
  # stops if the block returns +false+ or +nil+;
  # returns a new Array _omitting_ those elements for which the block returned a truthy value:
  #   a = [0, 1, 2, 3, 4, 5]
  #   a.drop_while {|element| element < 3 } # => [3, 4, 5]
  #
  # With no block given, returns a new \Enumerator:
  #   [0, 1].drop_while # => # => #<Enumerator: [0, 1]:drop_while>
  def drop_while; end

  # Iterates over array elements.
  #
  # When a block given, passes each successive array element to the block;
  # returns +self+:
  #   a = [:foo, 'bar', 2]
  #   a.each {|element|  puts "#{element.class} #{element}" }
  #
  # Output:
  #   Symbol foo
  #   String bar
  #   Integer 2
  #
  # Allows the array to be modified during iteration:
  #   a = [:foo, 'bar', 2]
  #   a.each {|element| puts element; a.clear if element.to_s.start_with?('b') }
  #
  # Output:
  #   foo
  #   bar
  #
  # When no block given, returns a new \Enumerator:
  #   a = [:foo, 'bar', 2]
  #   e = a.each
  #   e # => #<Enumerator: [:foo, "bar", 2]:each>
  #   a1 = e.each {|element|  puts "#{element.class} #{element}" }
  #
  # Output:
  #   Symbol foo
  #   String bar
  #   Integer 2
  #
  # Related: #each_index, #reverse_each.
  def each; end

  # Iterates over array indexes.
  #
  # When a block given, passes each successive array index to the block;
  # returns +self+:
  #   a = [:foo, 'bar', 2]
  #   a.each_index {|index|  puts "#{index} #{a[index]}" }
  #
  # Output:
  #   0 foo
  #   1 bar
  #   2 2
  #
  # Allows the array to be modified during iteration:
  #   a = [:foo, 'bar', 2]
  #   a.each_index {|index| puts index; a.clear if index > 0 }
  #
  # Output:
  #   0
  #   1
  #
  # When no block given, returns a new \Enumerator:
  #   a = [:foo, 'bar', 2]
  #   e = a.each_index
  #   e # => #<Enumerator: [:foo, "bar", 2]:each_index>
  #   a1 = e.each {|index|  puts "#{index} #{a[index]}"}
  #
  # Output:
  #   0 foo
  #   1 bar
  #   2 2
  #
  # Related: #each, #reverse_each.
  def each_index; end

  # Returns +true+ if the count of elements in +self+ is zero,
  # +false+ otherwise.
  def empty?; end

  # Returns +true+ if +self+ and +other_array+ are the same size,
  # and if, for each index +i+ in +self+, <tt>self[i].eql? other_array[i]</tt>:
  #   a0 = [:foo, 'bar', 2]
  #   a1 = [:foo, 'bar', 2]
  #   a1.eql?(a0) # => true
  #
  # Otherwise, returns +false+.
  #
  # This method is different from method {Array#==}[#method-i-3D-3D],
  # which compares using method <tt>Object#==</tt>.
  def eql?(other) end

  # Returns the element at offset  +index+.
  #
  # With the single \Integer argument +index+,
  # returns the element at offset +index+:
  #   a = [:foo, 'bar', 2]
  #   a.fetch(1) # => "bar"
  #
  # If +index+ is negative, counts from the end of the array:
  #   a = [:foo, 'bar', 2]
  #   a.fetch(-1) # => 2
  #   a.fetch(-2) # => "bar"
  #
  # With arguments +index+ and +default_value+,
  # returns the element at offset +index+ if index is in range,
  # otherwise returns +default_value+:
  #   a = [:foo, 'bar', 2]
  #   a.fetch(1, nil) # => "bar"
  #
  # With argument +index+ and a block,
  # returns the element at offset +index+ if index is in range
  # (and the block is not called); otherwise calls the block with index and returns its return value:
  #
  #   a = [:foo, 'bar', 2]
  #   a.fetch(1) {|index| raise 'Cannot happen' } # => "bar"
  #   a.fetch(50) {|index| "Value for #{index}" } # => "Value for 50"
  def fetch(...) end

  # Replaces specified elements in +self+ with specified objects; returns +self+.
  #
  # With argument +obj+ and no block given, replaces all elements with that one object:
  #   a = ['a', 'b', 'c', 'd']
  #   a # => ["a", "b", "c", "d"]
  #   a.fill(:X) # => [:X, :X, :X, :X]
  #
  # With arguments +obj+ and \Integer +start+, and no block given,
  # replaces elements based on the given start.
  #
  # If +start+ is in range (<tt>0 <= start < array.size</tt>),
  # replaces all elements from offset +start+ through the end:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, 2) # => ["a", "b", :X, :X]
  #
  # If +start+ is too large (<tt>start >= array.size</tt>), does nothing:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, 4) # => ["a", "b", "c", "d"]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, 5) # => ["a", "b", "c", "d"]
  #
  # If +start+ is negative, counts from the end (starting index is <tt>start + array.size</tt>):
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, -2) # => ["a", "b", :X, :X]
  #
  # If +start+ is too small (less than and far from zero), replaces all elements:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, -6) # => [:X, :X, :X, :X]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, -50) # => [:X, :X, :X, :X]
  #
  # With arguments +obj+, \Integer +start+, and \Integer +length+, and no block given,
  # replaces elements based on the given +start+ and +length+.
  #
  # If +start+ is in range, replaces +length+ elements beginning at offset +start+:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, 1, 1) # => ["a", :X, "c", "d"]
  #
  # If +start+ is negative, counts from the end:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, -2, 1) # => ["a", "b", :X, "d"]
  #
  # If +start+ is large (<tt>start >= array.size</tt>), extends +self+ with +nil+:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, 5, 0) # => ["a", "b", "c", "d", nil]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, 5, 2) # => ["a", "b", "c", "d", nil, :X, :X]
  #
  # If +length+ is zero or negative, replaces no elements:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, 1, 0) # => ["a", "b", "c", "d"]
  #   a.fill(:X, 1, -1) # => ["a", "b", "c", "d"]
  #
  # With arguments +obj+ and \Range +range+, and no block given,
  # replaces elements based on the given range.
  #
  # If the range is positive and ascending (<tt>0 < range.begin <= range.end</tt>),
  # replaces elements from <tt>range.begin</tt> to <tt>range.end</tt>:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, (1..1)) # => ["a", :X, "c", "d"]
  #
  # If <tt>range.first</tt> is negative, replaces no elements:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, (-1..1)) # => ["a", "b", "c", "d"]
  #
  # If <tt>range.last</tt> is negative, counts from the end:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, (0..-2)) # => [:X, :X, :X, "d"]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, (1..-2)) # => ["a", :X, :X, "d"]
  #
  # If <tt>range.last</tt> and <tt>range.last</tt> are both negative,
  # both count from the end of the array:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, (-1..-1)) # => ["a", "b", "c", :X]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(:X, (-2..-2)) # => ["a", "b", :X, "d"]
  #
  # With no arguments and a block given, calls the block with each index;
  # replaces the corresponding element with the block's return value:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill { |index| "new_#{index}" } # => ["new_0", "new_1", "new_2", "new_3"]
  #
  # With argument +start+ and a block given, calls the block with each index
  # from offset +start+ to the end; replaces the corresponding element
  # with the block's return value:
  #
  # If start is in range (<tt>0 <= start < array.size</tt>),
  # replaces from offset +start+ to the end:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(1) { |index| "new_#{index}" } # => ["a", "new_1", "new_2", "new_3"]
  #
  # If +start+ is too large(<tt>start >= array.size</tt>), does nothing:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(4) { |index| fail 'Cannot happen' } # => ["a", "b", "c", "d"]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(4) { |index| fail 'Cannot happen' } # => ["a", "b", "c", "d"]
  #
  # If +start+ is negative, counts from the end:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(-2) { |index| "new_#{index}" } # => ["a", "b", "new_2", "new_3"]
  #
  # If start is too small (<tt>start <= -array.size</tt>, replaces all elements:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(-6) { |index| "new_#{index}" } # => ["new_0", "new_1", "new_2", "new_3"]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(-50) { |index| "new_#{index}" } # => ["new_0", "new_1", "new_2", "new_3"]
  #
  # With arguments +start+ and +length+, and a block given,
  # calls the block for each index specified by start length;
  # replaces the corresponding element with the block's return value.
  #
  # If +start+ is in range, replaces +length+ elements beginning at offset +start+:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(1, 1) { |index| "new_#{index}" } # => ["a", "new_1", "c", "d"]
  #
  # If start is negative, counts from the end:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(-2, 1) { |index| "new_#{index}" } # => ["a", "b", "new_2", "d"]
  #
  # If +start+ is large (<tt>start >= array.size</tt>), extends +self+ with +nil+:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(5, 0) { |index| "new_#{index}" } # => ["a", "b", "c", "d", nil]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(5, 2) { |index| "new_#{index}" } # => ["a", "b", "c", "d", nil, "new_5", "new_6"]
  #
  # If +length+ is zero or less, replaces no elements:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(1, 0) { |index| "new_#{index}" } # => ["a", "b", "c", "d"]
  #   a.fill(1, -1) { |index| "new_#{index}" } # => ["a", "b", "c", "d"]
  #
  # With arguments +obj+ and +range+, and a block given,
  # calls the block with each index in the given range;
  # replaces the corresponding element with the block's return value.
  #
  # If the range is positive and ascending (<tt>range 0 < range.begin <= range.end</tt>,
  # replaces elements from <tt>range.begin</tt> to <tt>range.end</tt>:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(1..1) { |index| "new_#{index}" } # => ["a", "new_1", "c", "d"]
  #
  # If +range.first+ is negative, does nothing:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(-1..1) { |index| fail 'Cannot happen' } # => ["a", "b", "c", "d"]
  #
  # If <tt>range.last</tt> is negative, counts from the end:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(0..-2) { |index| "new_#{index}" } # => ["new_0", "new_1", "new_2", "d"]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(1..-2) { |index| "new_#{index}" } # => ["a", "new_1", "new_2", "d"]
  #
  # If <tt>range.first</tt> and <tt>range.last</tt> are both negative,
  # both count from the end:
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(-1..-1) { |index| "new_#{index}" } # => ["a", "b", "c", "new_3"]
  #   a = ['a', 'b', 'c', 'd']
  #   a.fill(-2..-2) { |index| "new_#{index}" } # => ["a", "b", "new_2", "d"]
  def fill(...) end

  # Returns the index of a specified element.
  #
  # When argument +object+ is given but no block,
  # returns the index of the first element +element+
  # for which <tt>object == element</tt>:
  #   a = [:foo, 'bar', 2, 'bar']
  #   a.index('bar') # => 1
  #
  # Returns +nil+ if no such element found.
  #
  # When both argument +object+ and a block are given,
  # calls the block with each successive element;
  # returns the index of the first element for which the block returns a truthy value:
  #   a = [:foo, 'bar', 2, 'bar']
  #   a.index {|element| element == 'bar' } # => 1
  #
  # Returns +nil+ if the block never returns a truthy value.
  #
  # When neither an argument nor a block is given, returns a new Enumerator:
  #   a = [:foo, 'bar', 2]
  #   e = a.index
  #   e # => #<Enumerator: [:foo, "bar", 2]:index>
  #   e.each {|element| element == 'bar' } # => 1
  #
  # Array#find_index is an alias for Array#index.
  #
  # Related: #rindex.
  def find_index(*args) end
  alias index find_index

  # Returns elements from +self+; does not modify +self+.
  #
  # When no argument is given, returns the first element:
  #   a = [:foo, 'bar', 2]
  #   a.first # => :foo
  #   a # => [:foo, "bar", 2]
  #
  # If +self+ is empty, returns +nil+.
  #
  # When non-negative \Integer argument +n+ is given,
  # returns the first +n+ elements in a new \Array:
  #   a = [:foo, 'bar', 2]
  #   a.first(2) # => [:foo, "bar"]
  #
  # If <tt>n >= array.size</tt>, returns all elements:
  #   a = [:foo, 'bar', 2]
  #   a.first(50) # => [:foo, "bar", 2]
  #
  # If <tt>n == 0</tt> returns an new empty \Array:
  #   a = [:foo, 'bar', 2]
  #   a.first(0) # []
  #
  # Related: #last.
  def first(...) end

  # Returns a new \Array that is a recursive flattening of +self+:
  # - Each non-Array element is unchanged.
  # - Each \Array is replaced by its individual elements.
  #
  # With non-negative \Integer argument +level+, flattens recursively through +level+ levels:
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten(0) # => [0, [1, [2, 3], 4], 5]
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten(1) # => [0, 1, [2, 3], 4, 5]
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten(2) # => [0, 1, 2, 3, 4, 5]
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten(3) # => [0, 1, 2, 3, 4, 5]
  #
  # With no argument, a +nil+ argument, or with negative argument +level+, flattens all levels:
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten # => [0, 1, 2, 3, 4, 5]
  #   [0, 1, 2].flatten # => [0, 1, 2]
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten(-1) # => [0, 1, 2, 3, 4, 5]
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten(-2) # => [0, 1, 2, 3, 4, 5]
  #   [0, 1, 2].flatten(-1) # => [0, 1, 2]
  def flatten(...) end

  # Replaces each nested \Array in +self+ with the elements from that \Array;
  # returns +self+ if any changes, +nil+ otherwise.
  #
  # With non-negative \Integer argument +level+, flattens recursively through +level+ levels:
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten!(1) # => [0, 1, [2, 3], 4, 5]
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten!(2) # => [0, 1, 2, 3, 4, 5]
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten!(3) # => [0, 1, 2, 3, 4, 5]
  #   [0, 1, 2].flatten!(1) # => nil
  #
  # With no argument, a +nil+ argument, or with negative argument +level+, flattens all levels:
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten! # => [0, 1, 2, 3, 4, 5]
  #   [0, 1, 2].flatten! # => nil
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten!(-1) # => [0, 1, 2, 3, 4, 5]
  #   a = [ 0, [ 1, [2, 3], 4 ], 5 ]
  #   a.flatten!(-2) # => [0, 1, 2, 3, 4, 5]
  #   [0, 1, 2].flatten!(-1) # => nil
  def flatten!(...) end

  # Returns the integer hash value for +self+.
  #
  # Two arrays with the same content will have the same hash code (and will compare using eql?):
  #   [0, 1, 2].hash == [0, 1, 2].hash # => true
  #   [0, 1, 2].hash == [0, 1, 3].hash # => false
  def hash; end

  # Returns +true+ if for some index +i+ in +self+, <tt>obj == self[i]</tt>;
  # otherwise +false+:
  #   [0, 1, 2].include?(2) # => true
  #   [0, 1, 2].include?(3) # => false
  def include?(obj) end

  # Replaces the content of +self+ with the content of +other_array+; returns +self+:
  #   a = [:foo, 'bar', 2]
  #   a.replace(['foo', :bar, 3]) # => ["foo", :bar, 3]
  def initialize_copy(other_ary) end
  alias replace initialize_copy

  # Inserts given +objects+ before or after the element at \Integer index +offset+;
  # returns +self+.
  #
  # When +index+ is non-negative, inserts all given +objects+
  # before the element at offset +index+:
  #   a = [:foo, 'bar', 2]
  #   a.insert(1, :bat, :bam) # => [:foo, :bat, :bam, "bar", 2]
  #
  # Extends the array if +index+ is beyond the array (<tt>index >= self.size</tt>):
  #   a = [:foo, 'bar', 2]
  #   a.insert(5, :bat, :bam)
  #   a # => [:foo, "bar", 2, nil, nil, :bat, :bam]
  #
  # Does nothing if no objects given:
  #   a = [:foo, 'bar', 2]
  #   a.insert(1)
  #   a.insert(50)
  #   a.insert(-50)
  #   a # => [:foo, "bar", 2]
  #
  # When +index+ is negative, inserts all given +objects+
  # _after_ the element at offset <tt>index+self.size</tt>:
  #   a = [:foo, 'bar', 2]
  #   a.insert(-2, :bat, :bam)
  #   a # => [:foo, "bar", :bat, :bam, 2]
  def insert(index, *objects) end

  # Returns the new \String formed by calling method <tt>#inspect</tt>
  # on each array element:
  #   a = [:foo, 'bar', 2]
  #   a.inspect # => "[:foo, \"bar\", 2]"
  #
  # Array#to_s is an alias for Array#inspect.
  def inspect; end
  alias to_s inspect

  # Returns a new \Array containing each element found both in +self+
  # and in all of the given Arrays +other_arrays+;
  # duplicates are omitted; items are compared using <tt>eql?</tt>:
  #   [0, 1, 2, 3].intersection([0, 1, 2], [0, 1, 3]) # => [0, 1]
  #   [0, 0, 1, 1, 2, 3].intersection([0, 1, 2], [0, 1, 3]) # => [0, 1]
  #
  # Preserves order from +self+:
  #   [0, 1, 2].intersection([2, 1, 0]) # => [0, 1, 2]
  #
  # Returns a copy of +self+ if no arguments given.
  #
  # Related: Array#&.
  def intersection(*other_arrays) end

  # Returns the new \String formed by joining the array elements after conversion.
  # For each element +element+
  # - Uses <tt>element.to_s</tt> if +element+ is not a <tt>kind_of?(Array)</tt>.
  # - Uses recursive <tt>element.join(separator)</tt> if +element+ is a <tt>kind_of?(Array)</tt>.
  #
  # With no argument, joins using the output field separator, <tt>$,</tt>:
  #   a = [:foo, 'bar', 2]
  #   $, # => nil
  #   a.join # => "foobar2"
  #
  # With \string argument +separator+, joins using that separator:
  #   a = [:foo, 'bar', 2]
  #   a.join("\n") # => "foo\nbar\n2"
  #
  # Joins recursively for nested Arrays:
  #  a = [:foo, [:bar, [:baz, :bat]]]
  #  a.join # => "foobarbazbat"
  def join(...) end

  # Retains those elements for which the block returns a truthy value;
  # deletes all other elements; returns +self+:
  #   a = [:foo, 'bar', 2, :bam]
  #   a.keep_if {|element| element.to_s.start_with?('b') } # => ["bar", :bam]
  #
  # Returns a new \Enumerator if no block given:
  #   a = [:foo, 'bar', 2, :bam]
  #   a.keep_if # => #<Enumerator: [:foo, "bar", 2, :bam]:keep_if>
  def keep_if; end

  # Returns elements from +self+; +self+ is not modified.
  #
  # When no argument is given, returns the last element:
  #   a = [:foo, 'bar', 2]
  #   a.last # => 2
  #   a # => [:foo, "bar", 2]
  #
  # If +self+ is empty, returns +nil+.
  #
  # When non-negative \Innteger argument +n+ is given,
  # returns the last +n+ elements in a new \Array:
  #   a = [:foo, 'bar', 2]
  #   a.last(2) # => ["bar", 2]
  #
  # If <tt>n >= array.size</tt>, returns all elements:
  #   a = [:foo, 'bar', 2]
  #   a.last(50) # => [:foo, "bar", 2]
  #
  # If <tt>n == 0</tt>, returns an new empty \Array:
  #   a = [:foo, 'bar', 2]
  #   a.last(0) # []
  #
  # Related: #first.
  def last(...) end

  # Returns the count of elements in +self+.
  def length; end
  alias size length

  # Returns one of the following:
  # - The maximum-valued element from +self+.
  # - A new \Array of maximum-valued elements selected from +self+.
  #
  # When no block is given, each element in +self+ must respond to method <tt><=></tt>
  # with an \Integer.
  #
  # With no argument and no block, returns the element in +self+
  # having the maximum value per method <tt><=></tt>:
  #   [0, 1, 2].max # => 2
  #
  # With an argument \Integer +n+ and no block, returns a new \Array with at most +n+ elements,
  # in descending order per method <tt><=></tt>:
  #   [0, 1, 2, 3].max(3) # => [3, 2, 1]
  #   [0, 1, 2, 3].max(6) # => [3, 2, 1]
  #
  # When a block is given, the block must return an \Integer.
  #
  # With a block and no argument, calls the block <tt>self.size-1</tt> times to compare elements;
  # returns the element having the maximum value per the block:
  #   ['0', '00', '000'].max {|a, b| a.size <=> b.size } # => "000"
  #
  # With an argument +n+ and a block, returns a new \Array with at most +n+ elements,
  # in descending order per the block:
  #   ['0', '00', '000'].max(2) {|a, b| a.size <=> b.size } # => ["000", "00"]
  def max(...) end

  # Returns one of the following:
  # - The minimum-valued element from +self+.
  # - A new \Array of minimum-valued elements selected from +self+.
  #
  # When no block is given, each element in +self+ must respond to method <tt><=></tt>
  # with an \Integer.
  #
  # With no argument and no block, returns the element in +self+
  # having the minimum value per method <tt><=></tt>:
  #   [0, 1, 2].min # => 0
  #
  # With \Integer argument +n+ and no block, returns a new \Array with at most +n+ elements,
  # in ascending order per method <tt><=></tt>:
  #   [0, 1, 2, 3].min(3) # => [0, 1, 2]
  #   [0, 1, 2, 3].min(6) # => [0, 1, 2, 3]
  #
  # When a block is given, the block must return an Integer.
  #
  # With a block and no argument, calls the block <tt>self.size-1</tt> times to compare elements;
  # returns the element having the minimum value per the block:
  #   ['0', '00', '000'].min { |a, b| a.size <=> b.size } # => "0"
  #
  # With an argument +n+ and a block, returns a new \Array with at most +n+ elements,
  # in ascending order per the block:
  #   [0, 1, 2, 3].min(3) # => [0, 1, 2]
  #   [0, 1, 2, 3].min(6) # => [0, 1, 2, 3]
  def min(...) end

  # Returns a new 2-element \Array containing the minimum and maximum values
  # from +self+, either per method <tt><=></tt> or per a given block:.
  #
  # When no block is given, each element in +self+ must respond to method <tt><=></tt>
  # with an \Integer;
  # returns a new 2-element \Array containing the minimum and maximum values
  # from +self+, per method <tt><=></tt>:
  #   [0, 1, 2].minmax # => [0, 2]
  #
  # When a block is given, the block must return an \Integer;
  # the block is called <tt>self.size-1</tt> times to compare elements;
  # returns a new 2-element \Array containing the minimum and maximum values
  # from +self+, per the block:
  #   ['0', '00', '000'].minmax {|a, b| a.size <=> b.size } # => ["0", "000"]
  def minmax; end

  # Returns +true+ if no element of +self+ meet a given criterion.
  #
  # With no block given and no argument, returns +true+ if +self+ has no truthy elements,
  # +false+ otherwise:
  #   [nil, false].none? # => true
  #   [nil, 0, false].none? # => false
  #   [].none? # => true
  #
  # With a block given and no argument, calls the block with each element in +self+;
  # returns +true+ if the block returns no truthy value, +false+ otherwise:
  #   [0, 1, 2].none? {|element| element > 3 } # => true
  #   [0, 1, 2].none? {|element| element > 1 } # => false
  #
  # If argument +obj+ is given, returns +true+ if <tt>obj.===</tt> no element, +false+ otherwise:
  #   ['food', 'drink'].none?(/bar/) # => true
  #   ['food', 'drink'].none?(/foo/) # => false
  #   [].none?(/foo/) # => true
  #   [0, 1, 2].none?(3) # => true
  #   [0, 1, 2].none?(1) # => false
  #
  # Related: Enumerable#none?
  def none?(...) end

  # Returns +true+ if exactly one element of +self+ meets a given criterion.
  #
  # With no block given and no argument, returns +true+ if +self+ has exactly one truthy element,
  # +false+ otherwise:
  #   [nil, 0].one? # => true
  #   [0, 0].one? # => false
  #   [nil, nil].one? # => false
  #   [].one? # => false
  #
  # With a block given and no argument, calls the block with each element in +self+;
  # returns +true+ if the block a truthy value for exactly one element, +false+ otherwise:
  #   [0, 1, 2].one? {|element| element > 0 } # => false
  #   [0, 1, 2].one? {|element| element > 1 } # => true
  #   [0, 1, 2].one? {|element| element > 2 } # => false
  #
  # If argument +obj+ is given, returns +true+ if <tt>obj.===</tt> exactly one element,
  # +false+ otherwise:
  #   [0, 1, 2].one?(0) # => true
  #   [0, 0, 1].one?(0) # => false
  #   [1, 1, 2].one?(0) # => false
  #   ['food', 'drink'].one?(/bar/) # => false
  #   ['food', 'drink'].one?(/foo/) # => true
  #   [].one?(/foo/) # => false
  #
  # Related: Enumerable#one?
  def one?(...) end

  # Packs the contents of <i>arr</i> into a binary sequence according to
  # the directives in <i>aTemplateString</i> (see the table below)
  # Directives ``A,'' ``a,'' and ``Z'' may be followed by a count,
  # which gives the width of the resulting field. The remaining
  # directives also may take a count, indicating the number of array
  # elements to convert. If the count is an asterisk
  # (``<code>*</code>''), all remaining array elements will be
  # converted. Any of the directives ``<code>sSiIlL</code>'' may be
  # followed by an underscore (``<code>_</code>'') or
  # exclamation mark (``<code>!</code>'') to use the underlying
  # platform's native size for the specified type; otherwise, they use a
  # platform-independent size. Spaces are ignored in the template
  # string. See also String#unpack.
  #
  #    a = [ "a", "b", "c" ]
  #    n = [ 65, 66, 67 ]
  #    a.pack("A3A3A3")   #=> "a  b  c  "
  #    a.pack("a3a3a3")   #=> "a\000\000b\000\000c\000\000"
  #    n.pack("ccc")      #=> "ABC"
  #
  # If <i>aBufferString</i> is specified and its capacity is enough,
  # +pack+ uses it as the buffer and returns it.
  # When the offset is specified by the beginning of <i>aTemplateString</i>,
  # the result is filled after the offset.
  # If original contents of <i>aBufferString</i> exists and it's longer than
  # the offset, the rest of <i>offsetOfBuffer</i> are overwritten by the result.
  # If it's shorter, the gap is filled with ``<code>\0</code>''.
  #
  # Note that ``buffer:'' option does not guarantee not to allocate memory
  # in +pack+.  If the capacity of <i>aBufferString</i> is not enough,
  # +pack+ allocates memory.
  #
  # Directives for +pack+.
  #
  #  Integer       | Array   |
  #  Directive     | Element | Meaning
  #  ----------------------------------------------------------------------------
  #  C             | Integer | 8-bit unsigned (unsigned char)
  #  S             | Integer | 16-bit unsigned, native endian (uint16_t)
  #  L             | Integer | 32-bit unsigned, native endian (uint32_t)
  #  Q             | Integer | 64-bit unsigned, native endian (uint64_t)
  #  J             | Integer | pointer width unsigned, native endian (uintptr_t)
  #                |         | (J is available since Ruby 2.3.)
  #                |         |
  #  c             | Integer | 8-bit signed (signed char)
  #  s             | Integer | 16-bit signed, native endian (int16_t)
  #  l             | Integer | 32-bit signed, native endian (int32_t)
  #  q             | Integer | 64-bit signed, native endian (int64_t)
  #  j             | Integer | pointer width signed, native endian (intptr_t)
  #                |         | (j is available since Ruby 2.3.)
  #                |         |
  #  S_ S!         | Integer | unsigned short, native endian
  #  I I_ I!       | Integer | unsigned int, native endian
  #  L_ L!         | Integer | unsigned long, native endian
  #  Q_ Q!         | Integer | unsigned long long, native endian (ArgumentError
  #                |         | if the platform has no long long type.)
  #                |         | (Q_ and Q! is available since Ruby 2.1.)
  #  J!            | Integer | uintptr_t, native endian (same with J)
  #                |         | (J! is available since Ruby 2.3.)
  #                |         |
  #  s_ s!         | Integer | signed short, native endian
  #  i i_ i!       | Integer | signed int, native endian
  #  l_ l!         | Integer | signed long, native endian
  #  q_ q!         | Integer | signed long long, native endian (ArgumentError
  #                |         | if the platform has no long long type.)
  #                |         | (q_ and q! is available since Ruby 2.1.)
  #  j!            | Integer | intptr_t, native endian (same with j)
  #                |         | (j! is available since Ruby 2.3.)
  #                |         |
  #  S> s> S!> s!> | Integer | same as the directives without ">" except
  #  L> l> L!> l!> |         | big endian
  #  I!> i!>       |         | (available since Ruby 1.9.3)
  #  Q> q> Q!> q!> |         | "S>" is same as "n"
  #  J> j> J!> j!> |         | "L>" is same as "N"
  #                |         |
  #  S< s< S!< s!< | Integer | same as the directives without "<" except
  #  L< l< L!< l!< |         | little endian
  #  I!< i!<       |         | (available since Ruby 1.9.3)
  #  Q< q< Q!< q!< |         | "S<" is same as "v"
  #  J< j< J!< j!< |         | "L<" is same as "V"
  #                |         |
  #  n             | Integer | 16-bit unsigned, network (big-endian) byte order
  #  N             | Integer | 32-bit unsigned, network (big-endian) byte order
  #  v             | Integer | 16-bit unsigned, VAX (little-endian) byte order
  #  V             | Integer | 32-bit unsigned, VAX (little-endian) byte order
  #                |         |
  #  U             | Integer | UTF-8 character
  #  w             | Integer | BER-compressed integer
  #
  #  Float        | Array   |
  #  Directive    | Element | Meaning
  #  ---------------------------------------------------------------------------
  #  D d          | Float   | double-precision, native format
  #  F f          | Float   | single-precision, native format
  #  E            | Float   | double-precision, little-endian byte order
  #  e            | Float   | single-precision, little-endian byte order
  #  G            | Float   | double-precision, network (big-endian) byte order
  #  g            | Float   | single-precision, network (big-endian) byte order
  #
  #  String       | Array   |
  #  Directive    | Element | Meaning
  #  ---------------------------------------------------------------------------
  #  A            | String  | arbitrary binary string (space padded, count is width)
  #  a            | String  | arbitrary binary string (null padded, count is width)
  #  Z            | String  | same as ``a'', except that null is added with *
  #  B            | String  | bit string (MSB first)
  #  b            | String  | bit string (LSB first)
  #  H            | String  | hex string (high nibble first)
  #  h            | String  | hex string (low nibble first)
  #  u            | String  | UU-encoded string
  #  M            | String  | quoted printable, MIME encoding (see also RFC2045)
  #               |         | (text mode but input must use LF and output LF)
  #  m            | String  | base64 encoded string (see RFC 2045)
  #               |         | (if count is 0, no line feed are added, see RFC 4648)
  #               |         | (count specifies input bytes between each LF,
  #               |         | rounded down to nearest multiple of 3)
  #  P            | String  | pointer to a structure (fixed-length string)
  #  p            | String  | pointer to a null-terminated string
  #
  #  Misc.        | Array   |
  #  Directive    | Element | Meaning
  #  ---------------------------------------------------------------------------
  #  @            | ---     | moves to absolute position
  #  X            | ---     | back up a byte
  #  x            | ---     | null byte
  def pack(...) end

  # When invoked with a block, yield all permutations of elements of +self+; returns +self+.
  # The order of permutations is indeterminate.
  #
  # When a block and an in-range positive \Integer argument +n+ (<tt>0 < n <= self.size</tt>)
  # are given, calls the block with all +n+-tuple permutations of +self+.
  #
  # Example:
  #   a = [0, 1, 2]
  #   a.permutation(2) {|permutation| p permutation }
  # Output:
  #   [0, 1]
  #   [0, 2]
  #   [1, 0]
  #   [1, 2]
  #   [2, 0]
  #   [2, 1]
  # Another example:
  #   a = [0, 1, 2]
  #   a.permutation(3) {|permutation| p permutation }
  # Output:
  #   [0, 1, 2]
  #   [0, 2, 1]
  #   [1, 0, 2]
  #   [1, 2, 0]
  #   [2, 0, 1]
  #   [2, 1, 0]
  #
  # When +n+ is zero, calls the block once with a new empty \Array:
  #   a = [0, 1, 2]
  #   a.permutation(0) {|permutation| p permutation }
  # Output:
  #   []
  #
  # When +n+ is out of range (negative or larger than <tt>self.size</tt>),
  # does not call the block:
  #   a = [0, 1, 2]
  #   a.permutation(-1) {|permutation| fail 'Cannot happen' }
  #   a.permutation(4) {|permutation| fail 'Cannot happen' }
  #
  # When a block given but no argument,
  # behaves the same as <tt>a.permutation(a.size)</tt>:
  #   a = [0, 1, 2]
  #   a.permutation {|permutation| p permutation }
  # Output:
  #   [0, 1, 2]
  #   [0, 2, 1]
  #   [1, 0, 2]
  #   [1, 2, 0]
  #   [2, 0, 1]
  #   [2, 1, 0]
  #
  # Returns a new \Enumerator if no block given:
  #   a = [0, 1, 2]
  #   a.permutation # => #<Enumerator: [0, 1, 2]:permutation>
  #   a.permutation(2) # => #<Enumerator: [0, 1, 2]:permutation(2)>
  def permutation(...) end

  # Removes and returns trailing elements.
  #
  # When no argument is given and +self+ is not empty,
  # removes and returns the last element:
  #   a = [:foo, 'bar', 2]
  #   a.pop # => 2
  #   a # => [:foo, "bar"]
  #
  # Returns +nil+ if the array is empty.
  #
  # When a non-negative \Integer argument +n+ is given and is in range,
  # removes and returns the last +n+ elements in a new \Array:
  #   a = [:foo, 'bar', 2]
  #   a.pop(2) # => ["bar", 2]
  #
  # If +n+ is positive and out of range,
  # removes and returns all elements:
  #   a = [:foo, 'bar', 2]
  #   a.pop(50) # => [:foo, "bar", 2]
  #
  # Related: #push, #shift, #unshift.
  def pop(...) end

  # Computes and returns or yields all combinations of elements from all the Arrays,
  # including both +self+ and +other_arrays+.
  # - The number of combinations is the product of the sizes of all the arrays,
  #   including both +self+ and +other_arrays+.
  # - The order of the returned combinations is indeterminate.
  #
  # When no block is given, returns the combinations as an \Array of Arrays:
  #   a = [0, 1, 2]
  #   a1 = [3, 4]
  #   a2 = [5, 6]
  #   p = a.product(a1)
  #   p.size # => 6 # a.size * a1.size
  #   p # => [[0, 3], [0, 4], [1, 3], [1, 4], [2, 3], [2, 4]]
  #   p = a.product(a1, a2)
  #   p.size # => 12 # a.size * a1.size * a2.size
  #   p # => [[0, 3, 5], [0, 3, 6], [0, 4, 5], [0, 4, 6], [1, 3, 5], [1, 3, 6], [1, 4, 5], [1, 4, 6], [2, 3, 5], [2, 3, 6], [2, 4, 5], [2, 4, 6]]
  #
  # If any argument is an empty \Array, returns an empty \Array.
  #
  # If no argument is given, returns an \Array of 1-element Arrays,
  # each containing an element of +self+:
  #   a.product # => [[0], [1], [2]]
  #
  # When a block is given, yields each combination as an \Array; returns +self+:
  #   a.product(a1) {|combination| p combination }
  # Output:
  #   [0, 3]
  #   [0, 4]
  #   [1, 3]
  #   [1, 4]
  #   [2, 3]
  #   [2, 4]
  #
  # If any argument is an empty \Array, does not call the block:
  #   a.product(a1, a2, []) {|combination| fail 'Cannot happen' }
  #
  # If no argument is given, yields each element of +self+ as a 1-element \Array:
  #   a.product {|combination| p combination }
  # Output:
  #   [0]
  #   [1]
  #   [2]
  def product(*other_arrays) end

  # Appends trailing elements.
  #
  # Appends each argument in +objects+ to +self+;  returns +self+:
  #   a = [:foo, 'bar', 2]
  #   a.push(:baz, :bat) # => [:foo, "bar", 2, :baz, :bat]
  #
  # Appends each argument as one element, even if it is another \Array:
  #   a = [:foo, 'bar', 2]
  #   a1 = a.push([:baz, :bat], [:bam, :bad])
  #   a1 # => [:foo, "bar", 2, [:baz, :bat], [:bam, :bad]]
  #
  # Array#append is an alias for \Array#push.
  #
  # Related: #pop, #shift, #unshift.
  def push(*objects) end
  alias append push

  # Returns the first element in +self+ that is an \Array
  # whose second element <tt>==</tt> +obj+:
  #   a = [{foo: 0}, [2, 4], [4, 5, 6], [4, 5]]
  #   a.rassoc(4) # => [2, 4]
  #
  # Returns +nil+ if no such element is found.
  #
  # Related: #assoc.
  def rassoc(obj) end

  # Returns a new \Array whose elements are all those from +self+
  # for which the block returns +false+ or +nil+:
  #   a = [:foo, 'bar', 2, 'bat']
  #   a1 = a.reject {|element| element.to_s.start_with?('b') }
  #   a1 # => [:foo, 2]
  #
  # Returns a new \Enumerator if no block given:
  #    a = [:foo, 'bar', 2]
  #    a.reject # => #<Enumerator: [:foo, "bar", 2]:reject>
  def reject; end

  # Removes each element for which the block returns a truthy value.
  #
  # Returns +self+ if any elements removed:
  #   a = [:foo, 'bar', 2, 'bat']
  #   a.reject! {|element| element.to_s.start_with?('b') } # => [:foo, 2]
  #
  # Returns +nil+ if no elements removed.
  #
  # Returns a new \Enumerator if no block given:
  #   a = [:foo, 'bar', 2]
  #   a.reject! # => #<Enumerator: [:foo, "bar", 2]:reject!>
  def reject!; end

  # Calls the block with each repeated combination of length +n+ of the elements of +self+;
  # each combination is an \Array;
  # returns +self+. The order of the combinations is indeterminate.
  #
  # When a block and a positive \Integer argument +n+ are given, calls the block with each
  # +n+-tuple repeated combination of the elements of +self+.
  # The number of combinations is <tt>(n+1)(n+2)/2</tt>.
  #
  # +n+ = 1:
  #   a = [0, 1, 2]
  #   a.repeated_combination(1) {|combination| p combination }
  # Output:
  #   [0]
  #   [1]
  #   [2]
  #
  # +n+ = 2:
  #   a.repeated_combination(2) {|combination| p combination }
  # Output:
  #   [0, 0]
  #   [0, 1]
  #   [0, 2]
  #   [1, 1]
  #   [1, 2]
  #   [2, 2]
  #
  # If +n+ is zero, calls the block once with an empty \Array.
  #
  # If +n+ is negative, does not call the block:
  #   a.repeated_combination(-1) {|combination| fail 'Cannot happen' }
  #
  # Returns a new \Enumerator if no block given:
  #   a = [0, 1, 2]
  #   a.repeated_combination(2) # => #<Enumerator: [0, 1, 2]:combination(2)>
  #
  # Using Enumerators, it's convenient to show the combinations and counts
  # for some values of +n+:
  #   e = a.repeated_combination(0)
  #   e.size # => 1
  #   e.to_a # => [[]]
  #   e = a.repeated_combination(1)
  #   e.size # => 3
  #   e.to_a # => [[0], [1], [2]]
  #   e = a.repeated_combination(2)
  #   e.size # => 6
  #   e.to_a # => [[0, 0], [0, 1], [0, 2], [1, 1], [1, 2], [2, 2]]
  def repeated_combination(n) end

  # Calls the block with each repeated permutation of length +n+ of the elements of +self+;
  # each permutation is an \Array;
  # returns +self+. The order of the permutations is indeterminate.
  #
  # When a block and a positive \Integer argument +n+ are given, calls the block with each
  # +n+-tuple repeated permutation of the elements of +self+.
  # The number of permutations is <tt>self.size**n</tt>.
  #
  # +n+ = 1:
  #   a = [0, 1, 2]
  #   a.repeated_permutation(1) {|permutation| p permutation }
  # Output:
  #   [0]
  #   [1]
  #   [2]
  #
  # +n+ = 2:
  #   a.repeated_permutation(2) {|permutation| p permutation }
  # Output:
  #   [0, 0]
  #   [0, 1]
  #   [0, 2]
  #   [1, 0]
  #   [1, 1]
  #   [1, 2]
  #   [2, 0]
  #   [2, 1]
  #   [2, 2]
  #
  # If +n+ is zero, calls the block once with an empty \Array.
  #
  # If +n+ is negative, does not call the block:
  #   a.repeated_permutation(-1) {|permutation| fail 'Cannot happen' }
  #
  # Returns a new \Enumerator if no block given:
  #   a = [0, 1, 2]
  #   a.repeated_permutation(2) # => #<Enumerator: [0, 1, 2]:permutation(2)>
  #
  # Using Enumerators, it's convenient to show the permutations and counts
  # for some values of +n+:
  #   e = a.repeated_permutation(0)
  #   e.size # => 1
  #   e.to_a # => [[]]
  #   e = a.repeated_permutation(1)
  #   e.size # => 3
  #   e.to_a # => [[0], [1], [2]]
  #   e = a.repeated_permutation(2)
  #   e.size # => 9
  #   e.to_a # => [[0, 0], [0, 1], [0, 2], [1, 0], [1, 1], [1, 2], [2, 0], [2, 1], [2, 2]]
  def repeated_permutation(n) end

  # Returns a new \Array with the elements of +self+ in reverse order.
  #   a = ['foo', 'bar', 'two']
  #   a1 = a.reverse
  #   a1 # => ["two", "bar", "foo"]
  def reverse; end

  # Reverses +self+ in place:
  #   a = ['foo', 'bar', 'two']
  #   a.reverse! # => ["two", "bar", "foo"]
  def reverse!; end

  # Iterates backwards over array elements.
  #
  # When a block given, passes, in reverse order, each element to the block;
  # returns +self+:
  #   a = [:foo, 'bar', 2]
  #   a.reverse_each {|element|  puts "#{element.class} #{element}" }
  #
  # Output:
  #   Integer 2
  #   String bar
  #   Symbol foo
  #
  # Allows the array to be modified during iteration:
  #   a = [:foo, 'bar', 2]
  #   a.reverse_each {|element| puts element; a.clear if element.to_s.start_with?('b') }
  #
  # Output:
  #   2
  #   bar
  #
  # When no block given, returns a new \Enumerator:
  #   a = [:foo, 'bar', 2]
  #   e = a.reverse_each
  #   e # => #<Enumerator: [:foo, "bar", 2]:reverse_each>
  #   a1 = e.each {|element|  puts "#{element.class} #{element}" }
  # Output:
  #   Integer 2
  #   String bar
  #   Symbol foo
  #
  # Related: #each, #each_index.
  def reverse_each; end

  # Returns the index of the last element for which <tt>object == element</tt>.
  #
  # When argument +object+ is given but no block, returns the index of the last such element found:
  #   a = [:foo, 'bar', 2, 'bar']
  #   a.rindex('bar') # => 3
  #
  # Returns +nil+ if no such object found.
  #
  # When a block is given but no argument, calls the block with each successive element;
  # returns the index of the last element for which the block returns a truthy value:
  #   a = [:foo, 'bar', 2, 'bar']
  #   a.rindex {|element| element == 'bar' } # => 3
  #
  # Returns +nil+ if the block never returns a truthy value.
  #
  # When neither an argument nor a block is given, returns a new \Enumerator:
  #
  #   a = [:foo, 'bar', 2, 'bar']
  #   e = a.rindex
  #   e # => #<Enumerator: [:foo, "bar", 2, "bar"]:rindex>
  #   e.each {|element| element == 'bar' } # => 3
  #
  # Related: #index.
  def rindex(...) end

  # Returns a new \Array formed from +self+ with elements
  # rotated from one end to the other.
  #
  # When no argument given, returns a new \Array that is like +self+,
  # except that the first element has been rotated to the last position:
  #   a = [:foo, 'bar', 2, 'bar']
  #   a1 = a.rotate
  #   a1 # => ["bar", 2, "bar", :foo]
  #
  # When given a non-negative \Integer +count+,
  # returns a new \Array with +count+ elements rotated from the beginning to the end:
  #   a = [:foo, 'bar', 2]
  #   a1 = a.rotate(2)
  #   a1 # => [2, :foo, "bar"]
  #
  # If +count+ is large, uses <tt>count % array.size</tt> as the count:
  #   a = [:foo, 'bar', 2]
  #   a1 = a.rotate(20)
  #   a1 # => [2, :foo, "bar"]
  #
  # If +count+ is zero, returns a copy of +self+, unmodified:
  #   a = [:foo, 'bar', 2]
  #   a1 = a.rotate(0)
  #   a1 # => [:foo, "bar", 2]
  #
  # When given a negative \Integer +count+, rotates in the opposite direction,
  # from end to beginning:
  #   a = [:foo, 'bar', 2]
  #   a1 = a.rotate(-2)
  #   a1 # => ["bar", 2, :foo]
  #
  # If +count+ is small (far from zero), uses <tt>count % array.size</tt> as the count:
  #   a = [:foo, 'bar', 2]
  #   a1 = a.rotate(-5)
  #   a1 # => ["bar", 2, :foo]
  def rotate(...) end

  # Rotates +self+ in place by moving elements from one end to the other; returns +self+.
  #
  # When no argument given, rotates the first element to the last position:
  #   a = [:foo, 'bar', 2, 'bar']
  #   a.rotate! # => ["bar", 2, "bar", :foo]
  #
  # When given a non-negative \Integer +count+,
  # rotates +count+ elements from the beginning to the end:
  #   a = [:foo, 'bar', 2]
  #   a.rotate!(2)
  #   a # => [2, :foo, "bar"]
  #
  # If +count+ is large, uses <tt>count % array.size</tt> as the count:
  #   a = [:foo, 'bar', 2]
  #   a.rotate!(20)
  #   a # => [2, :foo, "bar"]
  #
  # If +count+ is zero, returns +self+ unmodified:
  #   a = [:foo, 'bar', 2]
  #   a.rotate!(0)
  #   a # => [:foo, "bar", 2]
  #
  # When given a negative Integer +count+, rotates in the opposite direction,
  # from end to beginning:
  #   a = [:foo, 'bar', 2]
  #   a.rotate!(-2)
  #   a # => ["bar", 2, :foo]
  #
  # If +count+ is small (far from zero), uses <tt>count % array.size</tt> as the count:
  #   a = [:foo, 'bar', 2]
  #   a.rotate!(-5)
  #   a # => ["bar", 2, :foo]
  def rotate!(...) end

  # Returns random elements from +self+.
  #
  # When no arguments are given, returns a random element from +self+:
  #    a = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  #    a.sample # => 3
  #    a.sample # => 8
  # If +self+ is empty, returns +nil+.
  #
  # When argument +n+ is given, returns a new \Array containing +n+ random
  # elements from +self+:
  #    a.sample(3) # => [8, 9, 2]
  #    a.sample(6) # => [9, 6, 10, 3, 1, 4]
  # Returns no more than <tt>a.size</tt> elements
  # (because no new duplicates are introduced):
  #    a.sample(a.size * 2) # => [6, 4, 1, 8, 5, 9, 10, 2, 3, 7]
  # But +self+ may contain duplicates:
  #    a = [1, 1, 1, 2, 2, 3]
  #    a.sample(a.size * 2) # => [1, 1, 3, 2, 1, 2]
  # Returns a new empty \Array if +self+ is empty.
  #
  # The optional +random+ argument will be used as the random number generator:
  #    a = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  #    a.sample(random: Random.new(1))     #=> 6
  #    a.sample(4, random: Random.new(1))  #=> [6, 10, 9, 2]
  def sample(...) end

  # Calls the block, if given, with each element of +self+;
  # returns a new \Array containing those elements of +self+
  # for which the block returns a truthy value:
  #   a = [:foo, 'bar', 2, :bam]
  #   a1 = a.select {|element| element.to_s.start_with?('b') }
  #   a1 # => ["bar", :bam]
  #
  # Returns a new \Enumerator if no block given:
  #   a = [:foo, 'bar', 2, :bam]
  #   a.select # => #<Enumerator: [:foo, "bar", 2, :bam]:select>
  #
  # Array#filter is an alias for Array#select.
  def select; end
  alias filter select

  # Calls the block, if given  with each element of +self+;
  # removes from +self+ those elements for which the block returns +false+ or +nil+.
  #
  # Returns +self+ if any elements were removed:
  #   a = [:foo, 'bar', 2, :bam]
  #   a.select! {|element| element.to_s.start_with?('b') } # => ["bar", :bam]
  #
  # Returns +nil+ if no elements were removed.
  #
  # Returns a new \Enumerator if no block given:
  #   a = [:foo, 'bar', 2, :bam]
  #   a.select! # => #<Enumerator: [:foo, "bar", 2, :bam]:select!>
  #
  # Array#filter! is an alias for Array#select!.
  def select!; end
  alias filter! select!

  # Removes and returns leading elements.
  #
  # When no argument is given, removes and returns the first element:
  #   a = [:foo, 'bar', 2]
  #   a.shift # => :foo
  #   a # => ['bar', 2]
  #
  # Returns +nil+ if +self+ is empty.
  #
  # When positive \Integer argument +n+ is given, removes the first +n+ elements;
  # returns those elements in a new \Array:
  #   a = [:foo, 'bar', 2]
  #   a.shift(2) # => [:foo, 'bar']
  #   a # => [2]
  #
  # If +n+ is as large as or larger than <tt>self.length</tt>,
  # removes all elements; returns those elements in a new \Array:
  #   a = [:foo, 'bar', 2]
  #   a.shift(3) # => [:foo, 'bar', 2]
  #
  # If +n+ is zero, returns a new empty \Array; +self+ is unmodified.
  #
  # Related: #push, #pop, #unshift.
  def shift(...) end

  # Returns a new array with elements of +self+ shuffled.
  #    a = [1, 2, 3] #=> [1, 2, 3]
  #    a.shuffle     #=> [2, 3, 1]
  #    a             #=> [1, 2, 3]
  #
  # The optional +random+ argument will be used as the random number generator:
  #    a.shuffle(random: Random.new(1))  #=> [1, 3, 2]
  def shuffle(random: Random) end

  # Shuffles the elements of +self+ in place.
  #    a = [1, 2, 3] #=> [1, 2, 3]
  #    a.shuffle!    #=> [2, 3, 1]
  #    a             #=> [2, 3, 1]
  #
  # The optional +random+ argument will be used as the random number generator:
  #    a.shuffle!(random: Random.new(1))  #=> [1, 3, 2]
  def shuffle!(random: Random) end

  # Removes and returns elements from +self+.
  #
  # When the only argument is an \Integer +n+,
  # removes and returns the _nth_ element in +self+:
  #   a = [:foo, 'bar', 2]
  #   a.slice!(1) # => "bar"
  #   a # => [:foo, 2]
  #
  # If +n+ is negative, counts backwards from the end of +self+:
  #   a = [:foo, 'bar', 2]
  #   a.slice!(-1) # => 2
  #   a # => [:foo, "bar"]
  #
  # If +n+ is out of range, returns +nil+.
  #
  # When the only arguments are Integers +start+ and +length+,
  # removes +length+ elements from +self+ beginning at offset  +start+;
  # returns the deleted objects in a new Array:
  #   a = [:foo, 'bar', 2]
  #   a.slice!(0, 2) # => [:foo, "bar"]
  #   a # => [2]
  #
  # If <tt>start + length</tt> exceeds the array size,
  # removes and returns all elements from offset +start+ to the end:
  #   a = [:foo, 'bar', 2]
  #   a.slice!(1, 50) # => ["bar", 2]
  #   a # => [:foo]
  #
  # If <tt>start == a.size</tt> and +length+ is non-negative,
  # returns a new empty \Array.
  #
  # If +length+ is negative, returns +nil+.
  #
  # When the only argument is a \Range object +range+,
  # treats <tt>range.min</tt> as +start+ above and <tt>range.size</tt> as +length+ above:
  #   a = [:foo, 'bar', 2]
  #    a.slice!(1..2) # => ["bar", 2]
  #   a # => [:foo]
  #
  # If <tt>range.start == a.size</tt>, returns a new empty \Array.
  #
  # If <tt>range.start</tt> is larger than the array size, returns +nil+.
  #
  # If <tt>range.end</tt> is negative, counts backwards from the end of the array:
  #   a = [:foo, 'bar', 2]
  #   a.slice!(0..-2) # => [:foo, "bar"]
  #   a # => [2]
  #
  # If <tt>range.start</tt> is negative,
  # calculates the start index backwards from the end of the array:
  #   a = [:foo, 'bar', 2]
  #   a.slice!(-2..2) # => ["bar", 2]
  #   a # => [:foo]
  def slice!(...) end

  # Returns a new \Array whose elements are those from +self+, sorted.
  #
  # With no block, compares elements using operator <tt><=></tt>
  # (see Comparable):
  #   a = 'abcde'.split('').shuffle
  #   a # => ["e", "b", "d", "a", "c"]
  #   a1 = a.sort
  #   a1 # => ["a", "b", "c", "d", "e"]
  #
  # With a block, calls the block with each element pair;
  # for each element pair +a+ and +b+, the block should return an integer:
  # - Negative when +b+ is to follow +a+.
  # - Zero when +a+ and +b+ are equivalent.
  # - Positive when +a+ is to follow +b+.
  #
  # Example:
  #   a = 'abcde'.split('').shuffle
  #   a # => ["e", "b", "d", "a", "c"]
  #   a1 = a.sort {|a, b| a <=> b }
  #   a1 # => ["a", "b", "c", "d", "e"]
  #   a2 = a.sort {|a, b| b <=> a }
  #   a2 # => ["e", "d", "c", "b", "a"]
  #
  # When the block returns zero, the order for +a+ and +b+ is indeterminate,
  # and may be unstable:
  #   a = 'abcde'.split('').shuffle
  #   a # => ["e", "b", "d", "a", "c"]
  #   a1 = a.sort {|a, b| 0 }
  #   a1 # =>  ["c", "e", "b", "d", "a"]
  #
  # Related: Enumerable#sort_by.
  def sort; end

  # Returns +self+ with its elements sorted in place.
  #
  # With no block, compares elements using operator <tt><=></tt>
  # (see Comparable):
  #   a = 'abcde'.split('').shuffle
  #   a # => ["e", "b", "d", "a", "c"]
  #   a.sort!
  #   a # => ["a", "b", "c", "d", "e"]
  #
  # With a block, calls the block with each element pair;
  # for each element pair +a+ and +b+, the block should return an integer:
  # - Negative when +b+ is to follow +a+.
  # - Zero when +a+ and +b+ are equivalent.
  # - Positive when +a+ is to follow +b+.
  #
  # Example:
  #   a = 'abcde'.split('').shuffle
  #   a # => ["e", "b", "d", "a", "c"]
  #   a.sort! {|a, b| a <=> b }
  #   a # => ["a", "b", "c", "d", "e"]
  #   a.sort! {|a, b| b <=> a }
  #   a # => ["e", "d", "c", "b", "a"]
  #
  # When the block returns zero, the order for +a+ and +b+ is indeterminate,
  # and may be unstable:
  #   a = 'abcde'.split('').shuffle
  #   a # => ["e", "b", "d", "a", "c"]
  #   a.sort! {|a, b| 0 }
  #   a # => ["d", "e", "c", "a", "b"]
  def sort!; end

  # Sorts the elements of +self+ in place,
  # using an ordering determined by the block; returns self.
  #
  # Calls the block with each successive element;
  # sorts elements based on the values returned from the block.
  #
  # For duplicates returned by the block, the ordering is indeterminate, and may be unstable.
  #
  # This example sorts strings based on their sizes:
  #   a = ['aaaa', 'bbb', 'cc', 'd']
  #   a.sort_by! {|element| element.size }
  #   a # => ["d", "cc", "bbb", "aaaa"]
  #
  # Returns a new \Enumerator if no block given:
  #
  #   a = ['aaaa', 'bbb', 'cc', 'd']
  #   a.sort_by! # => #<Enumerator: ["aaaa", "bbb", "cc", "d"]:sort_by!>
  def sort_by!; end

  #  When no block is given, returns the object equivalent to:
  #    sum = init
  #    array.each {|element| sum += element }
  #    sum
  #  For example, <tt>[e1, e2, e3].sum</tt> returns </tt>init + e1 + e2 + e3</tt>.
  #
  #  Examples:
  #    a = [0, 1, 2, 3]
  #    a.sum # => 6
  #    a.sum(100) # => 106
  #
  #  The elements need not be numeric, but must be <tt>+</tt>-compatible
  #  with each other and with +init+:
  #    a = ['abc', 'def', 'ghi']
  #    a.sum('jkl') # => "jklabcdefghi"
  #
  #  When a block is given, it is called with each element
  #  and the block's return value (instead of the element itself) is used as the addend:
  #    a = ['zero', 1, :two]
  #    s = a.sum('Coerced and concatenated: ') {|element| element.to_s }
  #    s # => "Coerced and concatenated: zero1two"
  #
  #  Notes:
  #  - Array#join and Array#flatten may be faster than Array#sum
  #    for an \Array of Strings or an \Array of Arrays.
  #  - Array#sum method may not respect method redefinition of "+" methods such as Integer#+.
  def sum(init = 0) end

  # Returns a new \Array containing the first +n+ element of +self+,
  # where +n+ is a non-negative \Integer;
  # does not modify +self+.
  #
  # Examples:
  #   a = [0, 1, 2, 3, 4, 5]
  #   a.take(1) # => [0]
  #   a.take(2) # => [0, 1]
  #   a.take(50) # => [0, 1, 2, 3, 4, 5]
  #   a # => [0, 1, 2, 3, 4, 5]
  def take(n) end

  # Returns a new \Array containing zero or more leading elements of +self+;
  # does not modify +self+.
  #
  # With a block given, calls the block with each successive element of +self+;
  # stops if the block returns +false+ or +nil+;
  # returns a new Array containing those elements for which the block returned a truthy value:
  #   a = [0, 1, 2, 3, 4, 5]
  #   a.take_while {|element| element < 3 } # => [0, 1, 2]
  #   a.take_while {|element| true } # => [0, 1, 2, 3, 4, 5]
  #   a # => [0, 1, 2, 3, 4, 5]
  #
  # With no block given, returns a new \Enumerator:
  #   [0, 1].take_while # => #<Enumerator: [0, 1]:take_while>
  def take_while; end

  # When +self+ is an instance of \Array, returns +self+:
  #   a = [:foo, 'bar', 2]
  #   a.to_a # => [:foo, "bar", 2]
  #
  # Otherwise, returns a new \Array containing the elements of +self+:
  #   class MyArray < Array; end
  #   a = MyArray.new(['foo', 'bar', 'two'])
  #   a.instance_of?(Array) # => false
  #   a.kind_of?(Array) # => true
  #   a1 = a.to_a
  #   a1 # => ["foo", "bar", "two"]
  #   a1.class # => Array # Not MyArray
  def to_a; end

  # Returns +self+.
  def to_ary; end

  # Returns a new \Hash formed from +self+.
  #
  # When a block is given, calls the block with each array element;
  # the block must return a 2-element \Array whose two elements
  # form a key-value pair in the returned \Hash:
  #   a = ['foo', :bar, 1, [2, 3], {baz: 4}]
  #   h = a.to_h {|item| [item, item] }
  #   h # => {"foo"=>"foo", :bar=>:bar, 1=>1, [2, 3]=>[2, 3], {:baz=>4}=>{:baz=>4}}
  #
  # When no block is given, +self+ must be an \Array of 2-element sub-arrays,
  # each sub-array is formed into a key-value pair in the new \Hash:
  #   [].to_h # => {}
  #   a = [['foo', 'zero'], ['bar', 'one'], ['baz', 'two']]
  #   h = a.to_h
  #   h # => {"foo"=>"zero", "bar"=>"one", "baz"=>"two"}
  def to_h; end

  # Transposes the rows and columns in an \Array of Arrays;
  # the nested Arrays must all be the same size:
  #   a = [[:a0, :a1], [:b0, :b1], [:c0, :c1]]
  #   a.transpose # => [[:a0, :b0, :c0], [:a1, :b1, :c1]]
  def transpose; end

  # Returns a new \Array that is the union of +self+ and all given Arrays +other_arrays+;
  # duplicates are removed;  order is preserved;  items are compared using <tt>eql?</tt>:
  #   [0, 1, 2, 3].union([4, 5], [6, 7]) # => [0, 1, 2, 3, 4, 5, 6, 7]
  #   [0, 1, 1].union([2, 1], [3, 1]) # => [0, 1, 2, 3]
  #   [0, 1, 2, 3].union([3, 2], [1, 0]) # => [0, 1, 2, 3]
  #
  # Returns a copy of +self+ if no arguments given.
  #
  # Related: Array#|.
  def union(*other_arrays) end

  # Returns a new \Array containing those elements from +self+ that are not duplicates,
  # the first occurrence always being retained.
  #
  # With no block given, identifies and omits duplicates using method <tt>eql?</tt>
  # to compare.
  #   a = [0, 0, 1, 1, 2, 2]
  #   a.uniq # => [0, 1, 2]
  #
  # With a block given, calls the block for each element;
  # identifies (using method <tt>eql?</tt>) and omits duplicate values,
  # that is, those elements for which the block returns the same value:
  #   a = ['a', 'aa', 'aaa', 'b', 'bb', 'bbb']
  #   a.uniq {|element| element.size } # => ["a", "aa", "aaa"]
  def uniq; end

  # Removes duplicate elements from +self+, the first occurrence always being retained;
  # returns +self+ if any elements removed, +nil+ otherwise.
  #
  # With no block given, identifies and removes elements using method <tt>eql?</tt>
  # to compare.
  #
  # Returns +self+ if any elements removed:
  #   a = [0, 0, 1, 1, 2, 2]
  #   a.uniq! # => [0, 1, 2]
  #
  # Returns +nil+ if no elements removed.
  #
  # With a block given, calls the block for each element;
  # identifies (using method <tt>eql?</tt>) and removes
  # elements for which the block returns duplicate values.
  #
  # Returns +self+ if any elements removed:
  #   a = ['a', 'aa', 'aaa', 'b', 'bb', 'bbb']
  #   a.uniq! {|element| element.size } # => ['a', 'aa', 'aaa']
  #
  # Returns +nil+ if no elements removed.
  def uniq!; end

  # Prepends the given +objects+ to +self+:
  #   a = [:foo, 'bar', 2]
  #   a.unshift(:bam, :bat) # => [:bam, :bat, :foo, "bar", 2]
  #
  # Array#prepend is an alias for Array#unshift.
  #
  # Related: #push, #pop, #shift.
  def unshift(*objects) end
  alias prepend unshift

  # Returns a new \Array whose elements are the elements
  # of +self+ at the given \Integer +indexes+.
  #
  # For each positive +index+, returns the element at offset +index+:
  #   a = [:foo, 'bar', 2]
  #   a.values_at(0, 2) # => [:foo, 2]
  #
  # The given +indexes+ may be in any order, and may repeat:
  #   a = [:foo, 'bar', 2]
  #   a.values_at(2, 0, 1, 0, 2) # => [2, :foo, "bar", :foo, 2]
  #
  # Assigns +nil+ for an +index+ that is too large:
  #   a = [:foo, 'bar', 2]
  #   a.values_at(0, 3, 1, 3) # => [:foo, nil, "bar", nil]
  #
  # Returns a new empty \Array if no arguments given.
  #
  # For each negative +index+, counts backward from the end of the array:
  #   a = [:foo, 'bar', 2]
  #   a.values_at(-1, -3) # => [2, :foo]
  #
  # Assigns +nil+ for an +index+ that is too small:
  #   a = [:foo, 'bar', 2]
  #   a.values_at(0, -5, 1, -6, 2) # => [:foo, nil, "bar", nil, 2]
  #
  # The given +indexes+ may have a mixture of signs:
  #   a = [:foo, 'bar', 2]
  #   a.values_at(0, -2, 1, -1) # => [:foo, "bar", "bar", 2]
  def values_at(*indexes) end

  # When no block given, returns a new \Array +new_array+ of size <tt>self.size</tt>
  # whose elements are Arrays.
  #
  # Each nested array <tt>new_array[n]</tt> is of size <tt>other_arrays.size+1</tt>,
  # and contains:
  # - The _nth_ element of +self+.
  # - The _nth_ element of each of the +other_arrays+.
  #
  # If all +other_arrays+ and +self+ are the same size:
  #   a = [:a0, :a1, :a2, :a3]
  #   b = [:b0, :b1, :b2, :b3]
  #   c = [:c0, :c1, :c2, :c3]
  #   d = a.zip(b, c)
  #   d # => [[:a0, :b0, :c0], [:a1, :b1, :c1], [:a2, :b2, :c2], [:a3, :b3, :c3]]
  #
  # If any array in +other_arrays+ is smaller than +self+,
  # fills to <tt>self.size</tt> with +nil+:
  #   a = [:a0, :a1, :a2, :a3]
  #   b = [:b0, :b1, :b2]
  #   c = [:c0, :c1]
  #   d = a.zip(b, c)
  #   d # => [[:a0, :b0, :c0], [:a1, :b1, :c1], [:a2, :b2, nil], [:a3, nil, nil]]
  #
  # If any array in +other_arrays+ is larger than +self+,
  # its trailing elements are ignored:
  #   a = [:a0, :a1, :a2, :a3]
  #   b = [:b0, :b1, :b2, :b3, :b4]
  #   c = [:c0, :c1, :c2, :c3, :c4, :c5]
  #   d = a.zip(b, c)
  #   d # => [[:a0, :b0, :c0], [:a1, :b1, :c1], [:a2, :b2, :c2], [:a3, :b3, :c3]]
  #
  # When a block is given, calls the block with each of the sub-arrays (formed as above); returns nil
  #   a = [:a0, :a1, :a2, :a3]
  #   b = [:b0, :b1, :b2, :b3]
  #   c = [:c0, :c1, :c2, :c3]
  #   a.zip(b, c) {|sub_array| p sub_array} # => nil
  #
  # Output:
  #   [:a0, :b0, :c0]
  #   [:a1, :b1, :c1]
  #   [:a2, :b2, :c2]
  #   [:a3, :b3, :c3]
  def zip(*other_arrays) end
end
