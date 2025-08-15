# frozen_string_literal: true

# == Summary
#
# Ruby extension for GNU dbm (gdbm) -- a simple database engine for storing
# key-value pairs on disk.
#
# == Description
#
# GNU dbm is a library for simple databases. A database is a file that stores
# key-value pairs. Gdbm allows the user to store, retrieve, and delete data by
# key. It furthermore allows a non-sorted traversal of all key-value pairs.
# A gdbm database thus provides the same functionality as a hash. As
# with objects of the Hash class, elements can be accessed with <tt>[]</tt>.
# Furthermore, GDBM mixes in the Enumerable module, thus providing convenient
# methods such as #find, #collect, #map, etc.
#
# A process is allowed to open several different databases at the same time.
# A process can open a database as a "reader" or a "writer". Whereas a reader
# has only read-access to the database, a writer has read- and write-access.
# A database can be accessed either by any number of readers or by exactly one
# writer at the same time.
#
# == Examples
#
# 1. Opening/creating a database, and filling it with some entries:
#
#      require 'gdbm'
#
#      gdbm = GDBM.new("fruitstore.db")
#      gdbm["ananas"]    = "3"
#      gdbm["banana"]    = "8"
#      gdbm["cranberry"] = "4909"
#      gdbm.close
#
# 2. Reading out a database:
#
#      require 'gdbm'
#
#      gdbm = GDBM.new("fruitstore.db")
#      gdbm.each_pair do |key, value|
#        print "#{key}: #{value}\n"
#      end
#      gdbm.close
#
#    produces
#
#      banana: 8
#      ananas: 3
#      cranberry: 4909
#
# == Links
#
# * http://www.gnu.org/software/gdbm/
class GDBM
  include Enumerable

  # flag for #new and #open. this flag is obsolete for gdbm >= 1.8
  FAST = _
  # open database as a writer; overwrite any existing databases
  NEWDB = _
  # flag for #new and #open
  NOLOCK = _
  # open database as a reader
  READER = _
  # flag for #new and #open. only for gdbm >= 1.8
  SYNC = _
  # version of the gdbm library
  VERSION = _
  # open database as a writer; if the database does not exist, create a new one
  WRCREAT = _
  # open database as a writer
  WRITER = _

  # If called without a block, this is synonymous to GDBM::new.
  # If a block is given, the new GDBM instance will be passed to the block
  # as a parameter, and the corresponding database file will be closed
  # after the execution of the block code has been finished.
  #
  # Example for an open call with a block:
  #
  #   require 'gdbm'
  #   GDBM.open("fruitstore.db") do |gdbm|
  #     gdbm.each_pair do |key, value|
  #       print "#{key}: #{value}\n"
  #     end
  #   end
  def self.open(filename, mode = 0o666, flags = nil) end

  # Creates a new GDBM instance by opening a gdbm file named _filename_.
  # If the file does not exist, a new file with file mode _mode_ will be
  # created. _flags_ may be one of the following:
  # * *READER*  - open as a reader
  # * *WRITER*  - open as a writer
  # * *WRCREAT* - open as a writer; if the database does not exist, create a new one
  # * *NEWDB*   - open as a writer; overwrite any existing databases
  #
  # The values *WRITER*, *WRCREAT* and *NEWDB* may be combined with the following
  # values by bitwise or:
  # * *SYNC*    - cause all database operations to be synchronized to the disk
  # * *NOLOCK*  - do not lock the database file
  #
  # If no _flags_ are specified, the GDBM object will try to open the database
  # file as a writer and will create it if it does not already exist
  # (cf. flag <tt>WRCREAT</tt>). If this fails (for instance, if another process
  # has already opened the database as a reader), it will try to open the
  # database file as a reader (cf. flag <tt>READER</tt>).
  def initialize(filename, mode = 0o666, flags = nil) end

  # Retrieves the _value_ corresponding to _key_.
  def [](key) end

  # Associates the value _value_ with the specified _key_.
  def []=(key, value) end
  alias store []=

  # Sets the size of the internal bucket cache to _size_.
  def cachesize=(size) end

  # Removes all the key-value pairs within _gdbm_.
  def clear; end

  # Closes the associated database file.
  def close; end

  # Returns true if the associated database file has been closed.
  def closed?; end

  # Removes the key-value-pair with the specified _key_ from this database and
  # returns the corresponding _value_. Returns nil if the database is empty.
  def delete(key) end

  # Deletes every key-value pair from _gdbm_ for which _block_ evaluates to true.
  def delete_if; end
  alias reject! delete_if

  # Executes _block_ for each key in the database, passing the _key_ and the
  # correspoding _value_ as a parameter.
  def each; end
  alias each_pair each

  # Executes _block_ for each key in the database, passing the
  # _key_ as a parameter.
  def each_key; end

  # Executes _block_ for each key in the database, passing the corresponding
  # _value_ as a parameter.
  def each_value; end

  # Returns true if the database is empty.
  def empty?; end

  # Turns the database's fast mode on or off. If fast mode is turned on, gdbm
  # does not wait for writes to be flushed to the disk before continuing.
  #
  # This option is obsolete for gdbm >= 1.8 since fast mode is turned on by
  # default. See also: #syncmode=
  def fastmode=(boolean) end

  # Retrieves the _value_ corresponding to _key_. If there is no value
  # associated with _key_, _default_ will be returned instead.
  def fetch(p1, p2 = v2) end

  # Returns true if the given value _v_ exists within the database.
  # Returns false otherwise.
  def has_value?(v) end
  alias value? has_value?

  # Returns true if the given key _k_ exists within the database.
  # Returns false otherwise.
  def include?(p1) end
  alias has_key? include?
  alias member? include?
  alias key? include?

  # Returns the _key_ for a given _value_. If several keys may map to the
  # same value, the key that is found first will be returned.
  def index(value) end

  def indexes(*args) end
  alias indices indexes

  # Returns a hash created by using _gdbm_'s values as keys, and the keys
  # as values.
  def invert; end

  # Returns an array of all keys of this database.
  def keys; end

  # Returns the number of key-value pairs in this database.
  def length; end
  alias size length

  # Returns a hash copy of _gdbm_ where all key-value pairs from _gdbm_ for
  # which _block_ evaluates to true are removed. See also: #delete_if
  def reject; end

  # Reorganizes the database file. This operation removes reserved space of
  # elements that have already been deleted. It is only useful after a lot of
  # deletions in the database.
  def reorganize; end

  # Replaces the content of _gdbm_ with the key-value pairs of _other_.
  # _other_ must have an each_pair method.
  def replace(other) end

  # Returns a new array of all values of the database for which _block_
  # evaluates to true.
  def select; end

  # Removes a key-value-pair from this database and returns it as a
  # two-item array [ _key_, _value_ ]. Returns nil if the database is empty.
  def shift; end

  # Unless the _gdbm_ object has been opened with the *SYNC* flag, it is not
  # guarenteed that database modification operations are immediately applied to
  # the database file. This method ensures that all recent modifications
  # to the database are written to the file. Blocks until all writing operations
  # to the disk have been finished.
  def sync; end

  # Turns the database's synchronization mode on or off. If the synchronization
  # mode is turned on, the database's in-memory state will be synchronized to
  # disk after every database modification operation. If the synchronization
  # mode is turned off, GDBM does not wait for writes to be flushed to the disk
  # before continuing.
  #
  # This option is only available for gdbm >= 1.8 where syncmode is turned off
  # by default. See also: #fastmode=
  def syncmode=(boolean) end

  # Returns an array of all key-value pairs contained in the database.
  def to_a; end

  # Returns a hash of all key-value pairs contained in the database.
  def to_hash; end

  # Adds the key-value pairs of _other_ to _gdbm_, overwriting entries with
  # duplicate keys with those from _other_. _other_ must have an each_pair
  # method.
  def update(other) end

  # Returns an array of all values of this database.
  def values; end

  # Returns an array of the values associated with each specified _key_.
  def values_at(key, *args) end
end
