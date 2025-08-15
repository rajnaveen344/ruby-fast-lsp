# frozen_string_literal: true

# ENV is a hash-like accessor for environment variables.
#
# === Interaction with the Operating System
#
# The ENV object interacts with the operating system's environment variables:
#
# - When you get the value for a name in ENV, the value is retrieved from among the current environment variables.
# - When you create or set a name-value pair in ENV, the name and value are immediately set in the environment variables.
# - When you delete a name-value pair in ENV, it is immediately deleted from the environment variables.
#
# === Names and Values
#
# Generally, a name or value is a String.
#
# ==== Valid Names and Values
#
# Each name or value must be one of the following:
#
# - A String.
# - An object that responds to \#to_str by returning a String, in which case that String will be used as the name or value.
#
# ==== Invalid Names and Values
#
# A new name:
#
# - May not be the empty string:
#     ENV[''] = '0'
#     # Raises Errno::EINVAL (Invalid argument - ruby_setenv())
#
# - May not contain character <code>"="</code>:
#     ENV['='] = '0'
#     # Raises Errno::EINVAL (Invalid argument - ruby_setenv(=))
#
# A new name or value:
#
# - May not be a non-String that does not respond to \#to_str:
#
#     ENV['foo'] = Object.new
#     # Raises TypeError (no implicit conversion of Object into String)
#     ENV[Object.new] = '0'
#     # Raises TypeError (no implicit conversion of Object into String)
#
# - May not contain the NUL character <code>"\0"</code>:
#
#     ENV['foo'] = "\0"
#     # Raises ArgumentError (bad environment variable value: contains null byte)
#     ENV["\0"] == '0'
#     # Raises ArgumentError (bad environment variable name: contains null byte)
#
# - May not have an ASCII-incompatible encoding such as UTF-16LE or ISO-2022-JP:
#
#     ENV['foo'] = '0'.force_encoding(Encoding::ISO_2022_JP)
#     # Raises ArgumentError (bad environment variable name: ASCII incompatible encoding: ISO-2022-JP)
#     ENV["foo".force_encoding(Encoding::ISO_2022_JP)] = '0'
#     # Raises ArgumentError (bad environment variable name: ASCII incompatible encoding: ISO-2022-JP)
#
# === About Ordering
#
# ENV enumerates its name/value pairs in the order found
# in the operating system's environment variables.
# Therefore the ordering of ENV content is OS-dependent, and may be indeterminate.
#
# This will be seen in:
# - A Hash returned by an ENV method.
# - An Enumerator returned by an ENV method.
# - An Array returned by ENV.keys, ENV.values, or ENV.to_a.
# - The String returned by ENV.inspect.
# - The Array returned by ENV.shift.
# - The name returned by ENV.key.
#
# === About the Examples
# Some methods in ENV return ENV itself. Typically, there are many environment variables.
# It's not useful to display a large ENV in the examples here,
# so most example snippets begin by resetting the contents of ENV:
# - ENV.replace replaces ENV with a new collection of entries.
# - ENV.clear empties ENV.
class ENV
  # Returns the value for the environment variable +name+ if it exists:
  #   ENV['foo'] = '0'
  #   ENV['foo'] # => "0"
  # Returns nil if the named variable does not exist:
  #   ENV.clear
  #   ENV['foo'] # => nil
  # Raises an exception if +name+ is invalid.
  # See {Invalid Names and Values}[#class-ENV-label-Invalid-Names+and+Values].
  def self.[](name) end

  # ENV.store is an alias for ENV.[]=.
  #
  # Creates, updates, or deletes the named environment variable, returning the value.
  # Both +name+ and +value+ may be instances of String.
  # See {Valid Names and Values}[#class-ENV-label-Valid+Names+and+Values].
  #
  # - If the named environment variable does not exist:
  #   - If +value+ is +nil+, does nothing.
  #       ENV.clear
  #       ENV['foo'] = nil # => nil
  #       ENV.include?('foo') # => false
  #       ENV.store('bar', nil) # => nil
  #       ENV.include?('bar') # => false
  #   - If +value+ is not +nil+, creates the environment variable with +name+ and +value+:
  #       # Create 'foo' using ENV.[]=.
  #       ENV['foo'] = '0' # => '0'
  #       ENV['foo'] # => '0'
  #       # Create 'bar' using ENV.store.
  #       ENV.store('bar', '1') # => '1'
  #       ENV['bar'] # => '1'
  # - If the named environment variable exists:
  #   - If +value+ is not +nil+, updates the environment variable with value +value+:
  #       # Update 'foo' using ENV.[]=.
  #       ENV['foo'] = '2' # => '2'
  #       ENV['foo'] # => '2'
  #       # Update 'bar' using ENV.store.
  #       ENV.store('bar', '3') # => '3'
  #       ENV['bar'] # => '3'
  #   - If +value+ is +nil+, deletes the environment variable:
  #       # Delete 'foo' using ENV.[]=.
  #       ENV['foo'] = nil # => nil
  #       ENV.include?('foo') # => false
  #       # Delete 'bar' using ENV.store.
  #       ENV.store('bar', nil) # => nil
  #       ENV.include?('bar') # => false
  #
  # Raises an exception if +name+ or +value+ is invalid.
  # See {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values].
  def self.[]=(name, value) end

  # Returns a 2-element Array containing the name and value of the environment variable
  # for +name+ if it exists:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.assoc('foo') # => ['foo' '0']
  # Returns +nil+ if +name+ is a valid String and there is no such environment variable:
  #   ENV.assoc('baz') # => false
  # Returns +nil+ if +name+ is the empty String or is a String containing character <code>'='</code>:
  #   ENV.assoc('') # => false
  #   ENV.assoc('=') # => false
  # Raises an exception if +name+ is a String containing the NUL character <code>"\0"</code>:
  #   ENV.assoc("\0") # Raises ArgumentError (bad environment variable name: contains null byte)
  # Raises an exception if +name+ has an encoding that is not ASCII-compatible:
  #   ENV.assoc("\xa1\xa1".force_encoding(Encoding::UTF_16LE))
  #   # Raises ArgumentError (bad environment variable name: ASCII incompatible encoding: UTF-16LE)
  # Raises an exception if +name+ is not a String:
  #   ENV.assoc(Object.new) # TypeError (no implicit conversion of Object into String)
  def self.assoc(name) end

  # Removes every environment variable; returns ENV:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.size # => 2
  #   ENV.clear # => ENV
  #   ENV.size # => 0
  def self.clear; end

  # Deletes the environment variable with +name+ if it exists and returns its value:
  #   ENV['foo'] = '0'
  #   ENV.delete('foo') # => '0'
  # Returns +nil+ if the named environment variable does not exist:
  #   ENV.delete('foo') # => nil
  # If a block given and the environment variable does not exist,
  # yields +name+ to the block and returns +nil+:
  #   ENV.delete('foo') { |name| puts name } # => nil
  #   foo
  # If a block given and the environment variable exists,
  # deletes the environment variable and returns its value (ignoring the block):
  #   ENV['foo'] = '0'
  #   ENV.delete('foo') { |name| fail 'ignored' } # => "0"
  # Raises an exception if +name+ is invalid.
  # See {Invalid Names and Values}[#class-ENV-label-Invalid-Names+and+Values].
  def self.delete(name) end

  # Deletes every environment variable for which the block evaluates to +true+.
  #
  # If no block is given an enumerator is returned instead.
  def self.delete_if; end

  # Yields each environment variable name and its value as a 2-element Array:
  #   h = {}
  #   ENV.each_pair { |name, value| h[name] = value } # => ENV
  #   h # => {"bar"=>"1", "foo"=>"0"}
  #
  # Returns an Enumerator if no block given:
  #   h = {}
  #   e = ENV.each_pair # => #<Enumerator: {"bar"=>"1", "foo"=>"0"}:each_pair>
  #   e.each { |name, value| h[name] = value } # => ENV
  #   h # => {"bar"=>"1", "foo"=>"0"}
  def self.each(...) end

  # Yields each environment variable name:
  #   ENV.replace('foo' => '0', 'bar' => '1') # => ENV
  #   names = []
  #   ENV.each_key { |name| names.push(name) } # => ENV
  #   names # => ["bar", "foo"]
  #
  # Returns an Enumerator if no block given:
  #   e = ENV.each_key # => #<Enumerator: {"bar"=>"1", "foo"=>"0"}:each_key>
  #   names = []
  #   e.each { |name| names.push(name) } # => ENV
  #   names # => ["bar", "foo"]
  def self.each_key; end

  # Yields each environment variable name and its value as a 2-element Array:
  #   h = {}
  #   ENV.each_pair { |name, value| h[name] = value } # => ENV
  #   h # => {"bar"=>"1", "foo"=>"0"}
  #
  # Returns an Enumerator if no block given:
  #   h = {}
  #   e = ENV.each_pair # => #<Enumerator: {"bar"=>"1", "foo"=>"0"}:each_pair>
  #   e.each { |name, value| h[name] = value } # => ENV
  #   h # => {"bar"=>"1", "foo"=>"0"}
  def self.each_pair; end

  # Yields each environment variable value:
  #   ENV.replace('foo' => '0', 'bar' => '1') # => ENV
  #   values = []
  #   ENV.each_value { |value| values.push(value) } # => ENV
  #   values # => ["1", "0"]
  #
  # Returns an Enumerator if no block given:
  #   e = ENV.each_value # => #<Enumerator: {"bar"=>"1", "foo"=>"0"}:each_value>
  #   values = []
  #   e.each { |value| values.push(value) } # => ENV
  #   values # => ["1", "0"]
  def self.each_value; end

  # Returns true when there are no environment variables
  def self.empty?; end

  # If +name+ is the name of an environment variable, returns its value:
  #   ENV['foo'] = '0'
  #   ENV.fetch('foo') # => '0'
  # Otherwise if a block is given (but not a default value),
  # yields +name+ to the block and returns the block's return value:
  #   ENV.fetch('foo') { |name| :need_not_return_a_string } # => :need_not_return_a_string
  # Otherwise if a default value is given (but not a block), returns the default value:
  #   ENV.delete('foo')
  #   ENV.fetch('foo', :default_need_not_be_a_string) # => :default_need_not_be_a_string
  # If the environment variable does not exist and both default and block are given,
  # issues a warning ("warning: block supersedes default value argument"),
  # yields +name+ to the block, and returns the block's return value:
  #   ENV.fetch('foo', :default) { |name| :block_return } # => :block_return
  # Raises KeyError if +name+ is valid, but not found,
  # and neither default value nor block is given:
  #   ENV.fetch('foo') # Raises KeyError (key not found: "foo")
  # Raises an exception if +name+ is invalid.
  # See {Invalid Names and Values}[#class-ENV-label-Invalid-Names+and+Values].
  def self.fetch(...) end

  # Returns a copy of the environment for entries where the block returns true.
  #
  # Returns an Enumerator if no block was given.
  #
  # ENV.filter is an alias for ENV.select.
  def self.filter; end

  # Equivalent to ENV.keep_if but returns +nil+ if no changes were made.
  #
  # ENV.filter! is an alias for ENV.select!.
  def self.filter!; end

  # Ruby does not allow ENV to be frozen, so calling ENV.freeze
  # raises TypeError.
  def self.freeze; end

  # ENV.has_key?, ENV.member?, and ENV.key? are aliases for ENV.include?.
  #
  # Returns +true+ if there is an environment variable with the given +name+:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.include?('foo') # => true
  # Returns +false+ if +name+ is a valid String and there is no such environment variable:
  #   ENV.include?('baz') # => false
  # Returns +false+ if +name+ is the empty String or is a String containing character <code>'='</code>:
  #   ENV.include?('') # => false
  #   ENV.include?('=') # => false
  # Raises an exception if +name+ is a String containing the NUL character <code>"\0"</code>:
  #   ENV.include?("\0") # Raises ArgumentError (bad environment variable name: contains null byte)
  # Raises an exception if +name+ has an encoding that is not ASCII-compatible:
  #   ENV.include?("\xa1\xa1".force_encoding(Encoding::UTF_16LE))
  #   # Raises ArgumentError (bad environment variable name: ASCII incompatible encoding: UTF-16LE)
  # Raises an exception if +name+ is not a String:
  #   ENV.include?(Object.new) # TypeError (no implicit conversion of Object into String)
  def self.has_key?(name) end

  # Returns +true+ if there is an environment variable with the given +value+.
  def self.has_value?(value) end

  # ENV.has_key?, ENV.member?, and ENV.key? are aliases for ENV.include?.
  #
  # Returns +true+ if there is an environment variable with the given +name+:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.include?('foo') # => true
  # Returns +false+ if +name+ is a valid String and there is no such environment variable:
  #   ENV.include?('baz') # => false
  # Returns +false+ if +name+ is the empty String or is a String containing character <code>'='</code>:
  #   ENV.include?('') # => false
  #   ENV.include?('=') # => false
  # Raises an exception if +name+ is a String containing the NUL character <code>"\0"</code>:
  #   ENV.include?("\0") # Raises ArgumentError (bad environment variable name: contains null byte)
  # Raises an exception if +name+ has an encoding that is not ASCII-compatible:
  #   ENV.include?("\xa1\xa1".force_encoding(Encoding::UTF_16LE))
  #   # Raises ArgumentError (bad environment variable name: ASCII incompatible encoding: UTF-16LE)
  # Raises an exception if +name+ is not a String:
  #   ENV.include?(Object.new) # TypeError (no implicit conversion of Object into String)
  def self.include?(name) end

  # Deprecated method that is equivalent to ENV.key
  def self.index(value) end

  # Returns the contents of the environment as a String.
  def self.inspect; end

  # Returns a new hash created by using environment variable names as values
  # and values as names.
  def self.invert; end

  # Deletes every environment variable where the block evaluates to +false+.
  #
  # Returns an enumerator if no block was given.
  def self.keep_if; end

  # Returns the name of the first environment variable with +value+ if it exists:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.key('0') # =>'foo'
  # The order in which environment variables are examined is OS-dependent.
  # See {About Ordering}[#class-ENV-label-About+Ordering].
  #
  # Returns +nil+ if there is no such value:
  #   ENV.key('2') # => nil
  # Raises an exception if +value+ is not a String:
  #   ENV.key(Object.new) # raises TypeError (no implicit conversion of Object into String)
  def self.key(value) end

  # ENV.has_key?, ENV.member?, and ENV.key? are aliases for ENV.include?.
  #
  # Returns +true+ if there is an environment variable with the given +name+:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.include?('foo') # => true
  # Returns +false+ if +name+ is a valid String and there is no such environment variable:
  #   ENV.include?('baz') # => false
  # Returns +false+ if +name+ is the empty String or is a String containing character <code>'='</code>:
  #   ENV.include?('') # => false
  #   ENV.include?('=') # => false
  # Raises an exception if +name+ is a String containing the NUL character <code>"\0"</code>:
  #   ENV.include?("\0") # Raises ArgumentError (bad environment variable name: contains null byte)
  # Raises an exception if +name+ has an encoding that is not ASCII-compatible:
  #   ENV.include?("\xa1\xa1".force_encoding(Encoding::UTF_16LE))
  #   # Raises ArgumentError (bad environment variable name: ASCII incompatible encoding: UTF-16LE)
  # Raises an exception if +name+ is not a String:
  #   ENV.include?(Object.new) # TypeError (no implicit conversion of Object into String)
  def self.key?(name) end

  # Returns all variable names in an Array:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.keys # => ['bar', 'foo']
  # The order of the names is OS-dependent.
  # See {About Ordering}[#class-ENV-label-About+Ordering].
  #
  # Returns the empty Array if ENV is empty:
  #   ENV.clear
  #   ENV.keys # => []
  def self.keys; end

  # Returns the number of environment variables.
  def self.length; end

  # ENV.has_key?, ENV.member?, and ENV.key? are aliases for ENV.include?.
  #
  # Returns +true+ if there is an environment variable with the given +name+:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.include?('foo') # => true
  # Returns +false+ if +name+ is a valid String and there is no such environment variable:
  #   ENV.include?('baz') # => false
  # Returns +false+ if +name+ is the empty String or is a String containing character <code>'='</code>:
  #   ENV.include?('') # => false
  #   ENV.include?('=') # => false
  # Raises an exception if +name+ is a String containing the NUL character <code>"\0"</code>:
  #   ENV.include?("\0") # Raises ArgumentError (bad environment variable name: contains null byte)
  # Raises an exception if +name+ has an encoding that is not ASCII-compatible:
  #   ENV.include?("\xa1\xa1".force_encoding(Encoding::UTF_16LE))
  #   # Raises ArgumentError (bad environment variable name: ASCII incompatible encoding: UTF-16LE)
  # Raises an exception if +name+ is not a String:
  #   ENV.include?(Object.new) # TypeError (no implicit conversion of Object into String)
  def self.member?(name) end

  # Adds the contents of +hash+ to the environment variables.  If no block is
  # specified entries with duplicate keys are overwritten, otherwise the value
  # of each duplicate name is determined by calling the block with the key, its
  # value from the environment and its value from the hash.
  def self.merge!(hash) end

  # Returns an Array of the name and value of the environment variable with
  # +value+ or +nil+ if the value cannot be found.
  def self.rassoc(value) end

  # Re-hashing the environment variables does nothing.  It is provided for
  # compatibility with Hash.
  def self.rehash; end

  # Same as ENV.delete_if, but works on (and returns) a copy of the
  # environment.
  def self.reject; end

  # Similar to ENV.delete_if, but returns +nil+ if no changes were made.
  #
  # Deletes each environment variable for which the block returns a truthy value,
  # returning ENV (if any deletions) or +nil+ (if not):
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.reject! { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"foo"=>"0"}
  #   ENV.reject! { |name, value| name.start_with?('b') } # => nil
  #
  # Returns an Enumerator if no block given:
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   e = ENV.reject! # => #<Enumerator: {"bar"=>"1", "baz"=>"2", "foo"=>"0"}:reject!>
  #   e.each { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"foo"=>"0"}
  #   e.each { |name, value| name.start_with?('b') } # => nil
  def self.reject!; end

  # Replaces the entire content of the environment variables
  # with the name/value pairs in the given +hash+;
  # returns ENV.
  #
  # Replaces the content of ENV with the given pairs:
  #   ENV.replace('foo' => '0', 'bar' => '1') # => ENV
  #   ENV.to_hash # => {"bar"=>"1", "foo"=>"0"}
  #
  # Raises an exception if a name or value is invalid.
  # See {Invalid Names and Values}[#class-ENV-label-Invalid-Names+and+Values].
  def self.replace(hash) end

  # Returns a copy of the environment for entries where the block returns true.
  #
  # Returns an Enumerator if no block was given.
  #
  # ENV.filter is an alias for ENV.select.
  def self.select; end

  # Equivalent to ENV.keep_if but returns +nil+ if no changes were made.
  #
  # ENV.filter! is an alias for ENV.select!.
  def self.select!; end

  # Removes the first environment variable from ENV and returns
  # a 2-element Array containing its name and value:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.to_hash # => {'bar' => '1', 'foo' => '0'}
  #   ENV.shift # => ['bar', '1']
  #   ENV.to_hash # => {'foo' => '0'}
  # Exactly which environment variable is "first" is OS-dependent.
  # See {About Ordering}[#class-ENV-label-About+Ordering].
  #
  # Returns +nil+ if the environment is empty:
  #   ENV.clear
  #   ENV.shift # => nil
  def self.shift; end

  # Returns the number of environment variables.
  def self.size; end

  # Returns a hash containing only the given keys from ENV and their values.
  #
  #    ENV.slice("TERM","HOME")  #=> {"TERM"=>"xterm-256color", "HOME"=>"/Users/rhc"}
  def self.slice(*keys) end

  # ENV.store is an alias for ENV.[]=.
  #
  # Creates, updates, or deletes the named environment variable, returning the value.
  # Both +name+ and +value+ may be instances of String.
  # See {Valid Names and Values}[#class-ENV-label-Valid+Names+and+Values].
  #
  # - If the named environment variable does not exist:
  #   - If +value+ is +nil+, does nothing.
  #       ENV.clear
  #       ENV['foo'] = nil # => nil
  #       ENV.include?('foo') # => false
  #       ENV.store('bar', nil) # => nil
  #       ENV.include?('bar') # => false
  #   - If +value+ is not +nil+, creates the environment variable with +name+ and +value+:
  #       # Create 'foo' using ENV.[]=.
  #       ENV['foo'] = '0' # => '0'
  #       ENV['foo'] # => '0'
  #       # Create 'bar' using ENV.store.
  #       ENV.store('bar', '1') # => '1'
  #       ENV['bar'] # => '1'
  # - If the named environment variable exists:
  #   - If +value+ is not +nil+, updates the environment variable with value +value+:
  #       # Update 'foo' using ENV.[]=.
  #       ENV['foo'] = '2' # => '2'
  #       ENV['foo'] # => '2'
  #       # Update 'bar' using ENV.store.
  #       ENV.store('bar', '3') # => '3'
  #       ENV['bar'] # => '3'
  #   - If +value+ is +nil+, deletes the environment variable:
  #       # Delete 'foo' using ENV.[]=.
  #       ENV['foo'] = nil # => nil
  #       ENV.include?('foo') # => false
  #       # Delete 'bar' using ENV.store.
  #       ENV.store('bar', nil) # => nil
  #       ENV.include?('bar') # => false
  #
  # Raises an exception if +name+ or +value+ is invalid.
  # See {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values].
  def self.store(name, value) end

  # Converts the environment variables into an array of names and value arrays.
  #
  #   ENV.to_a # => [["TERM", "xterm-color"], ["SHELL", "/bin/bash"], ...]
  def self.to_a; end

  # Creates a hash with a copy of the environment variables.
  def self.to_h; end

  # Creates a hash with a copy of the environment variables.
  def self.to_hash; end

  # Returns "ENV"
  def self.to_s; end

  # Adds the contents of +hash+ to the environment variables.  If no block is
  # specified entries with duplicate keys are overwritten, otherwise the value
  # of each duplicate name is determined by calling the block with the key, its
  # value from the environment and its value from the hash.
  def self.update(hash) end

  # Returns +true+ if there is an environment variable with the given +value+.
  def self.value?(value) end

  # Returns all environment variable values in an Array:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.values # => ['1', '0']
  # The order of the values is OS-dependent.
  # See {About Ordering}[#class-ENV-label-About+Ordering].
  #
  # Returns the empty Array if ENV is empty:
  #   ENV.clear
  #   ENV.values # => []
  def self.values; end

  # Returns an array containing the environment variable values associated with
  # the given names.  See also ENV.select.
  def self.values_at(name, *args) end
end
