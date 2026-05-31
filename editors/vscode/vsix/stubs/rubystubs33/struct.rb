# frozen_string_literal: true

# \Class \Struct provides a convenient way to create a simple class
# that can store and fetch values.
#
# This example creates a subclass of +Struct+, <tt>Struct::Customer</tt>;
# the first argument, a string, is the name of the subclass;
# the other arguments, symbols, determine the _members_ of the new subclass.
#
#   Customer = Struct.new('Customer', :name, :address, :zip)
#   Customer.name       # => "Struct::Customer"
#   Customer.class      # => Class
#   Customer.superclass # => Struct
#
# Corresponding to each member are two methods, a writer and a reader,
# that store and fetch values:
#
#   methods = Customer.instance_methods false
#   methods # => [:zip, :address=, :zip=, :address, :name, :name=]
#
# An instance of the subclass may be created,
# and its members assigned values, via method <tt>::new</tt>:
#
#   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
#   joe # => #<struct Struct::Customer name="Joe Smith", address="123 Maple, Anytown NC", zip=12345>
#
# The member values may be managed thus:
#
#   joe.name    # => "Joe Smith"
#   joe.name = 'Joseph Smith'
#   joe.name    # => "Joseph Smith"
#
# And thus; note that member name may be expressed as either a string or a symbol:
#
#   joe[:name]  # => "Joseph Smith"
#   joe[:name] = 'Joseph Smith, Jr.'
#   joe['name'] # => "Joseph Smith, Jr."
#
# See Struct::new.
#
# == What's Here
#
# First, what's elsewhere. \Class \Struct:
#
# - Inherits from {class Object}[rdoc-ref:Object@What-27s+Here].
# - Includes {module Enumerable}[rdoc-ref:Enumerable@What-27s+Here],
#   which provides dozens of additional methods.
#
# See also Data, which is a somewhat similar, but stricter concept for defining immutable
# value objects.
#
# Here, class \Struct provides methods that are useful for:
#
# - {Creating a Struct Subclass}[rdoc-ref:Struct@Methods+for+Creating+a+Struct+Subclass]
# - {Querying}[rdoc-ref:Struct@Methods+for+Querying]
# - {Comparing}[rdoc-ref:Struct@Methods+for+Comparing]
# - {Fetching}[rdoc-ref:Struct@Methods+for+Fetching]
# - {Assigning}[rdoc-ref:Struct@Methods+for+Assigning]
# - {Iterating}[rdoc-ref:Struct@Methods+for+Iterating]
# - {Converting}[rdoc-ref:Struct@Methods+for+Converting]
#
# === Methods for Creating a Struct Subclass
#
# - ::new: Returns a new subclass of \Struct.
#
# === Methods for Querying
#
# - #hash: Returns the integer hash code.
# - #length, #size: Returns the number of members.
#
# === Methods for Comparing
#
# - #==: Returns whether a given object is equal to +self+, using <tt>==</tt>
#   to compare member values.
# - #eql?: Returns whether a given object is equal to +self+,
#   using <tt>eql?</tt> to compare member values.
#
# === Methods for Fetching
#
# - #[]: Returns the value associated with a given member name.
# - #to_a, #values, #deconstruct: Returns the member values in +self+ as an array.
# - #deconstruct_keys: Returns a hash of the name/value pairs
#   for given member names.
# - #dig: Returns the object in nested objects that is specified
#   by a given member name and additional arguments.
# - #members: Returns an array of the member names.
# - #select, #filter: Returns an array of member values from +self+,
#   as selected by the given block.
# - #values_at: Returns an array containing values for given member names.
#
# === Methods for Assigning
#
# - #[]=: Assigns a given value to a given member name.
#
# === Methods for Iterating
#
# - #each: Calls a given block with each member name.
# - #each_pair: Calls a given block with each member name/value pair.
#
# === Methods for Converting
#
# - #inspect, #to_s: Returns a string representation of +self+.
# - #to_h: Returns a hash of the member name/value pairs in +self+.
class Struct
  include Enumerable

  # Returns +true+ if the class was initialized with <tt>keyword_init: true</tt>.
  # Otherwise returns +nil+ or +false+.
  #
  # Examples:
  #   Foo = Struct.new(:a)
  #   Foo.keyword_init? # => nil
  #   Bar = Struct.new(:a, keyword_init: true)
  #   Bar.keyword_init? # => true
  #   Baz = Struct.new(:a, keyword_init: false)
  #   Baz.keyword_init? # => false
  def self.keyword_init?; end

  # Returns the member names of the Struct descendant as an array:
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    Customer.members # => [:name, :address, :zip]
  def self.members; end

  # <tt>Struct.new</tt> returns a new subclass of +Struct+.  The new subclass:
  #
  # - May be anonymous, or may have the name given by +class_name+.
  # - May have members as given by +member_names+.
  # - May have initialization via ordinary arguments, or via keyword arguments
  #
  # The new subclass has its own method <tt>::new</tt>; thus:
  #
  #   Foo = Struct.new('Foo', :foo, :bar) # => Struct::Foo
  #   f = Foo.new(0, 1)                   # => #<struct Struct::Foo foo=0, bar=1>
  #
  # <b>\Class Name</b>
  #
  # With string argument +class_name+,
  # returns a new subclass of +Struct+ named <tt>Struct::<em>class_name</em></tt>:
  #
  #   Foo = Struct.new('Foo', :foo, :bar) # => Struct::Foo
  #   Foo.name                            # => "Struct::Foo"
  #   Foo.superclass                      # => Struct
  #
  # Without string argument +class_name+,
  # returns a new anonymous subclass of +Struct+:
  #
  #   Struct.new(:foo, :bar).name # => nil
  #
  # <b>Block</b>
  #
  # With a block given, the created subclass is yielded to the block:
  #
  #   Customer = Struct.new('Customer', :name, :address) do |new_class|
  #     p "The new subclass is #{new_class}"
  #     def greeting
  #       "Hello #{name} at #{address}"
  #     end
  #   end           # => Struct::Customer
  #   dave = Customer.new('Dave', '123 Main')
  #   dave # =>     #<struct Struct::Customer name="Dave", address="123 Main">
  #   dave.greeting # => "Hello Dave at 123 Main"
  #
  # Output, from <tt>Struct.new</tt>:
  #
  #   "The new subclass is Struct::Customer"
  #
  # <b>Member Names</b>
  #
  # Symbol arguments +member_names+
  # determines the members of the new subclass:
  #
  #   Struct.new(:foo, :bar).members        # => [:foo, :bar]
  #   Struct.new('Foo', :foo, :bar).members # => [:foo, :bar]
  #
  # The new subclass has instance methods corresponding to +member_names+:
  #
  #   Foo = Struct.new('Foo', :foo, :bar)
  #   Foo.instance_methods(false) # => [:foo, :bar, :foo=, :bar=]
  #   f = Foo.new                 # => #<struct Struct::Foo foo=nil, bar=nil>
  #   f.foo                       # => nil
  #   f.foo = 0                   # => 0
  #   f.bar                       # => nil
  #   f.bar = 1                   # => 1
  #   f                           # => #<struct Struct::Foo foo=0, bar=1>
  #
  # <b>Singleton Methods</b>
  #
  # A subclass returned by Struct.new has these singleton methods:
  #
  # - \Method <tt>::new </tt> creates an instance of the subclass:
  #
  #     Foo.new          # => #<struct Struct::Foo foo=nil, bar=nil>
  #     Foo.new(0)       # => #<struct Struct::Foo foo=0, bar=nil>
  #     Foo.new(0, 1)    # => #<struct Struct::Foo foo=0, bar=1>
  #     Foo.new(0, 1, 2) # Raises ArgumentError: struct size differs
  #
  #     # Initialization with keyword arguments:
  #     Foo.new(foo: 0)         # => #<struct Struct::Foo foo=0, bar=nil>
  #     Foo.new(foo: 0, bar: 1) # => #<struct Struct::Foo foo=0, bar=1>
  #     Foo.new(foo: 0, bar: 1, baz: 2)
  #     # Raises ArgumentError: unknown keywords: baz
  #
  # - \Method <tt>:inspect</tt> returns a string representation of the subclass:
  #
  #     Foo.inspect
  #     # => "Struct::Foo"
  #
  # - \Method <tt>::members</tt> returns an array of the member names:
  #
  #     Foo.members # => [:foo, :bar]
  #
  # <b>Keyword Argument</b>
  #
  # By default, the arguments for initializing an instance of the new subclass
  # can be both positional and keyword arguments.
  #
  # Optional keyword argument <tt>keyword_init:</tt> allows to force only one
  # type of arguments to be accepted:
  #
  #   KeywordsOnly = Struct.new(:foo, :bar, keyword_init: true)
  #   KeywordsOnly.new(bar: 1, foo: 0)
  #   # => #<struct KeywordsOnly foo=0, bar=1>
  #   KeywordsOnly.new(0, 1)
  #   # Raises ArgumentError: wrong number of arguments
  #
  #   PositionalOnly = Struct.new(:foo, :bar, keyword_init: false)
  #   PositionalOnly.new(0, 1)
  #   # => #<struct PositionalOnly foo=0, bar=1>
  #   PositionalOnly.new(bar: 1, foo: 0)
  #   # => #<struct PositionalOnly foo={:foo=>1, :bar=>2}, bar=nil>
  #   # Note that no error is raised, but arguments treated as one hash value
  #
  #   # Same as not providing keyword_init:
  #   Any = Struct.new(:foo, :bar, keyword_init: nil)
  #   Any.new(foo: 1, bar: 2)
  #   # => #<struct Any foo=1, bar=2>
  #   Any.new(1, 2)
  #   # => #<struct Any foo=1, bar=2>
  def initialize(...) end

  # Returns  +true+ if and only if the following are true; otherwise returns +false+:
  #
  # - <tt>other.class == self.class</tt>.
  # - For each member name +name+, <tt>other.name == self.name</tt>.
  #
  # Examples:
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe    = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe_jr = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe_jr == joe # => true
  #   joe_jr[:name] = 'Joe Smith, Jr.'
  #   # => "Joe Smith, Jr."
  #   joe_jr == joe # => false
  def ==(other) end

  # Returns a value from +self+.
  #
  # With symbol or string argument +name+ given, returns the value for the named member:
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe[:zip] # => 12345
  #
  # Raises NameError if +name+ is not the name of a member.
  #
  # With integer argument +n+ given, returns <tt>self.values[n]</tt>
  # if +n+ is in range;
  # see Array@Array+Indexes:
  #
  #   joe[2]  # => 12345
  #   joe[-2] # => "123 Maple, Anytown NC"
  #
  # Raises IndexError if +n+ is out of range.
  def [](...) end

  # Assigns a value to a member.
  #
  # With symbol or string argument +name+ given, assigns the given +value+
  # to the named member; returns +value+:
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe[:zip] = 54321 # => 54321
  #   joe # => #<struct Customer name="Joe Smith", address="123 Maple, Anytown NC", zip=54321>
  #
  # Raises NameError if +name+ is not the name of a member.
  #
  # With integer argument +n+ given, assigns the given +value+
  # to the +n+-th member if +n+ is in range;
  # see Array@Array+Indexes:
  #
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe[2] = 54321           # => 54321
  #   joe[-3] = 'Joseph Smith' # => "Joseph Smith"
  #   joe # => #<struct Customer name="Joseph Smith", address="123 Maple, Anytown NC", zip=54321>
  #
  # Raises IndexError if +n+ is out of range.
  def []=(...) end

  # Returns a hash of the name/value pairs for the given member names.
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   h = joe.deconstruct_keys([:zip, :address])
  #   h # => {:zip=>12345, :address=>"123 Maple, Anytown NC"}
  #
  # Returns all names and values if +array_of_names+ is +nil+:
  #
  #   h = joe.deconstruct_keys(nil)
  #   h # => {:name=>"Joseph Smith, Jr.", :address=>"123 Maple, Anytown NC", :zip=>12345}
  def deconstruct_keys(array_of_names) end

  #  Finds and returns an object among nested objects.
  #  The nested objects may be instances of various classes.
  #  See {Dig Methods}[rdoc-ref:dig_methods.rdoc].
  #
  #  Given symbol or string argument +name+,
  #  returns the object that is specified by +name+ and +identifiers+:
  #
  #   Foo = Struct.new(:a)
  #   f = Foo.new(Foo.new({b: [1, 2, 3]}))
  #   f.dig(:a) # => #<struct Foo a={:b=>[1, 2, 3]}>
  #   f.dig(:a, :a) # => {:b=>[1, 2, 3]}
  #   f.dig(:a, :a, :b) # => [1, 2, 3]
  #   f.dig(:a, :a, :b, 0) # => 1
  #   f.dig(:b, 0) # => nil
  #
  #  Given integer argument +n+,
  #  returns the object that is specified by +n+ and +identifiers+:
  #
  #   f.dig(0) # => #<struct Foo a={:b=>[1, 2, 3]}>
  #   f.dig(0, 0) # => {:b=>[1, 2, 3]}
  #   f.dig(0, 0, :b) # => [1, 2, 3]
  #   f.dig(0, 0, :b, 0) # => 1
  #   f.dig(:b, 0) # => nil
  def dig(...) end

  # Calls the given block with the value of each member; returns +self+:
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe.each {|value| p value }
  #
  # Output:
  #
  #   "Joe Smith"
  #   "123 Maple, Anytown NC"
  #   12345
  #
  # Returns an Enumerator if no block is given.
  #
  # Related: #each_pair.
  def each; end

  # Calls the given block with each member name/value pair; returns +self+:
  #
  #   Customer = Struct.new(:name, :address, :zip) # => Customer
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe.each_pair {|(name, value)| p "#{name} => #{value}" }
  #
  # Output:
  #
  #   "name => Joe Smith"
  #   "address => 123 Maple, Anytown NC"
  #   "zip => 12345"
  #
  # Returns an Enumerator if no block is given.
  #
  # Related: #each.
  def each_pair; end

  #  Returns +true+ if and only if the following are true; otherwise returns +false+:
  #
  #  - <tt>other.class == self.class</tt>.
  #  - For each member name +name+, <tt>other.name.eql?(self.name)</tt>.
  #
  #     Customer = Struct.new(:name, :address, :zip)
  #     joe    = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #     joe_jr = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #     joe_jr.eql?(joe) # => true
  #     joe_jr[:name] = 'Joe Smith, Jr.'
  #     joe_jr.eql?(joe) # => false
  #
  #  Related: Object#==.
  def eql?(other) end

  # Returns the integer hash value for +self+.
  #
  # Two structs of the same class and with the same content
  # will have the same hash code (and will compare using Struct#eql?):
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe    = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe_jr = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe.hash == joe_jr.hash # => true
  #   joe_jr[:name] = 'Joe Smith, Jr.'
  #   joe.hash == joe_jr.hash # => false
  #
  # Related: Object#hash.
  def hash; end

  # Returns a string representation of +self+:
  #
  #   Customer = Struct.new(:name, :address, :zip) # => Customer
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe.inspect # => "#<struct Customer name=\"Joe Smith\", address=\"123 Maple, Anytown NC\", zip=12345>"
  def inspect; end
  alias to_s inspect

  # Returns the member names from +self+ as an array:
  #
  #    Customer = Struct.new(:name, :address, :zip)
  #    Customer.new.members # => [:name, :address, :zip]
  #
  # Related: #to_a.
  def members; end

  # With a block given, returns an array of values from +self+
  # for which the block returns a truthy value:
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   a = joe.select {|value| value.is_a?(String) }
  #   a # => ["Joe Smith", "123 Maple, Anytown NC"]
  #   a = joe.select {|value| value.is_a?(Integer) }
  #   a # => [12345]
  #
  # With no block given, returns an Enumerator.
  def select; end
  alias filter select

  # Returns the number of members.
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe.size #=> 3
  def size; end
  alias length size

  # Returns the values in +self+ as an array:
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe.to_a # => ["Joe Smith", "123 Maple, Anytown NC", 12345]
  #
  # Related: #members.
  def to_a; end
  alias values to_a
  alias deconstruct to_a

  # Returns a hash containing the name and value for each member:
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   h = joe.to_h
  #   h # => {:name=>"Joe Smith", :address=>"123 Maple, Anytown NC", :zip=>12345}
  #
  # If a block is given, it is called with each name/value pair;
  # the block should return a 2-element array whose elements will become
  # a key/value pair in the returned hash:
  #
  #   h = joe.to_h{|name, value| [name.upcase, value.to_s.upcase]}
  #   h # => {:NAME=>"JOE SMITH", :ADDRESS=>"123 MAPLE, ANYTOWN NC", :ZIP=>"12345"}
  #
  # Raises ArgumentError if the block returns an inappropriate value.
  def to_h; end

  # Returns an array of values from +self+.
  #
  # With integer arguments +integers+ given,
  # returns an array containing each value given by one of +integers+:
  #
  #   Customer = Struct.new(:name, :address, :zip)
  #   joe = Customer.new("Joe Smith", "123 Maple, Anytown NC", 12345)
  #   joe.values_at(0, 2)    # => ["Joe Smith", 12345]
  #   joe.values_at(2, 0)    # => [12345, "Joe Smith"]
  #   joe.values_at(2, 1, 0) # => [12345, "123 Maple, Anytown NC", "Joe Smith"]
  #   joe.values_at(0, -3)   # => ["Joe Smith", "Joe Smith"]
  #
  # Raises IndexError if any of +integers+ is out of range;
  # see Array@Array+Indexes.
  #
  # With integer range argument +integer_range+ given,
  # returns an array containing each value given by the elements of the range;
  # fills with +nil+ values for range elements larger than the structure:
  #
  #   joe.values_at(0..2)
  #   # => ["Joe Smith", "123 Maple, Anytown NC", 12345]
  #   joe.values_at(-3..-1)
  #   # => ["Joe Smith", "123 Maple, Anytown NC", 12345]
  #   joe.values_at(1..4) # => ["123 Maple, Anytown NC", 12345, nil, nil]
  #
  # Raises RangeError if any element of the range is negative and out of range;
  # see Array@Array+Indexes.
  def values_at(...) end
end
