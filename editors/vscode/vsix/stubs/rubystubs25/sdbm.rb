# frozen_string_literal: true

# SDBM provides a simple file-based key-value store, which can only store
# String keys and values.
#
# Note that Ruby comes with the source code for SDBM, while the DBM and GDBM
# standard libraries rely on external libraries and headers.
#
# === Examples
#
# Insert values:
#
#   require 'sdbm'
#
#   SDBM.open 'my_database' do |db|
#     db['apple'] = 'fruit'
#     db['pear'] = 'fruit'
#     db['carrot'] = 'vegetable'
#     db['tomato'] = 'vegetable'
#   end
#
# Bulk update:
#
#   require 'sdbm'
#
#   SDBM.open 'my_database' do |db|
#     db.update('peach' => 'fruit', 'tomato' => 'fruit')
#   end
#
# Retrieve values:
#
#   require 'sdbm'
#
#   SDBM.open 'my_database' do |db|
#     db.each do |key, value|
#       puts "Key: #{key}, Value: #{value}"
#     end
#   end
#
# Outputs:
#
#   Key: apple, Value: fruit
#   Key: pear, Value: fruit
#   Key: carrot, Value: vegetable
#   Key: peach, Value: fruit
#   Key: tomato, Value: fruit
class SDBM
  include Enumerable

  # If called without a block, this is the same as SDBM.new.
  #
  # If a block is given, the new database will be passed to the block and
  # will be safely closed after the block has executed.
  #
  # Example:
  #
  #     require 'sdbm'
  #
  #     SDBM.open('my_database') do |db|
  #       db['hello'] = 'world'
  #     end
  def self.open(filename, mode = 0o666) end

  # Creates a new database handle by opening the given +filename+. SDBM actually
  # uses two physical files, with extensions '.dir' and '.pag'. These extensions
  # will automatically be appended to the +filename+.
  #
  # If the file does not exist, a new file will be created using the given
  # +mode+, unless +mode+ is explicitly set to nil. In the latter case, no
  # database will be created.
  #
  # If the file exists, it will be opened in read/write mode. If this fails, it
  # will be opened in read-only mode.
  def initialize(filename, mode = 0o666) end

  # Returns the +value+ in the database associated with the given +key+ string.
  #
  # If no value is found, returns +nil+.
  def [](key) end

  # Stores a new +value+ in the database with the given +key+ as an index.
  #
  # If the +key+ already exists, this will update the +value+ associated with
  # the +key+.
  #
  # Returns the given +value+.
  def []=(key, value) end
  alias store []=

  # Deletes all data from the database.
  def clear; end

  # Closes the database file.
  #
  # Raises SDBMError if the database is already closed.
  def close; end

  # Returns +true+ if the database is closed.
  def closed?; end

  # Deletes the key-value pair corresponding to the given +key+. If the
  # +key+ exists, the deleted value will be returned, otherwise +nil+.
  #
  # If a block is provided, the deleted +key+ and +value+ will be passed to
  # the block as arguments. If the +key+ does not exist in the database, the
  # value will be +nil+.
  def delete(key) end

  # Iterates over the key-value pairs in the database, deleting those for
  # which the block returns +true+.
  def delete_if; end
  alias reject! delete_if

  # Iterates over each key-value pair in the database.
  #
  # If no block is given, returns an Enumerator.
  def each(*several_variants) end
  alias each_pair each

  # Iterates over each +key+ in the database.
  #
  # If no block is given, returns an Enumerator.
  def each_key; end

  # Iterates over each +value+ in the database.
  #
  # If no block is given, returns an Enumerator.
  def each_value; end

  # Returns +true+ if the database is empty.
  def empty?; end

  # Returns the +value+ in the database associated with the given +key+ string.
  #
  # If a block is provided, the block will be called when there is no
  # +value+ associated with the given +key+. The +key+ will be passed in as an
  # argument to the block.
  #
  # If no block is provided and no value is associated with the given +key+,
  # then an IndexError will be raised.
  def fetch(key) end

  # Returns +true+ if the database contains the given +key+.
  def has_key?(key) end
  alias include? has_key?
  alias key? has_key?
  alias member? has_key?

  # Returns +true+ if the database contains the given +value+.
  def has_value?(key) end
  alias value? has_value?

  # Returns a Hash in which the key-value pairs have been inverted.
  #
  # Example:
  #
  #   require 'sdbm'
  #
  #   SDBM.open 'my_database' do |db|
  #     db.update('apple' => 'fruit', 'spinach' => 'vegetable')
  #
  #     db.invert  #=> {"fruit" => "apple", "vegetable" => "spinach"}
  #   end
  def invert; end

  # Returns the +key+ associated with the given +value+. If more than one
  # +key+ corresponds to the given +value+, then the first key to be found
  # will be returned. If no keys are found, +nil+ will be returned.
  def key(value) end

  # Returns a new Array containing the keys in the database.
  def keys; end

  # Returns the number of keys in the database.
  def length; end
  alias size length

  # Creates a new Hash using the key-value pairs from the database, then
  # calls Hash#reject with the given block, which returns a Hash with
  # only the key-value pairs for which the block returns +false+.
  def reject; end

  # Empties the database, then inserts the given key-value pairs.
  #
  # This method will work with any object which implements an each_pair
  # method, such as a Hash.
  def replace(pairs) end

  # Returns a new Array of key-value pairs for which the block returns +true+.
  #
  # Example:
  #
  #    require 'sdbm'
  #
  #    SDBM.open 'my_database' do |db|
  #      db['apple'] = 'fruit'
  #      db['pear'] = 'fruit'
  #      db['spinach'] = 'vegetable'
  #
  #      veggies = db.select do |key, value|
  #        value == 'vegetable'
  #      end #=> [["apple", "fruit"], ["pear", "fruit"]]
  #    end
  def select; end

  # Removes a key-value pair from the database and returns them as an
  # Array. If the database is empty, returns +nil+.
  def shift; end

  # Returns a new Array containing each key-value pair in the database.
  #
  # Example:
  #
  #   require 'sdbm'
  #
  #   SDBM.open 'my_database' do |db|
  #     db.update('apple' => 'fruit', 'spinach' => 'vegetable')
  #
  #     db.to_a  #=> [["apple", "fruit"], ["spinach", "vegetable"]]
  #   end
  def to_a; end

  # Returns a new Hash containing each key-value pair in the database.
  def to_hash; end

  # Insert or update key-value pairs.
  #
  # This method will work with any object which implements an each_pair
  # method, such as a Hash.
  def update(pairs) end

  # Returns a new Array containing the values in the database.
  def values; end

  # Returns an Array of values corresponding to the given keys.
  def values_at(key, *args) end
end
