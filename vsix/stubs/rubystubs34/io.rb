# frozen_string_literal: true

# An instance of class \IO (commonly called a _stream_)
# represents an input/output stream in the underlying operating system.
# \Class \IO is the basis for input and output in Ruby.
#
# \Class File is the only class in the Ruby core that is a subclass of \IO.
# Some classes in the Ruby standard library are also subclasses of \IO;
# these include TCPSocket and UDPSocket.
#
# The global constant ARGF (also accessible as <tt>$<</tt>)
# provides an IO-like stream that allows access to all file paths
# found in ARGV (or found in STDIN if ARGV is empty).
# ARGF is not itself a subclass of \IO.
#
# \Class StringIO provides an IO-like stream that handles a String.
# StringIO is not itself a subclass of \IO.
#
# Important objects based on \IO include:
#
# - $stdin.
# - $stdout.
# - $stderr.
# - Instances of class File.
#
# An instance of \IO may be created using:
#
# - IO.new: returns a new \IO object for the given integer file descriptor.
# - IO.open: passes a new \IO object to the given block.
# - IO.popen: returns a new \IO object that is connected to the $stdin and $stdout
#   of a newly-launched subprocess.
# - Kernel#open: Returns a new \IO object connected to a given source:
#   stream, file, or subprocess.
#
# Like a File stream, an \IO stream has:
#
# - A read/write mode, which may be read-only, write-only, or read/write;
#   see {Read/Write Mode}[rdoc-ref:File@Read-2FWrite+Mode].
# - A data mode, which may be text-only or binary;
#   see {Data Mode}[rdoc-ref:File@Data+Mode].
# - Internal and external encodings;
#   see {Encodings}[rdoc-ref:File@Encodings].
#
# And like other \IO streams, it has:
#
# - A position, which determines where in the stream the next
#   read or write is to occur;
#   see {Position}[rdoc-ref:IO@Position].
# - A line number, which is a special, line-oriented, "position"
#   (different from the position mentioned above);
#   see {Line Number}[rdoc-ref:IO@Line+Number].
#
# == Extension <tt>io/console</tt>
#
# Extension <tt>io/console</tt> provides numerous methods
# for interacting with the console;
# requiring it adds numerous methods to class \IO.
#
# == Example Files
#
# Many examples here use these variables:
#
#   # English text with newlines.
#   text = <<~EOT
#     First line
#     Second line
#
#     Fourth line
#     Fifth line
#   EOT
#
#   # Russian text.
#   russian = "\u{442 435 441 442}" # => "тест"
#
#   # Binary data.
#   data = "\u9990\u9991\u9992\u9993\u9994"
#
#   # Text file.
#   File.write('t.txt', text)
#
#   # File with Russian text.
#   File.write('t.rus', russian)
#
#   # File with binary data.
#   f = File.new('t.dat', 'wb:UTF-16')
#   f.write(data)
#   f.close
#
#
# == Open Options
#
# A number of \IO methods accept optional keyword arguments
# that determine how a new stream is to be opened:
#
# - +:mode+: Stream mode.
# - +:flags+: Integer file open flags;
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
# - +:path:+ If a string value is provided, it is used in #inspect and is available as
#   #path method.
#
# Also available are the options offered in String#encode,
# which may control conversion between external and internal encoding.
#
# == Basic \IO
#
# You can perform basic stream \IO with these methods,
# which typically operate on multi-byte strings:
#
# - IO#read: Reads and returns some or all of the remaining bytes from the stream.
# - IO#write: Writes zero or more strings to the stream;
#   each given object that is not already a string is converted via +to_s+.
#
# === Position
#
# An \IO stream has a nonnegative integer _position_,
# which is the byte offset at which the next read or write is to occur.
# A new stream has position zero (and line number zero);
# method +rewind+ resets the position (and line number) to zero.
#
# These methods discard {buffers}[rdoc-ref:IO@Buffering] and the
# Encoding::Converter instances used for that \IO.
#
# The relevant methods:
#
# - IO#tell (aliased as +#pos+): Returns the current position (in bytes) in the stream.
# - IO#pos=: Sets the position of the stream to a given integer +new_position+ (in bytes).
# - IO#seek: Sets the position of the stream to a given integer +offset+ (in bytes),
#   relative to a given position +whence+
#   (indicating the beginning, end, or current position).
# - IO#rewind: Positions the stream at the beginning (also resetting the line number).
#
# === Open and Closed Streams
#
# A new \IO stream may be open for reading, open for writing, or both.
#
# A stream is automatically closed when claimed by the garbage collector.
#
# Attempted reading or writing on a closed stream raises an exception.
#
# The relevant methods:
#
# - IO#close: Closes the stream for both reading and writing.
# - IO#close_read: Closes the stream for reading.
# - IO#close_write: Closes the stream for writing.
# - IO#closed?: Returns whether the stream is closed.
#
# === End-of-Stream
#
# You can query whether a stream is positioned at its end:
#
# - IO#eof? (also aliased as +#eof+): Returns whether the stream is at end-of-stream.
#
# You can reposition to end-of-stream by using method IO#seek:
#
#   f = File.new('t.txt')
#   f.eof? # => false
#   f.seek(0, :END)
#   f.eof? # => true
#   f.close
#
# Or by reading all stream content (which is slower than using IO#seek):
#
#   f.rewind
#   f.eof? # => false
#   f.read # => "First line\nSecond line\n\nFourth line\nFifth line\n"
#   f.eof? # => true
#
# == Line \IO
#
# \Class \IO supports line-oriented
# {input}[rdoc-ref:IO@Line+Input] and {output}[rdoc-ref:IO@Line+Output]
#
# === Line Input
#
# \Class \IO supports line-oriented input for
# {files}[rdoc-ref:IO@File+Line+Input] and {IO streams}[rdoc-ref:IO@Stream+Line+Input]
#
# ==== \File Line Input
#
# You can read lines from a file using these methods:
#
# - IO.foreach: Reads each line and passes it to the given block.
# - IO.readlines: Reads and returns all lines in an array.
#
# For each of these methods:
#
# - You can specify {open options}[rdoc-ref:IO@Open+Options].
# - Line parsing depends on the effective <i>line separator</i>;
#   see {Line Separator}[rdoc-ref:IO@Line+Separator].
# - The length of each returned line depends on the effective <i>line limit</i>;
#   see {Line Limit}[rdoc-ref:IO@Line+Limit].
#
# ==== Stream Line Input
#
# You can read lines from an \IO stream using these methods:
#
# - IO#each_line: Reads each remaining line, passing it to the given block.
# - IO#gets: Returns the next line.
# - IO#readline: Like #gets, but raises an exception at end-of-stream.
# - IO#readlines: Returns all remaining lines in an array.
#
# For each of these methods:
#
# - Reading may begin mid-line,
#   depending on the stream's _position_;
#   see {Position}[rdoc-ref:IO@Position].
# - Line parsing depends on the effective <i>line separator</i>;
#   see {Line Separator}[rdoc-ref:IO@Line+Separator].
# - The length of each returned line depends on the effective <i>line limit</i>;
#   see {Line Limit}[rdoc-ref:IO@Line+Limit].
#
# ===== Line Separator
#
# Each of the {line input methods}[rdoc-ref:IO@Line+Input] uses a <i>line separator</i>:
# the string that determines what is considered a line;
# it is sometimes called the <i>input record separator</i>.
#
# The default line separator is taken from global variable <tt>$/</tt>,
# whose initial value is <tt>"\n"</tt>.
#
# Generally, the line to be read next is all data
# from the current {position}[rdoc-ref:IO@Position]
# to the next line separator
# (but see {Special Line Separator Values}[rdoc-ref:IO@Special+Line+Separator+Values]):
#
#   f = File.new('t.txt')
#   # Method gets with no sep argument returns the next line, according to $/.
#   f.gets # => "First line\n"
#   f.gets # => "Second line\n"
#   f.gets # => "\n"
#   f.gets # => "Fourth line\n"
#   f.gets # => "Fifth line\n"
#   f.close
#
# You can use a different line separator by passing argument +sep+:
#
#   f = File.new('t.txt')
#   f.gets('l')   # => "First l"
#   f.gets('li')  # => "ine\nSecond li"
#   f.gets('lin') # => "ne\n\nFourth lin"
#   f.gets        # => "e\n"
#   f.close
#
# Or by setting global variable <tt>$/</tt>:
#
#   f = File.new('t.txt')
#   $/ = 'l'
#   f.gets # => "First l"
#   f.gets # => "ine\nSecond l"
#   f.gets # => "ine\n\nFourth l"
#   f.close
#
# ===== Special Line Separator Values
#
# Each of the {line input methods}[rdoc-ref:IO@Line+Input]
# accepts two special values for parameter +sep+:
#
# - +nil+: The entire stream is to be read ("slurped") into a single string:
#
#     f = File.new('t.txt')
#     f.gets(nil) # => "First line\nSecond line\n\nFourth line\nFifth line\n"
#     f.close
#
# - <tt>''</tt> (the empty string): The next "paragraph" is to be read
#   (paragraphs being separated by two consecutive line separators):
#
#     f = File.new('t.txt')
#     f.gets('') # => "First line\nSecond line\n\n"
#     f.gets('') # => "Fourth line\nFifth line\n"
#     f.close
#
# ===== Line Limit
#
# Each of the {line input methods}[rdoc-ref:IO@Line+Input]
# uses an integer <i>line limit</i>,
# which restricts the number of bytes that may be returned.
# (A multi-byte character will not be split, and so a returned line may be slightly longer
# than the limit).
#
# The default limit value is <tt>-1</tt>;
# any negative limit value means that there is no limit.
#
# If there is no limit, the line is determined only by +sep+.
#
#   # Text with 1-byte characters.
#   File.open('t.txt') {|f| f.gets(1) }  # => "F"
#   File.open('t.txt') {|f| f.gets(2) }  # => "Fi"
#   File.open('t.txt') {|f| f.gets(3) }  # => "Fir"
#   File.open('t.txt') {|f| f.gets(4) }  # => "Firs"
#   # No more than one line.
#   File.open('t.txt') {|f| f.gets(10) } # => "First line"
#   File.open('t.txt') {|f| f.gets(11) } # => "First line\n"
#   File.open('t.txt') {|f| f.gets(12) } # => "First line\n"
#
#   # Text with 2-byte characters, which will not be split.
#   File.open('t.rus') {|f| f.gets(1).size } # => 1
#   File.open('t.rus') {|f| f.gets(2).size } # => 1
#   File.open('t.rus') {|f| f.gets(3).size } # => 2
#   File.open('t.rus') {|f| f.gets(4).size } # => 2
#
# ===== Line Separator and Line Limit
#
# With arguments +sep+ and +limit+ given, combines the two behaviors:
#
# - Returns the next line as determined by line separator +sep+.
# - But returns no more bytes than are allowed by the limit +limit+.
#
# Example:
#
#   File.open('t.txt') {|f| f.gets('li', 20) } # => "First li"
#   File.open('t.txt') {|f| f.gets('li', 2) }  # => "Fi"
#
# ===== Line Number
#
# A readable \IO stream has a non-negative integer <i>line number</i>:
#
# - IO#lineno: Returns the line number.
# - IO#lineno=: Resets and returns the line number.
#
# Unless modified by a call to method IO#lineno=,
# the line number is the number of lines read
# by certain line-oriented methods,
# according to the effective {line separator}[rdoc-ref:IO@Line+Separator]:
#
# - IO.foreach: Increments the line number on each call to the block.
# - IO#each_line: Increments the line number on each call to the block.
# - IO#gets: Increments the line number.
# - IO#readline: Increments the line number.
# - IO#readlines: Increments the line number for each line read.
#
# A new stream is initially has line number zero (and position zero);
# method +rewind+ resets the line number (and position) to zero:
#
#   f = File.new('t.txt')
#   f.lineno # => 0
#   f.gets   # => "First line\n"
#   f.lineno # => 1
#   f.rewind
#   f.lineno # => 0
#   f.close
#
# Reading lines from a stream usually changes its line number:
#
#   f = File.new('t.txt', 'r')
#   f.lineno   # => 0
#   f.readline # => "This is line one.\n"
#   f.lineno   # => 1
#   f.readline # => "This is the second line.\n"
#   f.lineno   # => 2
#   f.readline # => "Here's the third line.\n"
#   f.lineno   # => 3
#   f.eof?     # => true
#   f.close
#
# Iterating over lines in a stream usually changes its line number:
#
#   File.open('t.txt') do |f|
#     f.each_line do |line|
#       p "position=#{f.pos} eof?=#{f.eof?} lineno=#{f.lineno}"
#     end
#   end
#
# Output:
#
#   "position=11 eof?=false lineno=1"
#   "position=23 eof?=false lineno=2"
#   "position=24 eof?=false lineno=3"
#   "position=36 eof?=false lineno=4"
#   "position=47 eof?=true lineno=5"
#
# Unlike the stream's {position}[rdoc-ref:IO@Position],
# the line number does not affect where the next read or write will occur:
#
#   f = File.new('t.txt')
#   f.lineno = 1000
#   f.lineno # => 1000
#   f.gets   # => "First line\n"
#   f.lineno # => 1001
#   f.close
#
# Associated with the line number is the global variable <tt>$.</tt>:
#
# - When a stream is opened, <tt>$.</tt> is not set;
#   its value is left over from previous activity in the process:
#
#     $. = 41
#     f = File.new('t.txt')
#     $. = 41
#     # => 41
#     f.close
#
# - When a stream is read, <tt>$.</tt> is set to the line number for that stream:
#
#     f0 = File.new('t.txt')
#     f1 = File.new('t.dat')
#     f0.readlines # => ["First line\n", "Second line\n", "\n", "Fourth line\n", "Fifth line\n"]
#     $.           # => 5
#     f1.readlines # => ["\xFE\xFF\x99\x90\x99\x91\x99\x92\x99\x93\x99\x94"]
#     $.           # => 1
#     f0.close
#     f1.close
#
# - Methods IO#rewind and IO#seek do not affect <tt>$.</tt>:
#
#     f = File.new('t.txt')
#     f.readlines # => ["First line\n", "Second line\n", "\n", "Fourth line\n", "Fifth line\n"]
#     $.          # => 5
#     f.rewind
#     f.seek(0, :SET)
#     $.          # => 5
#     f.close
#
# === Line Output
#
# You can write to an \IO stream line-by-line using this method:
#
# - IO#puts: Writes objects to the stream.
#
# == Character \IO
#
# You can process an \IO stream character-by-character using these methods:
#
# - IO#getc: Reads and returns the next character from the stream.
# - IO#readchar: Like #getc, but raises an exception at end-of-stream.
# - IO#ungetc: Pushes back ("unshifts") a character or integer onto the stream.
# - IO#putc: Writes a character to the stream.
# - IO#each_char: Reads each remaining character in the stream,
#   passing the character to the given block.
#
# == Byte \IO
#
# You can process an \IO stream byte-by-byte using these methods:
#
# - IO#getbyte: Returns the next 8-bit byte as an integer in range 0..255.
# - IO#readbyte: Like #getbyte, but raises an exception if at end-of-stream.
# - IO#ungetbyte: Pushes back ("unshifts") a byte back onto the stream.
# - IO#each_byte: Reads each remaining byte in the stream,
#   passing the byte to the given block.
#
# == Codepoint \IO
#
# You can process an \IO stream codepoint-by-codepoint:
#
# - IO#each_codepoint: Reads each remaining codepoint, passing it to the given block.
#
# == What's Here
#
# First, what's elsewhere. \Class \IO:
#
# - Inherits from {class Object}[rdoc-ref:Object@What-27s+Here].
# - Includes {module Enumerable}[rdoc-ref:Enumerable@What-27s+Here],
#   which provides dozens of additional methods.
#
# Here, class \IO provides methods that are useful for:
#
# - {Creating}[rdoc-ref:IO@Creating]
# - {Reading}[rdoc-ref:IO@Reading]
# - {Writing}[rdoc-ref:IO@Writing]
# - {Positioning}[rdoc-ref:IO@Positioning]
# - {Iterating}[rdoc-ref:IO@Iterating]
# - {Settings}[rdoc-ref:IO@Settings]
# - {Querying}[rdoc-ref:IO@Querying]
# - {Buffering}[rdoc-ref:IO@Buffering]
# - {Low-Level Access}[rdoc-ref:IO@Low-Level+Access]
# - {Other}[rdoc-ref:IO@Other]
#
# === Creating
#
# - ::new (aliased as ::for_fd): Creates and returns a new \IO object for the given
#   integer file descriptor.
# - ::open: Creates a new \IO object.
# - ::pipe: Creates a connected pair of reader and writer \IO objects.
# - ::popen: Creates an \IO object to interact with a subprocess.
# - ::select: Selects which given \IO instances are ready for reading,
#   writing, or have pending exceptions.
#
# === Reading
#
# - ::binread: Returns a binary string with all or a subset of bytes
#   from the given file.
# - ::read: Returns a string with all or a subset of bytes from the given file.
# - ::readlines: Returns an array of strings, which are the lines from the given file.
# - #getbyte: Returns the next 8-bit byte read from +self+ as an integer.
# - #getc: Returns the next character read from +self+ as a string.
# - #gets: Returns the line read from +self+.
# - #pread: Returns all or the next _n_ bytes read from +self+,
#   not updating the receiver's offset.
# - #read: Returns all remaining or the next _n_ bytes read from +self+
#   for a given _n_.
# - #read_nonblock: the next _n_ bytes read from +self+ for a given _n_,
#   in non-block mode.
# - #readbyte: Returns the next byte read from +self+;
#   same as #getbyte, but raises an exception on end-of-stream.
# - #readchar: Returns the next character read from +self+;
#   same as #getc, but raises an exception on end-of-stream.
# - #readline: Returns the next line read from +self+;
#   same as #getline, but raises an exception of end-of-stream.
# - #readlines: Returns an array of all lines read read from +self+.
# - #readpartial: Returns up to the given number of bytes from +self+.
#
# === Writing
#
# - ::binwrite: Writes the given string to the file at the given filepath,
#   in binary mode.
# - ::write: Writes the given string to +self+.
# - #<<: Appends the given string to +self+.
# - #print: Prints last read line or given objects to +self+.
# - #printf: Writes to +self+ based on the given format string and objects.
# - #putc: Writes a character to +self+.
# - #puts: Writes lines to +self+, making sure line ends with a newline.
# - #pwrite: Writes the given string at the given offset,
#   not updating the receiver's offset.
# - #write: Writes one or more given strings to +self+.
# - #write_nonblock: Writes one or more given strings to +self+ in non-blocking mode.
#
# === Positioning
#
# - #lineno: Returns the current line number in +self+.
# - #lineno=: Sets the line number is +self+.
# - #pos (aliased as #tell): Returns the current byte offset in +self+.
# - #pos=: Sets the byte offset in +self+.
# - #reopen: Reassociates +self+ with a new or existing \IO stream.
# - #rewind: Positions +self+ to the beginning of input.
# - #seek: Sets the offset for +self+ relative to given position.
#
# === Iterating
#
# - ::foreach: Yields each line of given file to the block.
# - #each (aliased as #each_line): Calls the given block
#   with each successive line in +self+.
# - #each_byte: Calls the given block with each successive byte in +self+
#   as an integer.
# - #each_char: Calls the given block with each successive character in +self+
#   as a string.
# - #each_codepoint: Calls the given block with each successive codepoint in +self+
#   as an integer.
#
# === Settings
#
# - #autoclose=: Sets whether +self+ auto-closes.
# - #binmode: Sets +self+ to binary mode.
# - #close: Closes +self+.
# - #close_on_exec=: Sets the close-on-exec flag.
# - #close_read: Closes +self+ for reading.
# - #close_write: Closes +self+ for writing.
# - #set_encoding: Sets the encoding for +self+.
# - #set_encoding_by_bom: Sets the encoding for +self+, based on its
#   Unicode byte-order-mark.
# - #sync=: Sets the sync-mode to the given value.
#
# === Querying
#
# - #autoclose?: Returns whether +self+ auto-closes.
# - #binmode?: Returns whether +self+ is in binary mode.
# - #close_on_exec?: Returns the close-on-exec flag for +self+.
# - #closed?: Returns whether +self+ is closed.
# - #eof? (aliased as #eof): Returns whether +self+ is at end-of-stream.
# - #external_encoding: Returns the external encoding object for +self+.
# - #fileno (aliased as #to_i): Returns the integer file descriptor for +self+
# - #internal_encoding: Returns the internal encoding object for +self+.
# - #pid: Returns the process ID of a child process associated with +self+,
#   if +self+ was created by ::popen.
# - #stat: Returns the File::Stat object containing status information for +self+.
# - #sync: Returns whether +self+ is in sync-mode.
# - #tty? (aliased as #isatty): Returns whether +self+ is a terminal.
#
# === Buffering
#
# - #fdatasync: Immediately writes all buffered data in +self+ to disk.
# - #flush: Flushes any buffered data within +self+ to the underlying
#   operating system.
# - #fsync: Immediately writes all buffered data and attributes in +self+ to disk.
# - #ungetbyte: Prepends buffer for +self+ with given integer byte or string.
# - #ungetc: Prepends buffer for +self+ with given string.
#
# === Low-Level Access
#
# - ::sysopen: Opens the file given by its path,
#   returning the integer file descriptor.
# - #advise: Announces the intention to access data from +self+ in a specific way.
# - #fcntl: Passes a low-level command to the file specified
#   by the given file descriptor.
# - #ioctl: Passes a low-level command to the device specified
#   by the given file descriptor.
# - #sysread: Returns up to the next _n_ bytes read from self using a low-level read.
# - #sysseek: Sets the offset for +self+.
# - #syswrite: Writes the given string to +self+ using a low-level write.
#
# === Other
#
# - ::copy_stream: Copies data from a source to a destination,
#   each of which is a filepath or an \IO-like object.
# - ::try_convert: Returns a new \IO object resulting from converting
#   the given object.
# - #inspect: Returns the string representation of +self+.
class IO
  include Enumerable
  include File::Constants

  # same as IO::EAGAINWaitReadable
  EWOULDBLOCKWaitReadable = _
  # same as IO::EAGAINWaitWritable
  EWOULDBLOCKWaitWritable = _
  # Priority event mask for IO#wait.
  PRIORITY = _
  # Readable event mask for IO#wait.
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
  # Writable event mask for IO#wait.
  WRITABLE = _

  # Behaves like IO.read, except that the stream is opened in binary mode
  # with ASCII-8BIT encoding.
  #
  # When called from class \IO (but not subclasses of \IO),
  # this method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  def self.binread(path, length = nil, offset = 0) end

  # Behaves like IO.write, except that the stream is opened in binary mode
  # with ASCII-8BIT encoding.
  #
  # When called from class \IO (but not subclasses of \IO),
  # this method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  def self.binwrite(path, string, offset = 0) end

  # Returns an File instance opened console.
  #
  # If +sym+ is given, it will be sent to the opened console with
  # +args+ and the result will be returned instead of the console IO
  # itself.
  #
  # You must require 'io/console' to use this method.
  def self.console(...) end

  # Copies from the given +src+ to the given +dst+,
  # returning the number of bytes copied.
  #
  # - The given +src+ must be one of the following:
  #
  #   - The path to a readable file, from which source data is to be read.
  #   - An \IO-like object, opened for reading and capable of responding
  #     to method +:readpartial+ or method +:read+.
  #
  # - The given +dst+ must be one of the following:
  #
  #   - The path to a writable file, to which data is to be written.
  #   - An \IO-like object, opened for writing and capable of responding
  #     to method +:write+.
  #
  # The examples here use file <tt>t.txt</tt> as source:
  #
  #   File.read('t.txt')
  #   # => "First line\nSecond line\n\nThird line\nFourth line\n"
  #   File.read('t.txt').size # => 47
  #
  # If only arguments +src+ and +dst+ are given,
  # the entire source stream is copied:
  #
  #   # Paths.
  #   IO.copy_stream('t.txt', 't.tmp')  # => 47
  #
  #   # IOs (recall that a File is also an IO).
  #   src_io = File.open('t.txt', 'r') # => #<File:t.txt>
  #   dst_io = File.open('t.tmp', 'w') # => #<File:t.tmp>
  #   IO.copy_stream(src_io, dst_io)   # => 47
  #   src_io.close
  #   dst_io.close
  #
  # With argument +src_length+ a non-negative integer,
  # no more than that many bytes are copied:
  #
  #   IO.copy_stream('t.txt', 't.tmp', 10) # => 10
  #   File.read('t.tmp')                   # => "First line"
  #
  # With argument +src_offset+ also given,
  # the source stream is read beginning at that offset:
  #
  #   IO.copy_stream('t.txt', 't.tmp', 11, 11) # => 11
  #   IO.read('t.tmp')                         # => "Second line"
  def self.copy_stream(src, dst, src_length = nil, src_offset = 0) end

  # Synonym for IO.new.
  def self.for_fd(fd, mode = 'r', **opts) end

  # Calls the block with each successive line read from the stream.
  #
  # When called from class \IO (but not subclasses of \IO),
  # this method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # The first argument must be a string that is the path to a file.
  #
  # With only argument +path+ given, parses lines from the file at the given +path+,
  # as determined by the default line separator,
  # and calls the block with each successive line:
  #
  #   File.foreach('t.txt') {|line| p line }
  #
  # Output: the same as above.
  #
  # For both forms, command and path, the remaining arguments are the same.
  #
  # With argument +sep+ given, parses lines as determined by that line separator
  # (see {Line Separator}[rdoc-ref:IO@Line+Separator]):
  #
  #   File.foreach('t.txt', 'li') {|line| p line }
  #
  # Output:
  #
  #   "First li"
  #   "ne\nSecond li"
  #   "ne\n\nThird li"
  #   "ne\nFourth li"
  #   "ne\n"
  #
  # Each paragraph:
  #
  #   File.foreach('t.txt', '') {|paragraph| p paragraph }
  #
  # Output:
  #
  #  "First line\nSecond line\n\n"
  #  "Third line\nFourth line\n"
  #
  # With argument +limit+ given, parses lines as determined by the default
  # line separator and the given line-length limit
  # (see {Line Separator}[rdoc-ref:IO@Line+Separator] and {Line Limit}[rdoc-ref:IO@Line+Limit]):
  #
  #   File.foreach('t.txt', 7) {|line| p line }
  #
  # Output:
  #
  #   "First l"
  #   "ine\n"
  #   "Second "
  #   "line\n"
  #   "\n"
  #   "Third l"
  #   "ine\n"
  #   "Fourth l"
  #   "line\n"
  #
  # With arguments +sep+ and +limit+ given,
  # combines the two behaviors
  # (see {Line Separator and Line Limit}[rdoc-ref:IO@Line+Separator+and+Line+Limit]).
  #
  # Optional keyword arguments +opts+ specify:
  #
  # - {Open Options}[rdoc-ref:IO@Open+Options].
  # - {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  # - {Line Options}[rdoc-ref:IO@Line+IO].
  #
  # Returns an Enumerator if no block is given.
  def self.foreach(...) end

  # Creates a new \IO object, via IO.new with the given arguments.
  #
  # With no block given, returns the \IO object.
  #
  # With a block given, calls the block with the \IO object
  # and returns the block's value.
  def self.open(fd, mode = 'r', **opts) end

  # Creates a pair of pipe endpoints, +read_io+ and +write_io+,
  # connected to each other.
  #
  # If argument +enc_string+ is given, it must be a string containing one of:
  #
  # - The name of the encoding to be used as the external encoding.
  # - The colon-separated names of two encodings to be used as the external
  #   and internal encodings.
  #
  # If argument +int_enc+ is given, it must be an Encoding object
  # or encoding name string that specifies the internal encoding to be used;
  # if argument +ext_enc+ is also given, it must be an Encoding object
  # or encoding name string that specifies the external encoding to be used.
  #
  # The string read from +read_io+ is tagged with the external encoding;
  # if an internal encoding is also specified, the string is converted
  # to, and tagged with, that encoding.
  #
  # If any encoding is specified,
  # optional hash arguments specify the conversion option.
  #
  # Optional keyword arguments +opts+ specify:
  #
  # - {Open Options}[rdoc-ref:IO@Open+Options].
  # - {Encoding Options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  #
  # With no block given, returns the two endpoints in an array:
  #
  #   IO.pipe # => [#<IO:fd 4>, #<IO:fd 5>]
  #
  # With a block given, calls the block with the two endpoints;
  # closes both endpoints and returns the value of the block:
  #
  #   IO.pipe {|read_io, write_io| p read_io; p write_io }
  #
  # Output:
  #
  #   #<IO:fd 6>
  #   #<IO:fd 7>
  #
  # Not available on all platforms.
  #
  # In the example below, the two processes close the ends of the pipe
  # that they are not using. This is not just a cosmetic nicety. The
  # read end of a pipe will not generate an end of file condition if
  # there are any writers with the pipe still open. In the case of the
  # parent process, the <tt>rd.read</tt> will never return if it
  # does not first issue a <tt>wr.close</tt>:
  #
  #   rd, wr = IO.pipe
  #
  #   if fork
  #     wr.close
  #     puts "Parent got: <#{rd.read}>"
  #     rd.close
  #     Process.wait
  #   else
  #     rd.close
  #     puts 'Sending message to parent'
  #     wr.write "Hi Dad"
  #     wr.close
  #   end
  #
  # <em>produces:</em>
  #
  #    Sending message to parent
  #    Parent got: <Hi Dad>
  def self.pipe(...) end

  # Executes the given command +cmd+ as a subprocess
  # whose $stdin and $stdout are connected to a new stream +io+.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # If no block is given, returns the new stream,
  # which depending on given +mode+ may be open for reading, writing, or both.
  # The stream should be explicitly closed (eventually) to avoid resource leaks.
  #
  # If a block is given, the stream is passed to the block
  # (again, open for reading, writing, or both);
  # when the block exits, the stream is closed,
  # and the block's value is assigned to global variable <tt>$?</tt> and returned.
  #
  # Optional argument +mode+ may be any valid \IO mode.
  # See {Access Modes}[rdoc-ref:File@Access+Modes].
  #
  # Required argument +cmd+ determines which of the following occurs:
  #
  # - The process forks.
  # - A specified program runs in a shell.
  # - A specified program runs with specified arguments.
  # - A specified program runs with specified arguments and a specified +argv0+.
  #
  # Each of these is detailed below.
  #
  # The optional hash argument +env+ specifies name/value pairs that are to be added
  # to the environment variables for the subprocess:
  #
  #   IO.popen({'FOO' => 'bar'}, 'ruby', 'r+') do |pipe|
  #     pipe.puts 'puts ENV["FOO"]'
  #     pipe.close_write
  #     pipe.gets
  #   end => "bar\n"
  #
  # Optional keyword arguments +opts+ specify:
  #
  # - {Open options}[rdoc-ref:IO@Open+Options].
  # - {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  # - Options for Kernel#spawn.
  #
  # <b>Forked \Process</b>
  #
  # When argument +cmd+ is the 1-character string <tt>'-'</tt>, causes the process to fork:
  #   IO.popen('-') do |pipe|
  #     if pipe
  #       $stderr.puts "In parent, child pid is #{pipe.pid}\n"
  #     else
  #       $stderr.puts "In child, pid is #{$$}\n"
  #     end
  #   end
  #
  # Output:
  #
  #   In parent, child pid is 26253
  #   In child, pid is 26253
  #
  # Note that this is not supported on all platforms.
  #
  # <b>Shell Subprocess</b>
  #
  # When argument +cmd+ is a single string (but not <tt>'-'</tt>),
  # the program named +cmd+ is run as a shell command:
  #
  #   IO.popen('uname') do |pipe|
  #     pipe.readlines
  #   end
  #
  # Output:
  #
  #   ["Linux\n"]
  #
  # Another example:
  #
  #   IO.popen('/bin/sh', 'r+') do |pipe|
  #     pipe.puts('ls')
  #     pipe.close_write
  #     $stderr.puts pipe.readlines.size
  #   end
  #
  # Output:
  #
  #   213
  #
  # <b>Program Subprocess</b>
  #
  # When argument +cmd+ is an array of strings,
  # the program named <tt>cmd[0]</tt> is run with all elements of +cmd+ as its arguments:
  #
  #   IO.popen(['du', '..', '.']) do |pipe|
  #     $stderr.puts pipe.readlines.size
  #   end
  #
  # Output:
  #
  #   1111
  #
  # <b>Program Subprocess with <tt>argv0</tt></b>
  #
  # When argument +cmd+ is an array whose first element is a 2-element string array
  # and whose remaining elements (if any) are strings:
  #
  # - <tt>cmd[0][0]</tt> (the first string in the nested array) is the name of a program that is run.
  # - <tt>cmd[0][1]</tt> (the second string in the nested array) is set as the program's <tt>argv[0]</tt>.
  # - <tt>cmd[1..-1]</tt> (the strings in the outer array) are the program's arguments.
  #
  # Example (sets <tt>$0</tt> to 'foo'):
  #
  #   IO.popen([['/bin/sh', 'foo'], '-c', 'echo $0']).read # => "foo\n"
  #
  # <b>Some Special Examples</b>
  #
  #   # Set IO encoding.
  #   IO.popen("nkf -e filename", :external_encoding=>"EUC-JP") {|nkf_io|
  #     euc_jp_string = nkf_io.read
  #   }
  #
  #   # Merge standard output and standard error using Kernel#spawn option. See Kernel#spawn.
  #   IO.popen(["ls", "/", :err=>[:child, :out]]) do |io|
  #     ls_result_with_error = io.read
  #   end
  #
  #   # Use mixture of spawn options and IO options.
  #   IO.popen(["ls", "/"], :err=>[:child, :out]) do |io|
  #     ls_result_with_error = io.read
  #   end
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
  # Output (from last section):
  #
  #    ["Linux\n"]
  #    Parent is 21346
  #    Thu Jan 15 22:41:19 JST 2009
  #    21346 is here, f is #<IO:fd 3>
  #    21352 is here, f is nil
  #    #<Process::Status: pid 21352 exit 0>
  #    <foo>bar;zot;
  #
  # Raises exceptions that IO.pipe and Kernel.spawn raise.
  def self.popen(*args) end

  # Opens the stream, reads and returns some or all of its content,
  # and closes the stream; returns +nil+ if no bytes were read.
  #
  # When called from class \IO (but not subclasses of \IO),
  # this method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # The first argument must be a string that is the path to a file.
  #
  # With only argument +path+ given, reads in text mode and returns the entire content
  # of the file at the given path:
  #
  #   IO.read('t.txt')
  #   # => "First line\nSecond line\n\nThird line\nFourth line\n"
  #
  # On Windows, text mode can terminate reading and leave bytes in the file
  # unread when encountering certain special bytes. Consider using
  # IO.binread if all bytes in the file should be read.
  #
  # With argument +length+, returns +length+ bytes if available:
  #
  #   IO.read('t.txt', 7) # => "First l"
  #   IO.read('t.txt', 700)
  #   # => "First line\r\nSecond line\r\n\r\nFourth line\r\nFifth line\r\n"
  #
  # With arguments +length+ and +offset+, returns +length+ bytes
  # if available, beginning at the given +offset+:
  #
  #   IO.read('t.txt', 10, 2)   # => "rst line\nS"
  #   IO.read('t.txt', 10, 200) # => nil
  #
  # Optional keyword arguments +opts+ specify:
  #
  # - {Open Options}[rdoc-ref:IO@Open+Options].
  # - {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  def self.read(path, length = nil, offset = 0, **opts) end

  # Returns an array of all lines read from the stream.
  #
  # When called from class \IO (but not subclasses of \IO),
  # this method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # The first argument must be a string that is the path to a file.
  #
  # With only argument +path+ given, parses lines from the file at the given +path+,
  # as determined by the default line separator,
  # and returns those lines in an array:
  #
  #   IO.readlines('t.txt')
  #   # => ["First line\n", "Second line\n", "\n", "Third line\n", "Fourth line\n"]
  #
  # With argument +sep+ given, parses lines as determined by that line separator
  # (see {Line Separator}[rdoc-ref:IO@Line+Separator]):
  #
  #   # Ordinary separator.
  #   IO.readlines('t.txt', 'li')
  #   # =>["First li", "ne\nSecond li", "ne\n\nThird li", "ne\nFourth li", "ne\n"]
  #   # Get-paragraphs separator.
  #   IO.readlines('t.txt', '')
  #   # => ["First line\nSecond line\n\n", "Third line\nFourth line\n"]
  #   # Get-all separator.
  #   IO.readlines('t.txt', nil)
  #   # => ["First line\nSecond line\n\nThird line\nFourth line\n"]
  #
  # With argument +limit+ given, parses lines as determined by the default
  # line separator and the given line-length limit
  # (see {Line Separator}[rdoc-ref:IO@Line+Separator] and {Line Limit}[rdoc-ref:IO@Line+Limit]:
  #
  #   IO.readlines('t.txt', 7)
  #   # => ["First l", "ine\n", "Second ", "line\n", "\n", "Third l", "ine\n", "Fourth ", "line\n"]
  #
  # With arguments +sep+ and +limit+ given,
  # combines the two behaviors
  # (see {Line Separator and Line Limit}[rdoc-ref:IO@Line+Separator+and+Line+Limit]).
  #
  # Optional keyword arguments +opts+ specify:
  #
  # - {Open Options}[rdoc-ref:IO@Open+Options].
  # - {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  # - {Line Options}[rdoc-ref:IO@Line+IO].
  def self.readlines(...) end

  # Invokes system call {select(2)}[https://linux.die.net/man/2/select],
  # which monitors multiple file descriptors,
  # waiting until one or more of the file descriptors
  # becomes ready for some class of I/O operation.
  #
  # Not implemented on all platforms.
  #
  # Each of the arguments +read_ios+, +write_ios+, and +error_ios+
  # is an array of IO objects.
  #
  # Argument +timeout+ is a numeric value (such as integer or float) timeout
  # interval in seconds.
  #
  # The method monitors the \IO objects given in all three arrays,
  # waiting for some to be ready;
  # returns a 3-element array whose elements are:
  #
  # - An array of the objects in +read_ios+ that are ready for reading.
  # - An array of the objects in +write_ios+ that are ready for writing.
  # - An array of the objects in +error_ios+ have pending exceptions.
  #
  # If no object becomes ready within the given +timeout+, +nil+ is returned.
  #
  # \IO.select peeks the buffer of \IO objects for testing readability.
  # If the \IO buffer is not empty, \IO.select immediately notifies
  # readability.  This "peek" only happens for \IO objects.  It does not
  # happen for IO-like objects such as OpenSSL::SSL::SSLSocket.
  #
  # The best way to use \IO.select is invoking it after non-blocking
  # methods such as #read_nonblock, #write_nonblock, etc.  The methods
  # raise an exception which is extended by IO::WaitReadable or
  # IO::WaitWritable.  The modules notify how the caller should wait
  # with \IO.select.  If IO::WaitReadable is raised, the caller should
  # wait for reading.  If IO::WaitWritable is raised, the caller should
  # wait for writing.
  #
  # So, blocking read (#readpartial) can be emulated using
  # #read_nonblock and \IO.select as follows:
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
  # Especially, the combination of non-blocking methods and \IO.select is
  # preferred for IO like objects such as OpenSSL::SSL::SSLSocket.  It
  # has #to_io method to return underlying IO object.  IO.select calls
  # #to_io to obtain the file descriptor to wait.
  #
  # This means that readability notified by \IO.select doesn't mean
  # readability from OpenSSL::SSL::SSLSocket object.
  #
  # The most likely situation is that OpenSSL::SSL::SSLSocket buffers
  # some data.  \IO.select doesn't see the buffer.  So \IO.select can
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
  # The combination of non-blocking methods and \IO.select is also useful
  # for streams such as tty, pipe socket socket when multiple processes
  # read from a stream.
  #
  # Finally, Linux kernel developers don't guarantee that
  # readability of select(2) means readability of following read(2) even
  # for a single process;
  # see {select(2)}[https://linux.die.net/man/2/select]
  #
  # Invoking \IO.select before IO#readpartial works well as usual.
  # However it is not the best way to use \IO.select.
  #
  # The writability notified by select(2) doesn't show
  # how many bytes are writable.
  # IO#write method blocks until given whole string is written.
  # So, <tt>IO#write(two or more bytes)</tt> can block after
  # writability is notified by \IO.select.  IO#write_nonblock is required
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
  # Example:
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
  # Output:
  #
  #     ping pong
  #     ping pong
  #     ping pong
  #     (snipped)
  #     ping
  def self.select(p1, p2 = v2, p3 = v3, p4 = v4) end

  # Opens the file at the given path with the given mode and permissions;
  # returns the integer file descriptor.
  #
  # If the file is to be readable, it must exist;
  # if the file is to be writable and does not exist,
  # it is created with the given permissions:
  #
  #   File.write('t.tmp', '')  # => 0
  #   IO.sysopen('t.tmp')      # => 8
  #   IO.sysopen('t.tmp', 'w') # => 9
  def self.sysopen(path, mode = 'r', perm = 0o666) end

  # Attempts to convert +object+ into an \IO object via method +to_io+;
  # returns the new \IO object if successful, or +nil+ otherwise:
  #
  #   IO.try_convert(STDOUT)   # => #<IO:<STDOUT>>
  #   IO.try_convert(ARGF)     # => #<IO:<STDIN>>
  #   IO.try_convert('STDOUT') # => nil
  def self.try_convert(object) end

  # Opens the stream, writes the given +data+ to it,
  # and closes the stream; returns the number of bytes written.
  #
  # When called from class \IO (but not subclasses of \IO),
  # this method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # The first argument must be a string that is the path to a file.
  #
  # With only argument +path+ given, writes the given +data+ to the file at that path:
  #
  #   IO.write('t.tmp', 'abc')    # => 3
  #   File.read('t.tmp')          # => "abc"
  #
  # If +offset+ is zero (the default), the file is overwritten:
  #
  #   IO.write('t.tmp', 'A')      # => 1
  #   File.read('t.tmp')          # => "A"
  #
  # If +offset+ in within the file content, the file is partly overwritten:
  #
  #   IO.write('t.tmp', 'abcdef') # => 3
  #   File.read('t.tmp')          # => "abcdef"
  #   # Offset within content.
  #   IO.write('t.tmp', '012', 2) # => 3
  #   File.read('t.tmp')          # => "ab012f"
  #
  # If +offset+ is outside the file content,
  # the file is padded with null characters <tt>"\u0000"</tt>:
  #
  #   IO.write('t.tmp', 'xyz', 10) # => 3
  #   File.read('t.tmp')           # => "ab012f\u0000\u0000\u0000\u0000xyz"
  #
  # Optional keyword arguments +opts+ specify:
  #
  # - {Open Options}[rdoc-ref:IO@Open+Options].
  # - {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  def self.write(path, data, offset = 0, **opts) end

  # Creates and returns a new \IO object (file stream) from a file descriptor.
  #
  # \IO.new may be useful for interaction with low-level libraries.
  # For higher-level interactions, it may be simpler to create
  # the file stream using File.open.
  #
  # Argument +fd+ must be a valid file descriptor (integer):
  #
  #   path = 't.tmp'
  #   fd = IO.sysopen(path) # => 3
  #   IO.new(fd)            # => #<IO:fd 3>
  #
  # The new \IO object does not inherit encoding
  # (because the integer file descriptor does not have an encoding):
  #
  #   fd = IO.sysopen('t.rus', 'rb')
  #   io = IO.new(fd)
  #   io.external_encoding # => #<Encoding:UTF-8> # Not ASCII-8BIT.
  #
  # Optional argument +mode+ (defaults to 'r') must specify a valid mode;
  # see {Access Modes}[rdoc-ref:File@Access+Modes]:
  #
  #   IO.new(fd, 'w')         # => #<IO:fd 3>
  #   IO.new(fd, File::WRONLY) # => #<IO:fd 3>
  #
  # Optional keyword arguments +opts+ specify:
  #
  # - {Open Options}[rdoc-ref:IO@Open+Options].
  # - {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  #
  # Examples:
  #
  #   IO.new(fd, internal_encoding: nil) # => #<IO:fd 3>
  #   IO.new(fd, autoclose: true)        # => #<IO:fd 3>
  def initialize(fd, mode = 'r', **opts) end

  # Writes the given +object+ to +self+,
  # which must be opened for writing (see {Access Modes}[rdoc-ref:File@Access+Modes]);
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

  # Invokes Posix system call
  # {posix_fadvise(2)}[https://linux.die.net/man/2/posix_fadvise],
  # which announces an intention to access data from the current file
  # in a particular manner.
  #
  # The arguments and results are platform-dependent.
  #
  # The relevant data is specified by:
  #
  # - +offset+: The offset of the first byte of data.
  # - +len+: The number of bytes to be accessed;
  #   if +len+ is zero, or is larger than the number of bytes remaining,
  #   all remaining bytes will be accessed.
  #
  # Argument +advice+ is one of the following symbols:
  #
  # - +:normal+: The application has no advice to give
  #   about its access pattern for the specified data.
  #   If no advice is given for an open file, this is the default assumption.
  # - +:sequential+: The application expects to access the specified data sequentially
  #   (with lower offsets read before higher ones).
  # - +:random+: The specified data will be accessed in random order.
  # - +:noreuse+: The specified data will be accessed only once.
  # - +:willneed+: The specified data will be accessed in the near future.
  # - +:dontneed+: The specified data will not be accessed in the near future.
  #
  # Not implemented on all platforms.
  def advise(advice, offset = 0, len = 0) end

  # Sets auto-close flag.
  #
  #    f = File.open(File::NULL)
  #    IO.for_fd(f.fileno).close
  #    f.gets # raises Errno::EBADF
  #
  #    f = File.open(File::NULL)
  #    g = IO.for_fd(f.fileno)
  #    g.autoclose = false
  #    g.close
  #    f.gets # won't cause Errno::EBADF
  def autoclose=(bool) end

  # Returns +true+ if the underlying file descriptor of _ios_ will be
  # closed at its finalization or at calling #close, otherwise +false+.
  def autoclose?; end

  # Beeps on the output console.
  #
  # You must require 'io/console' to use this method.
  def beep; end

  # Sets the stream's data mode as binary
  # (see {Data Mode}[rdoc-ref:File@Data+Mode]).
  #
  # A stream's data mode may not be changed from binary to text.
  def binmode; end

  # Returns +true+ if the stream is on binary mode, +false+ otherwise.
  # See {Data Mode}[rdoc-ref:File@Data+Mode].
  def binmode?; end

  # Yields while console input events are queued.
  #
  # This method is Windows only.
  #
  # You must require 'io/console' to use this method.
  def check_winsize_changed; end

  # Clears the entire screen and moves the cursor top-left corner.
  #
  # You must require 'io/console' to use this method.
  def clear_screen; end

  # Closes the stream for both reading and writing
  # if open for either or both; returns +nil+.
  # See {Open and Closed Streams}[rdoc-ref:IO@Open+and+Closed+Streams].
  #
  # If the stream is open for writing, flushes any buffered writes
  # to the operating system before closing.
  #
  # If the stream was opened by IO.popen, sets global variable <tt>$?</tt>
  # (child exit status).
  #
  # It is not an error to close an IO object that has already been closed.
  # It just returns nil.
  #
  # Example:
  #
  #   IO.popen('ruby', 'r+') do |pipe|
  #     puts pipe.closed?
  #     pipe.close
  #     puts $?
  #     puts pipe.closed?
  #   end
  #
  # Output:
  #
  #   false
  #   pid 13760 exit 0
  #   true
  #
  # Related: IO#close_read, IO#close_write, IO#closed?.
  def close; end

  # Sets a close-on-exec flag.
  #
  #    f = File.open(File::NULL)
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

  # Returns +true+ if the stream will be closed on exec, +false+ otherwise:
  #
  #   f = File.open('t.txt')
  #   f.close_on_exec? # => true
  #   f.close_on_exec = false
  #   f.close_on_exec? # => false
  #   f.close
  def close_on_exec?; end

  # Closes the stream for reading if open for reading;
  # returns +nil+.
  # See {Open and Closed Streams}[rdoc-ref:IO@Open+and+Closed+Streams].
  #
  # If the stream was opened by IO.popen and is also closed for writing,
  # sets global variable <tt>$?</tt> (child exit status).
  #
  # Example:
  #
  #   IO.popen('ruby', 'r+') do |pipe|
  #     puts pipe.closed?
  #     pipe.close_write
  #     puts pipe.closed?
  #     pipe.close_read
  #     puts $?
  #     puts pipe.closed?
  #   end
  #
  # Output:
  #
  #   false
  #   false
  #   pid 14748 exit 0
  #   true
  #
  # Related: IO#close, IO#close_write, IO#closed?.
  def close_read; end

  # Closes the stream for writing if open for writing;
  # returns +nil+.
  # See {Open and Closed Streams}[rdoc-ref:IO@Open+and+Closed+Streams].
  #
  # Flushes any buffered writes to the operating system before closing.
  #
  # If the stream was opened by IO.popen and is also closed for reading,
  # sets global variable <tt>$?</tt> (child exit status).
  #
  #   IO.popen('ruby', 'r+') do |pipe|
  #     puts pipe.closed?
  #     pipe.close_read
  #     puts pipe.closed?
  #     pipe.close_write
  #     puts $?
  #     puts pipe.closed?
  #   end
  #
  # Output:
  #
  #   false
  #   false
  #   pid 15044 exit 0
  #   true
  #
  # Related: IO#close, IO#close_read, IO#closed?.
  def close_write; end

  # Returns +true+ if the stream is closed for both reading and writing,
  # +false+ otherwise.
  # See {Open and Closed Streams}[rdoc-ref:IO@Open+and+Closed+Streams].
  #
  #   IO.popen('ruby', 'r+') do |pipe|
  #     puts pipe.closed?
  #     pipe.close_read
  #     puts pipe.closed?
  #     pipe.close_write
  #     puts pipe.closed?
  #   end
  #
  # Output:
  #
  #   false
  #   false
  #   true
  #
  # Related: IO#close_read, IO#close_write, IO#close.
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

  # Returns the current cursor position as a two-element array of integers (row, column)
  #
  #   io.cursor # => [3, 5]
  #
  # You must require 'io/console' to use this method.
  def cursor; end

  # Same as <tt>io.goto(line, column)</tt>
  #
  # See IO#goto.
  #
  # You must require 'io/console' to use this method.
  def cursor=(p1) end

  # Moves the cursor down +n+ lines.
  #
  # You must require 'io/console' to use this method.
  def cursor_down(n) end

  # Moves the cursor left +n+ columns.
  #
  # You must require 'io/console' to use this method.
  def cursor_left(n) end

  # Moves the cursor right +n+ columns.
  #
  # You must require 'io/console' to use this method.
  def cursor_right(n) end

  # Moves the cursor up +n+ lines.
  #
  # You must require 'io/console' to use this method.
  def cursor_up(n) end

  # Calls the block with each remaining line read from the stream;
  # returns +self+.
  # Does nothing if already at end-of-stream;
  # See {Line IO}[rdoc-ref:IO@Line+IO].
  #
  # With no arguments given, reads lines
  # as determined by line separator <tt>$/</tt>:
  #
  #   f = File.new('t.txt')
  #   f.each_line {|line| p line }
  #   f.each_line {|line| fail 'Cannot happen' }
  #   f.close
  #
  # Output:
  #
  #   "First line\n"
  #   "Second line\n"
  #   "\n"
  #   "Fourth line\n"
  #   "Fifth line\n"
  #
  # With only string argument +sep+ given,
  # reads lines as determined by line separator +sep+;
  # see {Line Separator}[rdoc-ref:IO@Line+Separator]:
  #
  #   f = File.new('t.txt')
  #   f.each_line('li') {|line| p line }
  #   f.close
  #
  # Output:
  #
  #   "First li"
  #   "ne\nSecond li"
  #   "ne\n\nFourth li"
  #   "ne\nFifth li"
  #   "ne\n"
  #
  # The two special values for +sep+ are honored:
  #
  #   f = File.new('t.txt')
  #   # Get all into one string.
  #   f.each_line(nil) {|line| p line }
  #   f.close
  #
  # Output:
  #
  #   "First line\nSecond line\n\nFourth line\nFifth line\n"
  #
  #   f.rewind
  #   # Get paragraphs (up to two line separators).
  #   f.each_line('') {|line| p line }
  #
  # Output:
  #
  #   "First line\nSecond line\n\n"
  #   "Fourth line\nFifth line\n"
  #
  # With only integer argument +limit+ given,
  # limits the number of bytes in each line;
  # see {Line Limit}[rdoc-ref:IO@Line+Limit]:
  #
  #   f = File.new('t.txt')
  #   f.each_line(8) {|line| p line }
  #   f.close
  #
  # Output:
  #
  #   "First li"
  #   "ne\n"
  #   "Second l"
  #   "ine\n"
  #   "\n"
  #   "Fourth l"
  #   "ine\n"
  #   "Fifth li"
  #   "ne\n"
  #
  # With arguments +sep+ and +limit+ given,
  # combines the two behaviors
  # (see {Line Separator and Line Limit}[rdoc-ref:IO@Line+Separator+and+Line+Limit]).
  #
  # Optional keyword argument +chomp+ specifies whether line separators
  # are to be omitted:
  #
  #   f = File.new('t.txt')
  #   f.each_line(chomp: true) {|line| p line }
  #   f.close
  #
  # Output:
  #
  #   "First line"
  #   "Second line"
  #   ""
  #   "Fourth line"
  #   "Fifth line"
  #
  # Returns an Enumerator if no block is given.
  def each(*args) end
  alias each_line each

  # Calls the given block with each byte (0..255) in the stream; returns +self+.
  # See {Byte IO}[rdoc-ref:IO@Byte+IO].
  #
  #   f = File.new('t.rus')
  #   a = []
  #   f.each_byte {|b| a << b }
  #   a # => [209, 130, 208, 181, 209, 129, 209, 130]
  #   f.close
  #
  # Returns an Enumerator if no block is given.
  #
  # Related: IO#each_char, IO#each_codepoint.
  def each_byte; end

  # Calls the given block with each character in the stream; returns +self+.
  # See {Character IO}[rdoc-ref:IO@Character+IO].
  #
  #   f = File.new('t.rus')
  #   a = []
  #   f.each_char {|c| a << c.ord }
  #   a # => [1090, 1077, 1089, 1090]
  #   f.close
  #
  # Returns an Enumerator if no block is given.
  #
  # Related: IO#each_byte, IO#each_codepoint.
  def each_char; end

  # Calls the given block with each codepoint in the stream; returns +self+:
  #
  #   f = File.new('t.rus')
  #   a = []
  #   f.each_codepoint {|c| a << c }
  #   a # => [1090, 1077, 1089, 1090]
  #   f.close
  #
  # Returns an Enumerator if no block is given.
  #
  # Related: IO#each_byte, IO#each_char.
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
  # see {Position}[rdoc-ref:IO@Position]:
  #
  #   f = File.open('t.txt')
  #   f.eof           # => false
  #   f.seek(0, :END) # => 0
  #   f.eof           # => true
  #   f.close
  #
  # Raises an exception unless the stream is opened for reading;
  # see {Mode}[rdoc-ref:File@Access+Modes].
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
  def eof; end
  alias eof? eof

  # Erases the line at the cursor corresponding to +mode+.
  # +mode+ may be either:
  # 0: after cursor
  # 1: before and cursor
  # 2: entire line
  #
  # You must require 'io/console' to use this method.
  def erase_line(mode) end

  # Erases the screen at the cursor corresponding to +mode+.
  # +mode+ may be either:
  # 0: after cursor
  # 1: before and cursor
  # 2: entire screen
  #
  # You must require 'io/console' to use this method.
  def erase_screen(mode) end

  # Returns the Encoding object that represents the encoding of the stream,
  # or +nil+ if the stream is in write mode and no encoding is specified.
  #
  # See {Encodings}[rdoc-ref:File@Encodings].
  def external_encoding; end

  # Invokes Posix system call {fcntl(2)}[https://linux.die.net/man/2/fcntl],
  # which provides a mechanism for issuing low-level commands to control or query
  # a file-oriented I/O stream. Arguments and results are platform
  # dependent.
  #
  # If +argument+ is a number, its value is passed directly;
  # if it is a string, it is interpreted as a binary sequence of bytes.
  # (Array#pack might be a useful way to build this string.)
  #
  # Not implemented on all platforms.
  def fcntl(integer_cmd, argument) end

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
  #   f.close
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

  # Reads and returns the next byte (in range 0..255) from the stream;
  # returns +nil+ if already at end-of-stream.
  # See {Byte IO}[rdoc-ref:IO@Byte+IO].
  #
  #   f = File.open('t.txt')
  #   f.getbyte # => 70
  #   f.close
  #   f = File.open('t.rus')
  #   f.getbyte # => 209
  #   f.close
  #
  # Related: IO#readbyte (may raise EOFError).
  def getbyte; end

  # Reads and returns the next 1-character string from the stream;
  # returns +nil+ if already at end-of-stream.
  # See {Character IO}[rdoc-ref:IO@Character+IO].
  #
  #   f = File.open('t.txt')
  #   f.getc     # => "F"
  #   f.close
  #   f = File.open('t.rus')
  #   f.getc.ord # => 1090
  #   f.close
  #
  # Related:  IO#readchar (may raise EOFError).
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
  #
  #    require 'io/console'
  #    IO::console.getpass("Enter password:")
  #    Enter password:
  #    # => "mypassword"
  def getpass(prompt = nil) end

  # Reads and returns a line from the stream;
  # assigns the return value to <tt>$_</tt>.
  # See {Line IO}[rdoc-ref:IO@Line+IO].
  #
  # With no arguments given, returns the next line
  # as determined by line separator <tt>$/</tt>, or +nil+ if none:
  #
  #   f = File.open('t.txt')
  #   f.gets # => "First line\n"
  #   $_     # => "First line\n"
  #   f.gets # => "\n"
  #   f.gets # => "Fourth line\n"
  #   f.gets # => "Fifth line\n"
  #   f.gets # => nil
  #   f.close
  #
  # With only string argument +sep+ given,
  # returns the next line as determined by line separator +sep+,
  # or +nil+ if none;
  # see {Line Separator}[rdoc-ref:IO@Line+Separator]:
  #
  #   f = File.new('t.txt')
  #   f.gets('l')   # => "First l"
  #   f.gets('li')  # => "ine\nSecond li"
  #   f.gets('lin') # => "ne\n\nFourth lin"
  #   f.gets        # => "e\n"
  #   f.close
  #
  # The two special values for +sep+ are honored:
  #
  #   f = File.new('t.txt')
  #   # Get all.
  #   f.gets(nil) # => "First line\nSecond line\n\nFourth line\nFifth line\n"
  #   f.rewind
  #   # Get paragraph (up to two line separators).
  #   f.gets('')  # => "First line\nSecond line\n\n"
  #   f.close
  #
  # With only integer argument +limit+ given,
  # limits the number of bytes in the line;
  # see {Line Limit}[rdoc-ref:IO@Line+Limit]:
  #
  #   # No more than one line.
  #   File.open('t.txt') {|f| f.gets(10) } # => "First line"
  #   File.open('t.txt') {|f| f.gets(11) } # => "First line\n"
  #   File.open('t.txt') {|f| f.gets(12) } # => "First line\n"
  #
  # With arguments +sep+ and +limit+ given,
  # combines the two behaviors
  # (see {Line Separator and Line Limit}[rdoc-ref:IO@Line+Separator+and+Line+Limit]).
  #
  # Optional keyword argument +chomp+ specifies whether line separators
  # are to be omitted:
  #
  #   f = File.open('t.txt')
  #   # Chomp the lines.
  #   f.gets(chomp: true) # => "First line"
  #   f.gets(chomp: true) # => "Second line"
  #   f.gets(chomp: true) # => ""
  #   f.gets(chomp: true) # => "Fourth line"
  #   f.gets(chomp: true) # => "Fifth line"
  #   f.gets(chomp: true) # => nil
  #   f.close
  def gets(...) end

  # Set the cursor position at +line+ and +column+.
  #
  # You must require 'io/console' to use this method.
  def goto(line, column) end

  # Set the cursor position at +column+ in the same line of the current
  # position.
  #
  # You must require 'io/console' to use this method.
  def goto_column(column) end

  # Flushes input buffer in kernel.
  #
  # You must require 'io/console' to use this method.
  def iflush; end

  # Returns a string representation of +self+:
  #
  #   f = File.open('t.txt')
  #   f.inspect # => "#<File:t.txt>"
  #   f.close
  def inspect; end

  # Returns the Encoding object that represents the encoding of the internal string,
  # if conversion is specified,
  # or +nil+ otherwise.
  #
  # See {Encodings}[rdoc-ref:File@Encodings].
  def internal_encoding; end

  # Invokes Posix system call {ioctl(2)}[https://linux.die.net/man/2/ioctl],
  # which issues a low-level command to an I/O device.
  #
  # Issues a low-level command to an I/O device.
  # The arguments and returned value are platform-dependent.
  # The effect of the call is platform-dependent.
  #
  # If argument +argument+ is an integer, it is passed directly;
  # if it is a string, it is interpreted as a binary sequence of bytes.
  #
  # Not implemented on all platforms.
  def ioctl(integer_cmd, argument) end

  # Flushes input and output buffers in kernel.
  #
  # You must require 'io/console' to use this method.
  def ioflush; end

  # Returns +true+ if the stream is associated with a terminal device (tty),
  # +false+ otherwise:
  #
  #   f = File.new('t.txt').isatty    #=> false
  #   f.close
  #   f = File.new('/dev/tty').isatty #=> true
  #   f.close
  def isatty; end
  alias tty? isatty

  # Returns the current line number for the stream;
  # see {Line Number}[rdoc-ref:IO@Line+Number].
  def lineno; end

  # Sets and returns the line number for the stream;
  # see {Line Number}[rdoc-ref:IO@Line+Number].
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
  #
  # This method set or clear O_NONBLOCK flag for the file descriptor
  # in <em>ios</em>.
  #
  # The behavior of most IO methods is not affected by this flag
  # because they retry system calls to complete their task
  # after EAGAIN and partial read/write.
  # (An exception is IO#syswrite which doesn't retry.)
  #
  # This method can be used to clear non-blocking mode of standard I/O.
  # Since nonblocking methods (read_nonblock, etc.) set non-blocking mode but
  # they doesn't clear it, this method is usable as follows.
  #
  #   END { STDOUT.nonblock = false }
  #   STDOUT.write_nonblock("foo")
  #
  # Since the flag is shared across processes and
  # many non-Ruby commands doesn't expect standard I/O with non-blocking mode,
  # it would be safe to clear the flag before Ruby program exits.
  #
  # For example following Ruby program leaves STDIN/STDOUT/STDER non-blocking mode.
  # (STDIN, STDOUT and STDERR are connected to a terminal.
  # So making one of them nonblocking-mode effects other two.)
  # Thus cat command try to read from standard input and
  # it causes "Resource temporarily unavailable" error (EAGAIN).
  #
  #   % ruby -e '
  #   STDOUT.write_nonblock("foo\n")'; cat
  #   foo
  #   cat: -: Resource temporarily unavailable
  #
  # Clearing the flag makes the behavior of cat command normal.
  # (cat command waits input from standard input.)
  #
  #   % ruby -rio/nonblock -e '
  #   END { STDOUT.nonblock = false }
  #   STDOUT.write_nonblock("foo")
  #   '; cat
  #   foo
  def nonblock=(boolean) end

  # Returns +true+ if an IO object is in non-blocking mode.
  def nonblock?; end

  # Returns number of bytes that can be read without blocking.
  # Returns zero if no information available.
  #
  # You must require 'io/wait' to use this method.
  def nread; end

  # Flushes output buffer in kernel.
  #
  # You must require 'io/console' to use this method.
  def oflush; end

  # Returns the path associated with the IO, or +nil+ if there is no path
  # associated with the IO. It is not guaranteed that the path exists on
  # the filesystem.
  #
  #   $stdin.path # => "<STDIN>"
  #
  #   File.open("testfile") {|f| f.path} # => "testfile"
  def path; end
  alias to_path path

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
  def pathconf(name) end

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
  # see {Position}[rdoc-ref:IO@Position]:
  #
  #   f = File.open('t.txt')
  #   f.tell     # => 0
  #   f.pos = 20 # => 20
  #   f.tell     # => 20
  #   f.close
  #
  # Related: IO#seek, IO#tell.
  def pos=(new_position) end

  # Behaves like IO#readpartial, except that it:
  #
  # - Reads at the given +offset+ (in bytes).
  # - Disregards, and does not modify, the stream's position
  #   (see {Position}[rdoc-ref:IO@Position]).
  # - Bypasses any user space buffering in the stream.
  #
  # Because this method does not disturb the stream's state
  # (its position, in particular), +pread+ allows multiple threads and processes
  # to use the same \IO object for reading at various offsets.
  #
  #   f = File.open('t.txt')
  #   f.read # => "First line\nSecond line\n\nFourth line\nFifth line\n"
  #   f.pos  # => 52
  #   # Read 12 bytes at offset 0.
  #   f.pread(12, 0) # => "First line\n"
  #   # Read 9 bytes at offset 8.
  #   f.pread(9, 8)  # => "ne\nSecon"
  #   f.close
  #
  # Not available on some platforms.
  def pread(...) end

  # Returns +true+ if +key+ is pressed.  +key+ may be a virtual key
  # code or its name (String or Symbol) with out "VK_" prefix.
  #
  # This method is Windows only.
  #
  # You must require 'io/console' to use this method.
  def pressed?(key) end

  # Writes the given objects to the stream; returns +nil+.
  # Appends the output record separator <tt>$OUTPUT_RECORD_SEPARATOR</tt>
  # (<tt>$\\</tt>), if it is not +nil+.
  # See {Line IO}[rdoc-ref:IO@Line+IO].
  #
  # With argument +objects+ given, for each object:
  #
  # - Converts via its method +to_s+ if not a string.
  # - Writes to the stream.
  # - If not the last object, writes the output field separator
  #   <tt>$OUTPUT_FIELD_SEPARATOR</tt> (<tt>$,</tt>) if it is not +nil+.
  #
  # With default separators:
  #
  #   f = File.open('t.tmp', 'w+')
  #   objects = [0, 0.0, Rational(0, 1), Complex(0, 0), :zero, 'zero']
  #   p $OUTPUT_RECORD_SEPARATOR
  #   p $OUTPUT_FIELD_SEPARATOR
  #   f.print(*objects)
  #   f.rewind
  #   p f.read
  #   f.close
  #
  # Output:
  #
  #   nil
  #   nil
  #   "00.00/10+0izerozero"
  #
  # With specified separators:
  #
  #   $\ = "\n"
  #   $, = ','
  #   f.rewind
  #   f.print(*objects)
  #   f.rewind
  #   p f.read
  #
  # Output:
  #
  #   "0,0.0,0/1,0+0i,zero,zero\n"
  #
  # With no argument given, writes the content of <tt>$_</tt>
  # (which is usually the most recent user input):
  #
  #   f = File.open('t.tmp', 'w+')
  #   gets # Sets $_ to the most recent user input.
  #   f.print
  #   f.close
  def print(*objects) end

  # Formats and writes +objects+ to the stream.
  #
  # For details on +format_string+, see
  # {Format Specifications}[rdoc-ref:format_specifications.rdoc].
  def printf(format_string, *objects) end

  # Writes a character to the stream.
  # See {Character IO}[rdoc-ref:IO@Character+IO].
  #
  # If +object+ is numeric, converts to integer if necessary,
  # then writes the character whose code is the
  # least significant byte;
  # if +object+ is a string, writes the first character:
  #
  #   $stdout.putc "A"
  #   $stdout.putc 65
  #
  # Output:
  #
  #    AA
  def putc(object) end

  # Writes the given +objects+ to the stream, which must be open for writing;
  # returns +nil+.\
  # Writes a newline after each that does not already end with a newline sequence.
  # If called without arguments, writes a newline.
  # See {Line IO}[rdoc-ref:IO@Line+IO].
  #
  # Note that each added newline is the character <tt>"\n"<//tt>,
  # not the output record separator (<tt>$\\</tt>).
  #
  # Treatment for each object:
  #
  # - String: writes the string.
  # - Neither string nor array: writes <tt>object.to_s</tt>.
  # - Array: writes each element of the array; arrays may be nested.
  #
  # To keep these examples brief, we define this helper method:
  #
  #   def show(*objects)
  #     # Puts objects to file.
  #     f = File.new('t.tmp', 'w+')
  #     f.puts(objects)
  #     # Return file content.
  #     f.rewind
  #     p f.read
  #     f.close
  #   end
  #
  #   # Strings without newlines.
  #   show('foo', 'bar', 'baz')     # => "foo\nbar\nbaz\n"
  #   # Strings, some with newlines.
  #   show("foo\n", 'bar', "baz\n") # => "foo\nbar\nbaz\n"
  #
  #   # Neither strings nor arrays:
  #   show(0, 0.0, Rational(0, 1), Complex(9, 0), :zero)
  #   # => "0\n0.0\n0/1\n9+0i\nzero\n"
  #
  #   # Array of strings.
  #   show(['foo', "bar\n", 'baz']) # => "foo\nbar\nbaz\n"
  #   # Nested arrays.
  #   show([[[0, 1], 2, 3], 4, 5])  # => "0\n1\n2\n3\n4\n5\n"
  def puts(*objects) end

  # Behaves like IO#write, except that it:
  #
  # - Writes at the given +offset+ (in bytes).
  # - Disregards, and does not modify, the stream's position
  #   (see {Position}[rdoc-ref:IO@Position]).
  # - Bypasses any user space buffering in the stream.
  #
  # Because this method does not disturb the stream's state
  # (its position, in particular), +pwrite+ allows multiple threads and processes
  # to use the same \IO object for writing at various offsets.
  #
  #   f = File.open('t.tmp', 'w+')
  #   # Write 6 bytes at offset 3.
  #   f.pwrite('ABCDEF', 3) # => 6
  #   f.rewind
  #   f.read # => "\u0000\u0000\u0000ABCDEF"
  #   f.close
  #
  # Not available on some platforms.
  def pwrite(object, offset) end

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

  # Reads bytes from the stream; the stream must be opened for reading
  # (see {Access Modes}[rdoc-ref:File@Access+Modes]):
  #
  # - If +maxlen+ is +nil+, reads all bytes using the stream's data mode.
  # - Otherwise reads up to +maxlen+ bytes in binary mode.
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
  #   # => "First line\nSecond line\n\nFourth line\nFifth line\n"
  #   f.rewind
  #   f.read(30) # => "First line\r\nSecond line\r\n\r\nFou"
  #   f.read(30) # => "rth line\r\nFifth line\r\n"
  #   f.read(30) # => nil
  #   f.close
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
  #   f.read(nil, s) # => "First line\nSecond line\n\nFourth line\nFifth line\n"
  #   s              # => "First line\nSecond line\n\nFourth line\nFifth line\n"
  #   f.rewind
  #   s = 'bar'
  #   f.read(30, s)  # => "First line\r\nSecond line\r\n\r\nFou"
  #   s              # => "First line\r\nSecond line\r\n\r\nFou"
  #   s = 'baz'
  #   f.read(30, s)  # => "rth line\r\nFifth line\r\n"
  #   s              # => "rth line\r\nFifth line\r\n"
  #   s = 'bat'
  #   f.read(30, s)  # => nil
  #   s              # => ""
  #   f.close
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
  #
  # Related: IO#write.
  def read(maxlen = nil, out_string = nil) end

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

  # Reads and returns the next byte (in range 0..255) from the stream;
  # raises EOFError if already at end-of-stream.
  # See {Byte IO}[rdoc-ref:IO@Byte+IO].
  #
  #   f = File.open('t.txt')
  #   f.readbyte # => 70
  #   f.close
  #   f = File.open('t.rus')
  #   f.readbyte # => 209
  #   f.close
  #
  # Related: IO#getbyte (will not raise EOFError).
  def readbyte; end

  # Reads and returns the next 1-character string from the stream;
  # raises EOFError if already at end-of-stream.
  # See {Character IO}[rdoc-ref:IO@Character+IO].
  #
  #   f = File.open('t.txt')
  #   f.readchar     # => "F"
  #   f.close
  #   f = File.open('t.rus')
  #   f.readchar.ord # => 1090
  #   f.close
  #
  # Related:  IO#getc (will not raise EOFError).
  def readchar; end

  # Reads a line as with IO#gets, but raises EOFError if already at end-of-stream.
  #
  # Optional keyword argument +chomp+ specifies whether line separators
  # are to be omitted.
  def readline(...) end

  # Reads and returns all remaining line from the stream;
  # does not modify <tt>$_</tt>.
  # See {Line IO}[rdoc-ref:IO@Line+IO].
  #
  # With no arguments given, returns lines
  # as determined by line separator <tt>$/</tt>, or +nil+ if none:
  #
  #   f = File.new('t.txt')
  #   f.readlines
  #   # => ["First line\n", "Second line\n", "\n", "Fourth line\n", "Fifth line\n"]
  #   f.readlines # => []
  #   f.close
  #
  # With only string argument +sep+ given,
  # returns lines as determined by line separator +sep+,
  # or +nil+ if none;
  # see {Line Separator}[rdoc-ref:IO@Line+Separator]:
  #
  #   f = File.new('t.txt')
  #   f.readlines('li')
  #   # => ["First li", "ne\nSecond li", "ne\n\nFourth li", "ne\nFifth li", "ne\n"]
  #   f.close
  #
  # The two special values for +sep+ are honored:
  #
  #   f = File.new('t.txt')
  #   # Get all into one string.
  #   f.readlines(nil)
  #   # => ["First line\nSecond line\n\nFourth line\nFifth line\n"]
  #   # Get paragraphs (up to two line separators).
  #   f.rewind
  #   f.readlines('')
  #   # => ["First line\nSecond line\n\n", "Fourth line\nFifth line\n"]
  #   f.close
  #
  # With only integer argument +limit+ given,
  # limits the number of bytes in each line;
  # see {Line Limit}[rdoc-ref:IO@Line+Limit]:
  #
  #   f = File.new('t.txt')
  #   f.readlines(8)
  #   # => ["First li", "ne\n", "Second l", "ine\n", "\n", "Fourth l", "ine\n", "Fifth li", "ne\n"]
  #   f.close
  #
  # With arguments +sep+ and +limit+ given,
  # combines the two behaviors
  # (see {Line Separator and Line Limit}[rdoc-ref:IO@Line+Separator+and+Line+Limit]).
  #
  # Optional keyword argument +chomp+ specifies whether line separators
  # are to be omitted:
  #
  #   f = File.new('t.txt')
  #   f.readlines(chomp: true)
  #   # => ["First line", "Second line", "", "Fourth line", "Fifth line"]
  #   f.close
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
  #   f.readpartial(20) # => "First line\nSecond l"
  #   f.readpartial(20) # => "ine\n\nFourth line\n"
  #   f.readpartial(20) # => "Fifth line\n"
  #   f.readpartial(20) # Raises EOFError.
  #   f.close
  #
  # With both argument +maxlen+ and string argument +out_string+ given,
  # returns modified +out_string+:
  #
  #   f = File.new('t.txt')
  #   s = 'foo'
  #   f.readpartial(20, s) # => "First line\nSecond l"
  #   s = 'bar'
  #   f.readpartial(0, s)  # => ""
  #   f.close
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

  # Returns a truthy value if input available without blocking, or a
  # falsy value.
  #
  # You must require 'io/wait' to use this method.
  def ready?; end

  # Reassociates the stream with another stream,
  # which may be of a different class.
  # This method may be used to redirect an existing stream
  # to a new destination.
  #
  # With argument +other_io+ given, reassociates with that stream:
  #
  #   # Redirect $stdin from a file.
  #   f = File.open('t.txt')
  #   $stdin.reopen(f)
  #   f.close
  #
  #   # Redirect $stdout to a file.
  #   f = File.open('t.tmp', 'w')
  #   $stdout.reopen(f)
  #   f.close
  #
  # With argument +path+ given, reassociates with a new stream to that file path:
  #
  #   $stdin.reopen('t.txt')
  #   $stdout.reopen('t.tmp', 'w')
  #
  # Optional keyword arguments +opts+ specify:
  #
  # - {Open Options}[rdoc-ref:IO@Open+Options].
  # - {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  def reopen(...) end

  # Repositions the stream to its beginning,
  # setting both the position and the line number to zero;
  # see {Position}[rdoc-ref:IO@Position]
  # and {Line Number}[rdoc-ref:IO@Line+Number]:
  #
  #   f = File.open('t.txt')
  #   f.tell     # => 0
  #   f.lineno   # => 0
  #   f.gets     # => "First line\n"
  #   f.tell     # => 12
  #   f.lineno   # => 1
  #   f.rewind   # => 0
  #   f.tell     # => 0
  #   f.lineno   # => 0
  #   f.close
  #
  # Note that this method cannot be used with streams such as pipes, ttys, and sockets.
  def rewind; end

  # Scrolls the entire scrolls backward +n+ lines.
  #
  # You must require 'io/console' to use this method.
  def scroll_backward(n) end

  # Scrolls the entire scrolls forward +n+ lines.
  #
  # You must require 'io/console' to use this method.
  def scroll_forward(n) end

  # Seeks to the position given by integer +offset+
  # (see {Position}[rdoc-ref:IO@Position])
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
  #     f.close
  #
  # - +:END+ or <tt>IO::SEEK_END</tt>:
  #   Repositions the stream to its end plus the given +offset+:
  #
  #     f = File.open('t.txt')
  #     f.tell            # => 0
  #     f.seek(0, :END)   # => 0  # Repositions to stream end.
  #     f.tell            # => 52
  #     f.seek(-20, :END) # => 0
  #     f.tell            # => 32
  #     f.seek(-40, :END) # => 0
  #     f.tell            # => 12
  #     f.close
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
  #     f.close
  #
  # Related: IO#pos=, IO#tell.
  def seek(offset, whence = IO::SEEK_SET) end

  # See {Encodings}[rdoc-ref:File@Encodings].
  #
  # Argument +ext_enc+, if given, must be an Encoding object
  # or a String with the encoding name;
  # it is assigned as the encoding for the stream.
  #
  # Argument +int_enc+, if given, must be an Encoding object
  # or a String with the encoding name;
  # it is assigned as the encoding for the internal string.
  #
  # Argument <tt>'ext_enc:int_enc'</tt>, if given, is a string
  # containing two colon-separated encoding names;
  # corresponding Encoding objects are assigned as the external
  # and internal encodings for the stream.
  #
  # If the external encoding of a string is binary/ASCII-8BIT,
  # the internal encoding of the string is set to nil, since no
  # transcoding is needed.
  #
  # Optional keyword arguments +enc_opts+ specify
  # {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  def set_encoding(...) end

  # If the stream begins with a BOM
  # ({byte order marker}[https://en.wikipedia.org/wiki/Byte_order_mark]),
  # consumes the BOM and sets the external encoding accordingly;
  # returns the result encoding if found, or +nil+ otherwise:
  #
  #  File.write('t.tmp', "\u{FEFF}abc")
  #  io = File.open('t.tmp', 'rb')
  #  io.set_encoding_by_bom # => #<Encoding:UTF-8>
  #  io.close
  #
  #  File.write('t.tmp', 'abc')
  #  io = File.open('t.tmp', 'rb')
  #  io.set_encoding_by_bom # => nil
  #  io.close
  #
  # Raises an exception if the stream is not binmode
  # or its encoding has already been set.
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
  #   f.close
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
  #   f.close
  #
  # Related: IO#fsync.
  def sync=(boolean) end

  # Behaves like IO#readpartial, except that it uses low-level system functions.
  #
  # This method should not be used with other stream-reader methods.
  def sysread(...) end

  # Behaves like IO#seek, except that it:
  #
  # - Uses low-level system functions.
  # - Returns the new position.
  def sysseek(offset, whence = IO::SEEK_SET) end

  # Writes the given +object+ to self, which must be opened for writing (see Modes);
  # returns the number bytes written.
  # If +object+ is not a string is converted via method to_s:
  #
  #   f = File.new('t.tmp', 'w')
  #   f.syswrite('foo') # => 3
  #   f.syswrite(30)    # => 2
  #   f.syswrite(:foo)  # => 3
  #   f.close
  #
  # This methods should not be used with other stream-writer methods.
  def syswrite(object) end

  # Returns the current position (in bytes) in +self+
  # (see {Position}[rdoc-ref:IO@Position]):
  #
  #   f = File.open('t.txt')
  #   f.tell # => 0
  #   f.gets # => "First line\n"
  #   f.tell # => 12
  #   f.close
  #
  # Related: IO#pos=, IO#seek.
  def tell; end
  alias pos tell

  # Get the internal timeout duration or nil if it was not set.
  def timeout; end

  # Sets the internal timeout to the specified duration or nil. The timeout
  # applies to all blocking operations where possible.
  #
  # When the operation performs longer than the timeout set, IO::TimeoutError
  # is raised.
  #
  # This affects the following methods (but is not limited to): #gets, #puts,
  # #read, #write, #wait_readable and #wait_writable. This also affects
  # blocking socket operations like Socket#accept and Socket#connect.
  #
  # Some operations like File#open and IO#close are not affected by the
  # timeout. A timeout during a write operation may leave the IO in an
  # inconsistent state, e.g. data was partially written. Generally speaking, a
  # timeout is a last ditch effort to prevent an application from hanging on
  # slow I/O operations, such as those that occur during a slowloris attack.
  def timeout=(...) end

  # Returns +self+.
  def to_io; end

  # Returns name of associated terminal (tty) if +io+ is not a tty.
  # Returns +nil+ otherwise.
  def ttyname; end

  # Pushes back ("unshifts") the given data onto the stream's buffer,
  # placing the data so that it is next to be read; returns +nil+.
  # See {Byte IO}[rdoc-ref:IO@Byte+IO].
  #
  # Note that:
  #
  # - Calling the method has no effect with unbuffered reads (such as IO#sysread).
  # - Calling #rewind on the stream discards the pushed-back data.
  #
  # When argument +integer+ is given, uses only its low-order byte:
  #
  #   File.write('t.tmp', '012')
  #   f = File.open('t.tmp')
  #   f.ungetbyte(0x41)   # => nil
  #   f.read              # => "A012"
  #   f.rewind
  #   f.ungetbyte(0x4243) # => nil
  #   f.read              # => "C012"
  #   f.close
  #
  # When argument +string+ is given, uses all bytes:
  #
  #   File.write('t.tmp', '012')
  #   f = File.open('t.tmp')
  #   f.ungetbyte('A')    # => nil
  #   f.read              # => "A012"
  #   f.rewind
  #   f.ungetbyte('BCDE') # => nil
  #   f.read              # => "BCDE012"
  #   f.close
  def ungetbyte(...) end

  # Pushes back ("unshifts") the given data onto the stream's buffer,
  # placing the data so that it is next to be read; returns +nil+.
  # See {Character IO}[rdoc-ref:IO@Character+IO].
  #
  # Note that:
  #
  # - Calling the method has no effect with unbuffered reads (such as IO#sysread).
  # - Calling #rewind on the stream discards the pushed-back data.
  #
  # When argument +integer+ is given, interprets the integer as a character:
  #
  #   File.write('t.tmp', '012')
  #   f = File.open('t.tmp')
  #   f.ungetc(0x41)     # => nil
  #   f.read             # => "A012"
  #   f.rewind
  #   f.ungetc(0x0442)   # => nil
  #   f.getc.ord         # => 1090
  #   f.close
  #
  # When argument +string+ is given, uses all characters:
  #
  #   File.write('t.tmp', '012')
  #   f = File.open('t.tmp')
  #   f.ungetc('A')      # => nil
  #   f.read      # => "A012"
  #   f.rewind
  #   f.ungetc("\u0442\u0435\u0441\u0442") # => nil
  #   f.getc.ord      # => 1090
  #   f.getc.ord      # => 1077
  #   f.getc.ord      # => 1089
  #   f.getc.ord      # => 1090
  #   f.close
  def ungetc(...) end

  # Waits until the IO becomes ready for the specified events and returns the
  # subset of events that become ready, or a falsy value when times out.
  #
  # The events can be a bit mask of +IO::READABLE+, +IO::WRITABLE+ or
  # +IO::PRIORITY+.
  #
  # Returns a truthy value immediately when buffered data is available.
  #
  # Optional parameter +mode+ is one of +:read+, +:write+, or
  # +:read_write+.
  #
  # You must require 'io/wait' to use this method.
  def wait(...) end

  # Waits until IO is priority and returns a truthy value or a falsy
  # value when times out. Priority data is sent and received using
  # the Socket::MSG_OOB flag and is typically limited to streams.
  #
  # You must require 'io/wait' to use this method.
  def wait_priority(...) end

  # Waits until IO is readable and returns a truthy value, or a falsy
  # value when times out.  Returns a truthy value immediately when
  # buffered data is available.
  #
  # You must require 'io/wait' to use this method.
  def wait_readable(...) end

  # Waits until IO is writable and returns a truthy value or a falsy
  # value when times out.
  #
  # You must require 'io/wait' to use this method.
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
  # which must be opened for writing
  # (see {Access Modes}[rdoc-ref:File@Access+Modes]);
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
  #
  # Related: IO#read.
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

  # IO::Buffer is a efficient zero-copy buffer for input/output. There are
  # typical use cases:
  #
  # * Create an empty buffer with ::new, fill it with buffer using #copy or
  #   #set_value, #set_string, get buffer with #get_string or write it directly
  #   to some file with #write.
  # * Create a buffer mapped to some string with ::for, then it could be used
  #   both for reading with #get_string or #get_value, and writing (writing will
  #   change the source string, too).
  # * Create a buffer mapped to some file with ::map, then it could be used for
  #   reading and writing the underlying file.
  # * Create a string of a fixed size with ::string, then #read into it, or
  #   modify it using #set_value.
  #
  # Interaction with string and file memory is performed by efficient low-level
  # C mechanisms like `memcpy`.
  #
  # The class is meant to be an utility for implementing more high-level mechanisms
  # like Fiber::Scheduler#io_read and Fiber::Scheduler#io_write and parsing binary
  # protocols.
  #
  # == Examples of Usage
  #
  # Empty buffer:
  #
  #   buffer = IO::Buffer.new(8)  # create empty 8-byte buffer
  #   # =>
  #   # #<IO::Buffer 0x0000555f5d1a5c50+8 INTERNAL>
  #   # ...
  #   buffer
  #   # =>
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
  #   IO::Buffer.for(string) do |buffer|
  #     buffer
  #     # =>
  #     # #<IO::Buffer 0x00007f3f02be9b18+4 SLICE>
  #     # 0x00000000  64 61 74 61                                     data
  #
  #     buffer.get_string(2)  # read content starting from offset 2
  #     # => "ta"
  #     buffer.set_string('---', 1) # write content, starting from offset 1
  #     # => 3
  #     buffer
  #     # =>
  #     # #<IO::Buffer 0x00007f3f02be9b18+4 SLICE>
  #     # 0x00000000  64 2d 2d 2d                                     d---
  #     string  # original string changed, too
  #     # => "d---"
  #   end
  #
  # \Buffer from file:
  #
  #   File.write('test.txt', 'test data')
  #   # => 9
  #   buffer = IO::Buffer.map(File.open('test.txt'))
  #   # =>
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
  # <b>The class is experimental and the interface is subject to change, this
  # is especially true of file mappings which may be removed entirely in
  # the future.</b>
  class Buffer
    include Comparable

    # Refers to big endian byte order, where the most significant byte is stored first. See #get_value for more details.
    BIG_ENDIAN = _
    # The default buffer size, typically a (small) multiple of the PAGE_SIZE.
    # Can be explicitly specified by setting the RUBY_IO_BUFFER_DEFAULT_SIZE
    # environment variable.
    DEFAULT_SIZE = _
    # Indicates that the memory in the buffer is owned by someone else. See #external? for more details.
    EXTERNAL = _
    # Refers to the byte order of the host machine. See #get_value for more details.
    HOST_ENDIAN = _
    # Indicates that the memory in the buffer is owned by the buffer. See #internal? for more details.
    INTERNAL = _
    # Refers to little endian byte order, where the least significant byte is stored first. See #get_value for more details.
    LITTLE_ENDIAN = _
    # Indicates that the memory in the buffer is locked and cannot be resized or freed. See #locked? and #locked for more details.
    LOCKED = _
    # Indicates that the memory in the buffer is mapped by the operating system. See #mapped? for more details.
    MAPPED = _
    # Refers to network byte order, which is the same as big endian. See #get_value for more details.
    NETWORK_ENDIAN = _
    # The operating system page size. Used for efficient page-aligned memory allocations.
    PAGE_SIZE = _
    # Indicates that the memory in the buffer is mapped privately and changes won't be replicated to the underlying file. See #private? for more details.
    PRIVATE = _
    # Indicates that the memory in the buffer is read only, and attempts to modify it will fail. See #readonly? for more details.
    READONLY = _
    # Indicates that the memory in the buffer is also mapped such that it can be shared with other processes. See #shared? for more details.
    SHARED = _

    # Creates a zero-copy IO::Buffer from the given string's memory. Without a
    # block a frozen internal copy of the string is created efficiently and used
    # as the buffer source. When a block is provided, the buffer is associated
    # directly with the string's internal buffer and updating the buffer will
    # update the string.
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
    #   File.write('test.txt', 'test')
    #
    #   buffer = IO::Buffer.map(File.open('test.txt'), nil, 0, IO::Buffer::READONLY)
    #   # => #<IO::Buffer 0x00000001014a0000+4 MAPPED READONLY>
    #
    #   buffer.readonly?   # => true
    #
    #   buffer.get_string
    #   # => "test"
    #
    #   buffer.set_string('b', 0)
    #   # `set_string': Buffer is not writable! (IO::Buffer::AccessError)
    #
    #   # create read/write mapping: length 4 bytes, offset 0, flags 0
    #   buffer = IO::Buffer.map(File.open('test.txt', 'r+'), 4, 0)
    #   buffer.set_string('b', 0)
    #   # => 1
    #
    #   # Check it
    #   File.read('test.txt')
    #   # => "best"
    #
    # Note that some operating systems may not have cache coherency between mapped
    # buffers and file reads.
    def self.map(*args) end

    # Returns the size of the given buffer type(s) in bytes.
    #
    #   IO::Buffer.size_of(:u32) # => 4
    #   IO::Buffer.size_of([:u32, :u32]) # => 8
    def self.size_of(...) end

    # Creates a new string of the given length and yields a zero-copy IO::Buffer
    # instance to the block which uses the string as a source. The block is
    # expected to write to the buffer and the string will be returned.
    #
    #    IO::Buffer.string(4) do |buffer|
    #      buffer.set_string("Ruby")
    #    end
    #    # => "Ruby"
    def self.string(length) end

    # Create a new zero-filled IO::Buffer of +size+ bytes.
    # By default, the buffer will be _internal_: directly allocated chunk
    # of the memory. But if the requested +size+ is more than OS-specific
    # IO::Buffer::PAGE_SIZE, the buffer would be allocated using the
    # virtual memory mechanism (anonymous +mmap+ on Unix, +VirtualAlloc+
    # on Windows). The behavior can be forced by passing IO::Buffer::MAPPED
    # as a second parameter.
    #
    #   buffer = IO::Buffer.new(4)
    #   # =>
    #   # #<IO::Buffer 0x000055b34497ea10+4 INTERNAL>
    #   # 0x00000000  00 00 00 00                                     ....
    #
    #   buffer.get_string(0, 1) # => "\x00"
    #
    #   buffer.set_string("test")
    #   buffer
    #   # =>
    #   # #<IO::Buffer 0x000055b34497ea10+4 INTERNAL>
    #   # 0x00000000  74 65 73 74                                     test
    def initialize(*args) end

    # Generate a new buffer the same size as the source by applying the binary AND
    # operation to the source, using the mask, repeating as necessary.
    #
    #   IO::Buffer.for("1234567890") & IO::Buffer.for("\xFF\x00\x00\xFF")
    #   # =>
    #   # #<IO::Buffer 0x00005589b2758480+4 INTERNAL>
    #   # 0x00000000  31 00 00 34 35 00 00 38 39 00                   1..45..89.
    def &(other) end

    # Buffers are compared by size and exact contents of the memory they are
    # referencing using +memcmp+.
    def <=>(other) end

    # Generate a new buffer the same size as the source by applying the binary XOR
    # operation to the source, using the mask, repeating as necessary.
    #
    #   IO::Buffer.for("1234567890") ^ IO::Buffer.for("\xFF\x00\x00\xFF")
    #   # =>
    #   # #<IO::Buffer 0x000055a2d5d10480+10 INTERNAL>
    #   # 0x00000000  ce 32 33 cb ca 36 37 c7 c6 30                   .23..67..0
    def ^(other) end

    # Generate a new buffer the same size as the source by applying the binary OR
    # operation to the source, using the mask, repeating as necessary.
    #
    #   IO::Buffer.for("1234567890") | IO::Buffer.for("\xFF\x00\x00\xFF")
    #   # =>
    #   # #<IO::Buffer 0x0000561785ae3480+10 INTERNAL>
    #   # 0x00000000  ff 32 33 ff ff 36 37 ff ff 30                   .23..67..0
    def |(other) end

    # Generate a new buffer the same size as the source by applying the binary NOT
    # operation to the source.
    #
    #   ~IO::Buffer.for("1234567890")
    #   # =>
    #   # #<IO::Buffer 0x000055a5ac42f120+10 INTERNAL>
    #   # 0x00000000  ce cd cc cb ca c9 c8 c7 c6 cf                   ..........
    def ~; end

    # Modify the source buffer in place by applying the binary AND
    # operation to the source, using the mask, repeating as necessary.
    #
    #   source = IO::Buffer.for("1234567890").dup # Make a read/write copy.
    #   # =>
    #   # #<IO::Buffer 0x000056307a0d0c20+10 INTERNAL>
    #   # 0x00000000  31 32 33 34 35 36 37 38 39 30                   1234567890
    #
    #   source.and!(IO::Buffer.for("\xFF\x00\x00\xFF"))
    #   # =>
    #   # #<IO::Buffer 0x000056307a0d0c20+10 INTERNAL>
    #   # 0x00000000  31 00 00 34 35 00 00 38 39 00                   1..45..89.
    def and!(mask) end

    # Fill buffer with +value+, starting with +offset+ and going for +length+
    # bytes.
    #
    #   buffer = IO::Buffer.for('test').dup
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 INTERNAL>
    #   #   0x00000000  74 65 73 74         test
    #
    #   buffer.clear
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 INTERNAL>
    #   #   0x00000000  00 00 00 00         ....
    #
    #   buf.clear(1) # fill with 1
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 INTERNAL>
    #   #   0x00000000  01 01 01 01         ....
    #
    #   buffer.clear(2, 1, 2) # fill with 2, starting from offset 1, for 2 bytes
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 INTERNAL>
    #   #   0x00000000  01 02 02 01         ....
    #
    #   buffer.clear(2, 1) # fill with 2, starting from offset 1
    #   # =>
    #   #   <IO::Buffer 0x00007fca40087c38+4 INTERNAL>
    #   #   0x00000000  01 02 02 02         ....
    def clear(*args) end

    # Efficiently copy from a source IO::Buffer into the buffer, at +offset+
    # using +memmove+. For copying String instances, see #set_string.
    #
    #   buffer = IO::Buffer.new(32)
    #   # =>
    #   # #<IO::Buffer 0x0000555f5ca22520+32 INTERNAL>
    #   # 0x00000000  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ................
    #   # 0x00000010  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ................  *
    #
    #   buffer.copy(IO::Buffer.for("test"), 8)
    #   # => 4 -- size of buffer copied
    #   buffer
    #   # =>
    #   # #<IO::Buffer 0x0000555f5cf8fe40+32 INTERNAL>
    #   # 0x00000000  00 00 00 00 00 00 00 00 74 65 73 74 00 00 00 00 ........test....
    #   # 0x00000010  00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 ................ *
    #
    # #copy can be used to put buffer into strings associated with buffer:
    #
    #   string = "data:    "
    #   # => "data:    "
    #   buffer = IO::Buffer.for(string) do |buffer|
    #     buffer.copy(IO::Buffer.for("test"), 5)
    #   end
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
    #   buffer.copy(IO::Buffer.for("boom"), 0)
    #   # => 4
    #   File.read('test.txt')
    #   # => "boom"
    #
    # Attempt to copy the buffer which will need place outside of buffer's
    # bounds will fail:
    #
    #   buffer = IO::Buffer.new(2)
    #   buffer.copy(IO::Buffer.for('test'), 0)
    #   # in `copy': Specified offset+length is bigger than the buffer size! (ArgumentError)
    #
    # It is safe to copy between memory regions that overlaps each other.
    # In such case, the data is copied as if the data was first copied from the source buffer to
    # a temporary buffer, and then copied from the temporary buffer to the destination buffer.
    #
    #   buffer = IO::Buffer.new(10)
    #   buffer.set_string("0123456789")
    #   buffer.copy(buffer, 3, 7)
    #   # => 7
    #   buffer
    #   # =>
    #   # #<IO::Buffer 0x000056494f8ce440+10 INTERNAL>
    #   # 0x00000000  30 31 32 30 31 32 33 34 35 36                   0120123456
    def copy(*args) end

    # Iterates over the buffer, yielding each +value+ of +buffer_type+ starting
    # from +offset+.
    #
    # If +count+ is given, only +count+ values will be yielded.
    #
    #   IO::Buffer.for("Hello World").each(:U8, 2, 2) do |offset, value|
    #     puts "#{offset}: #{value}"
    #   end
    #   # 2: 108
    #   # 3: 108
    def each(*args) end

    # Iterates over the buffer, yielding each byte starting from +offset+.
    #
    # If +count+ is given, only +count+ bytes will be yielded.
    #
    #   IO::Buffer.for("Hello World").each_byte(2, 2) do |offset, byte|
    #     puts "#{offset}: #{byte}"
    #   end
    #   # 2: 108
    #   # 3: 108
    def each_byte(*args) end

    # If the buffer has 0 size: it is created by ::new with size 0, or with ::for
    # from an empty string. (Note that empty files can't be mapped, so the buffer
    # created with ::map will never be empty.)
    def empty?; end

    # The buffer is _external_ if it references the memory which is not
    # allocated or mapped by the buffer itself.
    #
    # A buffer created using ::for has an external reference to the string's
    # memory.
    #
    # External buffer can't be resized.
    def external?; end

    # If the buffer references memory, release it back to the operating system.
    # * for a _mapped_ buffer (e.g. from file): unmap.
    # * for a buffer created from scratch: free memory.
    # * for a buffer created from string: undo the association.
    #
    # After the buffer is freed, no further operations can't be performed on it.
    #
    # You can resize a freed buffer to re-allocate it.
    #
    #   buffer = IO::Buffer.for('test')
    #   buffer.free
    #   # => #<IO::Buffer 0x0000000000000000+0 NULL>
    #
    #   buffer.get_value(:U8, 0)
    #   # in `get_value': The buffer is not allocated! (IO::Buffer::AllocationError)
    #
    #   buffer.get_string
    #   # in `get_string': The buffer is not allocated! (IO::Buffer::AllocationError)
    #
    #   buffer.null?
    #   # => true
    def free; end

    # Read a chunk or all of the buffer into a string, in the specified
    # +encoding+. If no encoding is provided +Encoding::BINARY+ is used.
    #
    #   buffer = IO::Buffer.for('test')
    #   buffer.get_string
    #   # => "test"
    #   buffer.get_string(2)
    #   # => "st"
    #   buffer.get_string(2, 1)
    #   # => "s"
    def get_string(*args) end

    # Read from buffer a value of +type+ at +offset+. +buffer_type+ should be one
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
    # A buffer type refers specifically to the type of binary buffer that is stored
    # in the buffer. For example, a +:u32+ buffer type is a 32-bit unsigned
    # integer in little-endian format.
    #
    #   string = [1.5].pack('f')
    #   # => "\x00\x00\xC0?"
    #   IO::Buffer.for(string).get_value(:f32, 0)
    #   # => 1.5
    def get_value(buffer_type, offset) end

    # Similar to #get_value, except that it can handle multiple buffer types and
    # returns an array of values.
    #
    #   string = [1.5, 2.5].pack('ff')
    #   IO::Buffer.for(string).get_values([:f32, :f32], 0)
    #   # => [1.5, 2.5]
    def get_values(buffer_types, offset) end

    # Returns a human-readable string representation of the buffer. The exact
    # format is subject to change.
    #
    #   buffer = IO::Buffer.for("Hello World")
    #   puts buffer.hexdump
    #   # 0x00000000  48 65 6c 6c 6f 20 57 6f 72 6c 64                Hello World
    #
    # As buffers are usually fairly big, you may want to limit the output by
    # specifying the offset and length:
    #
    #   puts buffer.hexdump(6, 5)
    #   # 0x00000006  57 6f 72 6c 64                                  World
    def hexdump(*args) end

    # Make an internal copy of the source buffer. Updates to the copy will not
    # affect the source buffer.
    #
    #   source = IO::Buffer.for("Hello World")
    #   # =>
    #   # #<IO::Buffer 0x00007fd598466830+11 EXTERNAL READONLY SLICE>
    #   # 0x00000000  48 65 6c 6c 6f 20 57 6f 72 6c 64                Hello World
    #   buffer = source.dup
    #   # =>
    #   # #<IO::Buffer 0x0000558cbec03320+11 INTERNAL>
    #   # 0x00000000  48 65 6c 6c 6f 20 57 6f 72 6c 64                Hello World
    def initialize_copy(p1) end

    # Inspect the buffer and report useful information about it's internal state.
    # Only a limited portion of the buffer will be displayed in a hexdump style
    # format.
    #
    #   buffer = IO::Buffer.for("Hello World")
    #   puts buffer.inspect
    #   # #<IO::Buffer 0x000000010198ccd8+11 EXTERNAL READONLY SLICE>
    #   # 0x00000000  48 65 6c 6c 6f 20 57 6f 72 6c 64                Hello World
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
    # The following operations acquire a lock: #resize, #free.
    #
    # Locking is not thread safe. It is designed as a safety net around
    # non-blocking system calls. You can only share a buffer between threads with
    # appropriate synchronisation techniques.
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
    #       buffer.set_string("test", 0)
    #     end
    #   end
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

    # Modify the source buffer in place by applying the binary NOT
    # operation to the source.
    #
    #   source = IO::Buffer.for("1234567890").dup # Make a read/write copy.
    #   # =>
    #   # #<IO::Buffer 0x000056307a33a450+10 INTERNAL>
    #   # 0x00000000  31 32 33 34 35 36 37 38 39 30                   1234567890
    #
    #   source.not!
    #   # =>
    #   # #<IO::Buffer 0x000056307a33a450+10 INTERNAL>
    #   # 0x00000000  ce cd cc cb ca c9 c8 c7 c6 cf                   ..........
    def not!; end

    # If the buffer was freed with #free, transferred with #transfer, or was
    # never allocated in the first place.
    #
    #   buffer = IO::Buffer.new(0)
    #   buffer.null? #=> true
    #
    #   buffer = IO::Buffer.new(4)
    #   buffer.null? #=> false
    #   buffer.free
    #   buffer.null? #=> true
    def null?; end

    # Modify the source buffer in place by applying the binary OR
    # operation to the source, using the mask, repeating as necessary.
    #
    #   source = IO::Buffer.for("1234567890").dup # Make a read/write copy.
    #   # =>
    #   # #<IO::Buffer 0x000056307a272350+10 INTERNAL>
    #   # 0x00000000  31 32 33 34 35 36 37 38 39 30                   1234567890
    #
    #   source.or!(IO::Buffer.for("\xFF\x00\x00\xFF"))
    #   # =>
    #   # #<IO::Buffer 0x000056307a272350+10 INTERNAL>
    #   # 0x00000000  ff 32 33 ff ff 36 37 ff ff 30                   .23..67..0
    def or!(mask) end

    # Read at least +length+ bytes from the +io+ starting at the specified +from+
    # position, into the buffer starting at +offset+. If an error occurs,
    # return <tt>-errno</tt>.
    #
    # If +length+ is not given or +nil+, it defaults to the size of the buffer
    # minus the offset, i.e. the entire buffer.
    #
    # If +length+ is zero, exactly one <tt>pread</tt> operation will occur.
    #
    # If +offset+ is not given, it defaults to zero, i.e. the beginning of the
    # buffer.
    #
    #   IO::Buffer.for('test') do |buffer|
    #     p buffer
    #     # =>
    #     # <IO::Buffer 0x00007fca40087c38+4 SLICE>
    #     # 0x00000000  74 65 73 74         test
    #
    #     # take 2 bytes from the beginning of urandom,
    #     # put them in buffer starting from position 2
    #     buffer.pread(File.open('/dev/urandom', 'rb'), 0, 2, 2)
    #     p buffer
    #     # =>
    #     # <IO::Buffer 0x00007f3bc65f2a58+4 EXTERNAL SLICE>
    #     # 0x00000000  05 35 73 74         te.5
    #   end
    def pread(*args) end

    # If the buffer is _private_, meaning modifications to the buffer will not
    # be replicated to the underlying file mapping.
    #
    #   # Create a test file:
    #   File.write('test.txt', 'test')
    #
    #   # Create a private mapping from the given file. Note that the file here
    #   # is opened in read-only mode, but it doesn't matter due to the private
    #   # mapping:
    #   buffer = IO::Buffer.map(File.open('test.txt'), nil, 0, IO::Buffer::PRIVATE)
    #   # => #<IO::Buffer 0x00007fce63f11000+4 MAPPED PRIVATE>
    #
    #   # Write to the buffer (invoking CoW of the underlying file buffer):
    #   buffer.set_string('b', 0)
    #   # => 1
    #
    #   # The file itself is not modified:
    #   File.read('test.txt')
    #   # => "test"
    def private?; end

    # Write at least +length+ bytes from the buffer starting at +offset+, into
    # the +io+ starting at the specified +from+ position. If an error occurs,
    # return <tt>-errno</tt>.
    #
    # If +length+ is not given or +nil+, it defaults to the size of the buffer
    # minus the offset, i.e. the entire buffer.
    #
    # If +length+ is zero, exactly one <tt>pwrite</tt> operation will occur.
    #
    # If +offset+ is not given, it defaults to zero, i.e. the beginning of the
    # buffer.
    #
    # If the +from+ position is beyond the end of the file, the gap will be
    # filled with null (0 value) bytes.
    #
    #   out = File.open('output.txt', File::RDWR) # open for read/write, no truncation
    #   IO::Buffer.for('1234567').pwrite(out, 2, 3, 1)
    #
    # This leads to +234+ (3 bytes, starting from position 1) being written into
    # <tt>output.txt</tt>, starting from file position 2.
    def pwrite(*args) end

    # Read at least +length+ bytes from the +io+, into the buffer starting at
    # +offset+. If an error occurs, return <tt>-errno</tt>.
    #
    # If +length+ is not given or +nil+, it defaults to the size of the buffer
    # minus the offset, i.e. the entire buffer.
    #
    # If +length+ is zero, exactly one <tt>read</tt> operation will occur.
    #
    # If +offset+ is not given, it defaults to zero, i.e. the beginning of the
    # buffer.
    #
    #   IO::Buffer.for('test') do |buffer|
    #     p buffer
    #     # =>
    #     # <IO::Buffer 0x00007fca40087c38+4 SLICE>
    #     # 0x00000000  74 65 73 74         test
    #     buffer.read(File.open('/dev/urandom', 'rb'), 2)
    #     p buffer
    #     # =>
    #     # <IO::Buffer 0x00007f3bc65f2a58+4 EXTERNAL SLICE>
    #     # 0x00000000  05 35 73 74         .5st
    #   end
    def read(*args) end

    # If the buffer is <i>read only</i>, meaning the buffer cannot be modified using
    # #set_value, #set_string or #copy and similar.
    #
    # Frozen strings and read-only files create read-only buffers.
    def readonly?; end

    # Resizes a buffer to a +new_size+ bytes, preserving its content.
    # Depending on the old and new size, the memory area associated with
    # the buffer might be either extended, or rellocated at different
    # address with content being copied.
    #
    #   buffer = IO::Buffer.new(4)
    #   buffer.set_string("test", 0)
    #   buffer.resize(8) # resize to 8 bytes
    #   # =>
    #   # #<IO::Buffer 0x0000555f5d1a1630+8 INTERNAL>
    #   # 0x00000000  74 65 73 74 00 00 00 00                         test....
    #
    # External buffer (created with ::for), and locked buffer
    # can not be resized.
    def resize(new_size) end

    # Efficiently copy from a source String into the buffer, at +offset+ using
    # +memmove+.
    #
    #   buf = IO::Buffer.new(8)
    #   # =>
    #   # #<IO::Buffer 0x0000557412714a20+8 INTERNAL>
    #   # 0x00000000  00 00 00 00 00 00 00 00                         ........
    #
    #   # set buffer starting from offset 1, take 2 bytes starting from string's
    #   # second
    #   buf.set_string('test', 1, 2, 1)
    #   # => 2
    #   buf
    #   # =>
    #   # #<IO::Buffer 0x0000557412714a20+8 INTERNAL>
    #   # 0x00000000  00 65 73 00 00 00 00 00                         .es.....
    #
    # See also #copy for examples of how buffer writing might be used for changing
    # associated strings and files.
    def set_string(*args) end

    # Write to a buffer a +value+ of +type+ at +offset+. +type+ should be one of
    # symbols described in #get_value.
    #
    #   buffer = IO::Buffer.new(8)
    #   # =>
    #   # #<IO::Buffer 0x0000555f5c9a2d50+8 INTERNAL>
    #   # 0x00000000  00 00 00 00 00 00 00 00
    #
    #   buffer.set_value(:U8, 1, 111)
    #   # => 1
    #
    #   buffer
    #   # =>
    #   # #<IO::Buffer 0x0000555f5c9a2d50+8 INTERNAL>
    #   # 0x00000000  00 6f 00 00 00 00 00 00                         .o......
    #
    # Note that if the +type+ is integer and +value+ is Float, the implicit truncation is performed:
    #
    #   buffer = IO::Buffer.new(8)
    #   buffer.set_value(:U32, 0, 2.5)
    #
    #   buffer
    #   # =>
    #   # #<IO::Buffer 0x0000555f5c9a2d50+8 INTERNAL>
    #   # 0x00000000  00 00 00 02 00 00 00 00
    #   #                      ^^ the same as if we'd pass just integer 2
    def set_value(type, offset, value) end

    # Write +values+ of +buffer_types+ at +offset+ to the buffer. +buffer_types+
    # should be an array of symbols as described in #get_value. +values+ should
    # be an array of values to write.
    #
    #   buffer = IO::Buffer.new(8)
    #   buffer.set_values([:U8, :U16], 0, [1, 2])
    #   buffer
    #   # =>
    #   # #<IO::Buffer 0x696f717561746978+8 INTERNAL>
    #   # 0x00000000  01 00 02 00 00 00 00 00                         ........
    def set_values(buffer_types, offset, values) end

    # If the buffer is _shared_, meaning it references memory that can be shared
    # with other processes (and thus might change without being modified
    # locally).
    #
    #   # Create a test file:
    #   File.write('test.txt', 'test')
    #
    #   # Create a shared mapping from the given file, the file must be opened in
    #   # read-write mode unless we also specify IO::Buffer::READONLY:
    #   buffer = IO::Buffer.map(File.open('test.txt', 'r+'), nil, 0)
    #   # => #<IO::Buffer 0x00007f1bffd5e000+4 EXTERNAL MAPPED SHARED>
    #
    #   # Write to the buffer, which will modify the mapped file:
    #   buffer.set_string('b', 0)
    #   # => 1
    #
    #   # The file itself is modified:
    #   File.read('test.txt')
    #   # => "best"
    def shared?; end

    # Returns the size of the buffer that was explicitly set (on creation with ::new
    # or on #resize), or deduced on buffer's creation from string or file.
    def size; end

    # Produce another IO::Buffer which is a slice (or view into) the current one
    # starting at +offset+ bytes and going for +length+ bytes.
    #
    # The slicing happens without copying of memory, and the slice keeps being
    # associated with the original buffer's source (string, or file), if any.
    #
    # If the offset is not given, it will be zero. If the offset is negative, it
    # will raise an ArgumentError.
    #
    # If the length is not given, the slice will be as long as the original
    # buffer minus the specified offset. If the length is negative, it will raise
    # an ArgumentError.
    #
    # Raises RuntimeError if the <tt>offset+length</tt> is out of the current
    # buffer's bounds.
    #
    #   string = 'test'
    #   buffer = IO::Buffer.for(string).dup
    #
    #   slice = buffer.slice
    #   # =>
    #   # #<IO::Buffer 0x0000000108338e68+4 SLICE>
    #   # 0x00000000  74 65 73 74                                     test
    #
    #   buffer.slice(2)
    #   # =>
    #   # #<IO::Buffer 0x0000000108338e6a+2 SLICE>
    #   # 0x00000000  73 74                                           st
    #
    #   slice = buffer.slice(1, 2)
    #   # =>
    #   # #<IO::Buffer 0x00007fc3d34ebc49+2 SLICE>
    #   # 0x00000000  65 73                                           es
    #
    #   # Put "o" into 0s position of the slice
    #   slice.set_string('o', 0)
    #   slice
    #   # =>
    #   # #<IO::Buffer 0x00007fc3d34ebc49+2 SLICE>
    #   # 0x00000000  6f 73                                           os
    #
    #   # it is also visible at position 1 of the original buffer
    #   buffer
    #   # =>
    #   # #<IO::Buffer 0x00007fc3d31e2d80+4 INTERNAL>
    #   # 0x00000000  74 6f 73 74                                     tost
    def slice(*args) end

    # Short representation of the buffer. It includes the address, size and
    # symbolic flags. This format is subject to change.
    #
    #   puts IO::Buffer.new(4) # uses to_s internally
    #   # #<IO::Buffer 0x000055769f41b1a0+4 INTERNAL>
    def to_s; end

    # Transfers ownership of the underlying memory to a new buffer, causing the
    # current buffer to become uninitialized.
    #
    #   buffer = IO::Buffer.new('test')
    #   other = buffer.transfer
    #   other
    #   # =>
    #   # #<IO::Buffer 0x00007f136a15f7b0+4 SLICE>
    #   # 0x00000000  74 65 73 74                                     test
    #   buffer
    #   # =>
    #   # #<IO::Buffer 0x0000000000000000+0 NULL>
    #   buffer.null?
    #   # => true
    def transfer; end

    # Returns whether the buffer buffer is accessible.
    #
    # A buffer becomes invalid if it is a slice of another buffer (or string)
    # which has been freed or re-allocated at a different address.
    def valid?; end

    # Returns an array of values of +buffer_type+ starting from +offset+.
    #
    # If +count+ is given, only +count+ values will be returned.
    #
    #   IO::Buffer.for("Hello World").values(:U8, 2, 2)
    #   # => [108, 108]
    def values(*args) end

    # Write at least +length+ bytes from the buffer starting at +offset+, into the +io+.
    # If an error occurs, return <tt>-errno</tt>.
    #
    # If +length+ is not given or +nil+, it defaults to the size of the buffer
    # minus the offset, i.e. the entire buffer.
    #
    # If +length+ is zero, exactly one <tt>write</tt> operation will occur.
    #
    # If +offset+ is not given, it defaults to zero, i.e. the beginning of the
    # buffer.
    #
    #   out = File.open('output.txt', 'wb')
    #   IO::Buffer.for('1234567').write(out, 3)
    #
    # This leads to +123+ being written into <tt>output.txt</tt>
    def write(*args) end

    # Modify the source buffer in place by applying the binary XOR
    # operation to the source, using the mask, repeating as necessary.
    #
    #   source = IO::Buffer.for("1234567890").dup # Make a read/write copy.
    #   # =>
    #   # #<IO::Buffer 0x000056307a25b3e0+10 INTERNAL>
    #   # 0x00000000  31 32 33 34 35 36 37 38 39 30                   1234567890
    #
    #   source.xor!(IO::Buffer.for("\xFF\x00\x00\xFF"))
    #   # =>
    #   # #<IO::Buffer 0x000056307a25b3e0+10 INTERNAL>
    #   # 0x00000000  ce 32 33 cb ca 36 37 c7 c6 30                   .23..67..0
    def xor!(mask) end

    # Raised when you try to write to a read-only buffer, or resize an external buffer.
    class AccessError < RuntimeError
    end

    # Raised when the buffer cannot be allocated for some reason, or you try to use a buffer that's not allocated.
    class AllocationError < RuntimeError
    end

    # Raised if you try to access a buffer slice which no longer references a valid memory range of the underlying source.
    class InvalidatedError < RuntimeError
    end

    # Raised when an operation would resize or re-allocate a locked buffer.
    class LockedError < RuntimeError
    end

    # Raised if the mask given to a binary operation is invalid, e.g. zero length or overlaps the target buffer.
    class MaskError < ArgumentError
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

  # Can be raised by IO operations when IO#timeout= is set.
  class TimeoutError < IOError
  end

  # exception to wait for reading. see IO.select.
  module WaitReadable
  end

  # exception to wait for writing. see IO.select.
  module WaitWritable
  end
end
