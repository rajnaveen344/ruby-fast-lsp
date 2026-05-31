# frozen_string_literal: true

# A <code>Hash</code> is a collection of key-value pairs. It is
# similar to an <code>Array</code>, except that indexing is done via
# arbitrary keys of any object type, not an integer index. The order
# in which you traverse a hash by either key or value may seem
# arbitrary, and will generally not be in the insertion order.
#
# Hashes have a <em>default value</em> that is returned when accessing
# keys that do not exist in the hash. By default, that value is
# <code>nil</code>.
#
# <code>Hash</code> uses <code>key.eql?</code> to test keys for equality.
# If you need to use instances of your own classes as keys in a <code>Hash</code>,
# it is recommended that you define both the <code>eql?</code> and <code>hash</code>
# methods. The <code>hash</code> method must have the property that
# <code>a.eql?(b)</code> implies <code>a.hash == b.hash</code>.
#
#   class MyClass
#     attr_reader :str
#     def initialize(str)
#       @str = str
#     end
#     def eql?(o)
#       o.is_a?(MyClass) && str == o.str
#     end
#     def hash
#       @str.hash
#     end
#   end
#
#   a = MyClass.new("some string")
#   b = MyClass.new("some string")
#   a.eql? b  #=> true
#
#   h = {}
#
#   h[a] = 1
#   h[a]      #=> 1
#   h[b]      #=> 1
#
#   h[b] = 2
#   h[a]      #=> 2
#   h[b]      #=> 2
class Hash
  include Enumerable

  # Creates a new hash populated with the given objects. Equivalent to
  # the literal <code>{ <i>key</i>, <i>value</i>, ... }</code>. Keys and
  # values occur in pairs, so there must be an even number of arguments.
  #
  #    Hash["a", 100, "b", 200]       #=> {"a"=>100, "b"=>200}
  #    Hash["a" => 100, "b" => 200]   #=> {"a"=>100, "b"=>200}
  #    { "a" => 100, "b" => 200 }     #=> {"a"=>100, "b"=>200}
  def self.[](*key_value) end

  # Returns a new, empty hash. If this hash is subsequently accessed by
  # a key that doesn't correspond to a hash entry, the value returned
  # depends on the style of <code>new</code> used to create the hash. In
  # the first form, the access returns <code>nil</code>. If
  # <i>obj</i> is specified, this single object will be used for
  # all <em>default values</em>. If a block is specified, it will be
  # called with the hash object and the key, and should return the
  # default value. It is the block's responsibility to store the value
  # in the hash if required.
  #
  #    h = Hash.new("Go Fish")
  #    h["a"] = 100
  #    h["b"] = 200
  #    h["a"]           #=> 100
  #    h["c"]           #=> "Go Fish"
  #    # The following alters the single default object
  #    h["c"].upcase!   #=> "GO FISH"
  #    h["d"]           #=> "GO FISH"
  #    h.keys           #=> ["a", "b"]
  #
  #    # While this creates a new default object each time
  #    h = Hash.new { |hash, key| hash[key] = "Go Fish: #{key}" }
  #    h["c"]           #=> "Go Fish: c"
  #    h["c"].upcase!   #=> "GO FISH: C"
  #    h["d"]           #=> "Go Fish: d"
  #    h.keys           #=> ["c", "d"]
  def initialize(*several_variants) end

  # Equality---Two hashes are equal if they each contain the same number
  # of keys and if each key-value pair is equal to (according to
  # <code>Object#==</code>) the corresponding elements in the other
  # hash.
  #
  #    h1 = { "a" => 1, "c" => 2 }
  #    h2 = { 7 => 35, "c" => 2, "a" => 1 }
  #    h3 = { "a" => 1, "c" => 2, 7 => 35 }
  #    h4 = { "a" => 1, "d" => 2, "f" => 35 }
  #    h1 == h2   #=> false
  #    h2 == h3   #=> true
  #    h3 == h4   #=> false
  def ==(other) end

  # Element Reference---Retrieves the <i>value</i> object corresponding
  # to the <i>key</i> object. If not found, returns the a default value (see
  # <code>Hash::new</code> for details).
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h["a"]   #=> 100
  #    h["c"]   #=> nil
  def [](key) end

  # Element Assignment---Associates the value given by
  # <i>value</i> with the key given by <i>key</i>.
  # <i>key</i> should not have its value changed while it is in
  # use as a key (a <code>String</code> passed as a key will be
  # duplicated and frozen).
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h["a"] = 9
  #    h["c"] = 4
  #    h   #=> {"a"=>9, "b"=>200, "c"=>4}
  def []=(key, value) end
  alias store []=

  # Removes all key-value pairs from <i>hsh</i>.
  #
  #    h = { "a" => 100, "b" => 200 }   #=> {"a"=>100, "b"=>200}
  #    h.clear                          #=> {}
  def clear; end

  # Returns the default value, the value that would be returned by
  # <i>hsh</i>[<i>key</i>] if <i>key</i> did not exist in <i>hsh</i>.
  # See also <code>Hash::new</code> and <code>Hash#default=</code>.
  #
  #    h = Hash.new                            #=> {}
  #    h.default                               #=> nil
  #    h.default(2)                            #=> nil
  #
  #    h = Hash.new("cat")                     #=> {}
  #    h.default                               #=> "cat"
  #    h.default(2)                            #=> "cat"
  #
  #    h = Hash.new {|h,k| h[k] = k.to_i*10}   #=> {}
  #    h.default                               #=> nil
  #    h.default(2)                            #=> 20
  def default(key = nil) end

  # Sets the default value, the value returned for a key that does not
  # exist in the hash. It is not possible to set the a default to a
  # <code>Proc</code> that will be executed on each key lookup.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.default = "Go fish"
  #    h["a"]     #=> 100
  #    h["z"]     #=> "Go fish"
  #    # This doesn't do what you might hope...
  #    h.default = proc do |hash, key|
  #      hash[key] = key + key
  #    end
  #    h[2]       #=> #<Proc:0x401b3948@-:6>
  #    h["cat"]   #=> #<Proc:0x401b3948@-:6>
  def default=(obj) end

  # If <code>Hash::new</code> was invoked with a block, return that
  # block, otherwise return <code>nil</code>.
  #
  #    h = Hash.new {|h,k| h[k] = k*k }   #=> {}
  #    p = h.default_proc                 #=> #<Proc:0x401b3d08@-:1>
  #    a = []                             #=> []
  #    p.call(a, 2)
  #    a                                  #=> [nil, nil, 4]
  def default_proc; end

  # Deletes and returns a key-value pair from <i>hsh</i> whose key is
  # equal to <i>key</i>. If the key is not found, returns <code>nil</code>.
  # If the optional code block is given and the key is not found,
  # pass in the key and return the result of <i>block</i>.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.delete("a")                              #=> 100
  #    h.delete("z")                              #=> nil
  #    h.delete("z") { |el| "#{el} not found" }   #=> "z not found"
  def delete(key) end

  # Deletes every key-value pair from <i>hsh</i> for which <i>block</i>
  # evaluates to <code>true</code>.
  #
  #    h = { "a" => 100, "b" => 200, "c" => 300 }
  #    h.delete_if {|key, value| key >= "b" }   #=> {"a"=>100}
  def delete_if; end

  # Calls <i>block</i> once for each key in <i>hsh</i>, passing the key
  # and value to the block as a two-element array. Because of the assignment
  # semantics of block parameters, these elements will be split out if the
  # block has two formal parameters. Also see <code>Hash.each_pair</code>, which
  # will be marginally more efficient for blocks with two parameters.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.each {|key, value| puts "#{key} is #{value}" }
  #
  # <em>produces:</em>
  #
  #    a is 100
  #    b is 200
  def each; end

  # Calls <i>block</i> once for each key in <i>hsh</i>, passing the key
  # as a parameter.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.each_key {|key| puts key }
  #
  # <em>produces:</em>
  #
  #    a
  #    b
  def each_key; end

  # Calls <i>block</i> once for each key in <i>hsh</i>, passing the key
  # and value as parameters.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.each_pair {|key, value| puts "#{key} is #{value}" }
  #
  # <em>produces:</em>
  #
  #    a is 100
  #    b is 200
  def each_pair; end

  # Calls <i>block</i> once for each key in <i>hsh</i>, passing the
  # value as a parameter.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.each_value {|value| puts value }
  #
  # <em>produces:</em>
  #
  #    100
  #    200
  def each_value; end

  # Returns <code>true</code> if <i>hsh</i> contains no key-value pairs.
  #
  #    {}.empty?   #=> true
  def empty?; end

  # Returns <code>true</code> if <i>hash</i> and <i>other</i> are
  # both hashes with the same content.
  def eql?(other) end

  # Returns a value from the hash for the given key. If the key can't be
  # found, there are several options: With no other arguments, it will
  # raise an <code>IndexError</code> exception; if <i>default</i> is
  # given, then that will be returned; if the optional code block is
  # specified, then that will be run and its result returned.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.fetch("a")                            #=> 100
  #    h.fetch("z", "go fish")                 #=> "go fish"
  #    h.fetch("z") { |el| "go fish, #{el}"}   #=> "go fish, z"
  #
  # The following example shows that an exception is raised if the key
  # is not found and a default value is not supplied.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.fetch("z")
  #
  # <em>produces:</em>
  #
  #    prog.rb:2:in `fetch': key not found (IndexError)
  #     from prog.rb:2
  def fetch(*several_variants) end

  # Returns <code>true</code> if the given value is present for some key
  # in <i>hsh</i>.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.has_value?(100)   #=> true
  #    h.has_value?(999)   #=> false
  def has_value?(value) end
  alias value? has_value?

  # Compute a hash-code for this array. Two arrays with the same content
  # will have the same hash code (and will compare using <code>eql?</code>).
  def hash; end

  # Returns <code>true</code> if the given key is present in <i>hsh</i>.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.has_key?("a")   #=> true
  #    h.has_key?("z")   #=> false
  def include?(key) end
  alias member? include?
  alias has_key? include?
  alias key? include?

  # Returns the key for a given value. If not found, returns <code>nil</code>.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.index(200)   #=> "b"
  #    h.index(999)   #=> nil
  def index(value) end

  # Deprecated in favor of <code>Hash#select</code>.
  def indexes(key, *args) end
  alias indices indexes

  # Replaces the contents of <i>hsh</i> with the contents of
  # <i>other_hash</i>.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.replace({ "c" => 300, "d" => 400 })   #=> {"c"=>300, "d"=>400}
  def initialize_copy(p1) end
  alias replace initialize_copy

  # Return the contents of this hash as a string.
  def inspect; end

  # Returns a new hash created by using <i>hsh</i>'s values as keys, and
  # the keys as values.
  #
  #    h = { "n" => 100, "m" => 100, "y" => 300, "d" => 200, "a" => 0 }
  #    h.invert   #=> {0=>"a", 100=>"n", 200=>"d", 300=>"y"}
  def invert; end

  # Returns a new array populated with the keys from this hash. See also
  # <code>Hash#values</code>.
  #
  #    h = { "a" => 100, "b" => 200, "c" => 300, "d" => 400 }
  #    h.keys   #=> ["a", "b", "c", "d"]
  def keys; end

  # Returns a new hash containing the contents of <i>other_hash</i> and
  # the contents of <i>hsh</i>, overwriting entries in <i>hsh</i> with
  # duplicate keys with those from <i>other_hash</i>.
  #
  #    h1 = { "a" => 100, "b" => 200 }
  #    h2 = { "b" => 254, "c" => 300 }
  #    h1.merge(h2)   #=> {"a"=>100, "b"=>254, "c"=>300}
  #    h1             #=> {"a"=>100, "b"=>200}
  def merge(other_hash) end

  # Rebuilds the hash based on the current hash values for each key. If
  # values of key objects have changed since they were inserted, this
  # method will reindex <i>hsh</i>. If <code>Hash#rehash</code> is
  # called while an iterator is traversing the hash, an
  # <code>IndexError</code> will be raised in the iterator.
  #
  #    a = [ "a", "b" ]
  #    c = [ "c", "d" ]
  #    h = { a => 100, c => 300 }
  #    h[a]       #=> 100
  #    a[0] = "z"
  #    h[a]       #=> nil
  #    h.rehash   #=> {["z", "b"]=>100, ["c", "d"]=>300}
  #    h[a]       #=> 100
  def rehash; end

  # Same as <code>Hash#delete_if</code>, but works on (and returns) a
  # copy of the <i>hsh</i>. Equivalent to
  # <code><i>hsh</i>.dup.delete_if</code>.
  def reject; end

  # Equivalent to <code>Hash#delete_if</code>, but returns
  # <code>nil</code> if no changes were made.
  def reject!; end

  # Returns a new array consisting of <code>[key,value]</code>
  # pairs for which the block returns true.
  # Also see <code>Hash.values_at</code>.
  #
  #    h = { "a" => 100, "b" => 200, "c" => 300 }
  #    h.select {|k,v| k > "a"}  #=> [["b", 200], ["c", 300]]
  #    h.select {|k,v| v < 200}  #=> [["a", 100]]
  def select; end

  # Removes a key-value pair from <i>hsh</i> and returns it as the
  # two-item array <code>[</code> <i>key, value</i> <code>]</code>, or
  # the hash's default value if the hash is empty.
  #
  #    h = { 1 => "a", 2 => "b", 3 => "c" }
  #    h.shift   #=> [1, "a"]
  #    h         #=> {2=>"b", 3=>"c"}
  def shift; end

  # Returns the number of key-value pairs in the hash.
  #
  #    h = { "d" => 100, "a" => 200, "v" => 300, "e" => 400 }
  #    h.length        #=> 4
  #    h.delete("a")   #=> 200
  #    h.length        #=> 3
  def size; end
  alias length size

  # Converts <i>hsh</i> to a nested array of <code>[</code> <i>key,
  # value</i> <code>]</code> arrays and sorts it, using
  # <code>Array#sort</code>.
  #
  #    h = { "a" => 20, "b" => 30, "c" => 10  }
  #    h.sort                       #=> [["a", 20], ["b", 30], ["c", 10]]
  #    h.sort {|a,b| a[1]<=>b[1]}   #=> [["c", 10], ["a", 20], ["b", 30]]
  def sort; end

  # Converts <i>hsh</i> to a nested array of <code>[</code> <i>key,
  # value</i> <code>]</code> arrays.
  #
  #    h = { "c" => 300, "a" => 100, "d" => 400, "c" => 300  }
  #    h.to_a   #=> [["a", 100], ["c", 300], ["d", 400]]
  def to_a; end

  # Returns <i>self</i>.
  def to_hash; end

  # Converts <i>hsh</i> to a string by converting the hash to an array
  # of <code>[</code> <i>key, value</i> <code>]</code> pairs and then
  # converting that array to a string using <code>Array#join</code> with
  # the default separator.
  #
  #    h = { "c" => 300, "a" => 100, "d" => 400, "c" => 300  }
  #    h.to_s   #=> "a100c300d400"
  def to_s; end

  # Adds the contents of <i>other_hash</i> to <i>hsh</i>.  If no
  # block is specified entries with duplicate keys are overwritten
  # with the values from <i>other_hash</i>, otherwise the value
  # of each duplicate key is determined by calling the block with
  # the key, its value in <i>hsh</i> and its value in <i>other_hash</i>.
  #
  #    h1 = { "a" => 100, "b" => 200 }
  #    h2 = { "b" => 254, "c" => 300 }
  #    h1.merge!(h2)   #=> {"a"=>100, "b"=>254, "c"=>300}
  #
  #    h1 = { "a" => 100, "b" => 200 }
  #    h2 = { "b" => 254, "c" => 300 }
  #    h1.merge!(h2) { |key, v1, v2| v1 }
  #                    #=> {"a"=>100, "b"=>200, "c"=>300}
  def update(other_hash) end
  alias merge! update

  # Returns a new array populated with the values from <i>hsh</i>. See
  # also <code>Hash#keys</code>.
  #
  #    h = { "a" => 100, "b" => 200, "c" => 300 }
  #    h.values   #=> [100, 200, 300]
  def values; end

  # Return an array containing the values associated with the given keys.
  # Also see <code>Hash.select</code>.
  #
  #   h = { "cat" => "feline", "dog" => "canine", "cow" => "bovine" }
  #   h.values_at("cow", "cat")  #=> ["bovine", "feline"]
  def values_at(key, *args) end
end
