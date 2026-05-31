# frozen_string_literal: true

# Pseudo I/O on String object.
class StringIO < Data
  include Enumerable

  # Equivalent to StringIO.new except that when it is called with a block, it
  # yields with the new instance and closes it, and returns the result which
  # returned from the block.
  def self.open(string = '', *mode) end

  # Creates new StringIO instance from with _string_ and _mode_.
  def initialize(*args) end

  # See IO#<<.
  def <<(obj) end

  def binmode; end

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

  # See IO#each.
  def each(sep_string = $/) end
  alias each_line each
  alias lines each

  # See IO#each_byte.
  def each_byte; end
  alias bytes each_byte

  # See IO#each_char.
  def each_char; end
  alias chars each_char

  # Returns true if *strio* is at end of file. The stringio must be
  # opened for reading or an +IOError+ will be raised.
  def eof; end
  alias eof? eof

  def fcntl; end

  def fileno; end

  def flush; end

  def fsync; end

  # See IO#getc.
  def getc; end
  alias getbyte getc

  # See IO#gets.
  def gets(sep_string = $/) end

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

  def path; end

  def pid; end

  # Returns the current offset (in bytes) of *strio*.
  def pos; end

  # Seeks to the given position (in bytes) in *strio*.
  def pos=(integer) end

  # See IO#print.
  def print(*several_variants) end

  # See IO#printf.
  def printf(format_string, *objects) end

  # See IO#putc.
  def putc(obj) end

  # See IO#puts.
  def puts(obj = '', *arg) end

  # See IO#read.
  def read(*args) end

  # See IO#readchar.
  def readchar; end
  alias readbyte readchar

  # See IO#readline.
  def readline(sep_string = $/) end

  # See IO#readlines.
  def readlines(sep_string = $/) end

  # Reinitializes *strio* with the given <i>other_StrIO</i> or _string_
  # and _mode_ (see StringIO#new).
  def reopen(*several_variants) end

  # Positions *strio* to the beginning of input, resetting
  # +lineno+ to zero.
  def rewind; end

  # Seeks to a given offset _amount_ in the stream according to
  # the value of _whence_ (see IO#seek).
  def seek(amount, whence = SEEK_SET) end

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

  # Similar to #read, but raises +EOFError+ at end of string instead of
  # returning +nil+, as well as IO#sysread does.
  def sysread(integer, *outbuf) end

  # Appends the given string to the underlying buffer string of *strio*.
  # The stream must be opened for writing.  If the argument is not a
  # string, it will be converted to a string using <code>to_s</code>.
  # Returns the number of bytes written.  See IO#write.
  def syswrite(string) end

  # Returns the current offset (in bytes) of *strio*.
  def tell; end

  # Truncates the buffer string to at most _integer_ bytes. The *strio*
  # must be opened for writing.
  def truncate(integer) end

  # Pushes back one character (passed as a parameter) onto *strio*
  # such that a subsequent buffered read will return it.  Pushing back
  # behind the beginning of the buffer string is not possible.  Nothing
  # will be done if such an attempt is made.
  # In other case, there is no limitation for multiple pushbacks.
  def ungetc(integer) end

  # Appends the given string to the underlying buffer string of *strio*.
  # The stream must be opened for writing.  If the argument is not a
  # string, it will be converted to a string using <code>to_s</code>.
  # Returns the number of bytes written.  See IO#write.
  def write(string) end
end
