# frozen_string_literal: true

# A class which allows both internal and external iteration.
#
# An Enumerator can be created by the following methods.
# - Object#to_enum
# - Object#enum_for
# - Enumerator.new
#
# Most methods have two forms: a block form where the contents
# are evaluated for each item in the enumeration, and a non-block form
# which returns a new Enumerator wrapping the iteration.
#
#   enumerator = %w(one two three).each
#   puts enumerator.class # => Enumerator
#
#   enumerator.each_with_object("foo") do |item, obj|
#     puts "#{obj}: #{item}"
#   end
#
#   # foo: one
#   # foo: two
#   # foo: three
#
#   enum_with_obj = enumerator.each_with_object("foo")
#   puts enum_with_obj.class # => Enumerator
#
#   enum_with_obj.each do |item, obj|
#     puts "#{obj}: #{item}"
#   end
#
#   # foo: one
#   # foo: two
#   # foo: three
#
# This allows you to chain Enumerators together.  For example, you
# can map a list's elements to strings containing the index
# and the element as a string via:
#
#   puts %w[foo bar baz].map.with_index { |w, i| "#{i}:#{w}" }
#   # => ["0:foo", "1:bar", "2:baz"]
#
# An Enumerator can also be used as an external iterator.
# For example, Enumerator#next returns the next value of the iterator
# or raises StopIteration if the Enumerator is at the end.
#
#   e = [1,2,3].each   # returns an enumerator object.
#   puts e.next   # => 1
#   puts e.next   # => 2
#   puts e.next   # => 3
#   puts e.next   # raises StopIteration
#
# You can use this to implement an internal iterator as follows:
#
#   def ext_each(e)
#     while true
#       begin
#         vs = e.next_values
#       rescue StopIteration
#         return $!.result
#       end
#       y = yield(*vs)
#       e.feed y
#     end
#   end
#
#   o = Object.new
#
#   def o.each
#     puts yield
#     puts yield(1)
#     puts yield(1, 2)
#     3
#   end
#
#   # use o.each as an internal iterator directly.
#   puts o.each {|*x| puts x; [:b, *x] }
#   # => [], [:b], [1], [:b, 1], [1, 2], [:b, 1, 2], 3
#
#   # convert o.each to an external iterator for
#   # implementing an internal iterator.
#   puts ext_each(o.to_enum) {|*x| puts x; [:b, *x] }
#   # => [], [:b], [1], [:b, 1], [1, 2], [:b, 1, 2], 3
class Enumerator
  include Enumerable

  # Creates an infinite enumerator from any block, just called over and
  # over.  The result of the previous iteration is passed to the next one.
  # If +initial+ is provided, it is passed to the first iteration, and
  # becomes the first element of the enumerator; if it is not provided,
  # the first iteration receives +nil+, and its result becomes the first
  # element of the iterator.
  #
  # Raising StopIteration from the block stops an iteration.
  #
  #   Enumerator.produce(1, &:succ)   # => enumerator of 1, 2, 3, 4, ....
  #
  #   Enumerator.produce { rand(10) } # => infinite random number sequence
  #
  #   ancestors = Enumerator.produce(node) { |prev| node = prev.parent or raise StopIteration }
  #   enclosing_section = ancestors.find { |n| n.type == :section }
  #
  # Using ::produce together with Enumerable methods like Enumerable#detect,
  # Enumerable#slice, Enumerable#take_while can provide Enumerator-based alternatives
  # for +while+ and +until+ cycles:
  #
  #   # Find next Tuesday
  #   require "date"
  #   Enumerator.produce(Date.today, &:succ).detect(&:tuesday?)
  #
  #   # Simple lexer:
  #   require "strscan"
  #   scanner = StringScanner.new("7+38/6")
  #   PATTERN = %r{\d+|[-/+*]}
  #   Enumerator.produce { scanner.scan(PATTERN) }.slice_after { scanner.eos? }.first
  #   # => ["7", "+", "38", "/", "6"]
  def self.produce(initial = nil) end

  # Creates a new Enumerator object, which can be used as an
  # Enumerable.
  #
  # In the first form, iteration is defined by the given block, in
  # which a "yielder" object, given as block parameter, can be used to
  # yield a value by calling the +yield+ method (aliased as <code><<</code>):
  #
  #   fib = Enumerator.new do |y|
  #     a = b = 1
  #     loop do
  #       y << a
  #       a, b = b, a + b
  #     end
  #   end
  #
  #   fib.take(10) # => [1, 1, 2, 3, 5, 8, 13, 21, 34, 55]
  #
  # The optional parameter can be used to specify how to calculate the size
  # in a lazy fashion (see Enumerator#size). It can either be a value or
  # a callable object.
  #
  # In the deprecated second form, a generated Enumerator iterates over the
  # given object using the given method with the given arguments passed.
  #
  # Use of this form is discouraged.  Use Object#enum_for or Object#to_enum
  # instead.
  #
  #   e = Enumerator.new(ObjectSpace, :each_object)
  #       #-> ObjectSpace.enum_for(:each_object)
  #
  #   e.select { |obj| obj.is_a?(Class) }  # => array of all classes
  def initialize(...) end

  # Returns an enumerator object generated from this enumerator and a
  # given enumerable.
  #
  #   e = (1..3).each + [4, 5]
  #   e.to_a #=> [1, 2, 3, 4, 5]
  def +(other) end

  # Iterates over the block according to how this Enumerator was constructed.
  # If no block and no arguments are given, returns self.
  #
  # === Examples
  #
  #   "Hello, world!".scan(/\w+/)                     #=> ["Hello", "world"]
  #   "Hello, world!".to_enum(:scan, /\w+/).to_a      #=> ["Hello", "world"]
  #   "Hello, world!".to_enum(:scan).each(/\w+/).to_a #=> ["Hello", "world"]
  #
  #   obj = Object.new
  #
  #   def obj.each_arg(a, b=:b, *rest)
  #     yield a
  #     yield b
  #     yield rest
  #     :method_returned
  #   end
  #
  #   enum = obj.to_enum :each_arg, :a, :x
  #
  #   enum.each.to_a                  #=> [:a, :x, []]
  #   enum.each.equal?(enum)          #=> true
  #   enum.each { |elm| elm }         #=> :method_returned
  #
  #   enum.each(:y, :z).to_a          #=> [:a, :x, [:y, :z]]
  #   enum.each(:y, :z).equal?(enum)  #=> false
  #   enum.each(:y, :z) { |elm| elm } #=> :method_returned
  def each(...) end

  # Same as Enumerator#with_index(0), i.e. there is no starting offset.
  #
  # If no block is given, a new Enumerator is returned that includes the index.
  def each_with_index; end

  # Iterates the given block for each element with an arbitrary object, +obj+,
  # and returns +obj+
  #
  # If no block is given, returns a new Enumerator.
  #
  # === Example
  #
  #   to_three = Enumerator.new do |y|
  #     3.times do |x|
  #       y << x
  #     end
  #   end
  #
  #   to_three_with_string = to_three.with_object("foo")
  #   to_three_with_string.each do |x,string|
  #     puts "#{string}: #{x}"
  #   end
  #
  #   # => foo:0
  #   # => foo:1
  #   # => foo:2
  def each_with_object(obj) end
  alias with_object each_with_object

  # Sets the value to be returned by the next yield inside +e+.
  #
  # If the value is not set, the yield returns nil.
  #
  # This value is cleared after being yielded.
  #
  #   # Array#map passes the array's elements to "yield" and collects the
  #   # results of "yield" as an array.
  #   # Following example shows that "next" returns the passed elements and
  #   # values passed to "feed" are collected as an array which can be
  #   # obtained by StopIteration#result.
  #   e = [1,2,3].map
  #   p e.next           #=> 1
  #   e.feed "a"
  #   p e.next           #=> 2
  #   e.feed "b"
  #   p e.next           #=> 3
  #   e.feed "c"
  #   begin
  #     e.next
  #   rescue StopIteration
  #     p $!.result      #=> ["a", "b", "c"]
  #   end
  #
  #   o = Object.new
  #   def o.each
  #     x = yield         # (2) blocks
  #     p x               # (5) => "foo"
  #     x = yield         # (6) blocks
  #     p x               # (8) => nil
  #     x = yield         # (9) blocks
  #     p x               # not reached w/o another e.next
  #   end
  #
  #   e = o.to_enum
  #   e.next              # (1)
  #   e.feed "foo"        # (3)
  #   e.next              # (4)
  #   e.next              # (7)
  #                       # (10)
  def feed(obj) end

  # Creates a printable version of <i>e</i>.
  def inspect; end

  # Returns the next object in the enumerator, and move the internal position
  # forward.  When the position reached at the end, StopIteration is raised.
  #
  # === Example
  #
  #   a = [1,2,3]
  #   e = a.to_enum
  #   p e.next   #=> 1
  #   p e.next   #=> 2
  #   p e.next   #=> 3
  #   p e.next   #raises StopIteration
  #
  # Note that enumeration sequence by +next+ does not affect other non-external
  # enumeration methods, unless the underlying iteration methods itself has
  # side-effect, e.g. IO#each_line.
  def next; end

  # Returns the next object as an array in the enumerator, and move the
  # internal position forward.  When the position reached at the end,
  # StopIteration is raised.
  #
  # This method can be used to distinguish <code>yield</code> and <code>yield
  # nil</code>.
  #
  # === Example
  #
  #   o = Object.new
  #   def o.each
  #     yield
  #     yield 1
  #     yield 1, 2
  #     yield nil
  #     yield [1, 2]
  #   end
  #   e = o.to_enum
  #   p e.next_values
  #   p e.next_values
  #   p e.next_values
  #   p e.next_values
  #   p e.next_values
  #   e = o.to_enum
  #   p e.next
  #   p e.next
  #   p e.next
  #   p e.next
  #   p e.next
  #
  #   ## yield args       next_values      next
  #   #  yield            []               nil
  #   #  yield 1          [1]              1
  #   #  yield 1, 2       [1, 2]           [1, 2]
  #   #  yield nil        [nil]            nil
  #   #  yield [1, 2]     [[1, 2]]         [1, 2]
  #
  # Note that +next_values+ does not affect other non-external enumeration
  # methods unless underlying iteration method itself has side-effect, e.g.
  # IO#each_line.
  def next_values; end

  # Returns the next object in the enumerator, but doesn't move the internal
  # position forward.  If the position is already at the end, StopIteration
  # is raised.
  #
  # === Example
  #
  #   a = [1,2,3]
  #   e = a.to_enum
  #   p e.next   #=> 1
  #   p e.peek   #=> 2
  #   p e.peek   #=> 2
  #   p e.peek   #=> 2
  #   p e.next   #=> 2
  #   p e.next   #=> 3
  #   p e.peek   #raises StopIteration
  def peek; end

  # Returns the next object as an array, similar to Enumerator#next_values, but
  # doesn't move the internal position forward.  If the position is already at
  # the end, StopIteration is raised.
  #
  # === Example
  #
  #   o = Object.new
  #   def o.each
  #     yield
  #     yield 1
  #     yield 1, 2
  #   end
  #   e = o.to_enum
  #   p e.peek_values    #=> []
  #   e.next
  #   p e.peek_values    #=> [1]
  #   p e.peek_values    #=> [1]
  #   e.next
  #   p e.peek_values    #=> [1, 2]
  #   e.next
  #   p e.peek_values    # raises StopIteration
  def peek_values; end

  # Rewinds the enumeration sequence to the beginning.
  #
  # If the enclosed object responds to a "rewind" method, it is called.
  def rewind; end

  # Returns the size of the enumerator, or +nil+ if it can't be calculated lazily.
  #
  #   (1..100).to_a.permutation(4).size # => 94109400
  #   loop.size # => Float::INFINITY
  #   (1..100).drop_while.size # => nil
  def size; end

  # Iterates the given block for each element with an index, which
  # starts from +offset+.  If no block is given, returns a new Enumerator
  # that includes the index, starting from +offset+
  #
  # +offset+:: the starting index to use
  def with_index(offset = 0) end

  # Enumerator::ArithmeticSequence is a subclass of Enumerator,
  # that is a representation of sequences of numbers with common difference.
  # Instances of this class can be generated by the Range#step and Numeric#step
  # methods.
  class ArithmeticSequence < Enumerator
    # Returns <code>true</code> only if +obj+ is an Enumerator::ArithmeticSequence,
    # has equivalent begin, end, step, and exclude_end? settings.
    def ==(other) end
    alias === ==
    alias eql? ==

    # Returns the number that defines the first element of this arithmetic
    # sequence.
    def begin; end

    def each; end

    # Returns the number that defines the end of this arithmetic sequence.
    def end; end

    # Returns <code>true</code> if this arithmetic sequence excludes its end value.
    def exclude_end?; end

    # Returns the first number in this arithmetic sequence,
    # or an array of the first +n+ elements.
    def first(...) end

    # Compute a hash-value for this arithmetic sequence.
    # Two arithmetic sequences with same begin, end, step, and exclude_end?
    # values will generate the same hash-value.
    #
    # See also Object#hash.
    def hash; end

    # Convert this arithmetic sequence to a printable form.
    def inspect; end

    # Returns the last number in this arithmetic sequence,
    # or an array of the last +n+ elements.
    def last(...) end

    # Returns the number of elements in this arithmetic sequence if it is a finite
    # sequence.  Otherwise, returns <code>nil</code>.
    def size; end

    # Returns the number that defines the common difference between
    # two adjacent elements in this arithmetic sequence.
    def step; end
  end

  # Enumerator::Chain is a subclass of Enumerator, which represents a
  # chain of enumerables that works as a single enumerator.
  #
  # This type of objects can be created by Enumerable#chain and
  # Enumerator#+.
  class Chain < Enumerator
    # Generates a new enumerator object that iterates over the elements
    # of given enumerable objects in sequence.
    #
    #   e = Enumerator::Chain.new(1..3, [4, 5])
    #   e.to_a #=> [1, 2, 3, 4, 5]
    #   e.size #=> 5
    def initialize(*enums) end

    # Iterates over the elements of the first enumerable by calling the
    # "each" method on it with the given arguments, then proceeds to the
    # following enumerables in sequence until all of the enumerables are
    # exhausted.
    #
    # If no block is given, returns an enumerator.
    def each(*args) end

    # Returns a printable version of the enumerator chain.
    def inspect; end

    # Rewinds the enumerator chain by calling the "rewind" method on each
    # enumerable in reverse order.  Each call is performed only if the
    # enumerable responds to the method.
    def rewind; end

    # Returns the total size of the enumerator chain calculated by
    # summing up the size of each enumerable in the chain.  If any of the
    # enumerables reports its size as nil or Float::INFINITY, that value
    # is returned as the total size.
    def size; end
  end

  # Generator
  class Generator
    include Enumerable
  end

  # Enumerator::Lazy is a special type of Enumerator, that allows constructing
  # chains of operations without evaluating them immediately, and evaluating
  # values on as-needed basis. In order to do so it redefines most of Enumerable
  # methods so that they just construct another lazy enumerator.
  #
  # Enumerator::Lazy can be constructed from any Enumerable with the
  # Enumerable#lazy method.
  #
  #    lazy = (1..Float::INFINITY).lazy.select(&:odd?).drop(10).take_while { |i| i < 30 }
  #    # => #<Enumerator::Lazy: #<Enumerator::Lazy: #<Enumerator::Lazy: #<Enumerator::Lazy: 1..Infinity>:select>:drop(10)>:take_while>
  #
  # The real enumeration is performed when any non-redefined Enumerable method
  # is called, like Enumerable#first or Enumerable#to_a (the latter is aliased
  # as #force for more semantic code):
  #
  #    lazy.first(2)
  #    #=> [21, 23]
  #
  #    lazy.force
  #    #=> [21, 23, 25, 27, 29]
  #
  # Note that most Enumerable methods that could be called with or without
  # a block, on Enumerator::Lazy will always require a block:
  #
  #    [1, 2, 3].map       #=> #<Enumerator: [1, 2, 3]:map>
  #    [1, 2, 3].lazy.map  # ArgumentError: tried to call lazy map without a block
  #
  # This class allows idiomatic calculations on long or infinite sequences, as well
  # as chaining of calculations without constructing intermediate arrays.
  #
  # Example for working with a slowly calculated sequence:
  #
  #    require 'open-uri'
  #
  #    # This will fetch all URLs before selecting
  #    # necessary data
  #    URLS.map { |u| JSON.parse(open(u).read) }
  #      .select { |data| data.key?('stats') }
  #      .first(5)
  #
  #    # This will fetch URLs one-by-one, only till
  #    # there is enough data to satisfy the condition
  #    URLS.lazy.map { |u| JSON.parse(open(u).read) }
  #      .select { |data| data.key?('stats') }
  #      .first(5)
  #
  # Ending a chain with ".eager" generates a non-lazy enumerator, which
  # is suitable for returning or passing to another method that expects
  # a normal enumerator.
  #
  #    def active_items
  #      groups
  #        .lazy
  #        .flat_map(&:items)
  #        .reject(&:disabled)
  #        .eager
  #    end
  #
  #    # This works lazily; if a checked item is found, it stops
  #    # iteration and does not look into remaining groups.
  #    first_checked = active_items.find(&:checked)
  #
  #    # This returns an array of items like a normal enumerator does.
  #    all_checked = active_items.select(&:checked)
  class Lazy < Enumerator
    # Creates a new Lazy enumerator. When the enumerator is actually enumerated
    # (e.g. by calling #force), +obj+ will be enumerated and each value passed
    # to the given block. The block can yield values back using +yielder+.
    # For example, to create a "filter+map" enumerator:
    #
    #   def filter_map(sequence)
    #     Lazy.new(sequence) do |yielder, *values|
    #       result = yield *values
    #       yielder << result if result
    #     end
    #   end
    #
    #   filter_map(1..Float::INFINITY) {|i| i*i if i.even?}.first(5)
    #   #=> [4, 16, 36, 64, 100]
    def initialize(obj, size = nil) end

    # Like Enumerable#chunk, but chains operation to be lazy-evaluated.
    def chunk(*args) end
    alias slice_before chunk
    alias slice_after chunk
    alias slice_when chunk
    alias chunk_while chunk

    # Like Enumerable#drop, but chains operation to be lazy-evaluated.
    def drop(n) end
    alias _enumerable_drop drop

    # Like Enumerable#drop_while, but chains operation to be lazy-evaluated.
    def drop_while; end
    alias _enumerable_drop_while drop_while

    # Returns a non-lazy Enumerator converted from the lazy enumerator.
    def eager; end

    # Like Enumerable#filter_map, but chains operation to be lazy-evaluated.
    #
    #   (1..).lazy.filter_map { |i| i * 2 if i.even? }.first(5)
    #   #=> [4, 8, 12, 16, 20]
    def filter_map; end
    alias _enumerable_filter_map filter_map

    # Returns a new lazy enumerator with the concatenated results of running
    # +block+ once for every element in the lazy enumerator.
    #
    #   ["foo", "bar"].lazy.flat_map {|i| i.each_char.lazy}.force
    #   #=> ["f", "o", "o", "b", "a", "r"]
    #
    # A value +x+ returned by +block+ is decomposed if either of
    # the following conditions is true:
    #
    # * +x+ responds to both each and force, which means that
    #   +x+ is a lazy enumerator.
    # * +x+ is an array or responds to to_ary.
    #
    # Otherwise, +x+ is contained as-is in the return value.
    #
    #   [{a:1}, {b:2}].lazy.flat_map {|i| i}.force
    #   #=> [{:a=>1}, {:b=>2}]
    def flat_map; end
    alias collect_concat flat_map
    alias _enumerable_flat_map flat_map

    # Like Enumerable#grep, but chains operation to be lazy-evaluated.
    def grep(pattern) end
    alias _enumerable_grep grep

    # Like Enumerable#grep_v, but chains operation to be lazy-evaluated.
    def grep_v(pattern) end
    alias _enumerable_grep_v grep_v

    # Returns self.
    def lazy; end

    # Like Enumerable#map, but chains operation to be lazy-evaluated.
    #
    #    (1..Float::INFINITY).lazy.map {|i| i**2 }
    #    #=> #<Enumerator::Lazy: #<Enumerator::Lazy: 1..Infinity>:map>
    #    (1..Float::INFINITY).lazy.map {|i| i**2 }.first(3)
    #    #=> [1, 4, 9]
    def map; end
    alias collect map
    alias _enumerable_map map

    # Like Enumerable#reject, but chains operation to be lazy-evaluated.
    def reject; end
    alias _enumerable_reject reject

    # Like Enumerable#select, but chains operation to be lazy-evaluated.
    def select; end
    alias find_all select
    alias filter select
    alias _enumerable_select select

    # Like Enumerable#take, but chains operation to be lazy-evaluated.
    def take(n) end
    alias _enumerable_take take

    # Like Enumerable#take_while, but chains operation to be lazy-evaluated.
    def take_while; end
    alias _enumerable_take_while take_while

    # Expands +lazy+ enumerator to an array.
    # See Enumerable#to_a.
    def to_a; end
    alias force to_a

    # Similar to Object#to_enum, except it returns a lazy enumerator.
    # This makes it easy to define Enumerable methods that will
    # naturally remain lazy if called from a lazy enumerator.
    #
    # For example, continuing from the example in Object#to_enum:
    #
    #   # See Object#to_enum for the definition of repeat
    #   r = 1..Float::INFINITY
    #   r.repeat(2).first(5) # => [1, 1, 2, 2, 3]
    #   r.repeat(2).class # => Enumerator
    #   r.repeat(2).map{|n| n ** 2}.first(5) # => endless loop!
    #   # works naturally on lazy enumerator:
    #   r.lazy.repeat(2).class # => Enumerator::Lazy
    #   r.lazy.repeat(2).map{|n| n ** 2}.first(5) # => [1, 1, 4, 4, 9]
    def to_enum(method = :each, *args) end
    alias enum_for to_enum

    # Like Enumerable#uniq, but chains operation to be lazy-evaluated.
    def uniq; end
    alias _enumerable_uniq uniq

    # If a block is given, iterates the given block for each element
    # with an index, which starts from +offset+, and returns a
    # lazy enumerator that yields the same values (without the index).
    #
    # If a block is not given, returns a new lazy enumerator that
    # includes the index, starting from +offset+.
    #
    # +offset+:: the starting index to use
    #
    # See Enumerator#with_index.
    def with_index(offset = 0) end

    # Like Enumerable#zip, but chains operation to be lazy-evaluated.
    # However, if a block is given to zip, values are enumerated immediately.
    def zip(arg, *args) end
    alias _enumerable_zip zip

    private

    # Iterates the given block for each element with an index, which
    # starts from +offset+.  If no block is given, returns a new Enumerator
    # that includes the index, starting from +offset+
    #
    # +offset+:: the starting index to use
    def _enumerable_with_index(*args) end
  end

  # Producer
  class Producer
  end

  # Yielder
  class Yielder
    # Returns a Proc object that takes an argument and yields it.
    #
    # This method is implemented so that a Yielder object can be directly
    # passed to another method as a block argument.
    #
    #   enum = Enumerator.new { |y|
    #     Dir.glob("*.rb") { |file|
    #       File.open(file) { |f| f.each_line(&y) }
    #     }
    #   }
    def to_proc; end
  end
end
