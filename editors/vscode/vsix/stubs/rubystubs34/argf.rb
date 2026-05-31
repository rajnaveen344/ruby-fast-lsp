# frozen_string_literal: true

# == \ARGF and +ARGV+
#
# The \ARGF object works with the array at global variable +ARGV+
# to make <tt>$stdin</tt> and file streams available in the Ruby program:
#
# - **ARGV** may be thought of as the <b>argument vector</b> array.
#
#   Initially, it contains the command-line arguments and options
#   that are passed to the Ruby program;
#   the program can modify that array as it likes.
#
# - **ARGF** may be thought of as the <b>argument files</b> object.
#
#   It can access file streams and/or the <tt>$stdin</tt> stream,
#   based on what it finds in +ARGV+.
#   This provides a convenient way for the command line
#   to specify streams for a Ruby program to read.
#
# == Reading
#
# \ARGF may read from _source_ streams,
# which at any particular time are determined by the content of +ARGV+.
#
# === Simplest Case
#
# When the <i>very first</i> \ARGF read occurs with an empty +ARGV+ (<tt>[]</tt>),
# the source is <tt>$stdin</tt>:
#
# - \File +t.rb+:
#
#     p ['ARGV', ARGV]
#     p ['ARGF.read', ARGF.read]
#
# - Commands and outputs
#   (see below for the content of files +foo.txt+ and +bar.txt+):
#
#     $ echo "Open the pod bay doors, Hal." | ruby t.rb
#     ["ARGV", []]
#     ["ARGF.read", "Open the pod bay doors, Hal.\n"]
#
#     $ cat foo.txt bar.txt | ruby t.rb
#     ["ARGV", []]
#     ["ARGF.read", "Foo 0\nFoo 1\nBar 0\nBar 1\nBar 2\nBar 3\n"]
#
# === About the Examples
#
# Many examples here assume the existence of files +foo.txt+ and +bar.txt+:
#
#   $ cat foo.txt
#   Foo 0
#   Foo 1
#   $ cat bar.txt
#   Bar 0
#   Bar 1
#   Bar 2
#   Bar 3
#
# === Sources in +ARGV+
#
# For any \ARGF read _except_ the {simplest case}[rdoc-ref:ARGF@Simplest+Case]
# (that is, _except_ for the <i>very first</i> \ARGF read with an empty +ARGV+),
# the sources are found in +ARGV+.
#
# \ARGF assumes that each element in array +ARGV+ is a potential source,
# and is one of:
#
# - The string path to a file that may be opened as a stream.
# - The character <tt>'-'</tt>, meaning stream <tt>$stdin</tt>.
#
# Each element that is _not_ one of these
# should be removed from +ARGV+ before \ARGF accesses that source.
#
# In the following example:
#
# - Filepaths +foo.txt+ and +bar.txt+ may be retained as potential sources.
# - Options <tt>--xyzzy</tt> and <tt>--mojo</tt> should be removed.
#
# Example:
#
# - \File +t.rb+:
#
#     # Print arguments (and options, if any) found on command line.
#     p ['ARGV', ARGV]
#
# - Command and output:
#
#     $ ruby t.rb --xyzzy --mojo foo.txt bar.txt
#     ["ARGV", ["--xyzzy", "--mojo", "foo.txt", "bar.txt"]]
#
# \ARGF's stream access considers the elements of +ARGV+, left to right:
#
# - \File +t.rb+:
#
#     p "ARGV: #{ARGV}"
#     p "Line: #{ARGF.read}" # Read everything from all specified streams.
#
# - Command and output:
#
#     $ ruby t.rb foo.txt bar.txt
#     "ARGV: [\"foo.txt\", \"bar.txt\"]"
#     "Read: Foo 0\nFoo 1\nBar 0\nBar 1\nBar 2\nBar 3\n"
#
# Because the value at +ARGV+ is an ordinary array,
# you can manipulate it to control which sources \ARGF considers:
#
# - If you remove an element from +ARGV+, \ARGF will not consider the corresponding source.
# - If you add an element to +ARGV+, \ARGF will consider the corresponding source.
#
# Each element in +ARGV+ is removed when its corresponding source is accessed;
# when all sources have been accessed, the array is empty:
#
# - \File +t.rb+:
#
#     until ARGV.empty? && ARGF.eof?
#       p "ARGV: #{ARGV}"
#       p "Line: #{ARGF.readline}" # Read each line from each specified stream.
#     end
#
# - Command and output:
#
#     $ ruby t.rb foo.txt bar.txt
#     "ARGV: [\"foo.txt\", \"bar.txt\"]"
#     "Line: Foo 0\n"
#     "ARGV: [\"bar.txt\"]"
#     "Line: Foo 1\n"
#     "ARGV: [\"bar.txt\"]"
#     "Line: Bar 0\n"
#     "ARGV: []"
#     "Line: Bar 1\n"
#     "ARGV: []"
#     "Line: Bar 2\n"
#     "ARGV: []"
#     "Line: Bar 3\n"
#
# ==== Filepaths in +ARGV+
#
# The +ARGV+ array may contain filepaths the specify sources for \ARGF reading.
#
# This program prints what it reads from files at the paths specified
# on the command line:
#
# - \File +t.rb+:
#
#     p ['ARGV', ARGV]
#     # Read and print all content from the specified sources.
#     p ['ARGF.read', ARGF.read]
#
# - Command and output:
#
#     $ ruby t.rb foo.txt bar.txt
#     ["ARGV", [foo.txt, bar.txt]
#     ["ARGF.read", "Foo 0\nFoo 1\nBar 0\nBar 1\nBar 2\nBar 3\n"]
#
# ==== Specifying <tt>$stdin</tt> in +ARGV+
#
# To specify stream <tt>$stdin</tt> in +ARGV+, us the character <tt>'-'</tt>:
#
# - \File +t.rb+:
#
#     p ['ARGV', ARGV]
#     p ['ARGF.read', ARGF.read]
#
# - Command and output:
#
#     $ echo "Open the pod bay doors, Hal." | ruby t.rb -
#     ["ARGV", ["-"]]
#     ["ARGF.read", "Open the pod bay doors, Hal.\n"]
#
# When no character <tt>'-'</tt> is given, stream <tt>$stdin</tt> is ignored
# (exception:
# see {Specifying $stdin in ARGV}[rdoc-ref:ARGF@Specifying+-24stdin+in+ARGV]):
#
# - Command and output:
#
#     $ echo "Open the pod bay doors, Hal." | ruby t.rb foo.txt bar.txt
#     "ARGV: [\"foo.txt\", \"bar.txt\"]"
#     "Read: Foo 0\nFoo 1\nBar 0\nBar 1\nBar 2\nBar 3\n"
#
# ==== Mixtures and Repetitions in +ARGV+
#
# For an \ARGF reader, +ARGV+ may contain any mixture of filepaths
# and character <tt>'-'</tt>, including repetitions.
#
# ==== Modifications to +ARGV+
#
# The running Ruby program may make any modifications to the +ARGV+ array;
# the current value of +ARGV+ affects \ARGF reading.
#
# ==== Empty +ARGV+
#
# For an empty +ARGV+, an \ARGF read method either returns +nil+
# or raises an exception, depending on the specific method.
#
# === More Read Methods
#
# As seen above, method ARGF#read reads the content of all sources
# into a single string.
# Other \ARGF methods provide other ways to access that content;
# these include:
#
# - Byte access: #each_byte, #getbyte, #readbyte.
# - Character access: #each_char, #getc, #readchar.
# - Codepoint access: #each_codepoint.
# - Line access: #each_line, #gets, #readline, #readlines.
# - Source access: #read, #read_nonblock, #readpartial.
#
# === About \Enumerable
#
# \ARGF includes module Enumerable.
# Virtually all methods in \Enumerable call method <tt>#each</tt> in the including class.
#
# <b>Note well</b>: In \ARGF, method #each returns data from the _sources_,
# _not_ from +ARGV+;
# therefore, for example, <tt>ARGF#entries</tt> returns an array of lines from the sources,
# not an array of the strings from +ARGV+:
#
# - \File +t.rb+:
#
#     p ['ARGV', ARGV]
#     p ['ARGF.entries', ARGF.entries]
#
# - Command and output:
#
#     $ ruby t.rb foo.txt bar.txt
#     ["ARGV", ["foo.txt", "bar.txt"]]
#     ["ARGF.entries", ["Foo 0\n", "Foo 1\n", "Bar 0\n", "Bar 1\n", "Bar 2\n", "Bar 3\n"]]
#
# == Writing
#
# If <i>inplace mode</i> is in effect,
# \ARGF may write to target streams,
# which at any particular time are determined by the content of ARGV.
#
# Methods about inplace mode:
#
# - #inplace_mode
# - #inplace_mode=
# - #to_write_io
#
# Methods for writing:
#
# - #print
# - #printf
# - #putc
# - #puts
# - #write
class ARGF
  include Enumerable

  # Returns the +ARGV+ array, which contains the arguments passed to your
  # script, one per element.
  #
  # For example:
  #
  #     $ ruby argf.rb -v glark.txt
  #
  #     ARGF.argv   #=> ["-v", "glark.txt"]
  def argv; end

  # Puts ARGF into binary mode. Once a stream is in binary mode, it cannot
  # be reset to non-binary mode. This option has the following effects:
  #
  # *  Newline conversion is disabled.
  # *  Encoding conversion is disabled.
  # *  Content is treated as ASCII-8BIT.
  def binmode; end

  # Returns true if ARGF is being read in binary mode; false otherwise.
  # To enable binary mode use ARGF.binmode.
  #
  # For example:
  #
  #    ARGF.binmode?  #=> false
  #    ARGF.binmode
  #    ARGF.binmode?  #=> true
  def binmode?; end

  # Closes the current file and skips to the next file in ARGV. If there are
  # no more files to open, just closes the current file. STDIN will not be
  # closed.
  #
  # For example:
  #
  #    $ ruby argf.rb foo bar
  #
  #    ARGF.filename  #=> "foo"
  #    ARGF.close
  #    ARGF.filename  #=> "bar"
  #    ARGF.close
  def close; end

  # Returns _true_ if the current file has been closed; _false_ otherwise. Use
  # ARGF.close to actually close the current file.
  def closed?; end

  # Returns an enumerator which iterates over each line (separated by _sep_,
  # which defaults to your platform's newline character) of each file in
  # +ARGV+. If a block is supplied, each line in turn will be yielded to the
  # block, otherwise an enumerator is returned.
  # The optional _limit_ argument is an Integer specifying the maximum
  # length of each line; longer lines will be split according to this limit.
  #
  # This method allows you to treat the files supplied on the command line as
  # a single file consisting of the concatenation of each named file. After
  # the last line of the first file has been returned, the first line of the
  # second file is returned. The ARGF.filename and ARGF.lineno methods can be
  # used to determine the filename of the current line and line number of the
  # whole input, respectively.
  #
  # For example, the following code prints out each line of each named file
  # prefixed with its line number, displaying the filename once per file:
  #
  #    ARGF.each_line do |line|
  #      puts ARGF.filename if ARGF.file.lineno == 1
  #      puts "#{ARGF.file.lineno}: #{line}"
  #    end
  #
  # While the following code prints only the first file's name at first, and
  # the contents with line number counted through all named files.
  #
  #    ARGF.each_line do |line|
  #      puts ARGF.filename if ARGF.lineno == 1
  #      puts "#{ARGF.lineno}: #{line}"
  #    end
  def each(...) end
  alias each_line each

  # Iterates over each byte of each file in +ARGV+.
  # A byte is returned as an Integer in the range 0..255.
  #
  # This method allows you to treat the files supplied on the command line as
  # a single file consisting of the concatenation of each named file. After
  # the last byte of the first file has been returned, the first byte of the
  # second file is returned. The ARGF.filename method can be used to
  # determine the filename of the current byte.
  #
  # If no block is given, an enumerator is returned instead.
  #
  # For example:
  #
  #    ARGF.bytes.to_a  #=> [35, 32, ... 95, 10]
  def each_byte; end

  # Iterates over each character of each file in ARGF.
  #
  # This method allows you to treat the files supplied on the command line as
  # a single file consisting of the concatenation of each named file. After
  # the last character of the first file has been returned, the first
  # character of the second file is returned. The ARGF.filename method can
  # be used to determine the name of the file in which the current character
  # appears.
  #
  # If no block is given, an enumerator is returned instead.
  def each_char; end

  # Iterates over each codepoint of each file in ARGF.
  #
  # This method allows you to treat the files supplied on the command line as
  # a single file consisting of the concatenation of each named file. After
  # the last codepoint of the first file has been returned, the first
  # codepoint of the second file is returned. The ARGF.filename method can
  # be used to determine the name of the file in which the current codepoint
  # appears.
  #
  # If no block is given, an enumerator is returned instead.
  def each_codepoint; end

  # Returns true if the current file in ARGF is at end of file, i.e. it has
  # no data to read. The stream must be opened for reading or an IOError
  # will be raised.
  #
  #    $ echo "eof" | ruby argf.rb
  #
  #    ARGF.eof?                 #=> false
  #    3.times { ARGF.readchar }
  #    ARGF.eof?                 #=> false
  #    ARGF.readchar             #=> "\n"
  #    ARGF.eof?                 #=> true
  def eof; end
  alias eof? eof

  # Returns the external encoding for files read from ARGF as an Encoding
  # object. The external encoding is the encoding of the text as stored in a
  # file. Contrast with ARGF.internal_encoding, which is the encoding used to
  # represent this text within Ruby.
  #
  # To set the external encoding use ARGF.set_encoding.
  #
  # For example:
  #
  #    ARGF.external_encoding  #=>  #<Encoding:UTF-8>
  def external_encoding; end

  # Returns the current file as an IO or File object.
  # <code>$stdin</code> is returned when the current file is STDIN.
  #
  # For example:
  #
  #    $ echo "foo" > foo
  #    $ echo "bar" > bar
  #
  #    $ ruby argf.rb foo bar
  #
  #    ARGF.file      #=> #<File:foo>
  #    ARGF.read(5)   #=> "foo\nb"
  #    ARGF.file      #=> #<File:bar>
  def file; end

  # Returns the current filename. "-" is returned when the current file is
  # STDIN.
  #
  # For example:
  #
  #    $ echo "foo" > foo
  #    $ echo "bar" > bar
  #    $ echo "glark" > glark
  #
  #    $ ruby argf.rb foo bar glark
  #
  #    ARGF.filename  #=> "foo"
  #    ARGF.read(5)   #=> "foo\nb"
  #    ARGF.filename  #=> "bar"
  #    ARGF.skip
  #    ARGF.filename  #=> "glark"
  def filename; end
  alias path filename

  # Returns an integer representing the numeric file descriptor for
  # the current file. Raises an ArgumentError if there isn't a current file.
  #
  #    ARGF.fileno    #=> 3
  def fileno; end
  alias to_i fileno

  # Gets the next 8-bit byte (0..255) from ARGF. Returns +nil+ if called at
  # the end of the stream.
  #
  # For example:
  #
  #    $ echo "foo" > file
  #    $ ruby argf.rb file
  #
  #    ARGF.getbyte #=> 102
  #    ARGF.getbyte #=> 111
  #    ARGF.getbyte #=> 111
  #    ARGF.getbyte #=> 10
  #    ARGF.getbyte #=> nil
  def getbyte; end

  # Reads the next character from ARGF and returns it as a String. Returns
  # +nil+ at the end of the stream.
  #
  # ARGF treats the files named on the command line as a single file created
  # by concatenating their contents. After returning the last character of the
  # first file, it returns the first character of the second file, and so on.
  #
  # For example:
  #
  #    $ echo "foo" > file
  #    $ ruby argf.rb file
  #
  #    ARGF.getc  #=> "f"
  #    ARGF.getc  #=> "o"
  #    ARGF.getc  #=> "o"
  #    ARGF.getc  #=> "\n"
  #    ARGF.getc  #=> nil
  #    ARGF.getc  #=> nil
  def getc; end

  # Returns the next line from the current file in ARGF.
  #
  # By default lines are assumed to be separated by <code>$/</code>;
  # to use a different character as a separator, supply it as a String
  # for the _sep_ argument.
  #
  # The optional _limit_ argument specifies how many characters of each line
  # to return. By default all characters are returned.
  #
  # See IO.readlines for details about getline_args.
  def gets(...) end

  # Returns the file extension appended to the names of backup copies of
  # modified files under in-place edit mode. This value can be set using
  # ARGF.inplace_mode= or passing the +-i+ switch to the Ruby binary.
  def inplace_mode; end

  # Sets the filename extension for in-place editing mode to the given String.
  # The backup copy of each file being edited has this value appended to its
  # filename.
  #
  # For example:
  #
  #     $ ruby argf.rb file.txt
  #
  #     ARGF.inplace_mode = '.bak'
  #     ARGF.each_line do |line|
  #       print line.sub("foo","bar")
  #     end
  #
  # First, _file.txt.bak_ is created as a backup copy of _file.txt_.
  # Then, each line of _file.txt_ has the first occurrence of "foo" replaced with
  # "bar".
  def inplace_mode=(ext) end

  # Returns the internal encoding for strings read from ARGF as an
  # Encoding object.
  #
  # If ARGF.set_encoding has been called with two encoding names, the second
  # is returned. Otherwise, if +Encoding.default_external+ has been set, that
  # value is returned. Failing that, if a default external encoding was
  # specified on the command-line, that value is used. If the encoding is
  # unknown, +nil+ is returned.
  def internal_encoding; end

  # Returns the current line number of ARGF as a whole. This value
  # can be set manually with ARGF.lineno=.
  #
  # For example:
  #
  #     ARGF.lineno   #=> 0
  #     ARGF.readline #=> "This is line 1\n"
  #     ARGF.lineno   #=> 1
  def lineno; end

  # Sets the line number of ARGF as a whole to the given Integer.
  #
  # ARGF sets the line number automatically as you read data, so normally
  # you will not need to set it explicitly. To access the current line number
  # use ARGF.lineno.
  #
  # For example:
  #
  #     ARGF.lineno      #=> 0
  #     ARGF.readline    #=> "This is line 1\n"
  #     ARGF.lineno      #=> 1
  #     ARGF.lineno = 0  #=> 0
  #     ARGF.lineno      #=> 0
  def lineno=(integer) end

  # Seeks to the position given by _position_ (in bytes) in ARGF.
  #
  # For example:
  #
  #     ARGF.pos = 17
  #     ARGF.gets   #=> "This is line two\n"
  def pos=(position) end

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

  # Reads _length_ bytes from ARGF. The files named on the command line
  # are concatenated and treated as a single file by this method, so when
  # called without arguments the contents of this pseudo file are returned in
  # their entirety.
  #
  # _length_ must be a non-negative integer or +nil+.
  #
  # If _length_ is a positive integer, +read+ tries to read
  # _length_ bytes without any conversion (binary mode).
  # It returns +nil+ if an EOF is encountered before anything can be read.
  # Fewer than _length_ bytes are returned if an EOF is encountered during
  # the read.
  # In the case of an integer _length_, the resulting string is always
  # in ASCII-8BIT encoding.
  #
  # If _length_ is omitted or is +nil+, it reads until EOF
  # and the encoding conversion is applied, if applicable.
  # A string is returned even if EOF is encountered before any data is read.
  #
  # If _length_ is zero, it returns an empty string (<code>""</code>).
  #
  # If the optional _outbuf_ argument is present,
  # it must reference a String, which will receive the data.
  # The _outbuf_ will contain only the received data after the method call
  # even if it is not empty at the beginning.
  #
  # For example:
  #
  #    $ echo "small" > small.txt
  #    $ echo "large" > large.txt
  #    $ ./glark.rb small.txt large.txt
  #
  #    ARGF.read      #=> "small\nlarge"
  #    ARGF.read(200) #=> "small\nlarge"
  #    ARGF.read(2)   #=> "sm"
  #    ARGF.read(0)   #=> ""
  #
  # Note that this method behaves like the fread() function in C.
  # This means it retries to invoke read(2) system calls to read data
  # with the specified length.
  # If you need the behavior like a single read(2) system call,
  # consider ARGF#readpartial or ARGF#read_nonblock.
  def read(p1 = v1, p2 = v2) end

  # Reads at most _maxlen_ bytes from the ARGF stream in non-blocking mode.
  def read_nonblock(...) end

  # Reads the next 8-bit byte from ARGF and returns it as an Integer. Raises
  # an EOFError after the last byte of the last file has been read.
  #
  # For example:
  #
  #    $ echo "foo" > file
  #    $ ruby argf.rb file
  #
  #    ARGF.readbyte  #=> 102
  #    ARGF.readbyte  #=> 111
  #    ARGF.readbyte  #=> 111
  #    ARGF.readbyte  #=> 10
  #    ARGF.readbyte  #=> end of file reached (EOFError)
  def readbyte; end

  # Reads the next character from ARGF and returns it as a String. Raises
  # an EOFError after the last character of the last file has been read.
  #
  # For example:
  #
  #    $ echo "foo" > file
  #    $ ruby argf.rb file
  #
  #    ARGF.readchar  #=> "f"
  #    ARGF.readchar  #=> "o"
  #    ARGF.readchar  #=> "o"
  #    ARGF.readchar  #=> "\n"
  #    ARGF.readchar  #=> end of file reached (EOFError)
  def readchar; end

  # Returns the next line from the current file in ARGF.
  #
  # By default lines are assumed to be separated by <code>$/</code>;
  # to use a different character as a separator, supply it as a String
  # for the _sep_ argument.
  #
  # The optional _limit_ argument specifies how many characters of each line
  # to return. By default all characters are returned.
  #
  # An EOFError is raised at the end of the file.
  def readline(...) end

  # Reads each file in ARGF in its entirety, returning an Array containing
  # lines from the files. Lines are assumed to be separated by _sep_.
  #
  #    lines = ARGF.readlines
  #    lines[0]                #=> "This is line one\n"
  #
  # See +IO.readlines+ for a full description of all options.
  def readlines(...) end
  alias to_a readlines

  # Reads at most _maxlen_ bytes from the ARGF stream.
  #
  # If the optional _outbuf_ argument is present,
  # it must reference a String, which will receive the data.
  # The _outbuf_ will contain only the received data after the method call
  # even if it is not empty at the beginning.
  #
  # It raises EOFError on end of ARGF stream.
  # Since ARGF stream is a concatenation of multiple files,
  # internally EOF is occur for each file.
  # ARGF.readpartial returns empty strings for EOFs except the last one and
  # raises EOFError for the last one.
  def readpartial(...) end

  # Positions the current file to the beginning of input, resetting
  # ARGF.lineno to zero.
  #
  #    ARGF.readline   #=> "This is line one\n"
  #    ARGF.rewind     #=> 0
  #    ARGF.lineno     #=> 0
  #    ARGF.readline   #=> "This is line one\n"
  def rewind; end

  # Seeks to offset _amount_ (an Integer) in the ARGF stream according to
  # the value of _whence_. See IO#seek for further details.
  def seek(amount, whence = IO::SEEK_SET) end

  # If single argument is specified, strings read from ARGF are tagged with
  # the encoding specified.
  #
  # If two encoding names separated by a colon are given, e.g. "ascii:utf-8",
  # the read string is converted from the first encoding (external encoding)
  # to the second encoding (internal encoding), then tagged with the second
  # encoding.
  #
  # If two arguments are specified, they must be encoding objects or encoding
  # names. Again, the first specifies the external encoding; the second
  # specifies the internal encoding.
  #
  # If the external encoding and the internal encoding are specified, the
  # optional Hash argument can be used to adjust the conversion process. The
  # structure of this hash is explained in the String#encode documentation.
  #
  # For example:
  #
  #     ARGF.set_encoding('ascii')         # Tag the input as US-ASCII text
  #     ARGF.set_encoding(Encoding::UTF_8) # Tag the input as UTF-8 text
  #     ARGF.set_encoding('utf-8','ascii') # Transcode the input from US-ASCII
  #                                        # to UTF-8.
  def set_encoding(...) end

  # Sets the current file to the next file in ARGV. If there aren't any more
  # files it has no effect.
  #
  # For example:
  #
  #    $ ruby argf.rb foo bar
  #    ARGF.filename  #=> "foo"
  #    ARGF.skip
  #    ARGF.filename  #=> "bar"
  def skip; end

  # Returns the current offset (in bytes) of the current file in ARGF.
  #
  #    ARGF.pos    #=> 0
  #    ARGF.gets   #=> "This is line one\n"
  #    ARGF.pos    #=> 17
  def tell; end
  alias pos tell

  # Returns an IO object representing the current file. This will be a
  # File object unless the current file is a stream such as STDIN.
  #
  # For example:
  #
  #    ARGF.to_io    #=> #<File:glark.txt>
  #    ARGF.to_io    #=> #<IO:<STDIN>>
  def to_io; end

  # Returns "ARGF".
  def to_s; end
  alias inspect to_s

  # Returns IO instance tied to _ARGF_ for writing if inplace mode is
  # enabled.
  def to_write_io; end

  # Writes each of the given +objects+ if inplace mode.
  def write(*objects) end
end
