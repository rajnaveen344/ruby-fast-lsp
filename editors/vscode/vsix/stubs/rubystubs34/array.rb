# frozen_string_literal: true

# An \Array object is an ordered, integer-indexed collection of objects,
# called _elements_;
# the object represents
# an {array data structure}[https://en.wikipedia.org/wiki/Array_(data_structure)].
#
# An element may be any object (even another array);
# elements may be any mixture of objects of different types.
#
# Important data structures that use arrays include:
#
# - {Coordinate vector}[https://en.wikipedia.org/wiki/Coordinate_vector].
# - {Matrix}[https://en.wikipedia.org/wiki/Matrix_(mathematics)].
# - {Heap}[https://en.wikipedia.org/wiki/Heap_(data_structure)].
# - {Hash table}[https://en.wikipedia.org/wiki/Hash_table].
# - {Deque (double-ended queue)}[https://en.wikipedia.org/wiki/Double-ended_queue].
# - {Queue}[https://en.wikipedia.org/wiki/Queue_(abstract_data_type)].
# - {Stack}[https://en.wikipedia.org/wiki/Stack_(abstract_data_type)].
#
# There are also array-like data structures:
#
# - {Associative array}[https://en.wikipedia.org/wiki/Associative_array] (see Hash).
# - {Directory}[https://en.wikipedia.org/wiki/Directory_(computing)] (see Dir).
# - {Environment}[https://en.wikipedia.org/wiki/Environment_variable] (see ENV).
# - {Set}[https://en.wikipedia.org/wiki/Set_(abstract_data_type)] (see Set).
# - {String}[https://en.wikipedia.org/wiki/String_(computer_science)] (see String).
#
# == \Array Indexes
#
# \Array indexing starts at 0, as in C or Java.
#
# A non-negative index is an offset from the first element:
#
# - Index 0 indicates the first element.
# - Index 1 indicates the second element.
# - ...
#
# A negative index is an offset, backwards, from the end of the array:
#
# - Index -1 indicates the last element.
# - Index -2 indicates the next-to-last element.
# - ...
#
# === In-Range and Out-of-Range Indexes
#
# A non-negative index is <i>in range</i> if and only if it is smaller than
# the size of the array.  For a 3-element array:
#
# - Indexes 0 through 2 are in range.
# - Index 3 is out of range.
#
# A negative index is <i>in range</i> if and only if its absolute value is
# not larger than the size of the array.  For a 3-element array:
#
# - Indexes -1 through -3 are in range.
# - Index -4 is out of range.
#
# === Effective Index
#
# Although the effective index into an array is always an integer,
# some methods (both within class \Array and elsewhere)
# accept one or more non-integer arguments that are
# {integer-convertible objects}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects].
#
# == Creating Arrays
#
# You can create an \Array object explicitly with:
#
# - An {array literal}[rdoc-ref:syntax/literals.rdoc@Array+Literals]:
#
#     [1, 'one', :one, [2, 'two', :two]]
#
# - A {%w or %W string-array Literal}[rdoc-ref:syntax/literals.rdoc@25w+and+-25W-3A+String-Array+Literals]:
#
#     %w[foo bar baz] # => ["foo", "bar", "baz"]
#     %w[1 % *]       # => ["1", "%", "*"]
#
# - A {%i or %I symbol-array Literal}[rdoc-ref:syntax/literals.rdoc@25i+and+-25I-3A+Symbol-Array+Literals]:
#
#     %i[foo bar baz] # => [:foo, :bar, :baz]
#     %i[1 % *]       # => [:"1", :%, :*]
#
# - \Method Kernel#Array:
#
#     Array(["a", "b"])             # => ["a", "b"]
#     Array(1..5)                   # => [1, 2, 3, 4, 5]
#     Array(key: :value)            # => [[:key, :value]]
#     Array(nil)                    # => []
#     Array(1)                      # => [1]
#     Array({:a => "a", :b => "b"}) # => [[:a, "a"], [:b, "b"]]
#
# - \Method Array.new:
#
#     Array.new               # => []
#     Array.new(3)            # => [nil, nil, nil]
#     Array.new(4) {Hash.new} # => [{}, {}, {}, {}]
#     Array.new(3, true)      # => [true, true, true]
#
#   Note that the last example above populates the array
#   with references to the same object.
#   This is recommended only in cases where that object is a natively immutable object
#   such as a symbol, a numeric, +nil+, +true+, or +false+.
#
#   Another way to create an array with various objects, using a block;
#   this usage is safe for mutable objects such as hashes, strings or
#   other arrays:
#
#     Array.new(4) {|i| i.to_s } # => ["0", "1", "2", "3"]
#
#   Here is a way to create a multi-dimensional array:
#
#     Array.new(3) {Array.new(3)}
#     # => [[nil, nil, nil], [nil, nil, nil], [nil, nil, nil]]
#
# A number of Ruby methods, both in the core and in the standard library,
# provide instance method +to_a+, which converts an object to an array.
#
# - ARGF#to_a
# - Array#to_a
# - Enumerable#to_a
# - Hash#to_a
# - MatchData#to_a
# - NilClass#to_a
# - OptionParser#to_a
# - Range#to_a
# - Set#to_a
# - Struct#to_a
# - Time#to_a
# - Benchmark::Tms#to_a
# - CSV::Table#to_a
# - Enumerator::Lazy#to_a
# - Gem::List#to_a
# - Gem::NameTuple#to_a
# - Gem::Platform#to_a
# - Gem::RequestSet::Lockfile::Tokenizer#to_a
# - Gem::SourceList#to_a
# - OpenSSL::X509::Extension#to_a
# - OpenSSL::X509::Name#to_a
# - Racc::ISet#to_a
# - Rinda::RingFinger#to_a
# - Ripper::Lexer::Elem#to_a
# - RubyVM::InstructionSequence#to_a
# - YAML::DBM#to_a
#
# == Example Usage
#
# In addition to the methods it mixes in through the Enumerable module, the
# +Array+ class has proprietary methods for accessing, searching and otherwise
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
# == Obtaining Information about an +Array+
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
# == Removing Items from an +Array+
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
# Like all classes that include the Enumerable module, +Array+ has an each
# method, which defines what elements should be iterated over and how.  In
# case of Array's #each, all elements in the +Array+ instance are yielded to
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
# == Selecting Items from an +Array+
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
#
# == What's Here
#
# First, what's elsewhere. \Class +Array+:
#
# - Inherits from {class Object}[rdoc-ref:Object@What-27s+Here].
# - Includes {module Enumerable}[rdoc-ref:Enumerable@What-27s+Here],
#   which provides dozens of additional methods.
#
# Here, class +Array+ provides methods that are useful for:
#
# - {Creating an Array}[rdoc-ref:Array@Methods+for+Creating+an+Array]
# - {Querying}[rdoc-ref:Array@Methods+for+Querying]
# - {Comparing}[rdoc-ref:Array@Methods+for+Comparing]
# - {Fetching}[rdoc-ref:Array@Methods+for+Fetching]
# - {Assigning}[rdoc-ref:Array@Methods+for+Assigning]
# - {Deleting}[rdoc-ref:Array@Methods+for+Deleting]
# - {Combining}[rdoc-ref:Array@Methods+for+Combining]
# - {Iterating}[rdoc-ref:Array@Methods+for+Iterating]
# - {Converting}[rdoc-ref:Array@Methods+for+Converting]
# - {And more....}[rdoc-ref:Array@Other+Methods]
#
# === Methods for Creating an +Array+
#
# - ::[]: Returns a new array populated with given objects.
# - ::new: Returns a new array.
# - ::try_convert: Returns a new array created from a given object.
#
# See also {Creating Arrays}[rdoc-ref:Array@Creating+Arrays].
#
# === Methods for Querying
#
# - #all?: Returns whether all elements meet a given criterion.
# - #any?: Returns whether any element meets a given criterion.
# - #count: Returns the count of elements that meet a given criterion.
# - #empty?: Returns whether there are no elements.
# - #find_index (aliased as #index): Returns the index of the first element that meets a given criterion.
# - #hash: Returns the integer hash code.
# - #include?: Returns whether any element <tt>==</tt> a given object.
# - #length (aliased as #size): Returns the count of elements.
# - #none?: Returns whether no element <tt>==</tt> a given object.
# - #one?: Returns whether exactly one element <tt>==</tt> a given object.
# - #rindex: Returns the index of the last element that meets a given criterion.
#
# === Methods for Comparing
#
# - #<=>: Returns -1, 0, or 1, as +self+ is less than, equal to, or greater than a given object.
# - #==: Returns whether each element in +self+ is <tt>==</tt> to the corresponding element in a given object.
# - #eql?: Returns whether each element in +self+ is <tt>eql?</tt> to the corresponding element in a given object.
#
# === Methods for Fetching
#
# These methods do not modify +self+.
#
# - #[] (aliased as #slice): Returns consecutive elements as determined by a given argument.
# - #assoc: Returns the first element that is an array whose first element <tt>==</tt> a given object.
# - #at: Returns the element at a given offset.
# - #bsearch: Returns an element selected via a binary search as determined by a given block.
# - #bsearch_index: Returns the index of an element selected via a binary search as determined by a given block.
# - #compact: Returns an array containing all non-+nil+ elements.
# - #dig: Returns the object in nested objects that is specified by a given index and additional arguments.
# - #drop: Returns trailing elements as determined by a given index.
# - #drop_while: Returns trailing elements as determined by a given block.
# - #fetch: Returns the element at a given offset.
# - #fetch_values: Returns elements at given offsets.
# - #first: Returns one or more leading elements.
# - #last: Returns one or more trailing elements.
# - #max: Returns one or more maximum-valued elements, as determined by <tt>#<=></tt> or a given block.
# - #min: Returns one or more minimum-valued elements, as determined by <tt>#<=></tt> or a given block.
# - #minmax: Returns the minimum-valued and maximum-valued elements, as determined by <tt>#<=></tt> or a given block.
# - #rassoc: Returns the first element that is an array whose second element <tt>==</tt> a given object.
# - #reject: Returns an array containing elements not rejected by a given block.
# - #reverse: Returns all elements in reverse order.
# - #rotate: Returns all elements with some rotated from one end to the other.
# - #sample: Returns one or more random elements.
# - #select (aliased as #filter): Returns an array containing elements selected by a given block.
# - #shuffle: Returns elements in a random order.
# - #sort: Returns all elements in an order determined by <tt>#<=></tt> or a given block.
# - #take: Returns leading elements as determined by a given index.
# - #take_while: Returns leading elements as determined by a given block.
# - #uniq: Returns an array containing non-duplicate elements.
# - #values_at: Returns the elements at given offsets.
#
# === Methods for Assigning
#
# These methods add, replace, or reorder elements in +self+.
#
# - #<<: Appends an element.
# - #[]=: Assigns specified elements with a given object.
# - #concat: Appends all elements from given arrays.
# - #fill: Replaces specified elements with specified objects.
# - #flatten!: Replaces each nested array in +self+ with the elements from that array.
# - #initialize_copy (aliased as #replace): Replaces the content of +self+ with the content of a given array.
# - #insert: Inserts given objects at a given offset; does not replace elements.
# - #push (aliased as #append): Appends elements.
# - #reverse!: Replaces +self+ with its elements reversed.
# - #rotate!: Replaces +self+ with its elements rotated.
# - #shuffle!: Replaces +self+ with its elements in random order.
# - #sort!: Replaces +self+ with its elements sorted, as determined by <tt>#<=></tt> or a given block.
# - #sort_by!: Replaces +self+ with its elements sorted, as determined by a given block.
# - #unshift (aliased as #prepend): Prepends leading elements.
#
# === Methods for Deleting
#
# Each of these methods removes elements from +self+:
#
# - #clear: Removes all elements.
# - #compact!: Removes all +nil+ elements.
# - #delete: Removes elements equal to a given object.
# - #delete_at: Removes the element at a given offset.
# - #delete_if: Removes elements specified by a given block.
# - #keep_if: Removes elements not specified by a given block.
# - #pop: Removes and returns the last element.
# - #reject!: Removes elements specified by a given block.
# - #select! (aliased as #filter!): Removes elements not specified by a given block.
# - #shift:  Removes and returns the first element.
# - #slice!: Removes and returns a sequence of elements.
# - #uniq!: Removes duplicates.
#
# === Methods for Combining
#
# - #&: Returns an array containing elements found both in +self+ and a given array.
# - #+: Returns an array containing all elements of +self+ followed by all elements of a given array.
# - #-: Returns an array containing all elements of +self+ that are not found in a given array.
# - #|: Returns an array containing all element of +self+ and all elements of a given array, duplicates removed.
# - #difference: Returns an array containing all elements of +self+ that are not found in any of the given arrays..
# - #intersection: Returns an array containing elements found both in +self+ and in each given array.
# - #product: Returns or yields all combinations of elements from +self+ and given arrays.
# - #reverse: Returns an array containing all elements of +self+ in reverse order.
# - #union: Returns an array containing all elements of +self+ and all elements of given arrays, duplicates removed.
#
# === Methods for Iterating
#
# - #combination: Calls a given block with combinations of elements of +self+; a combination does not use the same element more than once.
# - #cycle: Calls a given block with each element, then does so again, for a specified number of times, or forever.
# - #each: Passes each element to a given block.
# - #each_index: Passes each element index to a given block.
# - #permutation: Calls a given block with permutations of elements of +self+; a permutation does not use the same element more than once.
# - #repeated_combination: Calls a given block with combinations of elements of +self+; a combination may use the same element more than once.
# - #repeated_permutation: Calls a given block with permutations of elements of +self+; a permutation may use the same element more than once.
# - #reverse_each:  Passes each element, in reverse order, to a given block.
#
# === Methods for Converting
#
# - #collect (aliased as #map): Returns an array containing the block return-value for each element.
# - #collect! (aliased as #map!): Replaces each element with a block return-value.
# - #flatten: Returns an array that is a recursive flattening of +self+.
# - #inspect (aliased as #to_s): Returns a new String containing the elements.
# - #join: Returns a newsString containing the elements joined by the field separator.
# - #to_a: Returns +self+ or a new array containing all elements.
# - #to_ary: Returns +self+.
# - #to_h: Returns a new hash formed from the elements.
# - #transpose: Transposes +self+, which must be an array of arrays.
# - #zip: Returns a new array of arrays containing +self+ and given arrays.
#
# === Other Methods
#
# - #*: Returns one of the following:
#
#   - With integer argument +n+, a new array that is the concatenation
#     of +n+ copies of +self+.
#   - With string argument +field_separator+, a new string that is equivalent to
#     <tt>join(field_separator)</tt>.
#
# - #pack: Packs the elements into a binary sequence.
# - #sum: Returns a sum of elements according to either <tt>+</tt> or a given block.
class Array
  include Enumerable

  # Returns a new array, populated with the given objects:
  #
  #   Array[1, 'a', /^A/]    # => [1, "a", /^A/]
  #   Array[]                # => []
  #   Array.[](1, 'a', /^A/) # => [1, "a", /^A/]
  #
  # Related: see {Methods for Creating an Array}[rdoc-ref:Array@Methods+for+Creating+an+Array].
  def self.[](*args) end

  # Attempts to return an array, based on the given +object+.
  #
  # If +object+ is an array, returns +object+.
  #
  # Otherwise if +object+ responds to <tt>:to_ary</tt>.
  # calls <tt>object.to_ary</tt>:
  # if the return value is an array or +nil+, returns that value;
  # if not, raises TypeError.
  #
  # Otherwise returns +nil+.
  #
  # Related: see {Methods for Creating an Array}[rdoc-ref:Array@Methods+for+Creating+an+Array].
  def self.try_convert(object) end

  # Returns a new array.
  #
  # With no block and no argument given, returns a new empty array:
  #
  #   Array.new # => []
  #
  # With no block and array argument given, returns a new array with the same elements:
  #
  #   Array.new([:foo, 'bar', 2]) # => [:foo, "bar", 2]
  #
  # With no block and integer argument given, returns a new array containing
  # that many instances of the given +default_value+:
  #
  #   Array.new(0)    # => []
  #   Array.new(3)    # => [nil, nil, nil]
  #   Array.new(2, 3) # => [3, 3]
  #
  # With a block given, returns an array of the given +size+;
  # calls the block with each +index+ in the range <tt>(0...size)</tt>;
  # the element at that +index+ in the returned array is the blocks return value:
  #
  #   Array.new(3)  {|index| "Element #{index}" } # => ["Element 0", "Element 1", "Element 2"]
  #
  # A common pitfall for new Rubyists is providing an expression as +default_value+:
  #
  #   array = Array.new(2, {})
  #   array # => [{}, {}]
  #   array[0][:a] = 1
  #   array # => [{a: 1}, {a: 1}], as array[0] and array[1] are same object
  #
  # If you want the elements of the array to be distinct, you should pass a block:
  #
  #   array = Array.new(2) { {} }
  #   array # => [{}, {}]
  #   array[0][:a] = 1
  #   array # => [{a: 1}, {}], as array[0] and array[1] are different objects
  #
  # Raises TypeError if the first argument is not either an array
  # or an {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects]).
  # Raises ArgumentError if the first argument is a negative integer.
  #
  # Related: see {Methods for Creating an Array}[rdoc-ref:Array@Methods+for+Creating+an+Array].
  def initialize(...) end

  # Returns a new array containing the _intersection_ of +self+ and +other_array+;
  # that is, containing those elements found in both +self+ and +other_array+:
  #
  #   [0, 1, 2, 3] & [1, 2] # => [1, 2]
  #
  # Omits duplicates:
  #
  #   [0, 1, 1, 0] & [0, 1] # => [0, 1]
  #
  # Preserves order from +self+:
  #
  #   [0, 1, 2] & [3, 2, 1, 0] # => [0, 1, 2]
  #
  # Identifies common elements using method <tt>#eql?</tt>
  # (as defined in each element of +self+).
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def &(other) end

  # When non-negative integer argument +n+ is given,
  # returns a new array built by concatenating +n+ copies of +self+:
  #
  #   a = ['x', 'y']
  #   a * 3 # => ["x", "y", "x", "y", "x", "y"]
  #
  # When string argument +string_separator+ is given,
  # equivalent to <tt>self.join(string_separator)</tt>:
  #
  #   [0, [0, 1], {foo: 0}] * ', ' # => "0, 0, 1, {foo: 0}"
  def *(...) end

  # Returns a new array containing all elements of +self+
  # followed by all elements of +other_array+:
  #
  #   a = [0, 1] + [2, 3]
  #   a # => [0, 1, 2, 3]
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def +(other) end

  # Returns a new array containing only those elements of +self+
  # that are not found in +other_array+;
  # the order from +self+ is preserved:
  #
  #   [0, 1, 1, 2, 1, 1, 3, 1, 1] - [1]             # => [0, 2, 3]
  #   [0, 1, 1, 2, 1, 1, 3, 1, 1] - [3, 2, 0, :foo] # => [1, 1, 1, 1, 1, 1]
  #   [0, 1, 2] - [:foo]                            # => [0, 1, 2]
  #
  # Element are compared using method <tt>#eql?</tt>
  # (as defined in each element of +self+).
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def -(other) end

  # Appends +object+ as the last element in +self+; returns +self+:
  #
  #   [:foo, 'bar', 2] << :baz # => [:foo, "bar", 2, :baz]
  #
  # Appends +object+ as a single element, even if it is another array:
  #
  #   [:foo, 'bar', 2] << [3, 4] # => [:foo, "bar", 2, [3, 4]]
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def <<(object) end

  # Returns -1, 0, or 1 as +self+ is determined
  # to be less than, equal to, or greater than +other_array+.
  #
  # Iterates over each index +i+ in <tt>(0...self.size)</tt>:
  #
  # - Computes <tt>result[i]</tt> as <tt>self[i] <=> other_array[i]</tt>.
  # - Immediately returns 1 if <tt>result[i]</tt> is 1:
  #
  #     [0, 1, 2] <=> [0, 0, 2] # => 1
  #
  # - Immediately returns -1 if <tt>result[i]</tt> is -1:
  #
  #     [0, 1, 2] <=> [0, 2, 2] # => -1
  #
  # - Continues if <tt>result[i]</tt> is 0.
  #
  # When every +result+ is 0,
  # returns <tt>self.size <=> other_array.size</tt>
  # (see Integer#<=>):
  #
  #   [0, 1, 2] <=> [0, 1]        # => 1
  #   [0, 1, 2] <=> [0, 1, 2]     # => 0
  #   [0, 1, 2] <=> [0, 1, 2, 3]  # => -1
  #
  # Note that when +other_array+ is larger than +self+,
  # its trailing elements do not affect the result:
  #
  #   [0, 1, 2] <=> [0, 1, 2, -3] # => -1
  #   [0, 1, 2] <=> [0, 1, 2, 0]  # => -1
  #   [0, 1, 2] <=> [0, 1, 2, 3]  # => -1
  #
  # Related: see {Methods for Comparing}[rdoc-ref:Array@Methods+for+Comparing].
  def <=>(other) end

  # Returns whether both:
  #
  # - +self+ and +other_array+ are the same size.
  # - Their corresponding elements are the same;
  #   that is, for each index +i+ in <tt>(0...self.size)</tt>,
  #   <tt>self[i] == other_array[i]</tt>.
  #
  # Examples:
  #
  #   [:foo, 'bar', 2] == [:foo, 'bar', 2]   # => true
  #   [:foo, 'bar', 2] == [:foo, 'bar', 2.0] # => true
  #   [:foo, 'bar', 2] == [:foo, 'bar']      # => false # Different sizes.
  #   [:foo, 'bar', 2] == [:foo, 'bar', 3]   # => false # Different elements.
  #
  # This method is different from method Array#eql?,
  # which compares elements using <tt>Object#eql?</tt>.
  #
  # Related: see {Methods for Comparing}[rdoc-ref:Array@Methods+for+Comparing].
  def ==(other) end

  # Returns elements from +self+; does not modify +self+.
  #
  # In brief:
  #
  #   a = [:foo, 'bar', 2]
  #
  #   # Single argument index: returns one element.
  #   a[0]     # => :foo          # Zero-based index.
  #   a[-1]    # => 2             # Negative index counts backwards from end.
  #
  #   # Arguments start and length: returns an array.
  #   a[1, 2]  # => ["bar", 2]
  #   a[-2, 2] # => ["bar", 2]    # Negative start counts backwards from end.
  #
  #   # Single argument range: returns an array.
  #   a[0..1]  # => [:foo, "bar"]
  #   a[0..-2] # => [:foo, "bar"] # Negative range-begin counts backwards from end.
  #   a[-2..2] # => ["bar", 2]    # Negative range-end counts backwards from end.
  #
  # When a single integer argument +index+ is given, returns the element at offset +index+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[0] # => :foo
  #   a[2] # => 2
  #   a # => [:foo, "bar", 2]
  #
  # If +index+ is negative, counts backwards from the end of +self+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[-1] # => 2
  #   a[-2] # => "bar"
  #
  # If +index+ is out of range, returns +nil+.
  #
  # When two Integer arguments +start+ and +length+ are given,
  # returns a new +Array+ of size +length+ containing successive elements beginning at offset +start+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[0, 2] # => [:foo, "bar"]
  #   a[1, 2] # => ["bar", 2]
  #
  # If <tt>start + length</tt> is greater than <tt>self.length</tt>,
  # returns all elements from offset +start+ to the end:
  #
  #   a = [:foo, 'bar', 2]
  #   a[0, 4] # => [:foo, "bar", 2]
  #   a[1, 3] # => ["bar", 2]
  #   a[2, 2] # => [2]
  #
  # If <tt>start == self.size</tt> and <tt>length >= 0</tt>,
  # returns a new empty +Array+.
  #
  # If +length+ is negative, returns +nil+.
  #
  # When a single Range argument +range+ is given,
  # treats <tt>range.min</tt> as +start+ above
  # and <tt>range.size</tt> as +length+ above:
  #
  #   a = [:foo, 'bar', 2]
  #   a[0..1] # => [:foo, "bar"]
  #   a[1..2] # => ["bar", 2]
  #
  # Special case: If <tt>range.start == a.size</tt>, returns a new empty +Array+.
  #
  # If <tt>range.end</tt> is negative, calculates the end index from the end:
  #
  #   a = [:foo, 'bar', 2]
  #   a[0..-1] # => [:foo, "bar", 2]
  #   a[0..-2] # => [:foo, "bar"]
  #   a[0..-3] # => [:foo]
  #
  # If <tt>range.start</tt> is negative, calculates the start index from the end:
  #
  #   a = [:foo, 'bar', 2]
  #   a[-1..2] # => [2]
  #   a[-2..2] # => ["bar", 2]
  #   a[-3..2] # => [:foo, "bar", 2]
  #
  # If <tt>range.start</tt> is larger than the array size, returns +nil+.
  #
  #   a = [:foo, 'bar', 2]
  #   a[4..1] # => nil
  #   a[4..0] # => nil
  #   a[4..-1] # => nil
  #
  # When a single Enumerator::ArithmeticSequence argument +aseq+ is given,
  # returns an +Array+ of elements corresponding to the indexes produced by
  # the sequence.
  #
  #   a = ['--', 'data1', '--', 'data2', '--', 'data3']
  #   a[(1..).step(2)] # => ["data1", "data2", "data3"]
  #
  # Unlike slicing with range, if the start or the end of the arithmetic sequence
  # is larger than array size, throws RangeError.
  #
  #   a = ['--', 'data1', '--', 'data2', '--', 'data3']
  #   a[(1..11).step(2)]
  #   # RangeError (((1..11).step(2)) out of range)
  #   a[(7..).step(2)]
  #   # RangeError (((7..).step(2)) out of range)
  #
  # If given a single argument, and its type is not one of the listed, tries to
  # convert it to Integer, and raises if it is impossible:
  #
  #   a = [:foo, 'bar', 2]
  #   # Raises TypeError (no implicit conversion of Symbol into Integer):
  #   a[:foo]
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def [](...) end
  alias slice []

  # Assigns elements in +self+, based on the given +object+; returns +object+.
  #
  # In brief:
  #
  #     a_orig = [:foo, 'bar', 2]
  #
  #     # With argument index.
  #     a = a_orig.dup
  #     a[0] = 'foo' # => "foo"
  #     a # => ["foo", "bar", 2]
  #     a = a_orig.dup
  #     a[7] = 'foo' # => "foo"
  #     a # => [:foo, "bar", 2, nil, nil, nil, nil, "foo"]
  #
  #     # With arguments start and length.
  #     a = a_orig.dup
  #     a[0, 2] = 'foo' # => "foo"
  #     a # => ["foo", 2]
  #     a = a_orig.dup
  #     a[6, 50] = 'foo' # => "foo"
  #     a # => [:foo, "bar", 2, nil, nil, nil, "foo"]
  #
  #     # With argument range.
  #     a = a_orig.dup
  #     a[0..1] = 'foo' # => "foo"
  #     a # => ["foo", 2]
  #     a = a_orig.dup
  #     a[6..50] = 'foo' # => "foo"
  #     a # => [:foo, "bar", 2, nil, nil, nil, "foo"]
  #
  # When Integer argument +index+ is given, assigns +object+ to an element in +self+.
  #
  # If +index+ is non-negative, assigns +object+ the element at offset +index+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[0] = 'foo' # => "foo"
  #   a # => ["foo", "bar", 2]
  #
  # If +index+ is greater than <tt>self.length</tt>, extends the array:
  #
  #   a = [:foo, 'bar', 2]
  #   a[7] = 'foo' # => "foo"
  #   a # => [:foo, "bar", 2, nil, nil, nil, nil, "foo"]
  #
  # If +index+ is negative, counts backwards from the end of the array:
  #
  #   a = [:foo, 'bar', 2]
  #   a[-1] = 'two' # => "two"
  #   a # => [:foo, "bar", "two"]
  #
  # When Integer arguments +start+ and +length+ are given and +object+ is not an +Array+,
  # removes <tt>length - 1</tt> elements beginning at offset +start+,
  # and assigns +object+ at offset +start+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[0, 2] = 'foo' # => "foo"
  #   a # => ["foo", 2]
  #
  # If +start+ is negative, counts backwards from the end of the array:
  #
  #   a = [:foo, 'bar', 2]
  #   a[-2, 2] = 'foo' # => "foo"
  #   a # => [:foo, "foo"]
  #
  # If +start+ is non-negative and outside the array (<tt> >= self.size</tt>),
  # extends the array with +nil+, assigns +object+ at offset +start+,
  # and ignores +length+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[6, 50] = 'foo' # => "foo"
  #   a # => [:foo, "bar", 2, nil, nil, nil, "foo"]
  #
  # If +length+ is zero, shifts elements at and following offset +start+
  # and assigns +object+ at offset +start+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[1, 0] = 'foo' # => "foo"
  #   a # => [:foo, "foo", "bar", 2]
  #
  # If +length+ is too large for the existing array, does not extend the array:
  #
  #   a = [:foo, 'bar', 2]
  #   a[1, 5] = 'foo' # => "foo"
  #   a # => [:foo, "foo"]
  #
  # When Range argument +range+ is given and +object+ is not an +Array+,
  # removes <tt>length - 1</tt> elements beginning at offset +start+,
  # and assigns +object+ at offset +start+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[0..1] = 'foo' # => "foo"
  #   a # => ["foo", 2]
  #
  # if <tt>range.begin</tt> is negative, counts backwards from the end of the array:
  #
  #   a = [:foo, 'bar', 2]
  #   a[-2..2] = 'foo' # => "foo"
  #   a # => [:foo, "foo"]
  #
  # If the array length is less than <tt>range.begin</tt>,
  # extends the array with +nil+, assigns +object+ at offset <tt>range.begin</tt>,
  # and ignores +length+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[6..50] = 'foo' # => "foo"
  #   a # => [:foo, "bar", 2, nil, nil, nil, "foo"]
  #
  # If <tt>range.end</tt> is zero, shifts elements at and following offset +start+
  # and assigns +object+ at offset +start+:
  #
  #   a = [:foo, 'bar', 2]
  #   a[1..0] = 'foo' # => "foo"
  #   a # => [:foo, "foo", "bar", 2]
  #
  # If <tt>range.end</tt> is negative, assigns +object+ at offset +start+,
  # retains <tt>range.end.abs -1</tt> elements past that, and removes those beyond:
  #
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
  #
  #   a = [:foo, 'bar', 2]
  #   a[1..5] = 'foo' # => "foo"
  #   a # => [:foo, "foo"]
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def []=(...) end

  # Returns the union of +self+ and +other_array+;
  # duplicates are removed; order is preserved;
  # items are compared using <tt>eql?</tt>:
  #
  #   [0, 1] | [2, 3] # => [0, 1, 2, 3]
  #   [0, 1, 1] | [2, 2, 3] # => [0, 1, 2, 3]
  #   [0, 1, 2] | [3, 2, 1, 0] # => [0, 1, 2, 3]
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def |(other) end

  # Returns whether for every element of +self+,
  # a given criterion is satisfied.
  #
  # With no block and no argument,
  # returns whether every element of +self+ is truthy:
  #
  #   [[], {}, '', 0, 0.0, Object.new].all? # => true  # All truthy objects.
  #   [[], {}, '', 0, 0.0, nil].all?        # => false # nil is not truthy.
  #   [[], {}, '', 0, 0.0, false].all?      # => false # false is not truthy.
  #
  # With argument +object+ given, returns whether <tt>object === ele</tt>
  # for every element +ele+ in +self+:
  #
  #   [0, 0, 0].all?(0)                    # => true
  #   [0, 1, 2].all?(1)                    # => false
  #   ['food', 'fool', 'foot'].all?(/foo/) # => true
  #   ['food', 'drink'].all?(/foo/)        # => false
  #
  # With a block given, calls the block with each element in +self+;
  # returns whether the block returns only truthy values:
  #
  #   [0, 1, 2].all? { |ele| ele < 3 } # => true
  #   [0, 1, 2].all? { |ele| ele < 2 } # => false
  #
  # With both a block and argument +object+ given,
  # ignores the block and uses +object+ as above.
  #
  # <b>Special case</b>: returns +true+ if +self+ is empty
  # (regardless of any given argument or block).
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def all?(...) end

  # Returns whether for any element of +self+, a given criterion is satisfied.
  #
  # With no block and no argument, returns whether any element of +self+ is truthy:
  #
  #   [nil, false, []].any? # => true  # Array object is truthy.
  #   [nil, false, {}].any? # => true  # Hash object is truthy.
  #   [nil, false, ''].any? # => true  # String object is truthy.
  #   [nil, false].any?     # => false # Nil and false are not truthy.
  #
  # With argument +object+ given,
  # returns whether <tt>object === ele</tt> for any element +ele+ in +self+:
  #
  #   [nil, false, 0].any?(0)          # => true
  #   [nil, false, 1].any?(0)          # => false
  #   [nil, false, 'food'].any?(/foo/) # => true
  #   [nil, false, 'food'].any?(/bar/) # => false
  #
  # With a block given,
  # calls the block with each element in +self+;
  # returns whether the block returns any truthy value:
  #
  #   [0, 1, 2].any? {|ele| ele < 1 } # => true
  #   [0, 1, 2].any? {|ele| ele < 0 } # => false
  #
  # With both a block and argument +object+ given,
  # ignores the block and uses +object+ as above.
  #
  # <b>Special case</b>: returns +false+ if +self+ is empty
  # (regardless of any given argument or block).
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def any?(...) end

  # Returns the first element +ele+ in +self+ such that +ele+ is an array
  # and <tt>ele[0] == object</tt>:
  #
  #   a = [{foo: 0}, [2, 4], [4, 5, 6], [4, 5]]
  #   a.assoc(4) # => [4, 5, 6]
  #
  # Returns +nil+ if no such element is found.
  #
  # Related: Array#rassoc;
  # see also {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def assoc(object) end

  # Returns the element of +self+ specified by the given +index+
  # or +nil+ if there is no such element;
  # +index+ must be an
  # {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects].
  #
  # For non-negative +index+, returns the element of +self+ at offset +index+:
  #
  #   a = [:foo, 'bar', 2]
  #   a.at(0)   # => :foo
  #   a.at(2)   # => 2
  #   a.at(2.0) # => 2
  #
  # For negative +index+, counts backwards from the end of +self+:
  #
  #   a.at(-2) # => "bar"
  #
  # Related: Array#[];
  # see also {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def at(index) end

  # Returns the element from +self+ found by a binary search,
  # or +nil+ if the search found no suitable element.
  #
  # See {Binary Searching}[rdoc-ref:bsearch.rdoc].
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def bsearch; end

  # Returns the integer index of the element from +self+ found by a binary search,
  # or +nil+ if the search found no suitable element.
  #
  # See {Binary Searching}[rdoc-ref:bsearch.rdoc].
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def bsearch_index; end

  # Removes all elements from +self+; returns +self+:
  #
  #   a = [:foo, 'bar', 2]
  #   a.clear # => []
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def clear; end

  # With a block given, calls the block with each element of +self+;
  # returns a new array whose elements are the return values from the block:
  #
  #   a = [:foo, 'bar', 2]
  #   a1 = a.map {|element| element.class }
  #   a1 # => [Symbol, String, Integer]
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: #collect!;
  # see also {Methods for Converting}[rdoc-ref:Array@Methods+for+Converting].
  def collect; end
  alias map collect

  # With a block given, calls the block with each element of +self+
  # and replaces the element with the block's return value;
  # returns +self+:
  #
  #   a = [:foo, 'bar', 2]
  #   a.map! { |element| element.class } # => [Symbol, String, Integer]
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: #collect;
  # see also {Methods for Converting}[rdoc-ref:Array@Methods+for+Converting].
  def collect!; end
  alias map! collect!

  # When a block and a positive
  # {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects]
  # argument +count+ (<tt>0 < count <= self.size</tt>)
  # are given, calls the block with each combination of +self+ of size +count+;
  # returns +self+:
  #
  #   a = %w[a b c]                                   # => ["a", "b", "c"]
  #   a.combination(2) {|combination| p combination } # => ["a", "b", "c"]
  #
  # Output:
  #
  #   ["a", "b"]
  #   ["a", "c"]
  #   ["b", "c"]
  #
  # The order of the yielded combinations is not guaranteed.
  #
  # When +count+ is zero, calls the block once with a new empty array:
  #
  #   a.combination(0) {|combination| p combination }
  #   [].combination(0) {|combination| p combination }
  #
  # Output:
  #
  #   []
  #   []
  #
  # When +count+ is negative or larger than +self.size+ and +self+ is non-empty,
  # does not call the block:
  #
  #   a.combination(-1) {|combination| fail 'Cannot happen' } # => ["a", "b", "c"]
  #   a.combination(4)  {|combination| fail 'Cannot happen' } # => ["a", "b", "c"]
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: Array#permutation;
  # see also {Methods for Iterating}[rdoc-ref:Array@Methods+for+Iterating].
  def combination(count) end

  # Returns a new array containing only the non-+nil+ elements from +self+;
  # element order is preserved:
  #
  #   a = [nil, 0, nil, false, nil, '', nil, [], nil, {}]
  #   a.compact # => [0, false, "", [], {}]
  #
  # Related: Array#compact!;
  # see also {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def compact; end

  # Removes all +nil+ elements from +self+;
  # Returns +self+ if any elements are removed, +nil+ otherwise:
  #
  #   a = [nil, 0, nil, false, nil, '', nil, [], nil, {}]
  #   a.compact! # => [0, false, "", [], {}]
  #   a          # => [0, false, "", [], {}]
  #   a.compact! # => nil
  #
  # Related: Array#compact;
  # see also {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def compact!; end

  # Adds to +self+ all elements from each array in +other_arrays+; returns +self+:
  #
  #   a = [0, 1]
  #   a.concat(['two', 'three'], [:four, :five], a)
  #   # => [0, 1, "two", "three", :four, :five, 0, 1]
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def concat(*other_arrays) end

  # Returns a count of specified elements.
  #
  # With no argument and no block, returns the count of all elements:
  #
  #   [0, :one, 'two', 3, 3.0].count # => 5
  #
  # With argument +object+ given, returns the count of elements <tt>==</tt> to +object+:
  #
  #   [0, :one, 'two', 3, 3.0].count(3) # => 2
  #
  # With no argument and a block given, calls the block with each element;
  # returns the count of elements for which the block returns a truthy value:
  #
  #   [0, 1, 2, 3].count {|element| element > 1 } # => 2
  #
  # With argument +object+ and a block given, issues a warning, ignores the block,
  # and returns the count of elements <tt>==</tt> to +object+.
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def count(...) end

  # With a block given, may call the block, depending on the value of argument +count+;
  # +count+ must be an
  # {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects],
  # or +nil+.
  #
  # When +count+ is positive,
  # calls the block with each element, then does so repeatedly,
  # until it has done so +count+ times; returns +nil+:
  #
  #   output = []
  #   [0, 1].cycle(2) {|element| output.push(element) } # => nil
  #   output # => [0, 1, 0, 1]
  #
  # When +count+ is zero or negative, does not call the block:
  #
  #   [0, 1].cycle(0) {|element| fail 'Cannot happen' }  # => nil
  #   [0, 1].cycle(-1) {|element| fail 'Cannot happen' } # => nil
  #
  # When +count+ is +nil+, cycles forever:
  #
  #   # Prints 0 and 1 forever.
  #   [0, 1].cycle {|element| puts element }
  #   [0, 1].cycle(nil) {|element| puts element }
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Iterating}[rdoc-ref:Array@Methods+for+Iterating].
  def cycle(count = nil) end

  # Removes zero or more elements from +self+.
  #
  # With no block given,
  # removes from +self+ each element +ele+ such that <tt>ele == object</tt>;
  # returns the last removed element:
  #
  #   a = [0, 1, 2, 2.0]
  #   a.delete(2) # => 2.0
  #   a           # => [0, 1]
  #
  # Returns +nil+ if no elements removed:
  #
  #   a.delete(2) # => nil
  #
  # With a block given,
  # removes from +self+ each element +ele+ such that <tt>ele == object</tt>.
  #
  # If any such elements are found, ignores the block
  # and returns the last removed element:
  #
  #   a = [0, 1, 2, 2.0]
  #   a.delete(2) {|element| fail 'Cannot happen' } # => 2.0
  #   a                                             # => [0, 1]
  #
  # If no such element is found, returns the block's return value:
  #
  #   a.delete(2) {|element| "Element #{element} not found." }
  #   # => "Element 2 not found."
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def delete(object) end

  # Removes the element of +self+ at the given +index+, which must be an
  # {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects].
  #
  # When +index+ is non-negative, deletes the element at offset +index+:
  #
  #   a = [:foo, 'bar', 2]
  #   a.delete_at(1) # => "bar"
  #   a # => [:foo, 2]
  #
  # When +index+ is negative, counts backward from the end of the array:
  #
  #   a = [:foo, 'bar', 2]
  #   a.delete_at(-2) # => "bar"
  #   a # => [:foo, 2]
  #
  # When +index+ is out of range, returns +nil+.
  #
  #   a = [:foo, 'bar', 2]
  #   a.delete_at(3)  # => nil
  #   a.delete_at(-4) # => nil
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def delete_at(index) end

  # With a block given, calls the block with each element of +self+;
  # removes the element if the block returns a truthy value;
  # returns +self+:
  #
  #   a = [:foo, 'bar', 2, 'bat']
  #   a.delete_if {|element| element.to_s.start_with?('b') } # => [:foo, 2]
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def delete_if; end

  # Returns a new array containing only those elements from +self+
  # that are not found in any of the given +other_arrays+;
  # items are compared using <tt>eql?</tt>;  order from +self+ is preserved:
  #
  #   [0, 1, 1, 2, 1, 1, 3, 1, 1].difference([1]) # => [0, 2, 3]
  #   [0, 1, 2, 3].difference([3, 0], [1, 3])     # => [2]
  #   [0, 1, 2].difference([4])                   # => [0, 1, 2]
  #   [0, 1, 2].difference                        # => [0, 1, 2]
  #
  # Returns a copy of +self+ if no arguments are given.
  #
  # Related: Array#-;
  # see also {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def difference(*args) end

  # Finds and returns the object in nested object
  # specified by +index+ and +identifiers+;
  # the nested objects may be instances of various classes.
  # See {Dig Methods}[rdoc-ref:dig_methods.rdoc].
  #
  # Examples:
  #
  #   a = [:foo, [:bar, :baz, [:bat, :bam]]]
  #   a.dig(1) # => [:bar, :baz, [:bat, :bam]]
  #   a.dig(1, 2) # => [:bat, :bam]
  #   a.dig(1, 2, 0) # => :bat
  #   a.dig(1, 2, 3) # => nil
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def dig(index, *identifiers) end

  # Returns a new array containing all but the first +count+ element of +self+,
  # where +count+ is a non-negative integer;
  # does not modify +self+.
  #
  # Examples:
  #
  #   a = [0, 1, 2, 3, 4, 5]
  #   a.drop(0) # => [0, 1, 2, 3, 4, 5]
  #   a.drop(1) # => [1, 2, 3, 4, 5]
  #   a.drop(2) # => [2, 3, 4, 5]
  #   a.drop(9) # => []
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def drop(count) end

  # With a block given, calls the block with each successive element of +self+;
  # stops if the block returns +false+ or +nil+;
  # returns a new array _omitting_ those elements for which the block returned a truthy value;
  # does not modify +self+:
  #
  #   a = [0, 1, 2, 3, 4, 5]
  #   a.drop_while {|element| element < 3 } # => [3, 4, 5]
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def drop_while; end

  # With a block given, iterates over the elements of +self+,
  # passing each element to the block;
  # returns +self+:
  #
  #   a = [:foo, 'bar', 2]
  #   a.each {|element|  puts "#{element.class} #{element}" }
  #
  # Output:
  #
  #   Symbol foo
  #   String bar
  #   Integer 2
  #
  # Allows the array to be modified during iteration:
  #
  #   a = [:foo, 'bar', 2]
  #   a.each {|element| puts element; a.clear if element.to_s.start_with?('b') }
  #
  # Output:
  #
  #   foo
  #   bar
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Iterating}[rdoc-ref:Array@Methods+for+Iterating].
  def each; end

  # With a block given, iterates over the elements of +self+,
  # passing each <i>array index</i> to the block;
  # returns +self+:
  #
  #   a = [:foo, 'bar', 2]
  #   a.each_index {|index|  puts "#{index} #{a[index]}" }
  #
  # Output:
  #
  #   0 foo
  #   1 bar
  #   2 2
  #
  # Allows the array to be modified during iteration:
  #
  #   a = [:foo, 'bar', 2]
  #   a.each_index {|index| puts index; a.clear if index > 0 }
  #   a # => []
  #
  # Output:
  #
  #   0
  #   1
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Iterating}[rdoc-ref:Array@Methods+for+Iterating].
  def each_index; end

  # Returns +true+ if the count of elements in +self+ is zero,
  # +false+ otherwise.
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def empty?; end

  # Returns +true+ if +self+ and +other_array+ are the same size,
  # and if, for each index +i+ in +self+, <tt>self[i].eql?(other_array[i])</tt>:
  #
  #   a0 = [:foo, 'bar', 2]
  #   a1 = [:foo, 'bar', 2]
  #   a1.eql?(a0) # => true
  #
  # Otherwise, returns +false+.
  #
  # This method is different from method Array#==,
  # which compares using method <tt>Object#==</tt>.
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def eql?(other) end

  # Returns the element of +self+ at offset +index+ if +index+ is in range; +index+ must be an
  # {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects].
  #
  # With the single argument +index+ and no block,
  # returns the element at offset +index+:
  #
  #   a = [:foo, 'bar', 2]
  #   a.fetch(1)   # => "bar"
  #   a.fetch(1.1) # => "bar"
  #
  # If +index+ is negative, counts from the end of the array:
  #
  #   a = [:foo, 'bar', 2]
  #   a.fetch(-1) # => 2
  #   a.fetch(-2) # => "bar"
  #
  # With arguments +index+ and +default_value+ (which may be any object) and no block,
  # returns +default_value+ if +index+ is out-of-range:
  #
  #   a = [:foo, 'bar', 2]
  #   a.fetch(1, nil)  # => "bar"
  #   a.fetch(3, :foo) # => :foo
  #
  # With argument +index+ and a block,
  # returns the element at offset +index+ if index is in range
  # (and the block is not called); otherwise calls the block with index and returns its return value:
  #
  #   a = [:foo, 'bar', 2]
  #   a.fetch(1) {|index| raise 'Cannot happen' } # => "bar"
  #   a.fetch(50) {|index| "Value for #{index}" } # => "Value for 50"
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def fetch(...) end

  # With no block given, returns a new array containing the elements of +self+
  # at the offsets specified by +indexes+. Each of the +indexes+ must be an
  # {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects]:
  #
  #    a = [:foo, :bar, :baz]
  #    a.fetch_values(2, 0)   # => [:baz, :foo]
  #    a.fetch_values(2.1, 0) # => [:baz, :foo]
  #    a.fetch_values         # => []
  #
  # For a negative index, counts backwards from the end of the array:
  #
  #    a.fetch_values(-2, -1) # [:bar, :baz]
  #
  # When no block is given, raises an exception if any index is out of range.
  #
  # With a block given, for each index:
  #
  # - If the index is in range, uses an element of +self+ (as above).
  # - Otherwise, calls the block with the index and uses the block's return value.
  #
  # Example:
  #
  #   a = [:foo, :bar, :baz]
  #   a.fetch_values(1, 0, 42, 777) { |index| index.to_s }
  #   # => [:bar, :foo, "42", "777"]
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def fetch_values(*indexes) end

  # Replaces selected elements in +self+;
  # may add elements to +self+;
  # always returns +self+ (never a new array).
  #
  # In brief:
  #
  #   # Non-negative start.
  #   ['a', 'b', 'c', 'd'].fill('-', 1, 2)          # => ["a", "-", "-", "d"]
  #   ['a', 'b', 'c', 'd'].fill(1, 2) {|e| e.to_s } # => ["a", "1", "2", "d"]
  #
  #   # Extends with specified values if necessary.
  #   ['a', 'b', 'c', 'd'].fill('-', 3, 2)          # => ["a", "b", "c", "-", "-"]
  #   ['a', 'b', 'c', 'd'].fill(3, 2) {|e| e.to_s } # => ["a", "b", "c", "3", "4"]
  #
  #   # Fills with nils if necessary.
  #   ['a', 'b', 'c', 'd'].fill('-', 6, 2)          # => ["a", "b", "c", "d", nil, nil, "-", "-"]
  #   ['a', 'b', 'c', 'd'].fill(6, 2) {|e| e.to_s } # => ["a", "b", "c", "d", nil, nil, "6", "7"]
  #
  #   # For negative start, counts backwards from the end.
  #   ['a', 'b', 'c', 'd'].fill('-', -3, 3)          # => ["a", "-", "-", "-"]
  #   ['a', 'b', 'c', 'd'].fill(-3, 3) {|e| e.to_s } # => ["a", "1", "2", "3"]
  #
  #   # Range.
  #   ['a', 'b', 'c', 'd'].fill('-', 1..2)          # => ["a", "-", "-", "d"]
  #   ['a', 'b', 'c', 'd'].fill(1..2) {|e| e.to_s } # => ["a", "1", "2", "d"]
  #
  # When arguments +start+ and +count+ are given,
  # they select the elements of +self+ to be replaced;
  # each must be an
  # {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects]
  # (or +nil+):
  #
  # - +start+ specifies the zero-based offset of the first element to be replaced;
  #   +nil+ means zero.
  # - +count+ is the number of consecutive elements to be replaced;
  #   +nil+ means "all the rest."
  #
  # With argument +object+ given,
  # that one object is used for all replacements:
  #
  #   o = Object.new           # => #<Object:0x0000014e7bff7600>
  #   a = ['a', 'b', 'c', 'd'] # => ["a", "b", "c", "d"]
  #   a.fill(o, 1, 2)
  #   # => ["a", #<Object:0x0000014e7bff7600>, #<Object:0x0000014e7bff7600>, "d"]
  #
  # With a block given, the block is called once for each element to be replaced;
  # the value passed to the block is the _index_ of the element to be replaced
  # (not the element itself);
  # the block's return value replaces the element:
  #
  #   a = ['a', 'b', 'c', 'd']               # => ["a", "b", "c", "d"]
  #   a.fill(1, 2) {|element| element.to_s } # => ["a", "1", "2", "d"]
  #
  # For arguments +start+ and +count+:
  #
  # - If +start+ is non-negative,
  #   replaces +count+ elements beginning at offset +start+:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', 0, 2) # => ["-", "-", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', 1, 2) # => ["a", "-", "-", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', 2, 2) # => ["a", "b", "-", "-"]
  #
  #     ['a', 'b', 'c', 'd'].fill(0, 2) {|e| e.to_s } # => ["0", "1", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill(1, 2) {|e| e.to_s } # => ["a", "1", "2", "d"]
  #     ['a', 'b', 'c', 'd'].fill(2, 2) {|e| e.to_s } # => ["a", "b", "2", "3"]
  #
  #   Extends +self+ if necessary:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', 3, 2) # => ["a", "b", "c", "-", "-"]
  #     ['a', 'b', 'c', 'd'].fill('-', 4, 2) # => ["a", "b", "c", "d", "-", "-"]
  #
  #     ['a', 'b', 'c', 'd'].fill(3, 2) {|e| e.to_s } # => ["a", "b", "c", "3", "4"]
  #     ['a', 'b', 'c', 'd'].fill(4, 2) {|e| e.to_s } # => ["a", "b", "c", "d", "4", "5"]
  #
  #   Fills with +nil+ if necessary:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', 5, 2) # => ["a", "b", "c", "d", nil, "-", "-"]
  #     ['a', 'b', 'c', 'd'].fill('-', 6, 2) # => ["a", "b", "c", "d", nil, nil, "-", "-"]
  #
  #     ['a', 'b', 'c', 'd'].fill(5, 2) {|e| e.to_s } # => ["a", "b", "c", "d", nil, "5", "6"]
  #     ['a', 'b', 'c', 'd'].fill(6, 2) {|e| e.to_s } # => ["a", "b", "c", "d", nil, nil, "6", "7"]
  #
  #   Does nothing if +count+ is non-positive:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', 2, 0)    # => ["a", "b", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', 2, -100) # => ["a", "b", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', 6, -100) # => ["a", "b", "c", "d"]
  #
  #     ['a', 'b', 'c', 'd'].fill(2, 0) {|e| fail 'Cannot happen' }    # => ["a", "b", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill(2, -100) {|e| fail 'Cannot happen' } # => ["a", "b", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill(6, -100) {|e| fail 'Cannot happen' } # => ["a", "b", "c", "d"]
  #
  # - If +start+ is negative, counts backwards from the end of +self+:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', -4, 3) # => ["-", "-", "-", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', -3, 3) # => ["a", "-", "-", "-"]
  #
  #     ['a', 'b', 'c', 'd'].fill(-4, 3) {|e| e.to_s } # => ["0", "1", "2", "d"]
  #     ['a', 'b', 'c', 'd'].fill(-3, 3) {|e| e.to_s } # => ["a", "1", "2", "3"]
  #
  #   Extends +self+ if necessary:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', -2, 3) # => ["a", "b", "-", "-", "-"]
  #     ['a', 'b', 'c', 'd'].fill('-', -1, 3) # => ["a", "b", "c", "-", "-", "-"]
  #
  #     ['a', 'b', 'c', 'd'].fill(-2, 3) {|e| e.to_s } # => ["a", "b", "2", "3", "4"]
  #     ['a', 'b', 'c', 'd'].fill(-1, 3) {|e| e.to_s } # => ["a", "b", "c", "3", "4", "5"]
  #
  #   Starts at the beginning of +self+ if +start+ is negative and out-of-range:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', -5, 2) # => ["-", "-", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', -6, 2) # => ["-", "-", "c", "d"]
  #
  #     ['a', 'b', 'c', 'd'].fill(-5, 2) {|e| e.to_s } # => ["0", "1", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill(-6, 2) {|e| e.to_s } # => ["0", "1", "c", "d"]
  #
  #   Does nothing if +count+ is non-positive:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', -2, 0)  # => ["a", "b", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', -2, -1) # => ["a", "b", "c", "d"]
  #
  #     ['a', 'b', 'c', 'd'].fill(-2, 0) {|e| fail 'Cannot happen' }  # => ["a", "b", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill(-2, -1) {|e| fail 'Cannot happen' } # => ["a", "b", "c", "d"]
  #
  # When argument +range+ is given,
  # it must be a Range object whose members are numeric;
  # its +begin+ and +end+ values determine the elements of +self+
  # to be replaced:
  #
  # - If both +begin+ and +end+ are positive, they specify the first and last elements
  #   to be replaced:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', 1..2)          # => ["a", "-", "-", "d"]
  #     ['a', 'b', 'c', 'd'].fill(1..2) {|e| e.to_s } # => ["a", "1", "2", "d"]
  #
  #   If +end+ is smaller than +begin+, replaces no elements:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', 2..1)          # => ["a", "b", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill(2..1) {|e| e.to_s } # => ["a", "b", "c", "d"]
  #
  # - If either is negative (or both are negative), counts backwards from the end of +self+:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', -3..2)  # => ["a", "-", "-", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', 1..-2)  # => ["a", "-", "-", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', -3..-2) # => ["a", "-", "-", "d"]
  #
  #     ['a', 'b', 'c', 'd'].fill(-3..2) {|e| e.to_s }  # => ["a", "1", "2", "d"]
  #     ['a', 'b', 'c', 'd'].fill(1..-2) {|e| e.to_s }  # => ["a", "1", "2", "d"]
  #     ['a', 'b', 'c', 'd'].fill(-3..-2) {|e| e.to_s } # => ["a", "1", "2", "d"]
  #
  # - If the +end+ value is excluded (see Range#exclude_end?), omits the last replacement:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', 1...2)  # => ["a", "-", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill('-', 1...-2) # => ["a", "-", "c", "d"]
  #
  #     ['a', 'b', 'c', 'd'].fill(1...2) {|e| e.to_s }  # => ["a", "1", "c", "d"]
  #     ['a', 'b', 'c', 'd'].fill(1...-2) {|e| e.to_s } # => ["a", "1", "c", "d"]
  #
  # - If the range is endless (see {Endless Ranges}[rdoc-ref:Range@Endless+Ranges]),
  #   replaces elements to the end of +self+:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', 1..)          # => ["a", "-", "-", "-"]
  #     ['a', 'b', 'c', 'd'].fill(1..) {|e| e.to_s } # => ["a", "1", "2", "3"]
  #
  # - If the range is beginless (see {Beginless Ranges}[rdoc-ref:Range@Beginless+Ranges]),
  #   replaces elements from the beginning of +self+:
  #
  #     ['a', 'b', 'c', 'd'].fill('-', ..2)          # => ["-", "-", "-", "d"]
  #     ['a', 'b', 'c', 'd'].fill(..2) {|e| e.to_s } # => ["0", "1", "2", "d"]
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def fill(...) end

  # Returns the zero-based integer index of a specified element, or +nil+.
  #
  # With only argument +object+ given,
  # returns the index of the first element +element+
  # for which <tt>object == element</tt>:
  #
  #   a = [:foo, 'bar', 2, 'bar']
  #   a.index('bar') # => 1
  #
  # Returns +nil+ if no such element found.
  #
  # With only a block given,
  # calls the block with each successive element;
  # returns the index of the first element for which the block returns a truthy value:
  #
  #   a = [:foo, 'bar', 2, 'bar']
  #   a.index {|element| element == 'bar' } # => 1
  #
  # Returns +nil+ if the block never returns a truthy value.
  #
  # With neither an argument nor a block given, returns a new Enumerator.
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def find_index(*args) end
  alias index find_index

  # Returns elements from +self+, or +nil+; does not modify +self+.
  #
  # With no argument given, returns the first element (if available):
  #
  #   a = [:foo, 'bar', 2]
  #   a.first # => :foo
  #   a # => [:foo, "bar", 2]
  #
  # If +self+ is empty, returns +nil+.
  #
  #   [].first # => nil
  #
  # With a non-negative integer argument +count+ given,
  # returns the first +count+ elements (as available) in a new array:
  #
  #   a.first(0)  # => []
  #   a.first(2)  # => [:foo, "bar"]
  #   a.first(50) # => [:foo, "bar", 2]
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def first(...) end

  # Returns a new array that is a recursive flattening of +self+
  # to +depth+ levels of recursion;
  # +depth+ must be an
  # {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects]
  # or +nil+.
  # At each level of recursion:
  #
  # - Each element that is an array is "flattened"
  #   (that is, replaced by its individual array elements).
  # - Each element that is not an array is unchanged
  #   (even if the element is an object that has instance method +flatten+).
  #
  # With non-negative integer argument +depth+, flattens recursively through +depth+ levels:
  #
  #   a = [ 0, [ 1, [2, 3], 4 ], 5, {foo: 0}, Set.new([6, 7]) ]
  #   a              # => [0, [1, [2, 3], 4], 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.flatten(0)   # => [0, [1, [2, 3], 4], 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.flatten(1  ) # => [0, 1, [2, 3], 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.flatten(1.1) # => [0, 1, [2, 3], 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.flatten(2)   # => [0, 1, 2, 3, 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.flatten(3)   # => [0, 1, 2, 3, 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #
  # With +nil+ or negative +depth+, flattens all levels.
  #
  #   a.flatten     # => [0, 1, 2, 3, 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.flatten(-1) # => [0, 1, 2, 3, 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #
  # Related: Array#flatten!;
  # see also {Methods for Converting}[rdoc-ref:Array@Methods+for+Converting].
  def flatten(depth = nil) end

  # Returns +self+ as a recursively flattening of +self+ to +depth+ levels of recursion;
  # +depth+ must be an
  # {integer-convertible object}[rdoc-ref:implicit_conversion.rdoc@Integer-Convertible+Objects],
  # or +nil+.
  # At each level of recursion:
  #
  # - Each element that is an array is "flattened"
  #   (that is, replaced by its individual array elements).
  # - Each element that is not an array is unchanged
  #   (even if the element is an object that has instance method +flatten+).
  #
  # Returns +nil+ if no elements were flattened.
  #
  # With non-negative integer argument +depth+, flattens recursively through +depth+ levels:
  #
  #   a = [ 0, [ 1, [2, 3], 4 ], 5, {foo: 0}, Set.new([6, 7]) ]
  #   a                   # => [0, [1, [2, 3], 4], 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.dup.flatten!(1)   # => [0, 1, [2, 3], 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.dup.flatten!(1.1) # => [0, 1, [2, 3], 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.dup.flatten!(2)   # => [0, 1, 2, 3, 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.dup.flatten!(3)   # => [0, 1, 2, 3, 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #
  # With +nil+ or negative argument +depth+, flattens all levels:
  #
  #   a.dup.flatten!     # => [0, 1, 2, 3, 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #   a.dup.flatten!(-1) # => [0, 1, 2, 3, 4, 5, {:foo=>0}, #<Set: {6, 7}>]
  #
  # Related: Array#flatten;
  # see also {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def flatten!(depth = nil) end

  # Freezes +self+ (if not already frozen); returns +self+:
  #
  #   a = []
  #   a.frozen? # => false
  #   a.freeze
  #   a.frozen? # => true
  #
  # No further changes may be made to +self+;
  # raises FrozenError if a change is attempted.
  #
  # Related: Kernel#frozen?.
  def freeze; end

  # Returns the integer hash value for +self+.
  #
  # Two arrays with the same content will have the same hash value
  # (and will compare using eql?):
  #
  #   ['a', 'b'].hash == ['a', 'b'].hash # => true
  #   ['a', 'b'].hash == ['a', 'c'].hash # => false
  #   ['a', 'b'].hash == ['a'].hash      # => false
  def hash; end

  # Returns whether for some element +element+ in +self+,
  # <tt>object == element</tt>:
  #
  #   [0, 1, 2].include?(2)   # => true
  #   [0, 1, 2].include?(2.0) # => true
  #   [0, 1, 2].include?(2.1) # => false
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def include?(object) end

  # Replaces the elements of +self+ with the elements of +other_array+, which must be an
  # {array-convertible object}[rdoc-ref:implicit_conversion.rdoc@Array-Convertible+Objects];
  # returns +self+:
  #
  #   a = ['a', 'b', 'c']   # => ["a", "b", "c"]
  #   a.replace(['d', 'e']) # => ["d", "e"]
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def initialize_copy(other_array) end
  alias replace initialize_copy

  # Inserts the given +objects+ as elements of +self+;
  # returns +self+.
  #
  # When +index+ is non-negative, inserts +objects+
  # _before_ the element at offset +index+:
  #
  #   a = ['a', 'b', 'c']     # => ["a", "b", "c"]
  #   a.insert(1, :x, :y, :z) # => ["a", :x, :y, :z, "b", "c"]
  #
  # Extends the array if +index+ is beyond the array (<tt>index >= self.size</tt>):
  #
  #   a = ['a', 'b', 'c']     # => ["a", "b", "c"]
  #   a.insert(5, :x, :y, :z) # => ["a", "b", "c", nil, nil, :x, :y, :z]
  #
  # When +index+ is negative, inserts +objects+
  # _after_ the element at offset <tt>index + self.size</tt>:
  #
  #   a = ['a', 'b', 'c']      # => ["a", "b", "c"]
  #   a.insert(-2, :x, :y, :z) # => ["a", "b", :x, :y, :z, "c"]
  #
  # With no +objects+ given, does nothing:
  #
  #   a = ['a', 'b', 'c'] # => ["a", "b", "c"]
  #   a.insert(1)         # => ["a", "b", "c"]
  #   a.insert(50)        # => ["a", "b", "c"]
  #   a.insert(-50)       # => ["a", "b", "c"]
  #
  # Raises IndexError if +objects+ are given and +index+ is negative and out of range.
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def insert(index, *objects) end

  # Returns the new string formed by calling method <tt>#inspect</tt>
  # on each array element:
  #
  #   a = [:foo, 'bar', 2]
  #   a.inspect # => "[:foo, \"bar\", 2]"
  #
  # Related: see {Methods for Converting}[rdoc-ref:Array@Methods+for+Converting].
  def inspect; end
  alias to_s inspect

  # Returns whether +other_array+ has at least one element that is +#eql?+ to some element of +self+:
  #
  #   [1, 2, 3].intersect?([3, 4, 5]) # => true
  #   [1, 2, 3].intersect?([4, 5, 6]) # => false
  #
  # Each element must correctly implement method <tt>#hash</tt>.
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def intersect?(other_array) end

  # Returns a new array containing each element in +self+ that is +#eql?+
  # to at least one element in each of the given +other_arrays+;
  # duplicates are omitted:
  #
  #   [0, 0, 1, 1, 2, 3].intersection([0, 1, 2], [0, 1, 3]) # => [0, 1]
  #
  # Each element must correctly implement method <tt>#hash</tt>.
  #
  # Order from +self+ is preserved:
  #
  #   [0, 1, 2].intersection([2, 1, 0]) # => [0, 1, 2]
  #
  # Returns a copy of +self+ if no arguments are given.
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def intersection(*other_arrays) end

  # Returns the new string formed by joining the converted elements of +self+;
  # for each element +element+:
  #
  # - Converts recursively using <tt>element.join(separator)</tt>
  #   if +element+ is a <tt>kind_of?(Array)</tt>.
  # - Otherwise, converts using <tt>element.to_s</tt>.
  #
  # With no argument given, joins using the output field separator, <tt>$,</tt>:
  #
  #   a = [:foo, 'bar', 2]
  #   $, # => nil
  #   a.join # => "foobar2"
  #
  # With string argument +separator+ given, joins using that separator:
  #
  #   a = [:foo, 'bar', 2]
  #   a.join("\n") # => "foo\nbar\n2"
  #
  # Joins recursively for nested arrays:
  #
  #  a = [:foo, [:bar, [:baz, :bat]]]
  #  a.join # => "foobarbazbat"
  #
  # Related: see {Methods for Converting}[rdoc-ref:Array@Methods+for+Converting].
  def join(separator = $,) end

  # With a block given, calls the block with each element of +self+;
  # removes the element from +self+ if the block does not return a truthy value:
  #
  #   a = [:foo, 'bar', 2, :bam]
  #   a.keep_if {|element| element.to_s.start_with?('b') } # => ["bar", :bam]
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def keep_if; end

  # Returns elements from +self+, or +nil+; +self+ is not modified.
  #
  # With no argument given, returns the last element, or +nil+ if +self+ is empty:
  #
  #   a = [:foo, 'bar', 2]
  #   a.last # => 2
  #   a # => [:foo, "bar", 2]
  #   [].last # => nil
  #
  # With non-negative integer argument +count+ given,
  # returns a new array containing the trailing +count+ elements of +self+, as available:
  #
  #   a = [:foo, 'bar', 2]
  #   a.last(2)  # => ["bar", 2]
  #   a.last(50) # => [:foo, "bar", 2]
  #   a.last(0)  # => []
  #   [].last(3) # => []
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def last(...) end

  # Returns the count of elements in +self+:
  #
  #   [0, 1, 2].length # => 3
  #   [].length        # => 0
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def length; end
  alias size length

  # Returns one of the following:
  #
  # - The maximum-valued element from +self+.
  # - A new array of maximum-valued elements from +self+.
  #
  # Does not modify +self+.
  #
  # With no block given, each element in +self+ must respond to method <tt>#<=></tt>
  # with a numeric.
  #
  # With no argument and no block, returns the element in +self+
  # having the maximum value per method <tt>#<=></tt>:
  #
  #   [1, 0, 3, 2].max # => 3
  #
  # With non-negative numeric argument +count+ and no block,
  # returns a new array with at most +count+ elements,
  # in descending order, per method <tt>#<=></tt>:
  #
  #   [1, 0, 3, 2].max(3)   # => [3, 2, 1]
  #   [1, 0, 3, 2].max(3.0) # => [3, 2, 1]
  #   [1, 0, 3, 2].max(9)   # => [3, 2, 1, 0]
  #   [1, 0, 3, 2].max(0)   # => []
  #
  # With a block given, the block must return a numeric.
  #
  # With a block and no argument, calls the block <tt>self.size - 1</tt> times to compare elements;
  # returns the element having the maximum value per the block:
  #
  #   ['0', '', '000', '00'].max {|a, b| a.size <=> b.size }
  #   # => "000"
  #
  # With non-negative numeric argument +count+ and a block,
  # returns a new array with at most +count+ elements,
  # in descending order, per the block:
  #
  #   ['0', '', '000', '00'].max(2) {|a, b| a.size <=> b.size }
  #   # => ["000", "00"]
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def max(...) end

  # Returns one of the following:
  #
  # - The minimum-valued element from +self+.
  # - A new array of minimum-valued elements from +self+.
  #
  # Does not modify +self+.
  #
  # With no block given, each element in +self+ must respond to method <tt>#<=></tt>
  # with a numeric.
  #
  # With no argument and no block, returns the element in +self+
  # having the minimum value per method <tt>#<=></tt>:
  #
  #   [1, 0, 3, 2].min # => 0
  #
  # With non-negative numeric argument +count+ and no block,
  # returns a new array with at most +count+ elements,
  # in ascending order, per method <tt>#<=></tt>:
  #
  #   [1, 0, 3, 2].min(3)   # => [0, 1, 2]
  #   [1, 0, 3, 2].min(3.0) # => [0, 1, 2]
  #   [1, 0, 3, 2].min(9)   # => [0, 1, 2, 3]
  #   [1, 0, 3, 2].min(0)   # => []
  #
  # With a block given, the block must return a numeric.
  #
  # With a block and no argument, calls the block <tt>self.size - 1</tt> times to compare elements;
  # returns the element having the minimum value per the block:
  #
  #   ['0', '', '000', '00'].min {|a, b| a.size <=> b.size }
  #   # => ""
  #
  # With non-negative numeric argument +count+ and a block,
  # returns a new array with at most +count+ elements,
  # in ascending order, per the block:
  #
  #   ['0', '', '000', '00'].min(2) {|a, b| a.size <=> b.size }
  #   # => ["", "0"]
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def min(...) end

  # Returns a 2-element array containing the minimum-valued and maximum-valued
  # elements from +self+;
  # does not modify +self+.
  #
  # With no block given, the minimum and maximum values are determined using method <tt>#<=></tt>:
  #
  #   [1, 0, 3, 2].minmax # => [0, 3]
  #
  # With a block given, the block must return a numeric;
  # the block is called <tt>self.size - 1</tt> times to compare elements;
  # returns the elements having the minimum and maximum values per the block:
  #
  #   ['0', '', '000', '00'].minmax {|a, b| a.size <=> b.size }
  #   # => ["", "000"]
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def minmax; end

  # Returns +true+ if no element of +self+ meets a given criterion, +false+ otherwise.
  #
  # With no block given and no argument, returns +true+ if +self+ has no truthy elements,
  # +false+ otherwise:
  #
  #   [nil, false].none?    # => true
  #   [nil, 0, false].none? # => false
  #   [].none?              # => true
  #
  # With argument +object+ given, returns +false+ if for any element +element+,
  # <tt>object === element</tt>; +true+ otherwise:
  #
  #   ['food', 'drink'].none?(/bar/) # => true
  #   ['food', 'drink'].none?(/foo/) # => false
  #   [].none?(/foo/)                # => true
  #   [0, 1, 2].none?(3)             # => true
  #   [0, 1, 2].none?(1)             # => false
  #
  # With a block given, calls the block with each element in +self+;
  # returns +true+ if the block returns no truthy value, +false+ otherwise:
  #
  #   [0, 1, 2].none? {|element| element > 3 } # => true
  #   [0, 1, 2].none? {|element| element > 1 } # => false
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def none?(...) end

  # Returns +true+ if exactly one element of +self+ meets a given criterion.
  #
  # With no block given and no argument, returns +true+ if +self+ has exactly one truthy element,
  # +false+ otherwise:
  #
  #   [nil, 0].one? # => true
  #   [0, 0].one? # => false
  #   [nil, nil].one? # => false
  #   [].one? # => false
  #
  # With a block given, calls the block with each element in +self+;
  # returns +true+ if the block a truthy value for exactly one element, +false+ otherwise:
  #
  #   [0, 1, 2].one? {|element| element > 0 } # => false
  #   [0, 1, 2].one? {|element| element > 1 } # => true
  #   [0, 1, 2].one? {|element| element > 2 } # => false
  #
  # With argument +object+ given, returns +true+ if for exactly one element +element+, <tt>object === element</tt>;
  # +false+ otherwise:
  #
  #   [0, 1, 2].one?(0) # => true
  #   [0, 0, 1].one?(0) # => false
  #   [1, 1, 2].one?(0) # => false
  #   ['food', 'drink'].one?(/bar/) # => false
  #   ['food', 'drink'].one?(/foo/) # => true
  #   [].one?(/foo/) # => false
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def one?(...) end

  # Formats each element in +self+ into a binary string; returns that string.
  # See {Packed Data}[rdoc-ref:packed_data.rdoc].
  def pack(template, buffer: nil) end

  # Iterates over permutations of the elements of +self+;
  # the order of permutations is indeterminate.
  #
  # With a block and an in-range positive integer argument +count+ (<tt>0 < count <= self.size</tt>) given,
  # calls the block with each permutation of +self+ of size +count+;
  # returns +self+:
  #
  #   a = [0, 1, 2]
  #   perms = []
  #   a.permutation(1) {|perm| perms.push(perm) }
  #   perms # => [[0], [1], [2]]
  #
  #   perms = []
  #   a.permutation(2) {|perm| perms.push(perm) }
  #   perms # => [[0, 1], [0, 2], [1, 0], [1, 2], [2, 0], [2, 1]]
  #
  #   perms = []
  #   a.permutation(3) {|perm| perms.push(perm) }
  #   perms # => [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]]
  #
  # When +count+ is zero, calls the block once with a new empty array:
  #
  #   perms = []
  #   a.permutation(0) {|perm| perms.push(perm) }
  #   perms # => [[]]
  #
  # When +count+ is out of range (negative or larger than <tt>self.size</tt>),
  # does not call the block:
  #
  #   a.permutation(-1) {|permutation| fail 'Cannot happen' }
  #   a.permutation(4) {|permutation| fail 'Cannot happen' }
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: {Methods for Iterating}[rdoc-ref:Array@Methods+for+Iterating].
  def permutation(count = size) end

  # Removes and returns trailing elements of +self+.
  #
  # With no argument given, removes and returns the last element, if available;
  # otherwise returns +nil+:
  #
  #   a = [:foo, 'bar', 2]
  #   a.pop  # => 2
  #   a      # => [:foo, "bar"]
  #   [].pop # => nil
  #
  # With non-negative integer argument +count+ given,
  # returns a new array containing the trailing +count+ elements of +self+, as available:
  #
  #   a = [:foo, 'bar', 2]
  #   a.pop(2) # => ["bar", 2]
  #   a        # => [:foo]
  #
  #   a = [:foo, 'bar', 2]
  #   a.pop(50) # => [:foo, "bar", 2]
  #   a         # => []
  #
  # Related: Array#push;
  # see also {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def pop(...) end

  # Computes all combinations of elements from all the arrays,
  # including both +self+ and +other_arrays+:
  #
  # - The number of combinations is the product of the sizes of all the arrays,
  #   including both +self+ and +other_arrays+.
  # - The order of the returned combinations is indeterminate.
  #
  # With no block given, returns the combinations as an array of arrays:
  #
  #   p = [0, 1].product([2, 3])
  #   # => [[0, 2], [0, 3], [1, 2], [1, 3]]
  #   p.size # => 4
  #   p = [0, 1].product([2, 3], [4, 5])
  #   # => [[0, 2, 4], [0, 2, 5], [0, 3, 4], [0, 3, 5], [1, 2, 4], [1, 2, 5], [1, 3, 4], [1, 3,...
  #   p.size # => 8
  #
  # If +self+ or any argument is empty, returns an empty array:
  #
  #   [].product([2, 3], [4, 5]) # => []
  #   [0, 1].product([2, 3], []) # => []
  #
  # If no argument is given, returns an array of 1-element arrays,
  # each containing an element of +self+:
  #
  #   a.product # => [[0], [1], [2]]
  #
  # With a block given, calls the block with each combination; returns +self+:
  #
  #   p = []
  #   [0, 1].product([2, 3]) {|combination| p.push(combination) }
  #   p # => [[0, 2], [0, 3], [1, 2], [1, 3]]
  #
  # If +self+ or any argument is empty, does not call the block:
  #
  #   [].product([2, 3], [4, 5]) {|combination| fail 'Cannot happen' }
  #   # => []
  #   [0, 1].product([2, 3], []) {|combination| fail 'Cannot happen' }
  #   # => [0, 1]
  #
  # If no argument is given, calls the block with each element of +self+ as a 1-element array:
  #
  #   p = []
  #   [0, 1].product {|combination| p.push(combination) }
  #   p # => [[0], [1]]
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def product(*other_arrays) end

  # Appends each argument in +objects+ to +self+; returns +self+:
  #
  #   a = [:foo, 'bar', 2] # => [:foo, "bar", 2]
  #   a.push(:baz, :bat)   # => [:foo, "bar", 2, :baz, :bat]
  #
  # Appends each argument as a single element, even if it is another array:
  #
  #   a = [:foo, 'bar', 2]               # => [:foo, "bar", 2]
  #   a.push([:baz, :bat], [:bam, :bad]) # => [:foo, "bar", 2, [:baz, :bat], [:bam, :bad]]
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def push(*objects) end
  alias append push

  # Returns the first element +ele+ in +self+ such that +ele+ is an array
  # and <tt>ele[1] == object</tt>:
  #
  #   a = [{foo: 0}, [2, 4], [4, 5, 6], [4, 5]]
  #   a.rassoc(4) # => [2, 4]
  #   a.rassoc(5) # => [4, 5, 6]
  #
  # Returns +nil+ if no such element is found.
  #
  # Related: Array#assoc;
  # see also {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def rassoc(object) end

  # With a block given, returns a new array whose elements are all those from +self+
  # for which the block returns +false+ or +nil+:
  #
  #   a = [:foo, 'bar', 2, 'bat']
  #   a1 = a.reject {|element| element.to_s.start_with?('b') }
  #   a1 # => [:foo, 2]
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def reject; end

  # With a block given, calls the block with each element of +self+;
  # removes each element for which the block returns a truthy value.
  #
  # Returns +self+ if any elements removed:
  #
  #   a = [:foo, 'bar', 2, 'bat']
  #   a.reject! {|element| element.to_s.start_with?('b') } # => [:foo, 2]
  #
  # Returns +nil+ if no elements removed.
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def reject!; end

  # With a block given, calls the block with each repeated combination of length +size+
  # of the elements of +self+;
  # each combination is an array;
  # returns +self+. The order of the combinations is indeterminate.
  #
  # If a positive integer argument +size+ is given,
  # calls the block with each +size+-tuple repeated combination of the elements of +self+.
  # The number of combinations is <tt>(size+1)(size+2)/2</tt>.
  #
  # Examples:
  #
  # - +size+ is 1:
  #
  #     c = []
  #     [0, 1, 2].repeated_combination(1) {|combination| c.push(combination) }
  #     c # => [[0], [1], [2]]
  #
  # - +size+ is 2:
  #
  #     c = []
  #     [0, 1, 2].repeated_combination(2) {|combination| c.push(combination) }
  #     c # => [[0, 0], [0, 1], [0, 2], [1, 1], [1, 2], [2, 2]]
  #
  # If +size+ is zero, calls the block once with an empty array.
  #
  # If +size+ is negative, does not call the block:
  #
  #   [0, 1, 2].repeated_combination(-1) {|combination| fail 'Cannot happen' }
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def repeated_combination(size) end

  # With a block given, calls the block with each repeated permutation of length +size+
  # of the elements of +self+;
  # each permutation is an array;
  # returns +self+. The order of the permutations is indeterminate.
  #
  # If a positive integer argument +size+ is given,
  # calls the block with each +size+-tuple repeated permutation of the elements of +self+.
  # The number of permutations is <tt>self.size**size</tt>.
  #
  # Examples:
  #
  # - +size+ is 1:
  #
  #     p = []
  #     [0, 1, 2].repeated_permutation(1) {|permutation| p.push(permutation) }
  #     p # => [[0], [1], [2]]
  #
  # - +size+ is 2:
  #
  #     p = []
  #     [0, 1, 2].repeated_permutation(2) {|permutation| p.push(permutation) }
  #     p # => [[0, 0], [0, 1], [0, 2], [1, 0], [1, 1], [1, 2], [2, 0], [2, 1], [2, 2]]
  #
  # If +size+ is zero, calls the block once with an empty array.
  #
  # If +size+ is negative, does not call the block:
  #
  #   [0, 1, 2].repeated_permutation(-1) {|permutation| fail 'Cannot happen' }
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def repeated_permutation(size) end

  # Returns a new array containing the elements of +self+ in reverse order:
  #
  #   [0, 1, 2].reverse # => [2, 1, 0]
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def reverse; end

  # Reverses the order of the elements of +self+;
  # returns +self+:
  #
  #   a = [0, 1, 2]
  #   a.reverse! # => [2, 1, 0]
  #   a          # => [2, 1, 0]
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def reverse!; end

  # When a block given, iterates backwards over the elements of +self+,
  # passing, in reverse order, each element to the block;
  # returns +self+:
  #
  #   a = []
  #   [0, 1, 2].reverse_each {|element| a.push(element) }
  #   a # => [2, 1, 0]
  #
  # Allows the array to be modified during iteration:
  #
  #   a = ['a', 'b', 'c']
  #   a.reverse_each {|element| a.clear if element.start_with?('b') }
  #   a # => []
  #
  # When no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Iterating}[rdoc-ref:Array@Methods+for+Iterating].
  def reverse_each; end

  # Returns the index of the last element for which <tt>object == element</tt>.
  #
  # With argument +object+ given, returns the index of the last such element found:
  #
  #   a = [:foo, 'bar', 2, 'bar']
  #   a.rindex('bar') # => 3
  #
  # Returns +nil+ if no such object found.
  #
  # With a block given, calls the block with each successive element;
  # returns the index of the last element for which the block returns a truthy value:
  #
  #   a = [:foo, 'bar', 2, 'bar']
  #   a.rindex {|element| element == 'bar' } # => 3
  #
  # Returns +nil+ if the block never returns a truthy value.
  #
  # When neither an argument nor a block is given, returns a new Enumerator.
  #
  # Related: see {Methods for Querying}[rdoc-ref:Array@Methods+for+Querying].
  def rindex(...) end

  # Returns a new array formed from +self+ with elements
  # rotated from one end to the other.
  #
  # With non-negative numeric +count+,
  # rotates elements from the beginning to the end:
  #
  #   [0, 1, 2, 3].rotate(2)   # => [2, 3, 0, 1]
  #   [0, 1, 2, 3].rotate(2.1) # => [2, 3, 0, 1]
  #
  # If +count+ is large, uses <tt>count % array.size</tt> as the count:
  #
  #   [0, 1, 2, 3].rotate(22) # => [2, 3, 0, 1]
  #
  # With a +count+ of zero, rotates no elements:
  #
  #   [0, 1, 2, 3].rotate(0) # => [0, 1, 2, 3]
  #
  # With negative numeric +count+, rotates in the opposite direction,
  # from the end to the beginning:
  #
  #   [0, 1, 2, 3].rotate(-1) # => [3, 0, 1, 2]
  #
  # If +count+ is small (far from zero), uses <tt>count % array.size</tt> as the count:
  #
  #   [0, 1, 2, 3].rotate(-21) # => [3, 0, 1, 2]
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def rotate(count = 1) end

  # Rotates +self+ in place by moving elements from one end to the other; returns +self+.
  #
  # With non-negative numeric +count+,
  # rotates +count+ elements from the beginning to the end:
  #
  #   [0, 1, 2, 3].rotate!(2)   # => [2, 3, 0, 1]
  #   [0, 1, 2, 3].rotate!(2.1) # => [2, 3, 0, 1]
  #
  # If +count+ is large, uses <tt>count % array.size</tt> as the count:
  #
  #   [0, 1, 2, 3].rotate!(21) # => [1, 2, 3, 0]
  #
  # If +count+ is zero, rotates no elements:
  #
  #   [0, 1, 2, 3].rotate!(0) # => [0, 1, 2, 3]
  #
  # With a negative numeric +count+, rotates in the opposite direction,
  # from end to beginning:
  #
  #   [0, 1, 2, 3].rotate!(-1) # => [3, 0, 1, 2]
  #
  # If +count+ is small (far from zero), uses <tt>count % array.size</tt> as the count:
  #
  #   [0, 1, 2, 3].rotate!(-21) # => [3, 0, 1, 2]
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def rotate!(count = 1) end

  # Returns random elements from +self+,
  # as selected by the object given by the keyword argument +random+.
  #
  # With no argument +count+ given, returns one random element from +self+:
  #
  #   a = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
  #   a.sample # => 3
  #   a.sample # => 8
  #
  # Returns +nil+ if +self+ is empty:
  #
  #   [].sample # => nil
  #
  # With a non-negative numeric argument +count+ given,
  # returns a new array containing +count+ random elements from +self+:
  #
  #   a.sample(3) # => [8, 9, 2]
  #   a.sample(6) # => [9, 6, 0, 3, 1, 4]
  #
  # The order of the result array is unrelated to the order of +self+.
  #
  # Returns a new empty +Array+ if +self+ is empty:
  #
  #   [].sample(4) # => []
  #
  # May return duplicates in +self+:
  #
  #   a = [1, 1, 1, 2, 2, 3]
  #   a.sample(a.size) # => [1, 1, 3, 2, 1, 2]
  #
  # Returns no more than <tt>a.size</tt> elements
  # (because no new duplicates are introduced):
  #
  #   a.sample(50) # => [6, 4, 1, 8, 5, 9, 0, 2, 3, 7]
  #
  # The object given with the keyword argument +random+ is used as the random number generator:
  #
  #   a = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  #   a.sample(random: Random.new(1))     # => 6
  #   a.sample(4, random: Random.new(1))  # => [6, 10, 9, 2]
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def sample(...) end

  # With a block given, calls the block with each element of +self+;
  # returns a new array containing those elements of +self+
  # for which the block returns a truthy value:
  #
  #   a = [:foo, 'bar', 2, :bam]
  #   a.select {|element| element.to_s.start_with?('b') }
  #   # => ["bar", :bam]
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def select; end
  alias filter select

  # With a block given, calls the block with each element of +self+;
  # removes from +self+ those elements for which the block returns +false+ or +nil+.
  #
  # Returns +self+ if any elements were removed:
  #
  #   a = [:foo, 'bar', 2, :bam]
  #   a.select! {|element| element.to_s.start_with?('b') } # => ["bar", :bam]
  #
  # Returns +nil+ if no elements were removed.
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def select!; end
  alias filter! select!

  # Removes and returns leading elements from +self+.
  #
  # With no argument, removes and returns one element, if available,
  # or +nil+ otherwise:
  #
  #   a = [0, 1, 2, 3]
  #   a.shift  # => 0
  #   a        # => [1, 2, 3]
  #   [].shift # => nil
  #
  # With non-negative numeric argument +count+ given,
  # removes and returns the first +count+ elements:
  #
  #   a = [0, 1, 2, 3]
  #   a.shift(2)   # => [0, 1]
  #   a            # => [2, 3]
  #   a.shift(1.1) # => [2]
  #   a            # => [3]
  #   a.shift(0)   # => []
  #   a            # => [3]
  #
  # If +count+ is large,
  # removes and returns all elements:
  #
  #   a = [0, 1, 2, 3]
  #   a.shift(50) # => [0, 1, 2, 3]
  #   a           # => []
  #
  # If +self+ is empty, returns a new empty array.
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def shift(...) end

  # Returns a new array containing all elements from +self+ in a random order,
  # as selected by the object given by the keyword argument +random+:
  #
  #   a =            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
  #   a.shuffle # => [0, 8, 1, 9, 6, 3, 4, 7, 2, 5]
  #   a.shuffle # => [8, 9, 0, 5, 1, 2, 6, 4, 7, 3]
  #
  # Duplicate elements are included:
  #
  #   a =            [0, 1, 0, 1, 0, 1, 0, 1, 0, 1]
  #   a.shuffle # => [1, 0, 1, 1, 0, 0, 1, 0, 0, 1]
  #   a.shuffle # => [1, 1, 0, 0, 0, 1, 1, 0, 0, 1]
  #
  # The object given with the keyword argument +random+ is used as the random number generator.
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def shuffle(random: Random) end

  # Shuffles all elements in +self+ into a random order,
  # as selected by the object given by the keyword argument +random+.
  # Returns +self+:
  #
  #   a =             [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
  #   a.shuffle! # => [5, 3, 8, 7, 6, 1, 9, 4, 2, 0]
  #   a.shuffle! # => [9, 4, 0, 6, 2, 8, 1, 5, 3, 7]
  #
  # Duplicate elements are included:
  #
  #   a =             [0, 1, 0, 1, 0, 1, 0, 1, 0, 1]
  #   a.shuffle! # => [1, 0, 0, 1, 1, 0, 1, 0, 0, 1]
  #   a.shuffle! # => [0, 1, 0, 1, 1, 0, 1, 0, 1, 0]
  #
  # The object given with the keyword argument +random+ is used as the random number generator.
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def shuffle!(random: Random) end

  # Removes and returns elements from +self+.
  #
  # With numeric argument +index+ given,
  # removes and returns the element at offset +index+:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(2)   # => "c"
  #   a             # => ["a", "b", "d"]
  #   a.slice!(2.1) # => "d"
  #   a             # => ["a", "b"]
  #
  # If +index+ is negative, counts backwards from the end of +self+:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(-2) # => "c"
  #   a            # => ["a", "b", "d"]
  #
  # If +index+ is out of range, returns +nil+.
  #
  # With numeric arguments +start+ and +length+ given,
  # removes +length+ elements from +self+ beginning at zero-based offset +start+;
  # returns the removed objects in a new array:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(1, 2)     # => ["b", "c"]
  #   a                  # => ["a", "d"]
  #   a.slice!(0.1, 1.1) # => ["a"]
  #   a                  # => ["d"]
  #
  # If +start+ is negative, counts backwards from the end of +self+:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(-2, 1) # => ["c"]
  #   a               # => ["a", "b", "d"]
  #
  # If +start+ is out-of-range, returns +nil+:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(5, 1)  # => nil
  #   a.slice!(-5, 1) # => nil
  #
  # If <tt>start + length</tt> exceeds the array size,
  # removes and returns all elements from offset +start+ to the end:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(2, 50) # => ["c", "d"]
  #   a               # => ["a", "b"]
  #
  # If <tt>start == a.size</tt> and +length+ is non-negative,
  # returns a new empty array.
  #
  # If +length+ is negative, returns +nil+.
  #
  # With Range argument +range+ given,
  # treats <tt>range.min</tt> as +start+ (as above)
  # and <tt>range.size</tt> as +length+ (as above):
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(1..2) # => ["b", "c"]
  #   a              # => ["a", "d"]
  #
  # If <tt>range.start == a.size</tt>, returns a new empty array:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(4..5) # => []
  #
  # If <tt>range.start</tt> is larger than the array size, returns +nil+:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(5..6) # => nil
  #
  # If <tt>range.start</tt> is negative,
  # calculates the start index by counting backwards from the end of +self+:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(-2..2) # => ["c"]
  #
  # If <tt>range.end</tt> is negative,
  # calculates the end index by counting backwards from the end of +self+:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.slice!(0..-2) # => ["a", "b", "c"]
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def slice!(...) end

  # Returns a new array containing the elements of +self+, sorted.
  #
  # With no block given, compares elements using operator <tt>#<=></tt>
  # (see Object#<=>):
  #
  #   [0, 2, 3, 1].sort # => [0, 1, 2, 3]
  #
  # With a block given, calls the block with each combination of pairs of elements from +self+;
  # for each pair +a+ and +b+, the block should return a numeric:
  #
  # - Negative when +b+ is to follow +a+.
  # - Zero when +a+ and +b+ are equivalent.
  # - Positive when +a+ is to follow +b+.
  #
  # Example:
  #
  #   a = [3, 2, 0, 1]
  #   a.sort {|a, b| a <=> b } # => [0, 1, 2, 3]
  #   a.sort {|a, b| b <=> a } # => [3, 2, 1, 0]
  #
  # When the block returns zero, the order for +a+ and +b+ is indeterminate,
  # and may be unstable.
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def sort; end

  # Like Array#sort, but returns +self+ with its elements sorted in place.
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def sort!; end

  # With a block given, sorts the elements of +self+ in place;
  # returns self.
  #
  # Calls the block with each successive element;
  # sorts elements based on the values returned from the block:
  #
  #   a = ['aaaa', 'bbb', 'cc', 'd']
  #   a.sort_by! {|element| element.size }
  #   a # => ["d", "cc", "bbb", "aaaa"]
  #
  # For duplicate values returned by the block, the ordering is indeterminate, and may be unstable.
  #
  # With no block given, returns a new Enumerator.
  #
  # Related: see {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def sort_by!; end

  #  With no block given, returns the sum of +init+ and all elements of +self+;
  #  for array +array+ and value +init+, equivalent to:
  #
  #    sum = init
  #    array.each {|element| sum += element }
  #    sum
  #
  #  For example, <tt>[e0, e1, e2].sum</tt> returns <tt>init + e0 + e1 + e2</tt>.
  #
  #  Examples:
  #
  #    [0, 1, 2, 3].sum                 # => 6
  #    [0, 1, 2, 3].sum(100)            # => 106
  #    ['abc', 'def', 'ghi'].sum('jkl') # => "jklabcdefghi"
  #    [[:foo, :bar], ['foo', 'bar']].sum([2, 3])
  #    # => [2, 3, :foo, :bar, "foo", "bar"]
  #
  #  The +init+ value and elements need not be numeric, but must all be <tt>+</tt>-compatible:
  #
  #    # Raises TypeError: Array can't be coerced into Integer.
  #    [[:foo, :bar], ['foo', 'bar']].sum(2)
  #
  #  With a block given, calls the block with each element of +self+;
  #  the block's return value (instead of the element itself) is used as the addend:
  #
  #    ['zero', 1, :two].sum('Coerced and concatenated: ') {|element| element.to_s }
  #    # => "Coerced and concatenated: zero1two"
  #
  #  Notes:
  #
  #  - Array#join and Array#flatten may be faster than Array#sum
  #    for an array of strings or an array of arrays.
  #  - Array#sum method may not respect method redefinition of "+" methods such as Integer#+.
  def sum(init = 0) end

  # Returns a new array containing the first +count+ element of +self+
  # (as available);
  # +count+ must be a non-negative numeric;
  # does not modify +self+:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.take(2)   # => ["a", "b"]
  #   a.take(2.1) # => ["a", "b"]
  #   a.take(50)  # => ["a", "b", "c", "d"]
  #   a.take(0)   # => []
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def take(count) end

  # With a block given, calls the block with each successive element of +self+;
  # stops iterating if the block returns +false+ or +nil+;
  # returns a new array containing those elements for which the block returned a truthy value:
  #
  #   a = [0, 1, 2, 3, 4, 5]
  #   a.take_while {|element| element < 3 } # => [0, 1, 2]
  #   a.take_while {|element| true }        # => [0, 1, 2, 3, 4, 5]
  #   a.take_while {|element| false }       # => []
  #
  # With no block given, returns a new Enumerator.
  #
  # Does not modify +self+.
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def take_while; end

  # When +self+ is an instance of +Array+, returns +self+.
  #
  # Otherwise, returns a new array containing the elements of +self+:
  #
  #   class MyArray < Array; end
  #   my_a = MyArray.new(['foo', 'bar', 'two'])
  #   a = my_a.to_a
  #   a # => ["foo", "bar", "two"]
  #   a.class # => Array # Not MyArray.
  #
  # Related: see {Methods for Converting}[rdoc-ref:Array@Methods+for+Converting].
  def to_a; end

  # Returns +self+.
  def to_ary; end

  # Returns a new hash formed from +self+.
  #
  # With no block given, each element of +self+ must be a 2-element sub-array;
  # forms each sub-array into a key-value pair in the new hash:
  #
  #   a = [['foo', 'zero'], ['bar', 'one'], ['baz', 'two']]
  #   a.to_h # => {"foo"=>"zero", "bar"=>"one", "baz"=>"two"}
  #   [].to_h # => {}
  #
  # With a block given, the block must return a 2-element array;
  # calls the block with each element of +self+;
  # forms each returned array into a key-value pair in the returned hash:
  #
  #   a = ['foo', :bar, 1, [2, 3], {baz: 4}]
  #   a.to_h {|element| [element, element.class] }
  #   # => {"foo"=>String, :bar=>Symbol, 1=>Integer, [2, 3]=>Array, {:baz=>4}=>Hash}
  #
  # Related: see {Methods for Converting}[rdoc-ref:Array@Methods+for+Converting].
  def to_h; end

  # Returns a new array that is +self+
  # as a {transposed matrix}[https://en.wikipedia.org/wiki/Transpose]:
  #
  #   a = [[:a0, :a1], [:b0, :b1], [:c0, :c1]]
  #   a.transpose # => [[:a0, :b0, :c0], [:a1, :b1, :c1]]
  #
  # The elements of +self+ must all be the same size.
  #
  # Related: see {Methods for Converting}[rdoc-ref:Array@Methods+for+Converting].
  def transpose; end

  # Returns a new array that is the union of the elements of +self+
  # and all given arrays +other_arrays+;
  # items are compared using <tt>eql?</tt>:
  #
  #   [0, 1, 2, 3].union([4, 5], [6, 7]) # => [0, 1, 2, 3, 4, 5, 6, 7]
  #
  # Removes duplicates (preserving the first found):
  #
  #   [0, 1, 1].union([2, 1], [3, 1]) # => [0, 1, 2, 3]
  #
  # Preserves order (preserving the position of the first found):
  #
  #   [3, 2, 1, 0].union([5, 3], [4, 2]) # => [3, 2, 1, 0, 5, 4]
  #
  # With no arguments given, returns a copy of +self+.
  #
  # Related: see {Methods for Combining}[rdoc-ref:Array@Methods+for+Combining].
  def union(*other_arrays) end

  # Returns a new array containing those elements from +self+ that are not duplicates,
  # the first occurrence always being retained.
  #
  # With no block given, identifies and omits duplicate elements using method <tt>eql?</tt>
  # to compare elements:
  #
  #   a = [0, 0, 1, 1, 2, 2]
  #   a.uniq # => [0, 1, 2]
  #
  # With a block given, calls the block for each element;
  # identifies and omits "duplicate" elements using method <tt>eql?</tt>
  # to compare <i>block return values</i>;
  # that is, an element is a duplicate if its block return value
  # is the same as that of a previous element:
  #
  #   a = ['a', 'aa', 'aaa', 'b', 'bb', 'bbb']
  #   a.uniq {|element| element.size } # => ["a", "aa", "aaa"]
  #
  # Related: {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def uniq; end

  # Removes duplicate elements from +self+, the first occurrence always being retained;
  # returns +self+ if any elements removed, +nil+ otherwise.
  #
  # With no block given, identifies and removes elements using method <tt>eql?</tt>
  # to compare elements:
  #
  #   a = [0, 0, 1, 1, 2, 2]
  #   a.uniq! # => [0, 1, 2]
  #   a.uniq! # => nil
  #
  # With a block given, calls the block for each element;
  # identifies and omits "duplicate" elements using method <tt>eql?</tt>
  # to compare <i>block return values</i>;
  # that is, an element is a duplicate if its block return value
  # is the same as that of a previous element:
  #
  #   a = ['a', 'aa', 'aaa', 'b', 'bb', 'bbb']
  #   a.uniq! {|element| element.size } # => ["a", "aa", "aaa"]
  #   a.uniq! {|element| element.size } # => nil
  #
  # Related: see {Methods for Deleting}[rdoc-ref:Array@Methods+for+Deleting].
  def uniq!; end

  # Prepends the given +objects+ to +self+:
  #
  #   a = [:foo, 'bar', 2]
  #   a.unshift(:bam, :bat) # => [:bam, :bat, :foo, "bar", 2]
  #
  # Related: Array#shift;
  # see also {Methods for Assigning}[rdoc-ref:Array@Methods+for+Assigning].
  def unshift(*objects) end
  alias prepend unshift

  # Returns elements from +self+ in a new array; does not modify +self+.
  #
  # The objects included in the returned array are the elements of +self+
  # selected by the given +specifiers+,
  # each of which must be a numeric index or a Range.
  #
  # In brief:
  #
  #   a = ['a', 'b', 'c', 'd']
  #
  #   # Index specifiers.
  #   a.values_at(2, 0, 2, 0)     # => ["c", "a", "c", "a"] # May repeat.
  #   a.values_at(-4, -3, -2, -1) # => ["a", "b", "c", "d"] # Counts backwards if negative.
  #   a.values_at(-50, 50)        # => [nil, nil]           # Outside of self.
  #
  #   # Range specifiers.
  #   a.values_at(1..3)       # => ["b", "c", "d"] # From range.begin to range.end.
  #   a.values_at(1...3)      # => ["b", "c"]      # End excluded.
  #   a.values_at(3..1)       # => []              # No such elements.
  #
  #   a.values_at(-3..3)  # => ["b", "c", "d"]     # Negative range.begin counts backwards.
  #   a.values_at(-50..3)                          # Raises RangeError.
  #
  #   a.values_at(1..-2)  # => ["b", "c"]          # Negative range.end counts backwards.
  #   a.values_at(1..-50) # => []                  # No such elements.
  #
  #   # Mixture of specifiers.
  #   a.values_at(2..3, 3, 0..1, 0) # => ["c", "d", "d", "a", "b", "a"]
  #
  # With no +specifiers+ given, returns a new empty array:
  #
  #   a = ['a', 'b', 'c', 'd']
  #   a.values_at # => []
  #
  # For each numeric specifier +index+, includes an element:
  #
  # - For each non-negative numeric specifier +index+ that is in-range (less than <tt>self.size</tt>),
  #   includes the element at offset +index+:
  #
  #     a.values_at(0, 2)     # => ["a", "c"]
  #     a.values_at(0.1, 2.9) # => ["a", "c"]
  #
  # - For each negative numeric +index+ that is in-range (greater than or equal to <tt>- self.size</tt>),
  #   counts backwards from the end of +self+:
  #
  #     a.values_at(-1, -4) # => ["d", "a"]
  #
  # The given indexes may be in any order, and may repeat:
  #
  #   a.values_at(2, 0, 1, 0, 2) # => ["c", "a", "b", "a", "c"]
  #
  # For each +index+ that is out-of-range, includes +nil+:
  #
  #   a.values_at(4, -5) # => [nil, nil]
  #
  # For each Range specifier +range+, includes elements
  # according to <tt>range.begin</tt> and <tt>range.end</tt>:
  #
  # - If both <tt>range.begin</tt> and <tt>range.end</tt>
  #   are non-negative and in-range (less than <tt>self.size</tt>),
  #   includes elements from index <tt>range.begin</tt>
  #   through <tt>range.end - 1</tt> (if <tt>range.exclude_end?</tt>),
  #   or through <tt>range.end</tt> (otherwise):
  #
  #     a.values_at(1..2)  # => ["b", "c"]
  #     a.values_at(1...2) # => ["b"]
  #
  # - If <tt>range.begin</tt> is negative and in-range (greater than or equal to <tt>- self.size</tt>),
  #   counts backwards from the end of +self+:
  #
  #     a.values_at(-2..3) # => ["c", "d"]
  #
  # - If <tt>range.begin</tt> is negative and out-of-range, raises an exception:
  #
  #     a.values_at(-5..3) # Raises RangeError.
  #
  # - If <tt>range.end</tt> is positive and out-of-range,
  #   extends the returned array with +nil+ elements:
  #
  #     a.values_at(1..5) # => ["b", "c", "d", nil, nil]
  #
  # - If <tt>range.end</tt> is negative and in-range,
  #   counts backwards from the end of +self+:
  #
  #     a.values_at(1..-2) # => ["b", "c"]
  #
  # - If <tt>range.end</tt> is negative and out-of-range,
  #   returns an empty array:
  #
  #     a.values_at(1..-5) # => []
  #
  # The given ranges may be in any order and may repeat:
  #
  #   a.values_at(2..3, 0..1, 2..3) # => ["c", "d", "a", "b", "c", "d"]
  #
  # The given specifiers may be any mixture of indexes and ranges:
  #
  #   a.values_at(3, 1..2, 0, 2..3) # => ["d", "b", "c", "a", "c", "d"]
  #
  # Related: see {Methods for Fetching}[rdoc-ref:Array@Methods+for+Fetching].
  def values_at(*specifiers) end

  # With no block given, combines +self+ with the collection of +other_arrays+;
  # returns a new array of sub-arrays:
  #
  #   [0, 1].zip(['zero', 'one'], [:zero, :one])
  #   # => [[0, "zero", :zero], [1, "one", :one]]
  #
  # Returned:
  #
  # - The outer array is of size <tt>self.size</tt>.
  # - Each sub-array is of size <tt>other_arrays.size + 1</tt>.
  # - The _nth_ sub-array contains (in order):
  #
  #   - The _nth_ element of +self+.
  #   - The _nth_ element of each of the other arrays, as available.
  #
  # Example:
  #
  #   a = [0, 1]
  #   zipped = a.zip(['zero', 'one'], [:zero, :one])
  #   # => [[0, "zero", :zero], [1, "one", :one]]
  #   zipped.size       # => 2 # Same size as a.
  #   zipped.first.size # => 3 # Size of other arrays plus 1.
  #
  # When the other arrays are all the same size as +self+,
  # the returned sub-arrays are a rearrangement containing exactly elements of all the arrays
  # (including +self+), with no omissions or additions:
  #
  #   a = [:a0, :a1, :a2, :a3]
  #   b = [:b0, :b1, :b2, :b3]
  #   c = [:c0, :c1, :c2, :c3]
  #   d = a.zip(b, c)
  #   pp d
  #   # =>
  #   [[:a0, :b0, :c0],
  #    [:a1, :b1, :c1],
  #    [:a2, :b2, :c2],
  #    [:a3, :b3, :c3]]
  #
  # When one of the other arrays is smaller than +self+,
  # pads the corresponding sub-array with +nil+ elements:
  #
  #   a = [:a0, :a1, :a2, :a3]
  #   b = [:b0, :b1, :b2]
  #   c = [:c0, :c1]
  #   d = a.zip(b, c)
  #   pp d
  #   # =>
  #   [[:a0, :b0, :c0],
  #    [:a1, :b1, :c1],
  #    [:a2, :b2, nil],
  #    [:a3, nil, nil]]
  #
  # When one of the other arrays is larger than +self+,
  # _ignores_ its trailing elements:
  #
  #   a = [:a0, :a1, :a2, :a3]
  #   b = [:b0, :b1, :b2, :b3, :b4]
  #   c = [:c0, :c1, :c2, :c3, :c4, :c5]
  #   d = a.zip(b, c)
  #   pp d
  #   # =>
  #   [[:a0, :b0, :c0],
  #    [:a1, :b1, :c1],
  #    [:a2, :b2, :c2],
  #    [:a3, :b3, :c3]]
  #
  # With a block given, calls the block with each of the other arrays;
  # returns +nil+:
  #
  #   d = []
  #   a = [:a0, :a1, :a2, :a3]
  #   b = [:b0, :b1, :b2, :b3]
  #   c = [:c0, :c1, :c2, :c3]
  #   a.zip(b, c) {|sub_array| d.push(sub_array.reverse) } # => nil
  #   pp d
  #   # =>
  #   [[:c0, :b0, :a0],
  #    [:c1, :b1, :a1],
  #    [:c2, :b2, :a2],
  #    [:c3, :b3, :a3]]
  #
  # For an *object* in *other_arrays* that is not actually an array,
  # forms the the "other array" as <tt>object.to_ary</tt>, if defined,
  # or as <tt>object.each.to_a</tt> otherwise.
  #
  # Related: see {Methods for Converting}[rdoc-ref:Array@Methods+for+Converting].
  def zip(*other_arrays) end
end
