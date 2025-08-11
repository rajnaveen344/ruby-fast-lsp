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
#
# == What's Here
#
# First, what's elsewhere. \Class \ENV:
#
# - Inherits from {class Object}[Object.html#class-Object-label-What-27s+Here].
# - Extends {module Enumerable}[Enumerable.html#module-Enumerable-label-What-27s+Here],
#
# Here, class \ENV provides methods that are useful for:
#
# - {Querying}[#class-ENV-label-Methods+for+Querying]
# - {Assigning}[#class-ENV-label-Methods+for+Assigning]
# - {Deleting}[#class-ENV-label-Methods+for+Deleting]
# - {Iterating}[#class-ENV-label-Methods+for+Iterating]
# - {Converting}[#class-ENV-label-Methods+for+Converting]
# - {And more ....}[#class-ENV-label-More+Methods]
#
# === Methods for Querying
#
# - ::[]:: Returns the value for the given environment variable name if it exists:
# - ::empty?:: Returns whether \ENV is empty.
# - ::has_value?, ::value?:: Returns whether the given value is in \ENV.
# - ::include?, ::has_key?, ::key?, ::member?:: Returns whether the given name
#                                               is in \ENV.
# - ::key:: Returns the name of the first entry with the given value.
# - ::size, ::length:: Returns the number of entries.
# - ::value?:: Returns whether any entry has the given value.
#
# === Methods for Assigning
#
# - ::[]=, ::store:: Creates, updates, or deletes the named environment variable.
# - ::clear:: Removes every environment variable; returns \ENV:
# - ::update, ::merge!:: Adds to \ENV each key/value pair in the given hash.
# - ::replace:: Replaces the entire content of the \ENV
#               with the name/value pairs in the given hash.
#
# === Methods for Deleting
#
# - ::delete:: Deletes the named environment variable name if it exists.
# - ::delete_if:: Deletes entries selected by the block.
# - ::keep_if:: Deletes entries not selected by the block.
# - ::reject!:: Similar to #delete_if, but returns +nil+ if no change was made.
# - ::select!, ::filter!:: Deletes entries selected by the block.
# - ::shift:: Removes and returns the first entry.
#
# === Methods for Iterating
#
# - ::each, ::each_pair:: Calls the block with each name/value pair.
# - ::each_key:: Calls the block with each name.
# - ::each_value:: Calls the block with each value.
#
# === Methods for Converting
#
# - ::assoc:: Returns a 2-element array containing the name and value
#             of the named environment variable if it exists:
# - ::clone:: Returns \ENV (and issues a warning).
# - ::except:: Returns a hash of all name/value pairs except those given.
# - ::fetch:: Returns the value for the given name.
# - ::inspect:: Returns the contents of \ENV as a string.
# - ::invert:: Returns a hash whose keys are the ENV values,
#              and whose values are the corresponding ENV names.
# - ::keys:: Returns an array of all names.
# - ::rassoc:: Returns the name and value of the first found entry
#              that has the given value.
# - ::reject:: Returns a hash of those entries not rejected by the block.
# - ::select, ::filter:: Returns a hash of name/value pairs selected by the block.
# - ::slice:: Returns a hash of the given names and their corresponding values.
# - ::to_a:: Returns the entries as an array of 2-element Arrays.
# - ::to_h:: Returns a hash of entries selected by the block.
# - ::to_hash:: Returns a hash of all entries.
# - ::to_s:: Returns the string <tt>'ENV'</tt>.
# - ::values:: Returns all values as an array.
# - ::values_at:: Returns an array of the values for the given name.
#
# === More Methods
#
# - ::dup:: Raises an exception.
# - ::freeze:: Raises an exception.
# - ::rehash:: Returns +nil+, without modifying \ENV.
class ENV
  # Returns the value for the environment variable +name+ if it exists:
  #   ENV['foo'] = '0'
  #   ENV['foo'] # => "0"
  # Returns +nil+ if the named variable does not exist.
  #
  # Raises an exception if +name+ is invalid.
  # See {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values].
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
  #   ENV.assoc('foo') # => ['foo', '0']
  # Returns +nil+ if +name+ is a valid String and there is no such environment variable.
  #
  # Returns +nil+ if +name+ is the empty String or is a String containing character <code>'='</code>.
  #
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

  # Returns ENV itself, and warns because ENV is a wrapper for the
  # process-wide environment variables and a clone is useless.
  # If +freeze+ keyword is given and not +nil+ or +false+, raises ArgumentError.
  # If +freeze+ keyword is given and +true+, raises TypeError, as ENV storage
  # cannot be frozen.
  def self.clone(freeze: nil) end

  # Deletes the environment variable with +name+ if it exists and returns its value:
  #   ENV['foo'] = '0'
  #   ENV.delete('foo') # => '0'
  #
  # If a block is not given and the named environment variable does not exist, returns +nil+.
  #
  # If a block given and the environment variable does not exist,
  # yields +name+ to the block and returns the value of the block:
  #   ENV.delete('foo') { |name| name * 2 } # => "foofoo"
  #
  # If a block given and the environment variable exists,
  # deletes the environment variable and returns its value (ignoring the block):
  #   ENV['foo'] = '0'
  #   ENV.delete('foo') { |name| raise 'ignored' } # => "0"
  #
  # Raises an exception if +name+ is invalid.
  # See {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values].
  def self.delete(...) end

  # Yields each environment variable name and its value as a 2-element Array,
  # deleting each environment variable for which the block returns a truthy value,
  # and returning ENV (regardless of whether any deletions):
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.delete_if { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"foo"=>"0"}
  #   ENV.delete_if { |name, value| name.start_with?('b') } # => ENV
  #
  # Returns an Enumerator if no block given:
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   e = ENV.delete_if # => #<Enumerator: {"bar"=>"1", "baz"=>"2", "foo"=>"0"}:delete_if!>
  #   e.each { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"foo"=>"0"}
  #   e.each { |name, value| name.start_with?('b') } # => ENV
  def self.delete_if; end

  # Raises TypeError, because ENV is a singleton object.
  # Use #to_h to get a copy of ENV data as a hash.
  def self.dup; end

  # Yields each environment variable name and its value as a 2-element \Array:
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

  # Yields each environment variable name and its value as a 2-element \Array:
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

  # Returns +true+ when there are no environment variables, +false+ otherwise:
  #   ENV.clear
  #   ENV.empty? # => true
  #   ENV['foo'] = '0'
  #   ENV.empty? # => false
  def self.empty?; end

  # Returns a hash except the given keys from ENV and their values.
  #
  #    ENV                       #=> {"LANG"=>"en_US.UTF-8", "TERM"=>"xterm-256color", "HOME"=>"/Users/rhc"}
  #    ENV.except("TERM","HOME") #=> {"LANG"=>"en_US.UTF-8"}
  def self.except(*keys) end

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
  # See {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values].
  def self.fetch(...) end

  # ENV.filter is an alias for ENV.select.
  #
  # Yields each environment variable name and its value as a 2-element Array,
  # returning a Hash of the names and values for which the block returns a truthy value:
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.select { |name, value| name.start_with?('b') } # => {"bar"=>"1", "baz"=>"2"}
  #   ENV.filter { |name, value| name.start_with?('b') } # => {"bar"=>"1", "baz"=>"2"}
  #
  # Returns an Enumerator if no block given:
  #   e = ENV.select # => #<Enumerator: {"bar"=>"1", "baz"=>"2", "foo"=>"0"}:select>
  #   e.each { |name, value | name.start_with?('b') } # => {"bar"=>"1", "baz"=>"2"}
  #   e = ENV.filter # => #<Enumerator: {"bar"=>"1", "baz"=>"2", "foo"=>"0"}:filter>
  #   e.each { |name, value | name.start_with?('b') } # => {"bar"=>"1", "baz"=>"2"}
  def self.filter; end

  # ENV.filter! is an alias for ENV.select!.
  #
  # Yields each environment variable name and its value as a 2-element Array,
  # deleting each entry for which the block returns +false+ or +nil+,
  # and returning ENV if any deletions made, or +nil+ otherwise:
  #
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.select! { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  #   ENV.select! { |name, value| true } # => nil
  #
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.filter! { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  #   ENV.filter! { |name, value| true } # => nil
  #
  # Returns an Enumerator if no block given:
  #
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   e = ENV.select! # => #<Enumerator: {"bar"=>"1", "baz"=>"2"}:select!>
  #   e.each { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  #   e.each { |name, value| true } # => nil
  #
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   e = ENV.filter! # => #<Enumerator: {"bar"=>"1", "baz"=>"2"}:filter!>
  #   e.each { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  #   e.each { |name, value| true } # => nil
  def self.filter!; end

  # Raises an exception:
  #   ENV.freeze # Raises TypeError (cannot freeze ENV)
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

  # Returns +true+ if +value+ is the value for some environment variable name, +false+ otherwise:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.value?('0') # => true
  #   ENV.has_value?('0') # => true
  #   ENV.value?('2') # => false
  #   ENV.has_value?('2') # => false
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

  # Returns the contents of the environment as a String:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.inspect # => "{\"bar\"=>\"1\", \"foo\"=>\"0\"}"
  def self.inspect; end

  # Returns a Hash whose keys are the ENV values,
  # and whose values are the corresponding ENV names:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.invert # => {"1"=>"bar", "0"=>"foo"}
  # For a duplicate ENV value, overwrites the hash entry:
  #   ENV.replace('foo' => '0', 'bar' => '0')
  #   ENV.invert # => {"0"=>"foo"}
  # Note that the order of the ENV processing is OS-dependent,
  # which means that the order of overwriting is also OS-dependent.
  # See {About Ordering}[#class-ENV-label-About+Ordering].
  def self.invert; end

  # Yields each environment variable name and its value as a 2-element Array,
  # deleting each environment variable for which the block returns +false+ or +nil+,
  # and returning ENV:
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.keep_if { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  #
  # Returns an Enumerator if no block given:
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   e = ENV.keep_if # => #<Enumerator: {"bar"=>"1", "baz"=>"2", "foo"=>"0"}:keep_if>
  #   e.each { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  def self.keep_if; end

  # Returns the name of the first environment variable with +value+, if it exists:
  #   ENV.replace('foo' => '0', 'bar' => '0')
  #   ENV.key('0') # => "foo"
  # The order in which environment variables are examined is OS-dependent.
  # See {About Ordering}[#class-ENV-label-About+Ordering].
  #
  # Returns +nil+ if there is no such value.
  #
  # Raises an exception if +value+ is invalid:
  #   ENV.key(Object.new) # raises TypeError (no implicit conversion of Object into String)
  # See {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values].
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
  # Returns the empty Array if ENV is empty.
  def self.keys; end

  # Returns the count of environment variables:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.length # => 2
  #   ENV.size # => 2
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

  # ENV.update is an alias for ENV.merge!.
  #
  # Adds to ENV each key/value pair in the given +hash+; returns ENV:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.merge!('baz' => '2', 'bat' => '3') # => {"bar"=>"1", "bat"=>"3", "baz"=>"2", "foo"=>"0"}
  # Deletes the ENV entry for a hash value that is +nil+:
  #   ENV.merge!('baz' => nil, 'bat' => nil) # => {"bar"=>"1", "foo"=>"0"}
  # For an already-existing name, if no block given, overwrites the ENV value:
  #   ENV.merge!('foo' => '4') # => {"bar"=>"1", "foo"=>"4"}
  # For an already-existing name, if block given,
  # yields the name, its ENV value, and its hash value;
  # the block's return value becomes the new name:
  #   ENV.merge!('foo' => '5') { |name, env_val, hash_val | env_val + hash_val } # => {"bar"=>"1", "foo"=>"45"}
  # Raises an exception if a name or value is invalid
  # (see {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values]);
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.merge!('foo' => '6', :bar => '7', 'baz' => '9') # Raises TypeError (no implicit conversion of Symbol into String)
  #   ENV # => {"bar"=>"1", "foo"=>"6"}
  #   ENV.merge!('foo' => '7', 'bar' => 8, 'baz' => '9') # Raises TypeError (no implicit conversion of Integer into String)
  #   ENV # => {"bar"=>"1", "foo"=>"7"}
  # Raises an exception if the block returns an invalid name:
  # (see {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values]):
  #   ENV.merge!('bat' => '8', 'foo' => '9') { |name, env_val, hash_val | 10 } # Raises TypeError (no implicit conversion of Integer into String)
  #   ENV # => {"bar"=>"1", "bat"=>"8", "foo"=>"7"}
  #
  # Note that for the exceptions above,
  # hash pairs preceding an invalid name or value are processed normally;
  # those following are ignored.
  def self.merge!(hash) end

  # Returns a 2-element Array containing the name and value of the
  # *first* *found* environment variable that has value +value+, if one
  # exists:
  #   ENV.replace('foo' => '0', 'bar' => '0')
  #   ENV.rassoc('0') # => ["bar", "0"]
  # The order in which environment variables are examined is OS-dependent.
  # See {About Ordering}[#class-ENV-label-About+Ordering].
  #
  # Returns +nil+ if there is no such environment variable.
  def self.rassoc(value) end

  # (Provided for compatibility with Hash.)
  #
  # Does not modify ENV; returns +nil+.
  def self.rehash; end

  # Yields each environment variable name and its value as a 2-element Array.
  # Returns a Hash whose items are determined by the block.
  # When the block returns a truthy value, the name/value pair is added to the return Hash;
  # otherwise the pair is ignored:
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.reject { |name, value| name.start_with?('b') } # => {"foo"=>"0"}
  # Returns an Enumerator if no block given:
  #   e = ENV.reject
  #   e.each { |name, value| name.start_with?('b') } # => {"foo"=>"0"}
  def self.reject; end

  # Similar to ENV.delete_if, but returns +nil+ if no changes were made.
  #
  # Yields each environment variable name and its value as a 2-element Array,
  # deleting each environment variable for which the block returns a truthy value,
  # and returning ENV (if any deletions) or +nil+ (if not):
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
  # Raises an exception if a name or value is invalid
  # (see {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values]):
  #   ENV.replace('foo' => '0', :bar => '1') # Raises TypeError (no implicit conversion of Symbol into String)
  #   ENV.replace('foo' => '0', 'bar' => 1) # Raises TypeError (no implicit conversion of Integer into String)
  #   ENV.to_hash # => {"bar"=>"1", "foo"=>"0"}
  def self.replace(hash) end

  # ENV.filter is an alias for ENV.select.
  #
  # Yields each environment variable name and its value as a 2-element Array,
  # returning a Hash of the names and values for which the block returns a truthy value:
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.select { |name, value| name.start_with?('b') } # => {"bar"=>"1", "baz"=>"2"}
  #   ENV.filter { |name, value| name.start_with?('b') } # => {"bar"=>"1", "baz"=>"2"}
  #
  # Returns an Enumerator if no block given:
  #   e = ENV.select # => #<Enumerator: {"bar"=>"1", "baz"=>"2", "foo"=>"0"}:select>
  #   e.each { |name, value | name.start_with?('b') } # => {"bar"=>"1", "baz"=>"2"}
  #   e = ENV.filter # => #<Enumerator: {"bar"=>"1", "baz"=>"2", "foo"=>"0"}:filter>
  #   e.each { |name, value | name.start_with?('b') } # => {"bar"=>"1", "baz"=>"2"}
  def self.select; end

  # ENV.filter! is an alias for ENV.select!.
  #
  # Yields each environment variable name and its value as a 2-element Array,
  # deleting each entry for which the block returns +false+ or +nil+,
  # and returning ENV if any deletions made, or +nil+ otherwise:
  #
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.select! { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  #   ENV.select! { |name, value| true } # => nil
  #
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.filter! { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  #   ENV.filter! { |name, value| true } # => nil
  #
  # Returns an Enumerator if no block given:
  #
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   e = ENV.select! # => #<Enumerator: {"bar"=>"1", "baz"=>"2"}:select!>
  #   e.each { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  #   e.each { |name, value| true } # => nil
  #
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   e = ENV.filter! # => #<Enumerator: {"bar"=>"1", "baz"=>"2"}:filter!>
  #   e.each { |name, value| name.start_with?('b') } # => ENV
  #   ENV # => {"bar"=>"1", "baz"=>"2"}
  #   e.each { |name, value| true } # => nil
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
  # Returns +nil+ if the environment is empty.
  def self.shift; end

  # Returns the count of environment variables:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.length # => 2
  #   ENV.size # => 2
  def self.size; end

  # Returns a Hash of the given ENV names and their corresponding values:
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2', 'bat' => '3')
  #   ENV.slice('foo', 'baz') # => {"foo"=>"0", "baz"=>"2"}
  #   ENV.slice('baz', 'foo') # => {"baz"=>"2", "foo"=>"0"}
  # Raises an exception if any of the +names+ is invalid
  # (see {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values]):
  #   ENV.slice('foo', 'bar', :bat) # Raises TypeError (no implicit conversion of Symbol into String)
  def self.slice(*names) end

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

  # Returns the contents of ENV as an Array of 2-element Arrays,
  # each of which is a name/value pair:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.to_a # => [["bar", "1"], ["foo", "0"]]
  def self.to_a; end

  # With no block, returns a Hash containing all name/value pairs from ENV:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.to_h # => {"bar"=>"1", "foo"=>"0"}
  # With a block, returns a Hash whose items are determined by the block.
  # Each name/value pair in ENV is yielded to the block.
  # The block must return a 2-element Array (name/value pair)
  # that is added to the return Hash as a key and value:
  #   ENV.to_h { |name, value| [name.to_sym, value.to_i] } # => {:bar=>1, :foo=>0}
  # Raises an exception if the block does not return an Array:
  #   ENV.to_h { |name, value| name } # Raises TypeError (wrong element type String (expected array))
  # Raises an exception if the block returns an Array of the wrong size:
  #   ENV.to_h { |name, value| [name] } # Raises ArgumentError (element has wrong array length (expected 2, was 1))
  def self.to_h; end

  # Returns a Hash containing all name/value pairs from ENV:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.to_hash # => {"bar"=>"1", "foo"=>"0"}
  def self.to_hash; end

  # Returns String 'ENV':
  #   ENV.to_s # => "ENV"
  def self.to_s; end

  # ENV.update is an alias for ENV.merge!.
  #
  # Adds to ENV each key/value pair in the given +hash+; returns ENV:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.merge!('baz' => '2', 'bat' => '3') # => {"bar"=>"1", "bat"=>"3", "baz"=>"2", "foo"=>"0"}
  # Deletes the ENV entry for a hash value that is +nil+:
  #   ENV.merge!('baz' => nil, 'bat' => nil) # => {"bar"=>"1", "foo"=>"0"}
  # For an already-existing name, if no block given, overwrites the ENV value:
  #   ENV.merge!('foo' => '4') # => {"bar"=>"1", "foo"=>"4"}
  # For an already-existing name, if block given,
  # yields the name, its ENV value, and its hash value;
  # the block's return value becomes the new name:
  #   ENV.merge!('foo' => '5') { |name, env_val, hash_val | env_val + hash_val } # => {"bar"=>"1", "foo"=>"45"}
  # Raises an exception if a name or value is invalid
  # (see {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values]);
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.merge!('foo' => '6', :bar => '7', 'baz' => '9') # Raises TypeError (no implicit conversion of Symbol into String)
  #   ENV # => {"bar"=>"1", "foo"=>"6"}
  #   ENV.merge!('foo' => '7', 'bar' => 8, 'baz' => '9') # Raises TypeError (no implicit conversion of Integer into String)
  #   ENV # => {"bar"=>"1", "foo"=>"7"}
  # Raises an exception if the block returns an invalid name:
  # (see {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values]):
  #   ENV.merge!('bat' => '8', 'foo' => '9') { |name, env_val, hash_val | 10 } # Raises TypeError (no implicit conversion of Integer into String)
  #   ENV # => {"bar"=>"1", "bat"=>"8", "foo"=>"7"}
  #
  # Note that for the exceptions above,
  # hash pairs preceding an invalid name or value are processed normally;
  # those following are ignored.
  def self.update(hash) end

  # Returns +true+ if +value+ is the value for some environment variable name, +false+ otherwise:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.value?('0') # => true
  #   ENV.has_value?('0') # => true
  #   ENV.value?('2') # => false
  #   ENV.has_value?('2') # => false
  def self.value?(value) end

  # Returns all environment variable values in an Array:
  #   ENV.replace('foo' => '0', 'bar' => '1')
  #   ENV.values # => ['1', '0']
  # The order of the values is OS-dependent.
  # See {About Ordering}[#class-ENV-label-About+Ordering].
  #
  # Returns the empty Array if ENV is empty.
  def self.values; end

  # Returns an Array containing the environment variable values associated with
  # the given names:
  #   ENV.replace('foo' => '0', 'bar' => '1', 'baz' => '2')
  #   ENV.values_at('foo', 'baz') # => ["0", "2"]
  #
  # Returns +nil+ in the Array for each name that is not an ENV name:
  #   ENV.values_at('foo', 'bat', 'bar', 'bam') # => ["0", nil, "1", nil]
  #
  # Returns an empty \Array if no names given.
  #
  # Raises an exception if any name is invalid.
  # See {Invalid Names and Values}[#class-ENV-label-Invalid+Names+and+Values].
  def self.values_at(*names) end
end
