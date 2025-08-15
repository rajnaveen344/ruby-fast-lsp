# frozen_string_literal: true

# The IO class is the basis for all input and output in Ruby.
# An I/O stream may be <em>duplexed</em> (that is, bidirectional), and
# so may use more than one native operating system stream.
#
# Many of the examples in this section use the File class, the only standard
# subclass of IO. The two classes are closely associated.  Like the File
# class, the Socket library subclasses from IO (such as TCPSocket or
# UDPSocket).
#
# The Kernel#open method can create an IO (or File) object for these types
# of arguments:
#
# * A plain string represents a filename suitable for the underlying
#   operating system.
#
# * A string starting with <code>"|"</code> indicates a subprocess.
#   The remainder of the string following the <code>"|"</code> is
#   invoked as a process with appropriate input/output channels
#   connected to it.
#
# * A string equal to <code>"|-"</code> will create another Ruby
#   instance as a subprocess.
#
# The IO may be opened with different file modes (read-only, write-only) and
# encodings for proper conversion.  See IO.new for these options.  See
# Kernel#open for details of the various command formats described above.
#
# IO.popen, the Open3 library, or  Process#spawn may also be used to
# communicate with subprocesses through an IO.
#
# Ruby will convert pathnames between different operating system
# conventions if possible.  For instance, on a Windows system the
# filename <code>"/gumby/ruby/test.rb"</code> will be opened as
# <code>"\gumby\ruby\test.rb"</code>.  When specifying a Windows-style
# filename in a Ruby string, remember to escape the backslashes:
#
#   "C:\\gumby\\ruby\\test.rb"
#
# Our examples here will use the Unix-style forward slashes;
# File::ALT_SEPARATOR can be used to get the platform-specific separator
# character.
#
# The global constant ARGF (also accessible as <code>$<</code>) provides an
# IO-like stream which allows access to all files mentioned on the
# command line (or STDIN if no files are mentioned). ARGF#path and its alias
# ARGF#filename are provided to access the name of the file currently being
# read.
#
# == io/console
#
# The io/console extension provides methods for interacting with the
# console.  The console can be accessed from IO.console or the standard
# input/output/error IO objects.
#
# Requiring io/console adds the following methods:
#
# * IO::console
# * IO#raw
# * IO#raw!
# * IO#cooked
# * IO#cooked!
# * IO#getch
# * IO#echo=
# * IO#echo?
# * IO#noecho
# * IO#winsize
# * IO#winsize=
# * IO#iflush
# * IO#ioflush
# * IO#oflush
#
# Example:
#
#   require 'io/console'
#   rows, columns = $stdout.winsize
#   puts "Your screen is #{columns} wide and #{rows} tall"
#
# == Example Files
#
# Many examples here use these filenames and their corresponding files:
#
# - <tt>t.txt</tt>: A text-only file that is assumed to exist via:
#
#     text = <<~EOT
#       This is line one.
#       This is the second line.
#       This is the third line.
#     EOT
#     File.write('t.txt', text)
#
# - <tt>t.dat</tt>: A data file that is assumed to exist via:
#
#     data = "\u9990\u9991\u9992\u9993\u9994"
#     f = File.open('t.dat', 'wb:UTF-16')
#     f.write(data)
#     f.close
#
# - <tt>t.rus</tt>: A Russian-language text file that is assumed to exist via:
#
#     File.write('t.rus', "\u{442 435 441 442}")
#
# - <tt>t.tmp</tt>: A file that is assumed _not_ to exist.
#
# == Modes
#
# A number of \IO method calls must or may specify a _mode_ for the stream;
# the mode determines how stream is to be accessible, including:
#
# - Whether the stream is to be read-only, write-only, or read-write.
# - Whether the stream is positioned at its beginning or its end.
# - Whether the stream treats data as text-only or binary.
# - The external and internal encodings.
#
# === Mode Specified as an \Integer
#
# When +mode+ is an integer it must be one or more (combined by bitwise OR (<tt>|</tt>)
# of the modes defined in File::Constants:
#
# - +File::RDONLY+: Open for reading only.
# - +File::WRONLY+: Open for writing only.
# - +File::RDWR+: Open for reading and writing.
# - +File::APPEND+: Open for appending only.
# - +File::CREAT+: Create file if it does not exist.
# - +File::EXCL+: Raise an exception if +File::CREAT+ is given and the file exists.
#
# Examples:
#
#   File.new('t.txt', File::RDONLY)
#   File.new('t.tmp', File::RDWR | File::CREAT | File::EXCL)
#
# Note: Method IO#set_encoding does not allow the mode to be specified as an integer.
#
# === Mode Specified As a \String
#
# When +mode+ is a string it must begin with one of the following:
#
# - <tt>'r'</tt>: Read-only stream, positioned at the beginning;
#   the stream cannot be changed to writable.
# - <tt>'w'</tt>: Write-only stream, positioned at the beginning;
#   the stream cannot be changed to readable.
# - <tt>'a'</tt>: Write-only stream, positioned at the end;
#   every write appends to the end;
#   the stream cannot be changed to readable.
# - <tt>'r+'</tt>: Read-write stream, positioned at the beginning.
# - <tt>'w+'</tt>: Read-write stream, positioned at the end.
# - <tt>'a+'</tt>: Read-write stream, positioned at the end.
#
# For a writable file stream (that is, any except read-only),
# the file is truncated to zero if it exists,
# and is created if it does not exist.
#
# Examples:
#
#   File.open('t.txt', 'r')
#   File.open('t.tmp', 'w')
#
# Either of the following may be suffixed to any of the above:
#
# - <tt>'t'</tt>: Text data; sets the default external encoding to +Encoding::UTF_8+;
#   on Windows, enables conversion between EOL and CRLF.
# - <tt>'b'</tt>: Binary data; sets the default external encoding to +Encoding::ASCII_8BIT+;
#   on Windows, suppresses conversion between EOL and CRLF.
#
# If neither is given, the stream defaults to text data.
#
# Examples:
#
#   File.open('t.txt', 'rt')
#   File.open('t.dat', 'rb')
#
# The following may be suffixed to any writable mode above:
#
# - <tt>'x'</tt>: Creates the file if it does not exist;
#   raises an exception if the file exists.
#
# Example:
#
#   File.open('t.tmp', 'wx')
#
# Finally, the mode string may specify encodings --
# either external encoding only or both external and internal encodings --
# by appending one or both encoding names, separated by colons:
#
#   f = File.new('t.dat', 'rb')
#   f.external_encoding # => #<Encoding:ASCII-8BIT>
#   f.internal_encoding # => nil
#   f = File.new('t.dat', 'rb:UTF-16')
#   f.external_encoding # => #<Encoding:UTF-16 (dummy)>
#   f.internal_encoding # => nil
#   f = File.new('t.dat', 'rb:UTF-16:UTF-16')
#   f.external_encoding # => #<Encoding:UTF-16 (dummy)>
#   f.internal_encoding # => #<Encoding:UTF-16>
#
# The numerous encoding names are available in array Encoding.name_list:
#
#   Encoding.name_list.size    # => 175
#   Encoding.name_list.take(3) # => ["ASCII-8BIT", "UTF-8", "US-ASCII"]
#
# == Encodings
#
# When the external encoding is set,
# strings read are tagged by that encoding
# when reading, and strings written are converted to that
# encoding when writing.
#
# When both external and internal encodings are set,
# strings read are converted from external to internal encoding,
# and strings written are converted from internal to external encoding.
# For further details about transcoding input and output, see Encoding.
#
# If the external encoding is <tt>'BOM|UTF-8'</tt>, <tt>'BOM|UTF-16LE'</tt>
# or <tt>'BOM|UTF16-BE'</tt>, Ruby checks for
# a Unicode BOM in the input document to help determine the encoding.  For
# UTF-16 encodings the file open mode must be binary.
# If the BOM is found, it is stripped and the external encoding from the BOM is used.
#
# Note that the BOM-style encoding option is case insensitive,
# so 'bom|utf-8' is also valid.)
#
# == Open Options
#
# A number of \IO methods accept an optional parameter +opts+,
# which determines how a new stream is to be opened:
#
# - +:mode+: Stream mode.
# - +:flags+: \Integer file open flags;
#   If +mode+ is also given, the two are bitwise-ORed.
# - +:external_encoding+: External encoding for the stream.
# - +:internal_encoding+: Internal encoding for the stream.
#   <tt>'-'</tt> is a synonym for the default internal encoding.
#   If the value is +nil+ no conversion occurs.
# - +:encoding+: Specifies external and internal encodings as <tt>'extern:intern'</tt>.
# - +:textmode+: If a truthy value, specifies the mode as text-only, binary otherwise.
# - +:binmode+: If a truthy value, specifies the mode as binary, text-only otherwise.
# - +:autoclose+: If a truthy value, specifies that the +fd+ will close
#   when the stream closes; otherwise it remains open.
#
# Also available are the options offered in String#encode,
# which may control conversion between external internal encoding.
#
# == Getline Options
#
# A number of \IO methods accept optional keyword arguments
# that determine how a stream is to be treated:
#
# - +:chomp+: If +true+, line separators are omitted; default is  +false+.
#
# == Position
#
# An \IO stream has a _position_, which is the non-negative integer offset
# (in bytes) in the stream where the next read or write will occur.
#
# Note that a text stream may have multi-byte characters,
# so a text stream whose position is +n+ (_bytes_) may not have +n+ _characters_
# preceding the current position -- there may be fewer.
#
# A new stream is initially positioned:
#
# - At the beginning (position +0+)
#   if its mode is <tt>'r'</tt>, <tt>'w'</tt>, or <tt>'r+'</tt>.
# - At the end (position <tt>self.size</tt>)
#   if its mode is <tt>'a'</tt>, <tt>'w+'</tt>, or <tt>'a+'</tt>.
#
# Methods to query the position:
#
# - IO#tell and its alias IO#pos return the position for an open stream.
# - IO#eof? and its alias IO#eof return whether the position is at the end
#   of a readable stream.
#
# Reading from a stream usually changes its position:
#
#   f = File.open('t.txt')
#   f.tell     # => 0
#   f.readline # => "This is line one.\n"
#   f.tell     # => 19
#   f.readline # => "This is the second line.\n"
#   f.tell     # => 45
#   f.eof?     # => false
#   f.readline # => "Here's the third line.\n"
#   f.eof?     # => true
#
# Writing to a stream usually changes its position:
#
#   f = File.open('t.tmp', 'w')
#   f.tell         # => 0
#   f.write('foo') # => 3
#   f.tell         # => 3
#   f.write('bar') # => 3
#   f.tell         # => 6
#
# Iterating over a stream usually changes its position:
#
#   f = File.open('t.txt')
#   f.each do |line|
#     p "position=#{f.pos} eof?=#{f.eof?} line=#{line}"
#   end
#
# Output:
#
#   "position=19 eof?=false line=This is line one.\n"
#   "position=45 eof?=false line=This is the second line.\n"
#   "position=70 eof?=true line=This is the third line.\n"
#
# The position may also be changed by certain other methods:
#
# - IO#pos= and IO#seek change the position to a specified offset.
# - IO#rewind changes the position to the beginning.
#
# == Line Number
#
# A readable \IO stream has a _line_ _number_,
# which is the non-negative integer line number
# in the stream where the next read will occur.
#
# A new stream is initially has line number +0+.
#
# \Method IO#lineno returns the line number.
#
# Reading lines from a stream usually changes its line number:
#
#   f = File.open('t.txt', 'r')
#   f.lineno   # => 0
#   f.readline # => "This is line one.\n"
#   f.lineno   # => 1
#   f.readline # => "This is the second line.\n"
#   f.lineno   # => 2
#   f.readline # => "Here's the third line.\n"
#   f.lineno   # => 3
#   f.eof?     # => true
#
# Iterating over lines in a stream usually changes its line number:
#
#      f = File.open('t.txt')
#      f.each_line do |line|
#        p "position=#{f.pos} eof?=#{f.eof?} line=#{line}"
#      end
#
# Output:
#
#  "position=19 eof?=false line=This is line one.\n"
#  "position=45 eof?=false line=This is the second line.\n"
#  "position=70 eof?=true line=This is the third line.\n"
#
# == What's Here
#
# First, what's elsewhere. \Class \IO:
#
# - Inherits from {class Object}[Object.html#class-Object-label-What-27s+Here].
# - Includes {module Enumerable}[Enumerable.html#module-Enumerable-label-What-27s+Here],
#   which provides dozens of additional methods.
#
# Here, class \IO provides methods that are useful for:
#
# - {Creating}[#class-IO-label-Creating]
# - {Reading}[#class-IO-label-Reading]
# - {Writing}[#class-IO-label-Writing]
# - {Positioning}[#class-IO-label-Positioning]
# - {Iterating}[#class-IO-label-Iterating]
# - {Settings}[#class-IO-label-Settings]
# - {Querying}[#class-IO-label-Querying]
# - {Buffering}[#class-IO-label-Buffering]
# - {Low-Level Access}[#class-IO-label-Low-Level+Access]
# - {Other}[#class-IO-label-Other]
#
# === Creating
#
# - ::new (aliased as ::for_fd):: Creates and returns a new \IO object for the given
#                                 integer file descriptor.
# - ::open:: Creates a new \IO object.
# - ::pipe:: Creates a connected pair of reader and writer \IO objects.
# - ::popen:: Creates an \IO object to interact with a subprocess.
# - ::select:: Selects which given \IO instances are ready for reading,
#   writing, or have pending exceptions.
#
# === Reading
#
# - ::binread:: Returns a binary string with all or a subset of bytes
#               from the given file.
# - ::read:: Returns a string with all or a subset of bytes from the given file.
# - ::readlines:: Returns an array of strings, which are the lines from the given file.
# - #getbyte:: Returns the next 8-bit byte read from +self+ as an integer.
# - #getc:: Returns the next character read from +self+ as a string.
# - #gets:: Returns the line read from +self+.
# - #pread:: Returns all or the next _n_ bytes read from +self+,
#            not updating the receiver's offset.
# - #read:: Returns all remaining or the next _n_ bytes read from +self+
#           for a given _n_.
# - #read_nonblock:: the next _n_ bytes read from +self+ for a given _n_,
#                    in non-block mode.
# - #readbyte:: Returns the next byte read from +self+;
#               same as #getbyte, but raises an exception on end-of-file.
# - #readchar:: Returns the next character read from +self+;
#               same as #getc, but raises an exception on end-of-file.
# - #readline:: Returns the next line read from +self+;
#               same as #getline, but raises an exception of end-of-file.
# - #readlines:: Returns an array of all lines read read from +self+.
# - #readpartial:: Returns up to the given number of bytes from +self+.
#
# === Writing
#
# - ::binwrite:: Writes the given string to the file at the given filepath,
#                in binary mode.
# - ::write:: Writes the given string to +self+.
# - {::<<}[#method-i-3C-3C]:: Appends the given string to +self+.
# - #print:: Prints last read line or given objects to +self+.
# - #printf:: Writes to +self+ based on the given format string and objects.
# - #putc:: Writes a character to +self+.
# - #puts:: Writes lines to +self+, making sure line ends with a newline.
# - #pwrite:: Writes the given string at the given offset,
#             not updating the receiver's offset.
# - #write:: Writes one or more given strings to +self+.
# - #write_nonblock:: Writes one or more given strings to +self+ in non-blocking mode.
#
# === Positioning
#
# - #lineno:: Returns the current line number in +self+.
# - #lineno=:: Sets the line number is +self+.
# - #pos (aliased as #tell):: Returns the current byte offset in +self+.
# - #pos=:: Sets the byte offset in +self+.
# - #reopen:: Reassociates +self+ with a new or existing \IO stream.
# - #rewind:: Positions +self+ to the beginning of input.
# - #seek:: Sets the offset for +self+ relative to given position.
#
# === Iterating
#
# - ::foreach:: Yields each line of given file to the block.
# - #each (aliased as #each_line):: Calls the given block
#                                   with each successive line in +self+.
# - #each_byte:: Calls the given block with each successive byte in +self+
#                as an integer.
# - #each_char:: Calls the given block with each successive character in +self+
#                as a string.
# - #each_codepoint:: Calls the given block with each successive codepoint in +self+
#                     as an integer.
#
# === Settings
#
# - #autoclose=:: Sets whether +self+ auto-closes.
# - #binmode:: Sets +self+ to binary mode.
# - #close:: Closes +self+.
# - #close_on_exec=:: Sets the close-on-exec flag.
# - #close_read:: Closes +self+ for reading.
# - #close_write:: Closes +self+ for writing.
# - #set_encoding:: Sets the encoding for +self+.
# - #set_encoding_by_bom:: Sets the encoding for +self+, based on its
#                          Unicode byte-order-mark.
# - #sync=:: Sets the sync-mode to the given value.
#
# === Querying
#
# - #autoclose?:: Returns whether +self+ auto-closes.
# - #binmode?:: Returns whether +self+ is in binary mode.
# - #close_on_exec?:: Returns the close-on-exec flag for +self+.
# - #closed?:: Returns whether +self+ is closed.
# - #eof? (aliased as #eof):: Returns whether +self+ is at end-of-file.
# - #external_encoding:: Returns the external encoding object for +self+.
# - #fileno (aliased as #to_i):: Returns the integer file descriptor for +self+
# - #internal_encoding:: Returns the internal encoding object for +self+.
# - #pid:: Returns the process ID of a child process associated with +self+,
#          if +self+ was created by ::popen.
# - #stat:: Returns the File::Stat object containing status information for +self+.
# - #sync:: Returns whether +self+ is in sync-mode.
# - #tty (aliased as #isatty):: Returns whether +self+ is a terminal.
#
# === Buffering
#
# - #fdatasync:: Immediately writes all buffered data in +self+ to disk.
# - #flush:: Flushes any buffered data within +self+ to the underlying
#            operating system.
# - #fsync:: Immediately writes all buffered data and attributes in +self+ to disk.
# - #ungetbyte:: Prepends buffer for +self+ with given integer byte or string.
# - #ungetc:: Prepends buffer for +self+ with given string.
#
# === Low-Level Access
#
# - ::sysopen:: Opens the file given by its path,
#               returning the integer file descriptor.
# - #advise:: Announces the intention to access data from +self+ in a specific way.
# - #fcntl:: Passes a low-level command to the file specified
#            by the given file descriptor.
# - #ioctl:: Passes a low-level command to the device specified
#            by the given file descriptor.
# - #sysread:: Returns up to the next _n_ bytes read from self using a low-level read.
# - #sysseek:: Sets the offset for +self+.
# - #syswrite:: Writes the given string to +self+ using a low-level write.
#
# === Other
#
# - ::copy_stream:: Copies data from a source to a destination,
#                   each of which is a filepath or an \IO-like object.
# - ::try_convert:: Returns a new \IO object resulting from converting
#                   the given object.
# - #inspect:: Returns the string representation of +self+.
class IO
  include Enumerable
  include File::Constants

  # same as IO::EAGAINWaitReadable
  EWOULDBLOCKWaitReadable = _
  # same as IO::EAGAINWaitWritable
  EWOULDBLOCKWaitWritable = _
  PRIORITY = _
  READABLE = _
  # Set I/O position from the current position
  SEEK_CUR = _
  # Set I/O position to the next location containing data
  SEEK_DATA = _
  # Set I/O position from the end
  SEEK_END = _
  # Set I/O position to the next hole
  SEEK_HOLE = _
  # Set I/O position from the beginning
  SEEK_SET = _
  WRITABLE = _

  # Opens the file, optionally seeks to the given <i>offset</i>, then
  # returns <i>length</i> bytes (defaulting to the rest of the file).
  # #binread ensures the file is closed before returning.  The open mode
  # would be <code>"rb:ASCII-8BIT"</code>.
  #
  # If +name+ starts with a pipe character (<code>"|"</code>) and the receiver
  # is the IO class, a subprocess is created in the same way as Kernel#open,
  # and its output is returned.
  # Consider to use File.binread to disable the behavior of subprocess invocation.
  #
  #    File.binread("testfile")           #=> "This is line one\nThis is line two\nThis is line three\nAnd so on...\n"
  #    File.binread("testfile", 20)       #=> "This is line one\nThi"
  #    File.binread("testfile", 20, 10)   #=> "ne one\nThis is line "
  #    IO.binread("| cat testfile")       #=> "This is line one\nThis is line two\nThis is line three\nAnd so on...\n"
  #
  # See also IO.read for details about +name+ and open_args.
  def self.binread(p1, p2 = v2, p3 = v3) end

  # Same as IO.write except opening the file in binary mode and
  # ASCII-8BIT encoding (<code>"wb:ASCII-8BIT"</code>).
  #
  # If +name+ starts with a pipe character (<code>"|"</code>) and the receiver
  # is the IO class, a subprocess is created in the same way as Kernel#open,
  # and its output is returned.
  # Consider to use File.binwrite to disable the behavior of subprocess invocation.
  #
  # See also IO.read for details about +name+ and open_args.
  def self.binwrite(...) end

  # Returns an File instance opened console.
  #
  # If +sym+ is given, it will be sent to the opened console with
  # +args+ and the result will be returned instead of the console IO
  # itself.
  #
  # You must require 'io/console' to use this method.
  def self.console(...) end

  # IO.copy_stream copies <i>src</i> to <i>dst</i>.
  # <i>src</i> and <i>dst</i> is either a filename or an IO-like object.
  # IO-like object for <i>src</i> should have #readpartial or #read
  # method.  IO-like object for <i>dst</i> should have #write method.
  # (Specialized mechanisms, such as sendfile system call, may be used
  # on appropriate situation.)
  #
  # This method returns the number of bytes copied.
  #
  # If optional arguments are not given,
  # the start position of the copy is
  # the beginning of the filename or
  # the current file offset of the IO.
  # The end position of the copy is the end of file.
  #
  # If <i>copy_length</i> is given,
  # No more than <i>copy_length</i> bytes are copied.
  #
  # If <i>src_offset</i> is given,
  # it specifies the start position of the copy.
  #
  # When <i>src_offset</i> is specified and
  # <i>src</i> is an IO,
  # IO.copy_stream doesn't move the current file offset.
  def self.copy_stream(...) end

  # Synonym for IO.new.
  def self.for_fd(*args) end

  # Executes the block for every line in the named I/O port, where lines
  # are separated by <em>sep</em>.
  #
  # If no block is given, an enumerator is returned instead.
  #
  # If +name+ starts with a pipe character (<code>"|"</code>) and the receiver
  # is the IO class, a subprocess is created in the same way as Kernel#open,
  # and its output is returned.
  # Consider to use File.foreach to disable the behavior of subprocess invocation.
  #
  #    File.foreach("testfile") {|x| print "GOT ", x }
  #    IO.foreach("| cat testfile") {|x| print "GOT ", x }
  #
  # <em>produces:</em>
  #
  #    GOT This is line one
  #    GOT This is line two
  #    GOT This is line three
  #    GOT And so on...
  #
  # If the last argument is a hash, it's the keyword argument to open.
  # See IO.readlines for details about getline_args.
  # And see also IO.read for details about open_args.
  def self.foreach(...) end

  # With no associated block, IO.open is a synonym for IO.new.  If
  # the optional code block is given, it will be passed +io+ as an argument,
  # and the IO object will automatically be closed when the block terminates.
  # In this instance, IO.open returns the value of the block.
  #
  # See IO.new for a description of the +fd+, +mode+ and +opt+ parameters.
  def self.open(*args) end

  # Creates a pair of pipe endpoints (connected to each other) and
  # returns them as a two-element array of IO objects:
  # <code>[</code> <i>read_io</i>, <i>write_io</i> <code>]</code>.
  #
  # If a block is given, the block is called and
  # returns the value of the block.
  # <i>read_io</i> and <i>write_io</i> are sent to the block as arguments.
  # If read_io and write_io are not closed when the block exits, they are closed.
  # i.e. closing read_io and/or write_io doesn't cause an error.
  #
  # Not available on all platforms.
  #
  # If an encoding (encoding name or encoding object) is specified as an optional argument,
  # read string from pipe is tagged with the encoding specified.
  # If the argument is a colon separated two encoding names "A:B",
  # the read string is converted from encoding A (external encoding)
  # to encoding B (internal encoding), then tagged with B.
  # If two optional arguments are specified, those must be
  # encoding objects or encoding names,
  # and the first one is the external encoding,
  # and the second one is the internal encoding.
  # If the external encoding and the internal encoding is specified,
  # optional hash argument specify the conversion option.
  #
  # In the example below, the two processes close the ends of the pipe
  # that they are not using. This is not just a cosmetic nicety. The
  # read end of a pipe will not generate an end of file condition if
  # there are any writers with the pipe still open. In the case of the
  # parent process, the <code>rd.read</code> will never return if it
  # does not first issue a <code>wr.close</code>.
  #
  #    rd, wr = IO.pipe
  #
  #    if fork
  #      wr.close
  #      puts "Parent got: <#{rd.read}>"
  #      rd.close
  #      Process.wait
  #    else
  #      rd.close
  #      puts "Sending message to parent"
  #      wr.write "Hi Dad"
  #      wr.close
  #    end
  #
  # <em>produces:</em>
  #
  #    Sending message to parent
  #    Parent got: <Hi Dad>
  def self.pipe(...) end

  # Runs the specified command as a subprocess; the subprocess's
  # standard input and output will be connected to the returned
  # IO object.
  #
  # The PID of the started process can be obtained by IO#pid method.
  #
  # _cmd_ is a string or an array as follows.
  #
  #   cmd:
  #     "-"                                      : fork
  #     commandline                              : command line string which is passed to a shell
  #     [env, cmdname, arg1, ..., opts]          : command name and zero or more arguments (no shell)
  #     [env, [cmdname, argv0], arg1, ..., opts] : command name, argv[0] and zero or more arguments (no shell)
  #   (env and opts are optional.)
  #
  # If _cmd_ is a +String+ ``<code>-</code>'',
  # then a new instance of Ruby is started as the subprocess.
  #
  # If <i>cmd</i> is an +Array+ of +String+,
  # then it will be used as the subprocess's +argv+ bypassing a shell.
  # The array can contain a hash at first for environments and
  # a hash at last for options similar to #spawn.
  #
  # The default mode for the new file object is ``r'',
  # but <i>mode</i> may be set to any of the modes listed in the description for class IO.
  # The last argument <i>opt</i> qualifies <i>mode</i>.
  #
  #   # set IO encoding
  #   IO.popen("nkf -e filename", :external_encoding=>"EUC-JP") {|nkf_io|
  #     euc_jp_string = nkf_io.read
  #   }
  #
  #   # merge standard output and standard error using
  #   # spawn option.  See the document of Kernel.spawn.
  #   IO.popen(["ls", "/", :err=>[:child, :out]]) {|ls_io|
  #     ls_result_with_error = ls_io.read
  #   }
  #
  #   # spawn options can be mixed with IO options
  #   IO.popen(["ls", "/"], :err=>[:child, :out]) {|ls_io|
  #     ls_result_with_error = ls_io.read
  #   }
  #
  # Raises exceptions which IO.pipe and Kernel.spawn raise.
  #
  # If a block is given, Ruby will run the command as a child connected
  # to Ruby with a pipe. Ruby's end of the pipe will be passed as a
  # parameter to the block.
  # At the end of block, Ruby closes the pipe and sets <code>$?</code>.
  # In this case IO.popen returns the value of the block.
  #
  # If a block is given with a _cmd_ of ``<code>-</code>'',
  # the block will be run in two separate processes: once in the parent,
  # and once in a child. The parent process will be passed the pipe
  # object as a parameter to the block, the child version of the block
  # will be passed +nil+, and the child's standard in and
  # standard out will be connected to the parent through the pipe. Not
  # available on all platforms.
  #
  #    f = IO.popen("uname")
  #    p f.readlines
  #    f.close
  #    puts "Parent is #{Process.pid}"
  #    IO.popen("date") {|f| puts f.gets }
  #    IO.popen("-") {|f| $stderr.puts "#{Process.pid} is here, f is #{f.inspect}"}
  #    p $?
  #    IO.popen(%w"sed -e s|^|<foo>| -e s&$&;zot;&", "r+") {|f|
  #      f.puts "bar"; f.close_write; puts f.gets
  #    }
  #
  # <em>produces:</em>
  #
  #    ["Linux\n"]
  #    Parent is 21346
  #    Thu Jan 15 22:41:19 JST 2009
  #    21346 is here, f is #<IO:fd 3>
  #    21352 is here, f is nil
  #    #<Process::Status: pid 21352 exit 0>
  #    <foo>bar;zot;
  def self.popen(*args) end

  # Opens the file, optionally seeks to the given +offset+, then returns
  # +length+ bytes (defaulting to the rest of the file).  #read ensures
  # the file is closed before returning.
  #
  # If +name+ starts with a pipe character (<code>"|"</code>) and the receiver
  # is the IO class, a subprocess is created in the same way as Kernel#open,
  # and its output is returned.
  # Consider to use File.read to disable the behavior of subprocess invocation.
  #
  # === Options
  #
  # The options hash accepts the following keys:
  #
  # :encoding::
  #   string or encoding
  #
  #   Specifies the encoding of the read string.  +:encoding+ will be ignored
  #   if +length+ is specified.  See Encoding.aliases for possible encodings.
  #
  # :mode::
  #   string or integer
  #
  #   Specifies the <i>mode</i> argument for open().  It must start
  #   with an "r", otherwise it will cause an error.
  #   See IO.new for the list of possible modes.
  #
  # :open_args::
  #   array
  #
  #   Specifies arguments for open() as an array.  This key can not be used
  #   in combination with either +:encoding+ or +:mode+.
  #
  # Examples:
  #
  #   File.read("testfile")            #=> "This is line one\nThis is line two\nThis is line three\nAnd so on...\n"
  #   File.read("testfile", 20)        #=> "This is line one\nThi"
  #   File.read("testfile", 20, 10)    #=> "ne one\nThis is line "
  #   File.read("binfile", mode: "rb") #=> "\xF7\x00\x00\x0E\x12"
  #   IO.read("|ls -a")                #=> ".\n..\n"...
  def self.read(p1, p2 = v2, p3 = v3, p4 = v4, p5 = {}) end

  # Reads the entire file specified by <i>name</i> as individual
  # lines, and returns those lines in an array. Lines are separated by
  # <i>sep</i>.
  #
  # If +name+ starts with a pipe character (<code>"|"</code>) and the receiver
  # is the IO class, a subprocess is created in the same way as Kernel#open,
  # and its output is returned.
  # Consider to use File.readlines to disable the behavior of subprocess invocation.
  #
  #    a = File.readlines("testfile")
  #    a[0]   #=> "This is line one\n"
  #
  #    b = File.readlines("testfile", chomp: true)
  #    b[0]   #=> "This is line one"
  #
  #    IO.readlines("|ls -a")     #=> [".\n", "..\n", ...]
  #
  # If the last argument is a hash, it's the keyword argument to open.
  #
  # === Options for getline
  #
  # The options hash accepts the following keys:
  #
  # :chomp::
  #   When the optional +chomp+ keyword argument has a true value,
  #   <code>\n</code>, <code>\r</code>, and <code>\r\n</code>
  #   will be removed from the end of each line.
  #
  # See also IO.read for details about +name+ and open_args.
  def self.readlines(...) end

  # Calls select(2) system call.
  # It monitors given arrays of IO objects, waits until one or more of
  # IO objects are ready for reading, are ready for writing, and have
  # pending exceptions respectively, and returns an array that contains
  # arrays of those IO objects.  It will return +nil+ if optional
  # <i>timeout</i> value is given and no IO object is ready in
  # <i>timeout</i> seconds.
  #
  # IO.select peeks the buffer of IO objects for testing readability.
  # If the IO buffer is not empty, IO.select immediately notifies
  # readability.  This "peek" only happens for IO objects.  It does not
  # happen for IO-like objects such as OpenSSL::SSL::SSLSocket.
  #
  # The best way to use IO.select is invoking it after nonblocking
  # methods such as #read_nonblock, #write_nonblock, etc.  The methods
  # raise an exception which is extended by IO::WaitReadable or
  # IO::WaitWritable.  The modules notify how the caller should wait
  # with IO.select.  If IO::WaitReadable is raised, the caller should
  # wait for reading.  If IO::WaitWritable is raised, the caller should
  # wait for writing.
  #
  # So, blocking read (#readpartial) can be emulated using
  # #read_nonblock and IO.select as follows:
  #
  #   begin
  #     result = io_like.read_nonblock(maxlen)
  #   rescue IO::WaitReadable
  #     IO.select([io_like])
  #     retry
  #   rescue IO::WaitWritable
  #     IO.select(nil, [io_like])
  #     retry
  #   end
  #
  # Especially, the combination of nonblocking methods and IO.select is
  # preferred for IO like objects such as OpenSSL::SSL::SSLSocket.  It
  # has #to_io method to return underlying IO object.  IO.select calls
  # #to_io to obtain the file descriptor to wait.
  #
  # This means that readability notified by IO.select doesn't mean
  # readability from OpenSSL::SSL::SSLSocket object.
  #
  # The most likely situation is that OpenSSL::SSL::SSLSocket buffers
  # some data.  IO.select doesn't see the buffer.  So IO.select can
  # block when OpenSSL::SSL::SSLSocket#readpartial doesn't block.
  #
  # However, several more complicated situations exist.
  #
  # SSL is a protocol which is sequence of records.
  # The record consists of multiple bytes.
  # So, the remote side of SSL sends a partial record, IO.select
  # notifies readability but OpenSSL::SSL::SSLSocket cannot decrypt a
  # byte and OpenSSL::SSL::SSLSocket#readpartial will block.
  #
  # Also, the remote side can request SSL renegotiation which forces
  # the local SSL engine to write some data.
  # This means OpenSSL::SSL::SSLSocket#readpartial may invoke #write
  # system call and it can block.
  # In such a situation, OpenSSL::SSL::SSLSocket#read_nonblock raises
  # IO::WaitWritable instead of blocking.
  # So, the caller should wait for ready for writability as above
  # example.
  #
  # The combination of nonblocking methods and IO.select is also useful
  # for streams such as tty, pipe socket socket when multiple processes
  # read from a stream.
  #
  # Finally, Linux kernel developers don't guarantee that
  # readability of select(2) means readability of following read(2) even
  # for a single process.
  # See select(2) manual on GNU/Linux system.
  #
  # Invoking IO.select before IO#readpartial works well as usual.
  # However it is not the best way to use IO.select.
  #
  # The writability notified by select(2) doesn't show
  # how many bytes are writable.
  # IO#write method blocks until given whole string is written.
  # So, <code>IO#write(two or more bytes)</code> can block after
  # writability is notified by IO.select.  IO#write_nonblock is required
  # to avoid the blocking.
  #
  # Blocking write (#write) can be emulated using #write_nonblock and
  # IO.select as follows: IO::WaitReadable should also be rescued for
  # SSL renegotiation in OpenSSL::SSL::SSLSocket.
  #
  #   while 0 < string.bytesize
  #     begin
  #       written = io_like.write_nonblock(string)
  #     rescue IO::WaitReadable
  #       IO.select([io_like])
  #       retry
  #     rescue IO::WaitWritable
  #       IO.select(nil, [io_like])
  #       retry
  #     end
  #     string = string.byteslice(written..-1)
  #   end
  #
  # === Parameters
  # read_array:: an array of IO objects that wait until ready for read
  # write_array:: an array of IO objects that wait until ready for write
  # error_array:: an array of IO objects that wait for exceptions
  # timeout:: a numeric value in second
  #
  # === Example
  #
  #     rp, wp = IO.pipe
  #     mesg = "ping "
  #     100.times {
  #       # IO.select follows IO#read.  Not the best way to use IO.select.
  #       rs, ws, = IO.select([rp], [wp])
  #       if r = rs[0]
  #         ret = r.read(5)
  #         print ret
  #         case ret
  #         when /ping/
  #           mesg = "pong\n"
  #         when /pong/
  #           mesg = "ping "
  #         end
  #       end
  #       if w = ws[0]
  #         w.write(mesg)
  #       end
  #     }
  #
  # <em>produces:</em>
  #
  #     ping pong
  #     ping pong
  #     ping pong
  #     (snipped)
  #     ping
  def self.select(p1, p2 = v2, p3 = v3, p4 = v4) end

  # Opens the given path, returning the underlying file descriptor as a
  # Integer.
  #
  #    IO.sysopen("testfile")   #=> 3
  def self.sysopen(p1, p2 = v2, p3 = v3) end

  # Attempts to convert +object+ into an \IO object via method +to_io+;
  # returns the new \IO object if successful, or +nil+ otherwise:
  #
  #   IO.try_convert(STDOUT)   # => #<IO:<STDOUT>>
  #   IO.try_convert(ARGF)     # => #<IO:<STDIN>>
  #   IO.try_convert('STDOUT') # => nil
  def self.try_convert(object) end

  # Opens the file, optionally seeks to the given <i>offset</i>, writes
  # <i>string</i>, then returns the length written.  #write ensures the
  # file is closed before returning.  If <i>offset</i> is not given in
  # write mode, the file is truncated.  Otherwise, it is not truncated.
  #
  # If +name+ starts with a pipe character (<code>"|"</code>) and the receiver
  # is the IO class, a subprocess is created in the same way as Kernel#open,
  # and its output is returned.
  # Consider to use File.write to disable the behavior of subprocess invocation.
  #
  #   File.write("testfile", "0123456789", 20)  #=> 10
  #   # File could contain:  "This is line one\nThi0123456789two\nThis is line three\nAnd so on...\n"
  #   File.write("testfile", "0123456789")      #=> 10
  #   # File would now read: "0123456789"
  #   IO.write("|tr a-z A-Z", "abc")            #=> 3
  #   # Prints "ABC" to the standard output
  #
  # If the last argument is a hash, it specifies options for the internal
  # open().  It accepts the following keys:
  #
  # :encoding::
  #   string or encoding
  #
  #   Specifies the encoding of the read string.
  #   See Encoding.aliases for possible encodings.
  #
  # :mode::
  #   string or integer
  #
  #   Specifies the <i>mode</i> argument for open().  It must start
  #   with "w", "a", or "r+", otherwise it will cause an error.
  #   See IO.new for the list of possible modes.
  #
  # :perm::
  #   integer
  #
  #   Specifies the <i>perm</i> argument for open().
  #
  # :open_args::
  #   array
  #
  #   Specifies arguments for open() as an array.
  #   This key can not be used in combination with other keys.
  #
  # See also IO.read for details about +name+ and open_args.
  def self.write(...) end

  # Returns a new IO object (a stream) for the given integer file descriptor
  # +fd+ and +mode+ string.  +opt+ may be used to specify parts of +mode+ in a
  # more readable fashion.  See also IO.sysopen and IO.for_fd.
  #
  # IO.new is called by various File and IO opening methods such as IO::open,
  # Kernel#open, and File::open.
  #
  # === Open Mode
  #
  # When +mode+ is an integer it must be combination of the modes defined in
  # File::Constants (+File::RDONLY+, <code>File::WRONLY|File::CREAT</code>).
  # See the open(2) man page for more information.
  #
  # When +mode+ is a string it must be in one of the following forms:
  #
  #   fmode
  #   fmode ":" ext_enc
  #   fmode ":" ext_enc ":" int_enc
  #   fmode ":" "BOM|UTF-*"
  #
  # +fmode+ is an IO open mode string, +ext_enc+ is the external encoding for
  # the IO and +int_enc+ is the internal encoding.
  #
  # ==== IO Open Mode
  #
  # Ruby allows the following open modes:
  #
  #     "r"  Read-only, starts at beginning of file  (default mode).
  #
  #     "r+" Read-write, starts at beginning of file.
  #
  #     "w"  Write-only, truncates existing file
  #          to zero length or creates a new file for writing.
  #
  #     "w+" Read-write, truncates existing file to zero length
  #          or creates a new file for reading and writing.
  #
  #     "a"  Write-only, each write call appends data at end of file.
  #          Creates a new file for writing if file does not exist.
  #
  #     "a+" Read-write, each write call appends data at end of file.
  #          Creates a new file for reading and writing if file does
  #          not exist.
  #
  # The following modes must be used separately, and along with one or more of
  # the modes seen above.
  #
  #     "b"  Binary file mode
  #          Suppresses EOL <-> CRLF conversion on Windows. And
  #          sets external encoding to ASCII-8BIT unless explicitly
  #          specified.
  #
  #     "t"  Text file mode
  #
  # The exclusive access mode ("x") can be used together with "w" to ensure
  # the file is created. Errno::EEXIST is raised when it already exists.
  # It may not be supported with all kinds of streams (e.g. pipes).
  #
  # When the open mode of original IO is read only, the mode cannot be
  # changed to be writable.  Similarly, the open mode cannot be changed from
  # write only to readable.
  #
  # When such a change is attempted the error is raised in different locations
  # according to the platform.
  #
  # === IO Encoding
  #
  # When +ext_enc+ is specified, strings read will be tagged by the encoding
  # when reading, and strings output will be converted to the specified
  # encoding when writing.
  #
  # When +ext_enc+ and +int_enc+ are specified read strings will be converted
  # from +ext_enc+ to +int_enc+ upon input, and written strings will be
  # converted from +int_enc+ to +ext_enc+ upon output.  See Encoding for
  # further details of transcoding on input and output.
  #
  # If "BOM|UTF-8", "BOM|UTF-16LE" or "BOM|UTF16-BE" are used, Ruby checks for
  # a Unicode BOM in the input document to help determine the encoding.  For
  # UTF-16 encodings the file open mode must be binary.  When present, the BOM
  # is stripped and the external encoding from the BOM is used.  When the BOM
  # is missing the given Unicode encoding is used as +ext_enc+.  (The BOM-set
  # encoding option is case insensitive, so "bom|utf-8" is also valid.)
  #
  # === Options
  #
  # +opt+ can be used instead of +mode+ for improved readability.  The
  # following keys are supported:
  #
  # :mode ::
  #   Same as +mode+ parameter
  #
  # :flags ::
  #   Specifies file open flags as integer.
  #   If +mode+ parameter is given, this parameter will be bitwise-ORed.
  #
  # :\external_encoding ::
  #   External encoding for the IO.
  #
  # :\internal_encoding ::
  #   Internal encoding for the IO.  "-" is a synonym for the default internal
  #   encoding.
  #
  #   If the value is +nil+ no conversion occurs.
  #
  # :encoding ::
  #   Specifies external and internal encodings as "extern:intern".
  #
  # :textmode ::
  #   If the value is truth value, same as "t" in argument +mode+.
  #
  # :binmode ::
  #   If the value is truth value, same as "b" in argument +mode+.
  #
  # :autoclose ::
  #   If the value is +false+, the +fd+ will be kept open after this IO
  #   instance gets finalized.
  #
  # Also, +opt+ can have same keys in String#encode for controlling conversion
  # between the external encoding and the internal encoding.
  #
  # === Example 1
  #
  #   fd = IO.sysopen("/dev/tty", "w")
  #   a = IO.new(fd,"w")
  #   $stderr.puts "Hello"
  #   a.puts "World"
  #
  # Produces:
  #
  #   Hello
  #   World
  #
  # === Example 2
  #
  #   require 'fcntl'
  #
  #   fd = STDERR.fcntl(Fcntl::F_DUPFD)
  #   io = IO.new(fd, mode: 'w:UTF-16LE', cr_newline: true)
  #   io.puts "Hello, World!"
  #
  #   fd = STDERR.fcntl(Fcntl::F_DUPFD)
  #   io = IO.new(fd, mode: 'w', cr_newline: true,
  #               external_encoding: Encoding::UTF_16LE)
  #   io.puts "Hello, World!"
  #
  # Both of above print "Hello, World!" in UTF-16LE to standard error output
  # with converting EOL generated by #puts to CR.
  def initialize(p1, p2 = v2, p3 = {}) end

  # Writes the given +object+ to +self+,
  # which must be opened for writing (see {Modes}[#class-IO-label-Modes]);
  # returns +self+;
  # if +object+ is not a string, it is converted via method +to_s+:
  #
  #   $stdout << 'Hello' << ', ' << 'World!' << "\n"
  #   $stdout << 'foo' << :bar << 2 << "\n"
  #
  # Output:
  #
  #   Hello, World!
  #   foobar2
  def <<(object) end

  # Announce an intention to access data from the current file in a
  # specific pattern. On platforms that do not support the
  # <em>posix_fadvise(2)</em> system call, this method is a no-op.
  #
  # _advice_ is one of the following symbols:
  #
  # :normal::     No advice to give; the default assumption for an open file.
  # :sequential:: The data will be accessed sequentially
  #               with lower offsets read before higher ones.
  # :random::     The data will be accessed in random order.
  # :willneed::   The data will be accessed in the near future.
  # :dontneed::   The data will not be accessed in the near future.
  # :noreuse::    The data will only be accessed once.
  #
  # The semantics of a piece of advice are platform-dependent. See
  # <em>man 2 posix_fadvise</em> for details.
  #
  # "data" means the region of the current file that begins at
  # _offset_ and extends for _len_ bytes. If _len_ is 0, the region
  # ends at the last byte of the file. By default, both _offset_ and
  # _len_ are 0, meaning that the advice applies to the entire file.
  #
  # If an error occurs, one of the following exceptions will be raised:
  #
  # IOError:: The IO stream is closed.
  # Errno::EBADF::
  #   The file descriptor of the current file is invalid.
  # Errno::EINVAL:: An invalid value for _advice_ was given.
  # Errno::ESPIPE::
  #   The file descriptor of the current file refers to a FIFO or
  #   pipe. (Linux raises Errno::EINVAL in this case).
  # TypeError::
  #   Either _advice_ was not a Symbol, or one of the
  #   other arguments was not an Integer.
  # RangeError:: One of the arguments given was too big/small.
  #
  # This list is not exhaustive; other Errno:: exceptions are also possible.
  def advise(advice, offset = 0, len = 0) end

  # Sets auto-close flag.
  #
  #    f = open("/dev/null")
  #    IO.for_fd(f.fileno)
  #    # ...
  #    f.gets # may cause Errno::EBADF
  #
  #    f = open("/dev/null")
  #    IO.for_fd(f.fileno).autoclose = false
  #    # ...
  #    f.gets # won't cause Errno::EBADF
  def autoclose=(bool) end

  # Returns +true+ if the underlying file descriptor of _ios_ will be
  # closed automatically at its finalization, otherwise +false+.
  def autoclose?; end

  def beep; end

  # Puts <em>ios</em> into binary mode.
  # Once a stream is in binary mode, it cannot be reset to nonbinary mode.
  #
  # - newline conversion disabled
  # - encoding conversion disabled
  # - content is treated as ASCII-8BIT
  def binmode; end

  # Returns <code>true</code> if <em>ios</em> is binmode.
  def binmode?; end

  def check_winsize_changed; end

  def clear_screen; end

  # Closes <em>ios</em> and flushes any pending writes to the operating
  # system. The stream is unavailable for any further data operations;
  # an IOError is raised if such an attempt is made. I/O streams are
  # automatically closed when they are claimed by the garbage collector.
  #
  # If <em>ios</em> is opened by IO.popen, #close sets
  # <code>$?</code>.
  #
  # Calling this method on closed IO object is just ignored since Ruby 2.3.
  def close; end

  # Sets a close-on-exec flag.
  #
  #    f = open("/dev/null")
  #    f.close_on_exec = true
  #    system("cat", "/proc/self/fd/#{f.fileno}") # cat: /proc/self/fd/3: No such file or directory
  #    f.closed?                #=> false
  #
  # Ruby sets close-on-exec flags of all file descriptors by default
  # since Ruby 2.0.0.
  # So you don't need to set by yourself.
  # Also, unsetting a close-on-exec flag can cause file descriptor leak
  # if another thread use fork() and exec() (via system() method for example).
  # If you really needs file descriptor inheritance to child process,
  # use spawn()'s argument such as fd=>fd.
  def close_on_exec=(bool) end

  # Returns <code>true</code> if <em>ios</em> will be closed on exec.
  #
  #    f = open("/dev/null")
  #    f.close_on_exec?                 #=> false
  #    f.close_on_exec = true
  #    f.close_on_exec?                 #=> true
  #    f.close_on_exec = false
  #    f.close_on_exec?                 #=> false
  def close_on_exec?; end

  # Closes the read end of a duplex I/O stream (i.e., one that contains
  # both a read and a write stream, such as a pipe). Will raise an
  # IOError if the stream is not duplexed.
  #
  #    f = IO.popen("/bin/sh","r+")
  #    f.close_read
  #    f.readlines
  #
  # <em>produces:</em>
  #
  #    prog.rb:3:in `readlines': not opened for reading (IOError)
  #     from prog.rb:3
  #
  # Calling this method on closed IO object is just ignored since Ruby 2.3.
  def close_read; end

  # Closes the write end of a duplex I/O stream (i.e., one that contains
  # both a read and a write stream, such as a pipe). Will raise an
  # IOError if the stream is not duplexed.
  #
  #    f = IO.popen("/bin/sh","r+")
  #    f.close_write
  #    f.print "nowhere"
  #
  # <em>produces:</em>
  #
  #    prog.rb:3:in `write': not opened for writing (IOError)
  #     from prog.rb:3:in `print'
  #     from prog.rb:3
  #
  # Calling this method on closed IO object is just ignored since Ruby 2.3.
  def close_write; end

  # Returns <code>true</code> if <em>ios</em> is completely closed (for
  # duplex streams, both reader and writer), <code>false</code>
  # otherwise.
  #
  #    f = File.new("testfile")
  #    f.close         #=> nil
  #    f.closed?       #=> true
  #    f = IO.popen("/bin/sh","r+")
  #    f.close_write   #=> nil
  #    f.closed?       #=> false
  #    f.close_read    #=> nil
  #    f.closed?       #=> true
  def closed?; end

  # Returns a data represents the current console mode.
  #
  # You must require 'io/console' to use this method.
  def console_mode; end

  # Sets the console mode to +mode+.
  #
  # You must require 'io/console' to use this method.
  def console_mode=(mode) end

  # Yields +self+ within cooked mode.
  #
  #   STDIN.cooked(&:gets)
  #
  # will read and return a line with echo back and line editing.
  #
  # You must require 'io/console' to use this method.
  def cooked; end

  # Enables cooked mode.
  #
  # If the terminal mode needs to be back, use io.cooked { ... }.
  #
  # You must require 'io/console' to use this method.
  def cooked!; end

  def cursor; end

  def cursor=(p1) end

  def cursor_down(p1) end

  def cursor_left(p1) end

  def cursor_right(p1) end

  def cursor_up(p1) end

  # Executes the block for every line in <em>ios</em>, where lines are
  # separated by <i>sep</i>. <em>ios</em> must be opened for
  # reading or an IOError will be raised.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    f = File.new("testfile")
  #    f.each {|line| puts "#{f.lineno}: #{line}" }
  #
  # <em>produces:</em>
  #
  #    1: This is line one
  #    2: This is line two
  #    3: This is line three
  #    4: And so on...
  #
  # See IO.readlines for details about getline_args.
  def each(...) end
  alias each_line each

  # Calls the given block once for each byte (0..255) in <em>ios</em>,
  # passing the byte as an argument. The stream must be opened for
  # reading or an IOError will be raised.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    f = File.new("testfile")
  #    checksum = 0
  #    f.each_byte {|x| checksum ^= x }   #=> #<File:testfile>
  #    checksum                           #=> 12
  def each_byte; end

  # Calls the given block once for each character in <em>ios</em>,
  # passing the character as an argument. The stream must be opened for
  # reading or an IOError will be raised.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    f = File.new("testfile")
  #    f.each_char {|c| print c, ' ' }   #=> #<File:testfile>
  def each_char; end

  # Passes the Integer ordinal of each character in <i>ios</i>,
  # passing the codepoint as an argument. The stream must be opened for
  # reading or an IOError will be raised.
  #
  # If no block is given, an enumerator is returned instead.
  def each_codepoint; end

  # Enables/disables echo back.
  # On some platforms, all combinations of this flags and raw/cooked
  # mode may not be valid.
  #
  # You must require 'io/console' to use this method.
  def echo=(flag) end

  # Returns +true+ if echo back is enabled.
  #
  # You must require 'io/console' to use this method.
  def echo?; end

  # Returns +true+ if the stream is positioned at its end, +false+ otherwise;
  # see {Position}[#class-IO-label-Position]:
  #
  #   f = File.open('t.txt')
  #   f.eof           # => false
  #   f.seek(0, :END) # => 0
  #   f.eof           # => true
  #
  # Raises an exception unless the stream is opened for reading;
  # see {Mode}[#class-IO-label-Mode].
  #
  # If +self+ is a stream such as pipe or socket, this method
  # blocks until the other end sends some data or closes it:
  #
  #   r, w = IO.pipe
  #   Thread.new { sleep 1; w.close }
  #   r.eof? # => true # After 1-second wait.
  #
  #   r, w = IO.pipe
  #   Thread.new { sleep 1; w.puts "a" }
  #   r.eof?  # => false # After 1-second wait.
  #
  #   r, w = IO.pipe
  #   r.eof?  # blocks forever
  #
  # Note that this method reads data to the input byte buffer.  So
  # IO#sysread may not behave as you intend with IO#eof?, unless you
  # call IO#rewind first (which is not available for some streams).
  #
  # I#eof? is an alias for IO#eof.
  def eof; end
  alias eof? eof

  def erase_line(p1) end

  def erase_screen(p1) end

  # Returns the Encoding object that represents the encoding of the file.
  # If _io_ is in write mode and no encoding is specified, returns +nil+.
  def external_encoding; end

  # Provides a mechanism for issuing low-level commands to control or
  # query file-oriented I/O streams. Arguments and results are platform
  # dependent. If <i>arg</i> is a number, its value is passed
  # directly. If it is a string, it is interpreted as a binary sequence
  # of bytes (Array#pack might be a useful way to build this string). On
  # Unix platforms, see <code>fcntl(2)</code> for details.  Not
  # implemented on all platforms.
  def fcntl(integer_cmd, arg) end

  # Immediately writes to disk all data buffered in the stream,
  # via the operating system's: <tt>fdatasync(2)</tt>, if supported,
  # otherwise via <tt>fsync(2)</tt>, if supported;
  # otherwise raises an exception.
  def fdatasync; end

  # Returns the integer file descriptor for the stream:
  #
  #   $stdin.fileno             # => 0
  #   $stdout.fileno            # => 1
  #   $stderr.fileno            # => 2
  #   File.open('t.txt').fileno # => 10
  #
  # IO#to_i is an alias for IO#fileno.
  def fileno; end
  alias to_i fileno

  # Flushes data buffered in +self+ to the operating system
  # (but does not necessarily flush data buffered in the operating system):
  #
  #   $stdout.print 'no newline' # Not necessarily flushed.
  #   $stdout.flush              # Flushed.
  def flush; end

  # Immediately writes to disk all data buffered in the stream,
  # via the operating system's <tt>fsync(2)</tt>.
  #
  # Note this difference:
  #
  # - IO#sync=: Ensures that data is flushed from the stream's internal buffers,
  #   but does not guarantee that the operating system actually writes the data to disk.
  # - IO#fsync: Ensures both that data is flushed from internal buffers,
  #   and that data is written to disk.
  #
  # Raises an exception if the operating system does not support <tt>fsync(2)</tt>.
  def fsync; end

  # Gets the next 8-bit byte (0..255) from <em>ios</em>. Returns
  # +nil+ if called at end of file.
  #
  #    f = File.new("testfile")
  #    f.getbyte   #=> 84
  #    f.getbyte   #=> 104
  def getbyte; end

  # Reads a one-character string from <em>ios</em>. Returns
  # +nil+ if called at end of file.
  #
  #    f = File.new("testfile")
  #    f.getc   #=> "h"
  #    f.getc   #=> "e"
  def getc; end

  # Reads and returns a character in raw mode.
  #
  # See IO#raw for details on the parameters.
  #
  # You must require 'io/console' to use this method.
  def getch(min: nil, time: nil, intr: nil) end

  # Reads and returns a line without echo back.
  # Prints +prompt+ unless it is +nil+.
  #
  # The newline character that terminates the
  # read line is removed from the returned string,
  # see String#chomp!.
  #
  # You must require 'io/console' to use this method.
  def getpass(prompt = nil) end

  # Reads and returns data from the stream;
  # assigns the return value to <tt>$_</tt>.
  #
  # With no arguments given, returns the next line
  # as determined by line separator <tt>$/</tt>, or +nil+ if none:
  #
  #   f = File.open('t.txt')
  #   f.gets # => "This is line one.\n"
  #   $_     # => "This is line one.\n"
  #   f.gets # => "This is the second line.\n"
  #   f.gets # => "This is the third line.\n"
  #   f.gets # => nil
  #
  # With string argument +sep+ given, but not argument +limit+,
  # returns the next line as determined by line separator +sep+,
  # or +nil+ if none:
  #
  #   f = File.open('t.txt')
  #   f.gets(' is') # => "This is"
  #   f.gets(' is') # => " line one.\nThis is"
  #   f.gets(' is') # => " the second line.\nThis is"
  #   f.gets(' is') # => " the third line.\n"
  #   f.gets(' is') # => nil
  #
  # Note two special values for +sep+:
  #
  # - +nil+: The entire stream is read and returned.
  # - <tt>''</tt> (empty string): The next "paragraph" is read and returned,
  #   the paragraph separator being two successive line separators.
  #
  # With integer argument +limit+ given,
  # returns up to <tt>limit+1</tt> bytes:
  #
  #   # Text with 1-byte characters.
  #   File.open('t.txt') {|f| f.gets(1) } # => "T"
  #   File.open('t.txt') {|f| f.gets(2) } # => "Th"
  #   File.open('t.txt') {|f| f.gets(3) } # => "Thi"
  #   File.open('t.txt') {|f| f.gets(4) } # => "This"
  #   # No more than one line.
  #   File.open('t.txt') {|f| f.gets(17) } # => "This is line one."
  #   File.open('t.txt') {|f| f.gets(18) } # => "This is line one.\n"
  #   File.open('t.txt') {|f| f.gets(19) } # => "This is line one.\n"
  #
  #   # Text with 2-byte characters, which will not be split.
  #   File.open('t.rus') {|f| f.gets(1).size } # => 1
  #   File.open('t.rus') {|f| f.gets(2).size } # => 1
  #   File.open('t.rus') {|f| f.gets(3).size } # => 2
  #   File.open('t.rus') {|f| f.gets(4).size } # => 2
  #
  # With arguments +sep+ and +limit+,
  # combines the two behaviors above:
  #
  # - Returns the next line as determined by line separator +sep+,
  #   or +nil+ if none.
  # - But returns no more than <tt>limit+1</tt> bytes.
  #
  # For all forms above, trailing optional keyword arguments may be given;
  # see {Getline Options}[#class-IO-label-Getline+Options]:
  #
  #   f = File.open('t.txt')
  #   # Chomp the lines.
  #   f.gets(chomp: true) # => "This is line one."
  #   f.gets(chomp: true) # => "This is the second line."
  #   f.gets(chomp: true) # => "This is the third line."
  #   f.gets(chomp: true) # => nil
  def gets(...) end

  def goto(p1, p2) end

  def goto_column(p1) end

  # Flushes input buffer in kernel.
  #
  # You must require 'io/console' to use this method.
  def iflush; end

  # Returns a string representation of +self+:
  #
  #   f = File.open('t.txt')
  #   f.inspect # => "#<File:t.txt>"
  def inspect; end

  # Returns the Encoding of the internal string if conversion is
  # specified.  Otherwise returns +nil+.
  def internal_encoding; end

  # Provides a mechanism for issuing low-level commands to control or
  # query I/O devices. Arguments and results are platform dependent. If
  # <i>arg</i> is a number, its value is passed directly. If it is a
  # string, it is interpreted as a binary sequence of bytes. On Unix
  # platforms, see <code>ioctl(2)</code> for details. Not implemented on
  # all platforms.
  def ioctl(integer_cmd, arg) end

  # Flushes input and output buffers in kernel.
  #
  # You must require 'io/console' to use this method.
  def ioflush; end

  # Returns <code>true</code> if <em>ios</em> is associated with a
  # terminal device (tty), <code>false</code> otherwise.
  #
  #    File.new("testfile").isatty   #=> false
  #    File.new("/dev/tty").isatty   #=> true
  def isatty; end
  alias tty? isatty

  # Returns the current line number in <em>ios</em>.  The stream must be
  # opened for reading. #lineno counts the number of times #gets is called
  # rather than the number of newlines encountered.  The two values will
  # differ if #gets is called with a separator other than newline.
  #
  # Methods that use <code>$/</code> like #each, #lines and #readline will
  # also increment #lineno.
  #
  # See also the <code>$.</code> variable.
  #
  #    f = File.new("testfile")
  #    f.lineno   #=> 0
  #    f.gets     #=> "This is line one\n"
  #    f.lineno   #=> 1
  #    f.gets     #=> "This is line two\n"
  #    f.lineno   #=> 2
  def lineno; end

  # Manually sets the current line number to the given value.
  # <code>$.</code> is updated only on the next read.
  #
  #    f = File.new("testfile")
  #    f.gets                     #=> "This is line one\n"
  #    $.                         #=> 1
  #    f.lineno = 1000
  #    f.lineno                   #=> 1000
  #    $.                         #=> 1         # lineno of last read
  #    f.gets                     #=> "This is line two\n"
  #    $.                         #=> 1001      # lineno of last read
  def lineno=(integer) end

  # Yields +self+ with disabling echo back.
  #
  #   STDIN.noecho(&:gets)
  #
  # will read and return a line without echo back.
  #
  # You must require 'io/console' to use this method.
  def noecho; end

  # Yields +self+ in non-blocking mode.
  #
  # When +false+ is given as an argument, +self+ is yielded in blocking mode.
  # The original mode is restored after the block is executed.
  def nonblock(...) end

  # Enables non-blocking mode on a stream when set to
  # +true+, and blocking mode when set to +false+.
  def nonblock=(boolean) end

  # Returns +true+ if an IO object is in non-blocking mode.
  def nonblock?; end

  # Returns number of bytes that can be read without blocking.
  # Returns zero if no information available.
  def nread; end

  # Flushes output buffer in kernel.
  #
  # You must require 'io/console' to use this method.
  def oflush; end

  # Returns pathname configuration variable using fpathconf().
  #
  # _name_ should be a constant under <code>Etc</code> which begins with <code>PC_</code>.
  #
  # The return value is an integer or nil.
  # nil means indefinite limit.  (fpathconf() returns -1 but errno is not set.)
  #
  #   require 'etc'
  #   IO.pipe {|r, w|
  #     p w.pathconf(Etc::PC_PIPE_BUF) #=> 4096
  #   }
  def pathconf(p1) end

  # Returns the process ID of a child process associated with the stream,
  # which will have been set by IO#popen, or +nil+ if the stream was not
  # created by IO#popen:
  #
  #   pipe = IO.popen("-")
  #   if pipe
  #     $stderr.puts "In parent, child pid is #{pipe.pid}"
  #   else
  #     $stderr.puts "In child, pid is #{$$}"
  #   end
  #
  # Output:
  #
  #   In child, pid is 26209
  #   In parent, child pid is 26209
  def pid; end

  # Seeks to the given +new_position+ (in bytes);
  # see {Position}[#class-IO-label-Position]:
  #
  #   f = File.open('t.txt')
  #   f.tell     # => 0
  #   f.pos = 20 # => 20
  #   f.tell     # => 20
  #
  # Related: IO#seek, IO#tell.
  def pos=(new_position) end

  # Reads <i>maxlen</i> bytes from <em>ios</em> using the pread system call
  # and returns them as a string without modifying the underlying
  # descriptor offset.  This is advantageous compared to combining IO#seek
  # and IO#read in that it is atomic, allowing multiple threads/process to
  # share the same IO object for reading the file at various locations.
  # This bypasses any userspace buffering of the IO layer.
  # If the optional <i>outbuf</i> argument is present, it must
  # reference a String, which will receive the data.
  # Raises SystemCallError on error, EOFError at end of file and
  # NotImplementedError if platform does not implement the system call.
  #
  #    File.write("testfile", "This is line one\nThis is line two\n")
  #    File.open("testfile") do |f|
  #      p f.read           # => "This is line one\nThis is line two\n"
  #      p f.pread(12, 0)   # => "This is line"
  #      p f.pread(9, 8)    # => "line one\n"
  #    end
  def pread(p1, p2, p3 = v3) end

  def pressed?(p1) end

  # Writes the given object(s) to <em>ios</em>. Returns +nil+.
  #
  # The stream must be opened for writing.
  # Each given object that isn't a string will be converted by calling
  # its <code>to_s</code> method.
  # When called without arguments, prints the contents of <code>$_</code>.
  #
  # If the output field separator (<code>$,</code>) is not +nil+,
  # it is inserted between objects.
  # If the output record separator (<code>$\\</code>) is not +nil+,
  # it is appended to the output.
  #
  #    $stdout.print("This is ", 100, " percent.\n")
  #
  # <em>produces:</em>
  #
  #    This is 100 percent.
  def print(...) end

  # Formats and writes to <em>ios</em>, converting parameters under
  # control of the format string. See Kernel#sprintf for details.
  def printf(*args) end

  # If <i>obj</i> is Numeric, write the character whose code is the
  # least-significant byte of <i>obj</i>.  If <i>obj</i> is String,
  # write the first character of <i>obj</i> to <em>ios</em>.  Otherwise,
  # raise TypeError.
  #
  #    $stdout.putc "A"
  #    $stdout.putc 65
  #
  # <em>produces:</em>
  #
  #    AA
  def putc(obj) end

  # Writes the given object(s) to <em>ios</em>.
  # Writes a newline after any that do not already end
  # with a newline sequence. Returns +nil+.
  #
  # The stream must be opened for writing.
  # If called with an array argument, writes each element on a new line.
  # Each given object that isn't a string or array will be converted
  # by calling its +to_s+ method.
  # If called without arguments, outputs a single newline.
  #
  #    $stdout.puts("this", "is", ["a", "test"])
  #
  # <em>produces:</em>
  #
  #    this
  #    is
  #    a
  #    test
  #
  # Note that +puts+ always uses newlines and is not affected
  # by the output record separator (<code>$\\</code>).
  def puts(obj = '', *arg) end

  # Writes the given string to <em>ios</em> at <i>offset</i> using pwrite()
  # system call.  This is advantageous to combining IO#seek and IO#write
  # in that it is atomic, allowing multiple threads/process to share the
  # same IO object for reading the file at various locations.
  # This bypasses any userspace buffering of the IO layer.
  # Returns the number of bytes written.
  # Raises SystemCallError on error and NotImplementedError
  # if platform does not implement the system call.
  #
  #    File.open("out", "w") do |f|
  #      f.pwrite("ABCDEF", 3)   #=> 6
  #    end
  #
  #    File.read("out")          #=> "\u0000\u0000\u0000ABCDEF"
  def pwrite(string, offset) end

  # Yields +self+ within raw mode, and returns the result of the block.
  #
  #   STDIN.raw(&:gets)
  #
  # will read and return a line without echo back and line editing.
  #
  # The parameter +min+ specifies the minimum number of bytes that
  # should be received when a read operation is performed. (default: 1)
  #
  # The parameter +time+ specifies the timeout in _seconds_ with a
  # precision of 1/10 of a second. (default: 0)
  #
  # If the parameter +intr+ is +true+, enables break, interrupt, quit,
  # and suspend special characters.
  #
  # Refer to the manual page of termios for further details.
  #
  # You must require 'io/console' to use this method.
  def raw(min: nil, time: nil, intr: nil) end

  # Enables raw mode, and returns +io+.
  #
  # If the terminal mode needs to be back, use <code>io.raw { ... }</code>.
  #
  # See IO#raw for details on the parameters.
  #
  # You must require 'io/console' to use this method.
  def raw!(min: nil, time: nil, intr: nil) end

  # Reads bytes from the stream (in binary mode):
  #
  # - If +maxlen+ is +nil+, reads all bytes.
  # - Otherwise reads +maxlen+ bytes, if available.
  # - Otherwise reads all bytes.
  #
  # Returns a string (either a new string or the given +out_string+)
  # containing the bytes read.
  # The encoding of the string depends on both +maxLen+ and +out_string+:
  #
  # - +maxlen+ is +nil+: uses internal encoding of +self+
  #   (regardless of whether +out_string+ was given).
  # - +maxlen+ not +nil+:
  #
  #   - +out_string+ given: encoding of +out_string+ not modified.
  #   - +out_string+ not given: ASCII-8BIT is used.
  #
  # <b>Without Argument +out_string+</b>
  #
  # When argument +out_string+ is omitted,
  # the returned value is a new string:
  #
  #   f = File.new('t.txt')
  #   f.read
  #   # => "This is line one.\nThis is the second line.\nThis is the third line.\n"
  #   f.rewind
  #   f.read(40)      # => "This is line one.\r\nThis is the second li"
  #   f.read(40)      # => "ne.\r\nThis is the third line.\r\n"
  #   f.read(40)      # => nil
  #
  # If +maxlen+ is zero, returns an empty string.
  #
  # <b> With Argument +out_string+</b>
  #
  # When argument +out_string+ is given,
  # the returned value is +out_string+, whose content is replaced:
  #
  #   f = File.new('t.txt')
  #   s = 'foo'      # => "foo"
  #   f.read(nil, s) # => "This is line one.\nThis is the second line.\nThis is the third line.\n"
  #   s              # => "This is line one.\nThis is the second line.\nThis is the third line.\n"
  #   f.rewind
  #   s = 'bar'
  #   f.read(40, s)  # => "This is line one.\r\nThis is the second li"
  #   s              # => "This is line one.\r\nThis is the second li"
  #   s = 'baz'
  #   f.read(40, s)  # => "ne.\r\nThis is the third line.\r\n"
  #   s              # => "ne.\r\nThis is the third line.\r\n"
  #   s = 'bat'
  #   f.read(40, s)  # => nil
  #   s              # => ""
  #
  # Note that this method behaves like the fread() function in C.
  # This means it retries to invoke read(2) system calls to read data
  # with the specified maxlen (or until EOF).
  #
  # This behavior is preserved even if the stream is in non-blocking mode.
  # (This method is non-blocking-flag insensitive as other methods.)
  #
  # If you need the behavior like a single read(2) system call,
  # consider #readpartial, #read_nonblock, and #sysread.
  def read(...) end

  # Reads at most <i>maxlen</i> bytes from <em>ios</em> using
  # the read(2) system call after O_NONBLOCK is set for
  # the underlying file descriptor.
  #
  # If the optional <i>outbuf</i> argument is present,
  # it must reference a String, which will receive the data.
  # The <i>outbuf</i> will contain only the received data after the method call
  # even if it is not empty at the beginning.
  #
  # read_nonblock just calls the read(2) system call.
  # It causes all errors the read(2) system call causes: Errno::EWOULDBLOCK, Errno::EINTR, etc.
  # The caller should care such errors.
  #
  # If the exception is Errno::EWOULDBLOCK or Errno::EAGAIN,
  # it is extended by IO::WaitReadable.
  # So IO::WaitReadable can be used to rescue the exceptions for retrying
  # read_nonblock.
  #
  # read_nonblock causes EOFError on EOF.
  #
  # On some platforms, such as Windows, non-blocking mode is not supported
  # on IO objects other than sockets. In such cases, Errno::EBADF will
  # be raised.
  #
  # If the read byte buffer is not empty,
  # read_nonblock reads from the buffer like readpartial.
  # In this case, the read(2) system call is not called.
  #
  # When read_nonblock raises an exception kind of IO::WaitReadable,
  # read_nonblock should not be called
  # until io is readable for avoiding busy loop.
  # This can be done as follows.
  #
  #   # emulates blocking read (readpartial).
  #   begin
  #     result = io.read_nonblock(maxlen)
  #   rescue IO::WaitReadable
  #     IO.select([io])
  #     retry
  #   end
  #
  # Although IO#read_nonblock doesn't raise IO::WaitWritable.
  # OpenSSL::Buffering#read_nonblock can raise IO::WaitWritable.
  # If IO and SSL should be used polymorphically,
  # IO::WaitWritable should be rescued too.
  # See the document of OpenSSL::Buffering#read_nonblock for sample code.
  #
  # Note that this method is identical to readpartial
  # except the non-blocking flag is set.
  #
  # By specifying a keyword argument _exception_ to +false+, you can indicate
  # that read_nonblock should not raise an IO::WaitReadable exception, but
  # return the symbol +:wait_readable+ instead. At EOF, it will return nil
  # instead of raising EOFError.
  def read_nonblock(...) end

  # Reads a byte as with IO#getbyte, but raises an EOFError on end of
  # file.
  def readbyte; end

  # Reads a one-character string from <em>ios</em>. Raises an
  # EOFError on end of file.
  #
  #    f = File.new("testfile")
  #    f.readchar   #=> "h"
  #    f.readchar   #=> "e"
  def readchar; end

  # Reads a line as with IO#gets, but raises an EOFError on end of file.
  def readline(...) end

  # Reads all of the lines in <em>ios</em>, and returns them in
  # an array. Lines are separated by the optional <i>sep</i>. If
  # <i>sep</i> is +nil+, the rest of the stream is returned
  # as a single record.
  # If the first argument is an integer, or an
  # optional second argument is given, the returning string would not be
  # longer than the given value in bytes. The stream must be opened for
  # reading or an IOError will be raised.
  #
  #    f = File.new("testfile")
  #    f.readlines[0]   #=> "This is line one\n"
  #
  #    f = File.new("testfile", chomp: true)
  #    f.readlines[0]   #=> "This is line one"
  #
  # See IO.readlines for details about getline_args.
  def readlines(...) end

  # Reads up to +maxlen+ bytes from the stream;
  # returns a string (either a new string or the given +out_string+).
  # Its encoding is:
  #
  # - The unchanged encoding of +out_string+, if +out_string+ is given.
  # - ASCII-8BIT, otherwise.
  #
  # - Contains +maxlen+ bytes from the stream, if available.
  # - Otherwise contains all available bytes, if any available.
  # - Otherwise is an empty string.
  #
  # With the single non-negative integer argument +maxlen+ given,
  # returns a new string:
  #
  #   f = File.new('t.txt')
  #   f.readpartial(30) # => "This is line one.\nThis is the"
  #   f.readpartial(30) # => " second line.\nThis is the thi"
  #   f.readpartial(30) # => "rd line.\n"
  #   f.eof             # => true
  #   f.readpartial(30) # Raises EOFError.
  #
  # With both argument +maxlen+ and string argument +out_string+ given,
  # returns modified +out_string+:
  #
  #   f = File.new('t.txt')
  #   s = 'foo'
  #   f.readpartial(30, s) # => "This is line one.\nThis is the"
  #   s = 'bar'
  #   f.readpartial(0, s)  # => ""
  #
  # This method is useful for a stream such as a pipe, a socket, or a tty.
  # It blocks only when no data is immediately available.
  # This means that it blocks only when _all_ of the following are true:
  #
  # - The byte buffer in the stream is empty.
  # - The content of the stream is empty.
  # - The stream is not at EOF.
  #
  # When blocked, the method waits for either more data or EOF on the stream:
  #
  # - If more data is read, the method returns the data.
  # - If EOF is reached, the method raises EOFError.
  #
  # When not blocked, the method responds immediately:
  #
  # - Returns data from the buffer if there is any.
  # - Otherwise returns data from the stream if there is any.
  # - Otherwise raises EOFError if the stream has reached EOF.
  #
  # Note that this method is similar to sysread. The differences are:
  #
  # - If the byte buffer is not empty, read from the byte buffer
  #   instead of "sysread for buffered IO (IOError)".
  # - It doesn't cause Errno::EWOULDBLOCK and Errno::EINTR.  When
  #   readpartial meets EWOULDBLOCK and EINTR by read system call,
  #   readpartial retries the system call.
  #
  # The latter means that readpartial is non-blocking-flag insensitive.
  # It blocks on the situation IO#sysread causes Errno::EWOULDBLOCK as
  # if the fd is blocking mode.
  #
  # Examples:
  #
  #    #                        # Returned      Buffer Content    Pipe Content
  #    r, w = IO.pipe           #
  #    w << 'abc'               #               ""                "abc".
  #    r.readpartial(4096)      # => "abc"      ""                ""
  #    r.readpartial(4096)      # (Blocks because buffer and pipe are empty.)
  #
  #    #                        # Returned      Buffer Content    Pipe Content
  #    r, w = IO.pipe           #
  #    w << 'abc'               #               ""                "abc"
  #    w.close                  #               ""                "abc" EOF
  #    r.readpartial(4096)      # => "abc"      ""                 EOF
  #    r.readpartial(4096)      # raises EOFError
  #
  #    #                        # Returned      Buffer Content    Pipe Content
  #    r, w = IO.pipe           #
  #    w << "abc\ndef\n"        #               ""                "abc\ndef\n"
  #    r.gets                   # => "abc\n"    "def\n"           ""
  #    w << "ghi\n"             #               "def\n"           "ghi\n"
  #    r.readpartial(4096)      # => "def\n"    ""                "ghi\n"
  #    r.readpartial(4096)      # => "ghi\n"    ""                ""
  def readpartial(...) end

  # Returns +true+ if input available without blocking, or +false+.
  def ready?; end

  # Reassociates <em>ios</em> with the I/O stream given in
  # <i>other_IO</i> or to a new stream opened on <i>path</i>. This may
  # dynamically change the actual class of this stream.
  # The +mode+ and +opt+ parameters accept the same values as IO.open.
  #
  #    f1 = File.new("testfile")
  #    f2 = File.new("testfile")
  #    f2.readlines[0]   #=> "This is line one\n"
  #    f2.reopen(f1)     #=> #<File:testfile>
  #    f2.readlines[0]   #=> "This is line one\n"
  def reopen(...) end

  # Repositions the stream to its beginning,
  # setting both the position and the line number to zero;
  # see {Position}[#class-IO-label-Position]
  # and {Line Number}[#class-IO-label-Line+Number]:
  #
  #   f = File.open('t.txt')
  #   f.tell     # => 0
  #   f.lineno   # => 0
  #   f.readline # => "This is line one.\n"
  #   f.tell     # => 19
  #   f.lineno   # => 1
  #   f.rewind   # => 0
  #   f.tell     # => 0
  #   f.lineno   # => 0
  #
  # Note that this method cannot be used with streams such as pipes, ttys, and sockets.
  def rewind; end

  def scroll_backward(p1) end

  def scroll_forward(p1) end

  # Seeks to the position given by integer +offset+
  # (see {Position}[#class-IO-label-Position])
  # and constant +whence+, which is one of:
  #
  # - +:CUR+ or <tt>IO::SEEK_CUR</tt>:
  #   Repositions the stream to its current position plus the given +offset+:
  #
  #     f = File.open('t.txt')
  #     f.tell            # => 0
  #     f.seek(20, :CUR)  # => 0
  #     f.tell            # => 20
  #     f.seek(-10, :CUR) # => 0
  #     f.tell            # => 10
  #
  # - +:END+ or <tt>IO::SEEK_END</tt>:
  #   Repositions the stream to its end plus the given +offset+:
  #
  #     f = File.open('t.txt')
  #     f.tell            # => 0
  #     f.seek(0, :END)   # => 0  # Repositions to stream end.
  #     f.tell            # => 70
  #     f.seek(-20, :END) # => 0
  #     f.tell            # => 50
  #     f.seek(-40, :END) # => 0
  #     f.tell            # => 30
  #
  # - +:SET+ or <tt>IO:SEEK_SET</tt>:
  #   Repositions the stream to the given +offset+:
  #
  #     f = File.open('t.txt')
  #     f.tell            # => 0
  #     f.seek(20, :SET) # => 0
  #     f.tell           # => 20
  #     f.seek(40, :SET) # => 0
  #     f.tell           # => 40
  #
  # Related: IO#pos=, IO#tell.
  def seek(offset, whence = IO::SEEK_SET) end

  # If single argument is specified, read string from io is tagged
  # with the encoding specified.  If encoding is a colon separated two
  # encoding names "A:B", the read string is converted from encoding A
  # (external encoding) to encoding B (internal encoding), then tagged
  # with B.  If two arguments are specified, those must be encoding
  # objects or encoding names, and the first one is the external encoding, and the
  # second one is the internal encoding.
  # If the external encoding and the internal encoding is specified,
  # optional hash argument specify the conversion option.
  def set_encoding(...) end

  # Checks if +ios+ starts with a BOM, and then consumes it and sets
  # the external encoding.  Returns the result encoding if found, or
  # nil.  If +ios+ is not binmode or its encoding has been set
  # already, an exception will be raised.
  #
  #   File.write("bom.txt", "\u{FEFF}abc")
  #   ios = File.open("bom.txt", "rb")
  #   ios.set_encoding_by_bom    #=>  #<Encoding:UTF-8>
  #
  #   File.write("nobom.txt", "abc")
  #   ios = File.open("nobom.txt", "rb")
  #   ios.set_encoding_by_bom    #=>  nil
  def set_encoding_by_bom; end

  # Returns status information for <em>ios</em> as an object of type
  # File::Stat.
  #
  #    f = File.new("testfile")
  #    s = f.stat
  #    "%o" % s.mode   #=> "100644"
  #    s.blksize       #=> 4096
  #    s.atime         #=> Wed Apr 09 08:53:54 CDT 2003
  def stat; end

  # Returns the current sync mode of the stream.
  # When sync mode is true, all output is immediately flushed to the underlying
  # operating system and is not buffered by Ruby internally. See also #fsync.
  #
  #   f = File.open('t.tmp', 'w')
  #   f.sync # => false
  #   f.sync = true
  #   f.sync # => true
  def sync; end

  # Sets the _sync_ _mode_ for the stream to the given value;
  # returns the given value.
  #
  # Values for the sync mode:
  #
  # - +true+: All output is immediately flushed to the
  #   underlying operating system and is not buffered internally.
  # - +false+: Output may be buffered internally.
  #
  # Example;
  #
  #   f = File.open('t.tmp', 'w')
  #   f.sync # => false
  #   f.sync = true
  #   f.sync # => true
  #
  # Related: IO#fsync.
  def sync=(boolean) end

  # Reads <i>maxlen</i> bytes from <em>ios</em> using a low-level
  # read and returns them as a string.  Do not mix with other methods
  # that read from <em>ios</em> or you may get unpredictable results.
  #
  # If the optional _outbuf_ argument is present,
  # it must reference a String, which will receive the data.
  # The _outbuf_ will contain only the received data after the method call
  # even if it is not empty at the beginning.
  #
  # Raises SystemCallError on error and EOFError at end of file.
  #
  #    f = File.new("testfile")
  #    f.sysread(16)   #=> "This is line one"
  def sysread(p1, p2 = v2) end

  # Seeks to a given <i>offset</i> in the stream according to the value
  # of <i>whence</i> (see IO#seek for values of <i>whence</i>). Returns
  # the new offset into the file.
  #
  #    f = File.new("testfile")
  #    f.sysseek(-13, IO::SEEK_END)   #=> 53
  #    f.sysread(10)                  #=> "And so on."
  def sysseek(offset, whence = IO::SEEK_SET) end

  # Writes the given string to <em>ios</em> using a low-level write.
  # Returns the number of bytes written. Do not mix with other methods
  # that write to <em>ios</em> or you may get unpredictable results.
  # Raises SystemCallError on error.
  #
  #    f = File.new("out", "w")
  #    f.syswrite("ABCDEF")   #=> 6
  def syswrite(string) end

  # Returns the current position (in bytes) in +self+
  # (see {Position}[#class-IO-label-Position]):
  #
  #   f = File.new('t.txt')
  #   f.tell     # => 0
  #   f.readline # => "This is line one.\n"
  #   f.tell     # => 19
  #
  # Related: IO#pos=, IO#seek.
  #
  # IO#pos is an alias for IO#tell.
  def tell; end
  alias pos tell

  # Returns +self+.
  def to_io; end

  # Pushes back bytes (passed as a parameter) onto <em>ios</em>,
  # such that a subsequent buffered read will return it.
  # It is only guaranteed to support a single byte, and only if ungetbyte
  # or ungetc has not already been called on <em>ios</em> since the previous
  # read of at least a single byte from <em>ios</em>.
  # However, it can support additional bytes if there is space in the
  # internal buffer to allow for it.
  #
  #    f = File.new("testfile")   #=> #<File:testfile>
  #    b = f.getbyte              #=> 0x38
  #    f.ungetbyte(b)             #=> nil
  #    f.getbyte                  #=> 0x38
  #
  # If given an integer, only uses the lower 8 bits of the integer as the byte
  # to push.
  #
  #    f = File.new("testfile")   #=> #<File:testfile>
  #    f.ungetbyte(0x102)         #=> nil
  #    f.getbyte                  #=> 0x2
  #
  # Calling this method prepends to the existing buffer, even if the method
  # has already been called previously:
  #
  #    f = File.new("testfile")   #=> #<File:testfile>
  #    f.ungetbyte("ab")          #=> nil
  #    f.ungetbyte("cd")          #=> nil
  #    f.read(5)                  #=> "cdab8"
  #
  # Has no effect with unbuffered reads (such as IO#sysread).
  def ungetbyte(...) end

  # Pushes back characters (passed as a parameter) onto <em>ios</em>,
  # such that a subsequent buffered read will return it.
  # It is only guaranteed to support a single byte, and only if ungetbyte
  # or ungetc has not already been called on <em>ios</em> since the previous
  # read of at least a single byte from <em>ios</em>.
  # However, it can support additional bytes if there is space in the
  # internal buffer to allow for it.
  #
  #    f = File.new("testfile")   #=> #<File:testfile>
  #    c = f.getc                 #=> "8"
  #    f.ungetc(c)                #=> nil
  #    f.getc                     #=> "8"
  #
  # If given an integer, the integer must represent a valid codepoint in the
  # external encoding of <em>ios</em>.
  #
  # Calling this method prepends to the existing buffer, even if the method
  # has already been called previously:
  #
  #    f = File.new("testfile")   #=> #<File:testfile>
  #    f.ungetc("ab")             #=> nil
  #    f.ungetc("cd")             #=> nil
  #    f.read(5)                  #=> "cdab8"
  #
  # Has no effect with unbuffered reads (such as IO#sysread).
  def ungetc(...) end

  # Waits until the IO becomes ready for the specified events and returns the
  # subset of events that become ready, or +false+ when times out.
  #
  # The events can be a bit mask of +IO::READABLE+, +IO::WRITABLE+ or
  # +IO::PRIORITY+.
  #
  # Returns +true+ immediately when buffered data is available.
  #
  # Optional parameter +mode+ is one of +:read+, +:write+, or
  # +:read_write+.
  def wait(...) end

  # Waits until IO is priority and returns +true+ or
  # +false+ when times out.
  def wait_priority(...) end

  # Waits until IO is readable and returns +true+, or
  # +false+ when times out.
  # Returns +true+ immediately when buffered data is available.
  def wait_readable(...) end

  # Waits until IO is writable and returns +true+ or
  # +false+ when times out.
  def wait_writable(...) end

  # Returns console size.
  #
  # You must require 'io/console' to use this method.
  def winsize; end

  # Tries to set console size.  The effect depends on the platform and
  # the running environment.
  #
  # You must require 'io/console' to use this method.
  def winsize=(p1) end

  # Writes each of the given +objects+ to +self+,
  # which must be opened for writing (see {Modes}[#class-IO-label-Modes]);
  # returns the total number bytes written;
  # each of +objects+ that is not a string is converted via method +to_s+:
  #
  #   $stdout.write('Hello', ', ', 'World!', "\n") # => 14
  #   $stdout.write('foo', :bar, 2, "\n")          # => 8
  #
  # Output:
  #
  #   Hello, World!
  #   foobar2
  def write(*objects) end

  # Writes the given string to <em>ios</em> using
  # the write(2) system call after O_NONBLOCK is set for
  # the underlying file descriptor.
  #
  # It returns the number of bytes written.
  #
  # write_nonblock just calls the write(2) system call.
  # It causes all errors the write(2) system call causes: Errno::EWOULDBLOCK, Errno::EINTR, etc.
  # The result may also be smaller than string.length (partial write).
  # The caller should care such errors and partial write.
  #
  # If the exception is Errno::EWOULDBLOCK or Errno::EAGAIN,
  # it is extended by IO::WaitWritable.
  # So IO::WaitWritable can be used to rescue the exceptions for retrying write_nonblock.
  #
  #   # Creates a pipe.
  #   r, w = IO.pipe
  #
  #   # write_nonblock writes only 65536 bytes and return 65536.
  #   # (The pipe size is 65536 bytes on this environment.)
  #   s = "a" * 100000
  #   p w.write_nonblock(s)     #=> 65536
  #
  #   # write_nonblock cannot write a byte and raise EWOULDBLOCK (EAGAIN).
  #   p w.write_nonblock("b")   # Resource temporarily unavailable (Errno::EAGAIN)
  #
  # If the write buffer is not empty, it is flushed at first.
  #
  # When write_nonblock raises an exception kind of IO::WaitWritable,
  # write_nonblock should not be called
  # until io is writable for avoiding busy loop.
  # This can be done as follows.
  #
  #   begin
  #     result = io.write_nonblock(string)
  #   rescue IO::WaitWritable, Errno::EINTR
  #     IO.select(nil, [io])
  #     retry
  #   end
  #
  # Note that this doesn't guarantee to write all data in string.
  # The length written is reported as result and it should be checked later.
  #
  # On some platforms such as Windows, write_nonblock is not supported
  # according to the kind of the IO object.
  # In such cases, write_nonblock raises <code>Errno::EBADF</code>.
  #
  # By specifying a keyword argument _exception_ to +false+, you can indicate
  # that write_nonblock should not raise an IO::WaitWritable exception, but
  # return the symbol +:wait_writable+ instead.
  def write_nonblock(...) end

  # IO::Buffer is a low-level efficient buffer for input/output. There are three
  # ways of using buffer:
  #
  # * Create an empty buffer with ::new, fill it with data using #copy or
  #   #set_value, #set_string, get data with #get_string;
  # * Create a buffer mapped to some string with ::for, then it could be used
  #   both for reading with #get_string or #get_value, and writing (writing will
  #   change the source string, too);
  # * Create a buffer mapped to some file with ::map, then it could be used for
  #   reading and writing the underlying file.
  #
  # Interaction with string and file memory is performed by efficient low-level
  # C mechanisms like `memcpy`.
  #
  # The class is meant to be an utility for implementing more high-level mechanisms
  # like Fiber::SchedulerInterface#io_read and Fiber::SchedulerInterface#io_write.
  #
  # <b>Examples of usage:</b>
  #
  # Empty buffer:
  #
  #   buffer = IO::Buffer.new(8)  # create empty 8-byte buffer
  #   #  =>
  #   # #<IO::Buffer 0x0000555f5d1a5c50+8 INTERNAL>
  #   # ...
  #   buffer
  #   #  =>
  #   # <IO::Buffer 0x0000555f5d156ab0+8 INTERNAL>
  #   # 0x00000000  00 00 00 00 00 00 00 00
  #   buffer.set_string('test', 2) # put there bytes of the "test" string, starting from offset 2
  #   # => 4
  #   buffer.get_string  # get the result
  #   # => "\x00\x00test\x00\x00"
  #
  # \Buffer from string:
  #
  #   string = 'data'
  #   buffer = IO::Buffer.for(str)
  #   #  =>
  #   # #<IO::Buffer 0x00007f3f02be9b18+4 SLICE>
  #   # ...
  #   buffer
  #   #  =>
  #   # #<IO::Buffer 0x00007f3f02be9b18+4 SLICE>
  #   # 0x00000000  64 61 74 61                                     data
  #
  #   buffer.get_string(2)  # read content starting from offset 2
  #   # => "ta"
  #   buffer.set_string('---', 1) # write content, starting from offset 1
  #   # => 3
  #   buffer
  #   #  =>
  #   # #<IO::Buffer 0x00007f3f02be9b18+4 SLICE>
  #   # 0x00000000  64 2d 2d 2d                                     d---
  #   string  # original string changed, too
  #   # => "d---"
  #
  # \Buffer from file:
  #
  #   File.write('test.txt', 'test data')
  #   # => 9
  #   buffer = IO::Buffer.map(File.open('test.txt'))
  #   #  =>
  #   # #<IO::Buffer 0x00007f3f0768c000+9 MAPPED IMMUTABLE>
  #   # ...
  #   buffer.get_string(5, 2) # read 2 bytes, starting from offset 5
  #   # => "da"
  #   buffer.set_string('---', 1) # attempt to write
  #   # in `set_string': Buffer is not writable! (IO::Buffer::AccessError)
  #
  #   # To create writable file-mapped buffer
  #   # Open file for read-write, pass size, offset, and flags=0
  #   buffer = IO::Buffer.map(File.open('test.txt', 'r+'), 9, 0, 0)
  #   buffer.set_string('---', 1)
  #   # => 3 -- bytes written
  #   File.read('test.txt')
  #   # => "t--- data"
  #
  # <b>The class is experimental and the interface is subject to change.</b>
  class Buffer
    include Comparable

    BIG_ENDIAN = _
    DEFAULT_SIZE = _
    EXTERNAL = _
    HOST_ENDIAN = _
    INTERNAL = _
    LITTLE_ENDIAN = _
    LOCKED = _
    MAPPED = _
    NETWORK_ENDIAN = _
    PAGE_SIZE = _
    PRIVATE = _
    READONLY = _

    # Creates a IO::Buffer from the given string's memory. Without a block a
    # frozen internal copy of the string is created efficiently and used as the
    # buffer source. When a block is provided, the buffer is associated directly
    # with the string's internal data and updating the buffer will update the
    # string.
    #
    # Until #free is invoked on the buffer, either explicitly or via the garbage
    # collector, the source string will be locked and cannot be modified.
    #
    # If the string is frozen, it will create a read-only buffer which cannot be
    # modified. If the string is shared, it may trigger a copy-on-write when
    # using the block form.
    #
    #   string = 'test'
    #   buffer = IO::Buffer.for(string)
    #   buffer.external? #=> true
    #
    #   buffer.get_string(0, 1)
    #   # => "t"
    #   string
    #   # => "best"
    #
    #   buffer.resize(100)
    #   # in `resize': Cannot resize external buffer! (IO::Buffer::AccessError)
    #
    #   IO::Buffer.for(string) do |buffer|
    #     buffer.set_string("T")
    #     string
    #     # => "Test"
    #   end
    def self.for(string) end

    # Create an IO::Buffer for reading from +file+ by memory-mapping the file.
    # +file_io+ should be a +File+ instance, opened for reading.
    #
    # Optional +size+ and +offset+ of mapping can be specified.
    #
    # By default, the buffer would be immutable (read only); to create a writable
    # mapping, you need to open a file in read-write mode, and explicitly pass
    # +flags+ argument without IO::Buffer::IMMUTABLE.
    #
    #    File.write('test.txt', 'test')
    #
    #    buffer = IO::Buffer.map(File.open('test.txt'), nil, 0, IO::Buffer::READONLY)
    #    # => #<IO::Buffer 0x00000001014a0000+4 MAPPED READONLY>
    #
    #    buffer.readonly?   # => true
    #
    #    buffer.get_string
    #    # => "test"
    #
    #    buffer.set_string('b', 0)
    #    # `set_string': Buffer is not writable! (IO::Buffer::AccessError)
    #
    #    # create read/write mapping: length 4 bytes, offset 0, flags 0
    #    buffer = IO::Buffer.map(File.open('test.txt', 'r+'), 4, 0)
    #    buffer.set_string('b', 0)
    #    # => 1
    #
    #    # Check it
    #    File.read('test.txt')
    #    # => "best"
    #
    # Note that some operating systems may not have cache coherency between mapped
    # buffers and file reads.
    def self.map(*args) end

    # Create a new zero-filled IO::Buffer of +size+ bytes.
    # By default, the buffer will be _internal_: directly allocated chunk
    # of the memory. But if the requested +size+ is more than OS-specific
    # IO::Bufer::PAGE_SIZE, the buffer would be allocated using the
    # virtual memory mechanism (anonymous +mmap+ on Unix, +VirtualAlloc+
    # on Windows). The behavior can be forced by passing IO::Buffer::MAPPED
    # as a second parameter.
    #
    # Examples
    #
    #   buffer = IO::Buffer.new(4)
    #   # =>
    #   #  #<IO::Buffer 0x000055b34497ea10+4 INTERNAL>
    #   #  0x00000000  00 00 00 00                                     ....
    #
    #   buffer.get_string(0, 1) # => "\x00"
    #
    #   buffer.set_string("test")
    #   buffer
    #   #  =>
    #   # #<IO::Buffer 0x000055b34497ea10+4 INTERNAL>
    #   # 0x00000000  74 65 73 74                                     test
    def initialize(*args) end

    # Buffers are compared by size and exact contents of the memory they are
    # referencing using +memcmp+.
    def <=>(other) end

    # Fill buffer with +value+, starting with +offset+ and going for +length+
    # bytes.
    #
    #   buffer = IO::Buffer.for('test')
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 SLICE>
    #   #   0x00000000  74 65 73 74         test
    #
    #   buffer.clear
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 SLICE>
    #   #   0x00000000  00 00 00 00         ....
    #
    #   buf.clear(1) # fill with 1
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 SLICE>
    #   #   0x00000000  01 01 01 01         ....
    #
    #   buffer.clear(2, 1, 2) # fill with 2, starting from offset 1, for 2 bytes
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 SLICE>
    #   #   0x00000000  01 02 02 01         ....
    #
    #   buffer.clear(2, 1) # fill with 2, starting from offset 1
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 SLICE>
    #   #   0x00000000  01 02 02 02         ....
    def clear(*args) end

    # Efficiently copy data from a source IO::Buffer into the buffer,
    # at +offset+ using +memcpy+. For copying String instances, see #set_string.
    #
    #   buffer = IO::Buffer.new(32)
    #   #  =>
    #   # #<IO::Buffer 0x0000555f5ca22520+32 INTERNAL>
    #   # 0x00000000  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ................
    #   # 0x00000010  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ................  *
    #
    #   buffer.copy(IO::Buffer.for("test"), 8)
    #   # => 4 -- size of data copied
    #   buffer
    #   #  =>
    #   # #<IO::Buffer 0x0000555f5cf8fe40+32 INTERNAL>
    #   # 0x00000000  00 00 00 00 00 00 00 00 74 65 73 74 00 00 00 00 ........test....
    #   # 0x00000010  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ................ *
    #
    # #copy can be used to put data into strings associated with buffer:
    #
    #   string= "data:    "
    #   # => "data:    "
    #   buffer = IO::Buffer.for(str)
    #   buffer.copy(IO::Buffer.for("test"), 5)
    #   # => 4
    #   string
    #   # => "data:test"
    #
    # Attempt to copy into a read-only buffer will fail:
    #
    #   File.write('test.txt', 'test')
    #   buffer = IO::Buffer.map(File.open('test.txt'), nil, 0, IO::Buffer::READONLY)
    #   buffer.copy(IO::Buffer.for("test"), 8)
    #   # in `copy': Buffer is not writable! (IO::Buffer::AccessError)
    #
    # See ::map for details of creation of mutable file mappings, this will
    # work:
    #
    #   buffer = IO::Buffer.map(File.open('test.txt', 'r+'))
    #   buffer.copy("boom", 0)
    #   # => 4
    #   File.read('test.txt')
    #   # => "boom"
    #
    # Attempt to copy the data which will need place outside of buffer's
    # bounds will fail:
    #
    #   buffer = IO::Buffer.new(2)
    #   buffer.copy('test', 0)
    #   # in `copy': Specified offset+length exceeds source size! (ArgumentError)
    def copy(*args) end

    #  If the buffer is _external_, meaning it references from memory which is not
    #  allocated or mapped by the buffer itself.
    #
    #  A buffer created using ::for has an external reference to the string's
    #  memory.
    #
    # External buffer can't be resized.
    def empty?; end

    def external?; end

    # If the buffer references memory, release it back to the operating system.
    # * for a _mapped_ buffer (e.g. from file): unmap.
    # * for a buffer created from scratch: free memory.
    # * for a buffer created from string: undo the association.
    #
    # After the buffer is freed, no further operations can't be performed on it.
    #
    #    buffer = IO::Buffer.for('test')
    #    buffer.free
    #    # => #<IO::Buffer 0x0000000000000000+0 NULL>
    #
    #    buffer.get_value(:U8, 0)
    #    # in `get_value': The buffer is not allocated! (IO::Buffer::AllocationError)
    #
    #    buffer.get_string
    #    # in `get_string': The buffer is not allocated! (IO::Buffer::AllocationError)
    #
    #    buffer.null?
    #    # => true
    #
    # You can resize a freed buffer to re-allocate it.
    def free; end

    # Read a chunk or all of the buffer into a string, in the specified
    # +encoding+. If no encoding is provided +Encoding::BINARY+ is used.
    #
    #    buffer = IO::Buffer.for('test')
    #    buffer.get_string
    #    # => "test"
    #    buffer.get_string(2)
    #    # => "st"
    #    buffer.get_string(2, 1)
    #    # => "s"
    def get_string(*args) end

    # Read from buffer a value of +type+ at +offset+. +type+ should be one
    # of symbols:
    #
    # * +:U8+: unsigned integer, 1 byte
    # * +:S8+: signed integer, 1 byte
    # * +:u16+: unsigned integer, 2 bytes, little-endian
    # * +:U16+: unsigned integer, 2 bytes, big-endian
    # * +:s16+: signed integer, 2 bytes, little-endian
    # * +:S16+: signed integer, 2 bytes, big-endian
    # * +:u32+: unsigned integer, 4 bytes, little-endian
    # * +:U32+: unsigned integer, 4 bytes, big-endian
    # * +:s32+: signed integer, 4 bytes, little-endian
    # * +:S32+: signed integer, 4 bytes, big-endian
    # * +:u64+: unsigned integer, 8 bytes, little-endian
    # * +:U64+: unsigned integer, 8 bytes, big-endian
    # * +:s64+: signed integer, 8 bytes, little-endian
    # * +:S64+: signed integer, 8 bytes, big-endian
    # * +:f32+: float, 4 bytes, little-endian
    # * +:F32+: float, 4 bytes, big-endian
    # * +:f64+: double, 8 bytes, little-endian
    # * +:F64+: double, 8 bytes, big-endian
    #
    # Example:
    #
    #   string = [1.5].pack('f')
    #   # => "\x00\x00\xC0?"
    #   IO::Buffer.for(string).get_value(:f32, 0)
    #   # => 1.5
    def get_value(type, offset) end

    def hexdump; end

    def inspect; end

    # If the buffer is _internal_, meaning it references memory allocated by the
    # buffer itself.
    #
    # An internal buffer is not associated with any external memory (e.g. string)
    # or file mapping.
    #
    # Internal buffers are created using ::new and is the default when the
    # requested size is less than the IO::Buffer::PAGE_SIZE and it was not
    # requested to be mapped on creation.
    #
    # Internal buffers can be resized, and such an operation will typically
    # invalidate all slices, but not always.
    def internal?; end

    # Allows to process a buffer in exclusive way, for concurrency-safety. While
    # the block is performed, the buffer is considered locked, and no other code
    # can enter the lock. Also, locked buffer can't be changed with #resize or
    # #free.
    #
    #   buffer = IO::Buffer.new(4)
    #   buffer.locked? #=> false
    #
    #   Fiber.schedule do
    #     buffer.locked do
    #       buffer.write(io) # theoretical system call interface
    #     end
    #   end
    #
    #   Fiber.schedule do
    #     # in `locked': Buffer already locked! (IO::Buffer::LockedError)
    #     buffer.locked do
    #       buffer.set_string(...)
    #     end
    #   end
    #
    # The following operations acquire a lock: #resize, #free.
    #
    # Locking is not thread safe. It is designed as a safety net around
    # non-blocking system calls. You can only share a buffer between threads with
    # appropriate synchronisation techniques.
    def locked; end

    # If the buffer is _locked_, meaning it is inside #locked block execution.
    # Locked buffer can't be resized or freed, and another lock can't be acquired
    # on it.
    #
    # Locking is not thread safe, but is a semantic used to ensure buffers don't
    # move while being used by a system call.
    #
    #   buffer.locked do
    #     buffer.write(io) # theoretical system call interface
    #   end
    def locked?; end

    # If the buffer is _mapped_, meaning it references memory mapped by the
    # buffer.
    #
    # Mapped buffers are either anonymous, if created by ::new with the
    # IO::Buffer::MAPPED flag or if the size was at least IO::Buffer::PAGE_SIZE,
    # or backed by a file if created with ::map.
    #
    # Mapped buffers can usually be resized, and such an operation will typically
    # invalidate all slices, but not always.
    def mapped?; end

    # If the buffer was freed with #free or was never allocated in the first
    # place.
    def null?; end

    def pread(p1, p2, p3) end

    def pwrite(p1, p2, p3) end

    def read(p1, p2) end

    def readonly?; end

    # Resizes a buffer to a +new_size+ bytes, preserving its content.
    # Depending on the old and new size, the memory area associated with
    # the buffer might be either extended, or rellocated at different
    # address with content being copied.
    #
    #   buffer = IO::Buffer.new(4)
    #   buffer.set_string("test", 0)
    #   buffer.resize(8) # resize to 8 bytes
    #   #  =>
    #   # #<IO::Buffer 0x0000555f5d1a1630+8 INTERNAL>
    #   # 0x00000000  74 65 73 74 00 00 00 00                         test....
    #
    # External buffer (created with ::for), and locked buffer
    # can not be resized.
    def resize(new_size) end

    def set_string(*args) end

    # Write to a buffer a +value+ of +type+ at +offset+. +type+ should be one of
    # symbols described in #get_value.
    #
    #   buffer = IO::Buffer.new(8)
    #   #  =>
    #   # #<IO::Buffer 0x0000555f5c9a2d50+8 INTERNAL>
    #   # 0x00000000  00 00 00 00 00 00 00 00
    #   buffer.set_value(:U8, 1, 111)
    #   # => 1
    #   buffer
    #   #  =>
    #   # #<IO::Buffer 0x0000555f5c9a2d50+8 INTERNAL>
    #   # 0x00000000  00 6f 00 00 00 00 00 00                         .o......
    #
    # Note that if the +type+ is integer and +value+ is Float, the implicit truncation is performed:
    #
    #   buffer = IO::Buffer.new(8)
    #   buffer.set_value(:U32, 0, 2.5)
    #   buffer
    #   #   =>
    #   #  #<IO::Buffer 0x0000555f5c9a2d50+8 INTERNAL>
    #   #  0x00000000  00 00 00 02 00 00 00 00
    #   #                       ^^ the same as if we'd pass just integer 2
    def set_value(type, offset, value) end

    # Returns the size of the buffer that was explicitly set (on creation with ::new
    # or on #resize), or deduced on buffer's creation from string or file.
    def size; end

    # Produce another IO::Buffer which is a slice (or view into) the current one
    # starting at +offset+ bytes and going for +length+ bytes.
    #
    # The slicing happens without copying of memory, and the slice keeps being
    # associated with the original buffer's source (string, or file), if any.
    #
    # Raises RuntimeError if the <tt>offset+length<tt> is out of the current
    # buffer's bounds.
    #
    #    string = 'test'
    #    buffer = IO::Buffer.for(string)
    #
    #    slice = buffer.slice(1, 2)
    #    # =>
    #    #  #<IO::Buffer 0x00007fc3d34ebc49+2 SLICE>
    #    #  0x00000000  65 73                                           es
    #
    #    # Put "o" into 0s position of the slice
    #    slice.set_string('o', 0)
    #    slice
    #    # =>
    #    #  #<IO::Buffer 0x00007fc3d34ebc49+2 SLICE>
    #    #  0x00000000  6f 73                                           os
    #
    #    # it is also visible at position 1 of the original buffer
    #    buffer
    #    # =>
    #    #  #<IO::Buffer 0x00007fc3d31e2d80+4 SLICE>
    #    #  0x00000000  74 6f 73 74                                     tost
    #
    #    # ...and original string
    #    string
    #    # => tost
    def slice(offset, length) end

    # Short representation of the buffer. It includes the address, size and
    # symbolic flags. This format is subject to change.
    #
    #   puts IO::Buffer.new(4) # uses to_s internally
    #   # #<IO::Buffer 0x000055769f41b1a0+4 INTERNAL>
    def to_s; end

    # Transfers ownership to a new buffer, deallocating the current one.
    #
    #    buffer = IO::Buffer.new('test')
    #    other = buffer.transfer
    #    other
    #    #  =>
    #    # #<IO::Buffer 0x00007f136a15f7b0+4 SLICE>
    #    # 0x00000000  74 65 73 74                                     test
    #    buffer
    #    #  =>
    #    # #<IO::Buffer 0x0000000000000000+0 NULL>
    #    buffer.null?
    #    # => true
    def transfer; end

    # Returns whether the buffer data is accessible.
    #
    # A buffer becomes invalid if it is a slice of another buffer which has been
    # freed.
    def valid?; end

    def write(p1, p2) end

    class AccessError < RuntimeError
    end

    class AllocationError < RuntimeError
    end

    class InvalidatedError < RuntimeError
    end

    class LockedError < RuntimeError
    end
  end

  class ConsoleMode
  end

  # exception to wait for reading by EAGAIN. see IO.select.
  class EAGAINWaitReadable < Errno::EAGAIN
    include IO::WaitReadable
  end

  # exception to wait for writing by EAGAIN. see IO.select.
  class EAGAINWaitWritable < Errno::EAGAIN
    include IO::WaitWritable
  end

  # exception to wait for reading by EINPROGRESS. see IO.select.
  class EINPROGRESSWaitReadable < Errno::EINPROGRESS
    include IO::WaitReadable
  end

  # exception to wait for writing by EINPROGRESS. see IO.select.
  class EINPROGRESSWaitWritable < Errno::EINPROGRESS
    include IO::WaitWritable
  end

  # exception to wait for reading by EWOULDBLOCK. see IO.select.
  class EWOULDBLOCKWaitReadable < Errno::EWOULDBLOCK
    include IO::WaitReadable
  end

  # exception to wait for writing by EWOULDBLOCK. see IO.select.
  class EWOULDBLOCKWaitWritable < Errno::EWOULDBLOCK
    include IO::WaitWritable
  end

  # exception to wait for reading. see IO.select.
  module WaitReadable
  end

  # exception to wait for writing. see IO.select.
  module WaitWritable
  end
end
