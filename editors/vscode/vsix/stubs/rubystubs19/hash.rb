# frozen_string_literal: true

# A <code>Hash</code> is a collection of key-value pairs. It is
# similar to an <code>Array</code>, except that indexing is done via
# arbitrary keys of any object type, not an integer index. Hashes enumerate
# their values in the order that the corresponding keys were inserted.
#
# Hashes have a <em>default value</em> that is returned when accessing
# keys that do not exist in the hash. By default, that value is
# <code>nil</code>.
class Hash
  include Enumerable

  # Creates a new hash populated with the given objects. Equivalent to
  # the literal <code>{ <i>key</i> => <i>value</i>, ... }</code>. In the first
  # form, keys and values occur in pairs, so there must be an even number of arguments.
  # The second and third form take a single argument which is either
  # an array of key-value pairs or an object convertible to a hash.
  #
  #    Hash["a", 100, "b", 200]             #=> {"a"=>100, "b"=>200}
  #    Hash[ [ ["a", 100], ["b", 200] ] ]   #=> {"a"=>100, "b"=>200}
  #    Hash["a" => 100, "b" => 200]         #=> {"a"=>100, "b"=>200}
  def self.[](*several_variants) end

  # Try to convert <i>obj</i> into a hash, using to_hash method.
  # Returns converted hash or nil if <i>obj</i> cannot be converted
  # for any reason.
  #
  #    Hash.try_convert({1=>2})   # => {1=>2}
  #    Hash.try_convert("1=>2")   # => nil
  def self.try_convert(obj) end

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
  # to the <i>key</i> object. If not found, returns the default value (see
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

  # Searches through the hash comparing _obj_ with the key using <code>==</code>.
  # Returns the key-value pair (two elements array) or +nil+
  # if no match is found.  See <code>Array#assoc</code>.
  #
  #    h = {"colors"  => ["red", "blue", "green"],
  #         "letters" => ["a", "b", "c" ]}
  #    h.assoc("letters")  #=> ["letters", ["a", "b", "c"]]
  #    h.assoc("foo")      #=> nil
  def assoc(obj) end

  # Removes all key-value pairs from <i>hsh</i>.
  #
  #    h = { "a" => 100, "b" => 200 }   #=> {"a"=>100, "b"=>200}
  #    h.clear                          #=> {}
  def clear; end

  # Makes <i>hsh</i> compare its keys by their identity, i.e. it
  # will consider exact same objects as same keys.
  #
  #    h1 = { "a" => 100, "b" => 200, :c => "c" }
  #    h1["a"]        #=> 100
  #    h1.compare_by_identity
  #    h1.compare_by_identity? #=> true
  #    h1["a"]        #=> nil  # different objects.
  #    h1[:c]         #=> "c"  # same symbols are all same.
  def compare_by_identity; end

  # Returns <code>true</code> if <i>hsh</i> will compare its keys by
  # their identity.  Also see <code>Hash#compare_by_identity</code>.
  def compare_by_identity?; end

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
  # exist in the hash. It is not possible to set the default to a
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

  # Sets the default proc to be executed on each key lookup.
  #
  #    h.default_proc = proc do |hash, key|
  #      hash[key] = key + key
  #    end
  #    h[2]       #=> 4
  #    h["cat"]   #=> "catcat"
  def default_proc=(proc_obj) end

  # Deletes and returns a key-value pair from <i>hsh</i> whose key is
  # equal to <i>key</i>. If the key is not found, returns the
  # <em>default value</em>. If the optional code block is given and the
  # key is not found, pass in the key and return the result of
  # <i>block</i>.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.delete("a")                              #=> 100
  #    h.delete("z")                              #=> nil
  #    h.delete("z") { |el| "#{el} not found" }   #=> "z not found"
  def delete(key) end

  # Deletes every key-value pair from <i>hsh</i> for which <i>block</i>
  # evaluates to <code>true</code>.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    h = { "a" => 100, "b" => 200, "c" => 300 }
  #    h.delete_if {|key, value| key >= "b" }   #=> {"a"=>100}
  def delete_if; end

  # Calls <i>block</i> once for each key in <i>hsh</i>, passing the key
  # as a parameter.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.each_key {|key| puts key }
  #
  # <em>produces:</em>
  #
  #    a
  #    b
  def each_key; end

  # Calls <i>block</i> once for each key in <i>hsh</i>, passing the key-value
  # pair as parameters.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.each {|key, value| puts "#{key} is #{value}" }
  #
  # <em>produces:</em>
  #
  #    a is 100
  #    b is 200
  def each_pair; end
  alias each each_pair

  # Calls <i>block</i> once for each key in <i>hsh</i>, passing the
  # value as a parameter.
  #
  # If no block is given, an enumerator is returned instead.
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
  # raise an <code>KeyError</code> exception; if <i>default</i> is
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
  #    prog.rb:2:in `fetch': key not found (KeyError)
  #     from prog.rb:2
  def fetch(*several_variants) end

  # Returns a new array that is a one-dimensional flattening of this
  # hash. That is, for every key or value that is an array, extract
  # its elements into the new array.  Unlike Array#flatten, this
  # method does not flatten recursively by default.  The optional
  # <i>level</i> argument determines the level of recursion to flatten.
  #
  #    a =  {1=> "one", 2 => [2,"two"], 3 => "three"}
  #    a.flatten    # => [1, "one", 2, [2, "two"], 3, "three"]
  #    a.flatten(2) # => [1, "one", 2, 2, "two", 3, "three"]
  def flatten(*several_variants) end

  # Returns <code>true</code> if the given value is present for some key
  # in <i>hsh</i>.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.has_value?(100)   #=> true
  #    h.has_value?(999)   #=> false
  def has_value?(value) end
  alias value? has_value?

  # Compute a hash-code for this hash. Two hashes with the same content
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

  # Replaces the contents of <i>hsh</i> with the contents of
  # <i>other_hash</i>.
  #
  #    h = { "a" => 100, "b" => 200 }
  #    h.replace({ "c" => 300, "d" => 400 })   #=> {"c"=>300, "d"=>400}
  def initialize_copy(p1) end
  alias replace initialize_copy

  # Return the contents of this hash as a string.
  #
  #     h = { "c" => 300, "a" => 100, "d" => 400, "c" => 300  }
  #     h.to_s   #=> "{\"c\"=>300, \"a\"=>100, \"d\"=>400}"
  def inspect; end
  alias to_s inspect

  # Returns a new hash created by using <i>hsh</i>'s values as keys, and
  # the keys as values.
  #
  #    h = { "n" => 100, "m" => 100, "y" => 300, "d" => 200, "a" => 0 }
  #    h.invert   #=> {0=>"a", 100=>"m", 200=>"d", 300=>"y"}
  def invert; end

  # Deletes every key-value pair from <i>hsh</i> for which <i>block</i>
  # evaluates to false.
  #
  # If no block is given, an enumerator is returned instead.
  def keep_if; end

  # Returns the key of an occurrence of a given value. If the value is
  # not found, returns <code>nil</code>.
  #
  #    h = { "a" => 100, "b" => 200, "c" => 300, "d" => 300 }
  #    h.key(200)   #=> "b"
  #    h.key(300)   #=> "c"
  #    h.key(999)   #=> nil
  def key(value) end

  # Returns a new array populated with the keys from this hash. See also
  # <code>Hash#values</code>.
  #
  #    h = { "a" => 100, "b" => 200, "c" => 300, "d" => 400 }
  #    h.keys   #=> ["a", "b", "c", "d"]
  def keys; end

  # Returns a new hash containing the contents of <i>other_hash</i> and
  # the contents of <i>hsh</i>. If no block is specified, the value for
  # entries with duplicate keys will be that of <i>other_hash</i>. Otherwise
  # the value for each duplicate key is determined by calling the block
  # with the key, its value in <i>hsh</i> and its value in <i>other_hash</i>.
  #
  #    h1 = { "a" => 100, "b" => 200 }
  #    h2 = { "b" => 254, "c" => 300 }
  #    h1.merge(h2)   #=> {"a"=>100, "b"=>254, "c"=>300}
  #    h1.merge(h2){|key, oldval, newval| newval - oldval}
  #                   #=> {"a"=>100, "b"=>54,  "c"=>300}
  #    h1             #=> {"a"=>100, "b"=>200}
  def merge(other_hash) end

  # Searches through the hash comparing _obj_ with the value using <code>==</code>.
  # Returns the first key-value pair (two-element array) that matches. See
  # also <code>Array#rassoc</code>.
  #
  #    a = {1=> "one", 2 => "two", 3 => "three", "ii" => "two"}
  #    a.rassoc("two")    #=> [2, "two"]
  #    a.rassoc("four")   #=> nil
  def rassoc(obj) end

  # Rebuilds the hash based on the current hash values for each key. If
  # values of key objects have changed since they were inserted, this
  # method will reindex <i>hsh</i>. If <code>Hash#rehash</code> is
  # called while an iterator is traversing the hash, an
  # <code>RuntimeError</code> will be raised in the iterator.
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

  # Returns a new hash consisting of entries for which the block returns true.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    h = { "a" => 100, "b" => 200, "c" => 300 }
  #    h.select {|k,v| k > "a"}  #=> {"b" => 200, "c" => 300}
  #    h.select {|k,v| v < 200}  #=> {"a" => 100}
  def select; end

  # Equivalent to <code>Hash#keep_if</code>, but returns
  # <code>nil</code> if no changes were made.
  def select!; end

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
  # value</i> <code>]</code> arrays.
  #
  #    h = { "c" => 300, "a" => 100, "d" => 400, "c" => 300  }
  #    h.to_a   #=> [["c", 300], ["a", 100], ["d", 400]]
  def to_a; end

  # Returns +self+.
  def to_hash; end

  # Adds the contents of <i>other_hash</i> to <i>hsh</i>.  If no
  # block is specified, entries with duplicate keys are overwritten
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
