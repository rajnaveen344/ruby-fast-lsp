# frozen_string_literal: true

# \IO streams for strings, with access similar to
# {IO}[rdoc-ref:IO];
# see {IO}[rdoc-ref:IO].
#
# === About the Examples
#
# Examples on this page assume that \StringIO has been required:
#
#   require 'stringio'
class StringIO
  include Enumerable
  include IO.generic_readable
  include IO.generic_writable

  VERSION = _

  # Note that +mode+ defaults to <tt>'r'</tt> if +string+ is frozen.
  #
  # Creates a new \StringIO instance formed from +string+ and +mode+;
  # see {Access Modes}[rdoc-ref:File@Access+Modes].
  #
  # With no block, returns the new instance:
  #
  #   strio = StringIO.open # => #<StringIO>
  #
  # With a block, calls the block with the new instance
  # and returns the block's value;
  # closes the instance on block exit.
  #
  #   StringIO.open {|strio| p strio }
  #   # => #<StringIO>
  #
  # Related: StringIO.new.
  def self.open(string = '', mode = 'r+') end

  # Note that +mode+ defaults to <tt>'r'</tt> if +string+ is frozen.
  #
  # Returns a new \StringIO instance formed from +string+ and +mode+;
  # see {Access Modes}[rdoc-ref:File@Access+Modes]:
  #
  #   strio = StringIO.new # => #<StringIO>
  #   strio.close
  #
  # The instance should be closed when no longer needed.
  #
  # Related: StringIO.open (accepts block; closes automatically).
  def initialize(string = '', mode = 'r+') end

  # Sets the data mode in +self+ to binary mode;
  # see {Data Mode}[rdoc-ref:File@Data+Mode].
  def binmode; end

  # Closes +self+ for both reading and writing.
  #
  # Raises IOError if reading or writing is attempted.
  #
  # Related: StringIO#close_read, StringIO#close_write.
  def close; end

  # Closes +self+ for reading; closed-write setting remains unchanged.
  #
  # Raises IOError if reading is attempted.
  #
  # Related: StringIO#close, StringIO#close_write.
  def close_read; end

  # Closes +self+ for writing; closed-read setting remains unchanged.
  #
  # Raises IOError if writing is attempted.
  #
  # Related: StringIO#close, StringIO#close_read.
  def close_write; end

  # Returns +true+ if +self+ is closed for both reading and writing,
  # +false+ otherwise.
  def closed?; end

  # Returns +true+ if +self+ is closed for reading, +false+ otherwise.
  def closed_read?; end

  # Returns +true+ if +self+ is closed for writing, +false+ otherwise.
  def closed_write?; end

  # Calls the block with each remaining line read from the stream;
  # does nothing if already at end-of-file;
  # returns +self+.
  # See {Line IO}[rdoc-ref:IO@Line+IO].
  def each(*args) end
  alias each_line each

  # With a block given, calls the block with each remaining byte in the stream;
  # see {Byte IO}[rdoc-ref:IO@Byte+IO].
  #
  # With no block given, returns an enumerator.
  def each_byte; end

  # With a block given, calls the block with each remaining character in the stream;
  # see {Character IO}[rdoc-ref:IO@Character+IO].
  #
  # With no block given, returns an enumerator.
  def each_char; end

  # With a block given, calls the block with each remaining codepoint in the stream;
  # see {Codepoint IO}[rdoc-ref:IO@Codepoint+IO].
  #
  # With no block given, returns an enumerator.
  def each_codepoint; end

  # Returns +true+ if positioned at end-of-stream, +false+ otherwise;
  # see {Position}[rdoc-ref:IO@Position].
  #
  # Raises IOError if the stream is not opened for reading.
  def eof; end
  alias eof? eof

  # Returns the Encoding object that represents the encoding of the file.
  # If the stream is write mode and no encoding is specified, returns
  # +nil+.
  def external_encoding; end

  # Raises NotImplementedError.
  def fcntl(*args) end

  # Returns +nil+.  Just for compatibility to IO.
  def fileno; end

  # Returns an object itself.  Just for compatibility to IO.
  def flush; end

  # Returns 0.  Just for compatibility to IO.
  def fsync; end

  # Reads and returns the next 8-bit byte from the stream;
  # see {Byte IO}[rdoc-ref:IO@Byte+IO].
  def getbyte; end

  # Reads and returns the next character from the stream;
  # see {Character IO}[rdoc-ref:IO@Character+IO].
  def getc; end

  # Reads and returns a line from the stream;
  # assigns the return value to <tt>$_</tt>;
  # see {Line IO}[rdoc-ref:IO@Line+IO].
  def gets(...) end

  # Returns the Encoding of the internal string if conversion is
  # specified.  Otherwise returns +nil+.
  def internal_encoding; end

  # Returns +false+.  Just for compatibility to IO.
  def isatty; end
  alias tty? isatty

  # Returns the current line number in +self+;
  # see {Line Number}[rdoc-ref:IO@Line+Number].
  def lineno; end

  # Sets the current line number in +self+ to the given +new_line_number+;
  # see {Line Number}[rdoc-ref:IO@Line+Number].
  def lineno=(new_line_number) end

  # Returns +nil+.  Just for compatibility to IO.
  def pid; end

  # Returns the current position (in bytes);
  # see {Position}[rdoc-ref:IO@Position].
  def pos; end

  # Sets the current position (in bytes);
  # see {Position}[rdoc-ref:IO@Position].
  def pos=(new_position) end

  # See IO#pread.
  def pread(...) end

  # See IO#putc.
  def putc(obj) end

  # See IO#read.
  def read(*args) end

  # See IO#readlines.
  def readlines(...) end

  # Reinitializes the stream with the given +other+ (string or StringIO) and +mode+;
  # see IO.new:
  #
  #   StringIO.open('foo') do |strio|
  #     p strio.string
  #     strio.reopen('bar')
  #     p strio.string
  #     other_strio = StringIO.new('baz')
  #     strio.reopen(other_strio)
  #     p strio.string
  #     other_strio.close
  #   end
  #
  # Output:
  #
  #   "foo"
  #   "bar"
  #   "baz"
  def reopen(other, mode = 'r+') end

  # Sets the current position and line number to zero;
  # see {Position}[rdoc-ref:IO@Position]
  # and {Line Number}[rdoc-ref:IO@Line+Number].
  def rewind; end

  # Sets the current position to the given integer +offset+ (in bytes),
  # with respect to a given constant +whence+;
  # see {Position}[rdoc-ref:IO@Position].
  def seek(offset, whence = SEEK_SET) end

  # Specify the encoding of the StringIO as <i>ext_enc</i>.
  # Use the default external encoding if <i>ext_enc</i> is nil.
  # 2nd argument <i>int_enc</i> and optional hash <i>opt</i> argument
  # are ignored; they are for API compatibility to IO.
  def set_encoding(p1, p2 = v2, p3 = {}) end

  def set_encoding_by_bom; end

  # Returns the size of the buffer string.
  def size; end
  alias length size

  # Returns underlying string:
  #
  #   StringIO.open('foo') do |strio|
  #     p strio.string
  #     strio.string = 'bar'
  #     p strio.string
  #   end
  #
  # Output:
  #
  #   "foo"
  #   "bar"
  #
  # Related: StringIO#string= (assigns the underlying string).
  def string; end

  # Assigns the underlying string as +other_string+, and sets position to zero;
  # returns +other_string+:
  #
  #   StringIO.open('foo') do |strio|
  #     p strio.string
  #     strio.string = 'bar'
  #     p strio.string
  #   end
  #
  # Output:
  #
  #   "foo"
  #   "bar"
  #
  # Related: StringIO#string (returns the underlying string).
  def string=(other_string) end

  # Returns +true+; implemented only for compatibility with other stream classes.
  def sync; end

  # Returns the argument unchanged.  Just for compatibility to IO.
  def sync=(p1) end

  # Returns the current position (in bytes);
  # see {Position}[rdoc-ref:IO@Position].
  def tell; end

  # Truncates the buffer string to at most _integer_ bytes. The stream
  # must be opened for writing.
  def truncate(integer) end

  # Pushes back ("unshifts") an 8-bit byte onto the stream;
  # see {Byte IO}[rdoc-ref:IO@Byte+IO].
  def ungetbyte(byte) end

  # Pushes back ("unshifts") a character or integer onto the stream;
  # see {Character IO}[rdoc-ref:IO@Character+IO].
  def ungetc(character) end

  # Appends the given string to the underlying buffer string.
  # The stream must be opened for writing.  If the argument is not a
  # string, it will be converted to a string using <code>to_s</code>.
  # Returns the number of bytes written.  See IO#write.
  def write(string, *args) end
end
