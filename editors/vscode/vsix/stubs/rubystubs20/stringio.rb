# frozen_string_literal: true

# Pseudo I/O on String object.
class StringIO < Data
  include Enumerable
  include IO.readable
  include IO.writable

  # Equivalent to StringIO.new except that when it is called with a block, it
  # yields with the new instance and closes it, and returns the result which
  # returned from the block.
  def self.open(string = '', *mode) end

  # Creates new StringIO instance from with _string_ and _mode_.
  def initialize(*args) end

  def binmode; end

  # This is a deprecated alias for <code>each_byte</code>.
  def bytes; end

  # This is a deprecated alias for <code>each_char</code>.
  def chars; end

  # Closes strio.  The *strio* is unavailable for any further data
  # operations; an +IOError+ is raised if such an attempt is made.
  def close; end

  # Closes the read end of a StringIO.  Will raise an +IOError+ if the
  # *strio* is not readable.
  def close_read; end

  # Closes the write end of a StringIO.  Will raise an  +IOError+ if the
  # *strio* is not writeable.
  def close_write; end

  # Returns +true+ if *strio* is completely closed, +false+ otherwise.
  def closed?; end

  # Returns +true+ if *strio* is not readable, +false+ otherwise.
  def closed_read?; end

  # Returns +true+ if *strio* is not writable, +false+ otherwise.
  def closed_write?; end

  # This is a deprecated alias for <code>each_codepoint</code>.
  def codepoints; end

  # See IO#each.
  def each(*several_variants) end
  alias each_line each

  # See IO#each_byte.
  def each_byte; end

  # See IO#each_char.
  def each_char; end

  # See IO#each_codepoint.
  def each_codepoint; end

  # Returns true if *strio* is at end of file. The stringio must be
  # opened for reading or an +IOError+ will be raised.
  def eof; end
  alias eof? eof

  # Returns the Encoding object that represents the encoding of the file.
  # If strio is write mode and no encoding is specified, returns <code>nil</code>.
  def external_encoding; end

  def fcntl; end

  def fileno; end

  def flush; end

  def fsync; end

  # See IO#getbyte.
  def getbyte; end

  # See IO#getc.
  def getc; end

  # See IO#gets.
  def gets(*several_variants) end

  # Returns the Encoding of the internal string if conversion is
  # specified.  Otherwise returns nil.
  def internal_encoding; end

  def isatty; end
  alias tty? isatty

  # Returns the current line number in *strio*. The stringio must be
  # opened for reading. +lineno+ counts the number of times  +gets+ is
  # called, rather than the number of newlines  encountered. The two
  # values will differ if +gets+ is  called with a separator other than
  # newline.  See also the  <code>$.</code> variable.
  def lineno; end

  # Manually sets the current line number to the given value.
  # <code>$.</code> is updated only on the next read.
  def lineno=(integer) end

  # This is a deprecated alias for <code>each_line</code>.
  def lines(*args) end

  def pid; end

  # Returns the current offset (in bytes) of *strio*.
  def pos; end

  # Seeks to the given position (in bytes) in *strio*.
  def pos=(integer) end

  # See IO#putc.
  def putc(obj) end

  # See IO#read.
  def read(*args) end

  # See IO#readlines.
  def readlines(*several_variants) end

  # Reinitializes *strio* with the given <i>other_StrIO</i> or _string_
  # and _mode_ (see StringIO#new).
  def reopen(*several_variants) end

  # Positions *strio* to the beginning of input, resetting
  # +lineno+ to zero.
  def rewind; end

  # Seeks to a given offset _amount_ in the stream according to
  # the value of _whence_ (see IO#seek).
  def seek(amount, whence = SEEK_SET) end

  # Specify the encoding of the StringIO as <i>ext_enc</i>.
  # Use the default external encoding if <i>ext_enc</i> is nil.
  # 2nd argument <i>int_enc</i> and optional hash <i>opt</i> argument
  # are ignored; they are for API compatibility to IO.
  def set_encoding(p1, p2 = v2, p3 = {}) end

  # Returns the size of the buffer string.
  def size; end
  alias length size

  # Returns underlying String object, the subject of IO.
  def string; end

  # Changes underlying String object, the subject of IO.
  def string=(string) end

  # Returns +true+ always.
  def sync; end

  def sync=(boolean) end

  # Returns the current offset (in bytes) of *strio*.
  def tell; end

  # Truncates the buffer string to at most _integer_ bytes. The *strio*
  # must be opened for writing.
  def truncate(integer) end

  # See IO#ungetbyte
  def ungetbyte(fixnum) end

  # Pushes back one character (passed as a parameter) onto *strio*
  # such that a subsequent buffered read will return it.  There is no
  # limitation for multiple pushbacks including pushing back behind the
  # beginning of the buffer string.
  def ungetc(string) end

  # Appends the given string to the underlying buffer string of *strio*.
  # The stream must be opened for writing.  If the argument is not a
  # string, it will be converted to a string using <code>to_s</code>.
  # Returns the number of bytes written.  See IO#write.
  def write(string) end
end
