# frozen_string_literal: true

# A \File object is a representation of a file in the underlying platform.
#
# \Class \File extends module FileTest, supporting such singleton methods
# as <tt>File.exist?</tt>.
#
# == About the Examples
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
# == Access Modes
#
# Methods File.new and File.open each create a \File object for a given file path.
#
# === \String Access Modes
#
# Methods File.new and File.open each may take string argument +mode+, which:
#
# - Begins with a 1- or 2-character
#   {read/write mode}[rdoc-ref:File@Read-2FWrite+Mode].
# - May also contain a 1-character {data mode}[rdoc-ref:File@Data+Mode].
# - May also contain a 1-character
#   {file-create mode}[rdoc-ref:File@File-Create+Mode].
#
# ==== Read/Write Mode
#
# The read/write +mode+ determines:
#
# - Whether the file is to be initially truncated.
#
# - Whether reading is allowed, and if so:
#
#   - The initial read position in the file.
#   - Where in the file reading can occur.
#
# - Whether writing is allowed, and if so:
#
#   - The initial write position in the file.
#   - Where in the file writing can occur.
#
# These tables summarize:
#
#   Read/Write Modes for Existing File
#
#   |------|-----------|----------|----------|----------|-----------|
#   | R/W  | Initial   |          | Initial  |          | Initial   |
#   | Mode | Truncate? |  Read    | Read Pos |  Write   | Write Pos |
#   |------|-----------|----------|----------|----------|-----------|
#   | 'r'  |    No     | Anywhere |    0     |   Error  |     -     |
#   | 'w'  |    Yes    |   Error  |    -     | Anywhere |     0     |
#   | 'a'  |    No     |   Error  |    -     | End only |    End    |
#   | 'r+' |    No     | Anywhere |    0     | Anywhere |     0     |
#   | 'w+' |    Yes    | Anywhere |    0     | Anywhere |     0     |
#   | 'a+' |    No     | Anywhere |   End    | End only |    End    |
#   |------|-----------|----------|----------|----------|-----------|
#
#   Read/Write Modes for \File To Be Created
#
#   |------|----------|----------|----------|-----------|
#   | R/W  |          | Initial  |          | Initial   |
#   | Mode |  Read    | Read Pos |  Write   | Write Pos |
#   |------|----------|----------|----------|-----------|
#   | 'w'  |   Error  |    -     | Anywhere |     0     |
#   | 'a'  |   Error  |    -     | End only |     0     |
#   | 'w+' | Anywhere |    0     | Anywhere |     0     |
#   | 'a+' | Anywhere |    0     | End only |    End    |
#   |------|----------|----------|----------|-----------|
#
# Note that modes <tt>'r'</tt> and <tt>'r+'</tt> are not allowed
# for a non-existent file (exception raised).
#
# In the tables:
#
# - +Anywhere+ means that methods IO#rewind, IO#pos=, and IO#seek
#   may be used to change the file's position,
#   so that allowed reading or writing may occur anywhere in the file.
# - <tt>End only</tt> means that writing can occur only at end-of-file,
#   and that methods IO#rewind, IO#pos=, and IO#seek do not affect writing.
# - +Error+ means that an exception is raised if disallowed reading or writing
#   is attempted.
#
# ===== Read/Write Modes for Existing \File
#
# - <tt>'r'</tt>:
#
#   - \File is not initially truncated:
#
#       f = File.new('t.txt') # => #<File:t.txt>
#       f.size == 0           # => false
#
#   - File's initial read position is 0:
#
#       f.pos # => 0
#
#   - \File may be read anywhere; see IO#rewind, IO#pos=, IO#seek:
#
#       f.readline # => "First line\n"
#       f.readline # => "Second line\n"
#
#       f.rewind
#       f.readline # => "First line\n"
#
#       f.pos = 1
#       f.readline # => "irst line\n"
#
#       f.seek(1, :CUR)
#       f.readline # => "econd line\n"
#
#   - Writing is not allowed:
#
#       f.write('foo') # Raises IOError.
#
# - <tt>'w'</tt>:
#
#   - \File is initially truncated:
#
#       path = 't.tmp'
#       File.write(path, text)
#       f = File.new(path, 'w')
#       f.size == 0 # => true
#
#   - File's initial write position is 0:
#
#       f.pos # => 0
#
#   - \File may be written anywhere (even past end-of-file);
#     see IO#rewind, IO#pos=, IO#seek:
#
#       f.write('foo')
#       f.flush
#       File.read(path) # => "foo"
#       f.pos # => 3
#
#       f.write('bar')
#       f.flush
#       File.read(path) # => "foobar"
#       f.pos # => 6
#
#       f.rewind
#       f.write('baz')
#       f.flush
#       File.read(path) # => "bazbar"
#       f.pos # => 3
#
#       f.pos = 3
#       f.write('foo')
#       f.flush
#       File.read(path) # => "bazfoo"
#       f.pos # => 6
#
#       f.seek(-3, :END)
#       f.write('bam')
#       f.flush
#       File.read(path) # => "bazbam"
#       f.pos # => 6
#
#       f.pos = 8
#       f.write('bah')  # Zero padding as needed.
#       f.flush
#       File.read(path) # => "bazbam\u0000\u0000bah"
#       f.pos # => 11
#
#   - Reading is not allowed:
#
#       f.read # Raises IOError.
#
# - <tt>'a'</tt>:
#
#   - \File is not initially truncated:
#
#       path = 't.tmp'
#       File.write(path, 'foo')
#       f = File.new(path, 'a')
#       f.size == 0 # => false
#
#   - File's initial position is 0 (but is ignored):
#
#       f.pos # => 0
#
#   - \File may be written only at end-of-file;
#     IO#rewind, IO#pos=, IO#seek do not affect writing:
#
#       f.write('bar')
#       f.flush
#       File.read(path) # => "foobar"
#       f.write('baz')
#       f.flush
#       File.read(path) # => "foobarbaz"
#
#       f.rewind
#       f.write('bat')
#       f.flush
#       File.read(path) # => "foobarbazbat"
#
#   - Reading is not allowed:
#
#       f.read # Raises IOError.
#
# - <tt>'r+'</tt>:
#
#   - \File is not initially truncated:
#
#       path = 't.tmp'
#       File.write(path, text)
#       f = File.new(path, 'r+')
#       f.size == 0 # => false
#
#   - File's initial read position is 0:
#
#       f.pos # => 0
#
#   - \File may be read or written anywhere (even past end-of-file);
#     see IO#rewind, IO#pos=, IO#seek:
#
#       f.readline # => "First line\n"
#       f.readline # => "Second line\n"
#
#       f.rewind
#       f.readline # => "First line\n"
#
#       f.pos = 1
#       f.readline # => "irst line\n"
#
#       f.seek(1, :CUR)
#       f.readline # => "econd line\n"
#
#       f.rewind
#       f.write('WWW')
#       f.flush
#       File.read(path)
#       # => "WWWst line\nSecond line\nFourth line\nFifth line\n"
#
#       f.pos = 10
#       f.write('XXX')
#       f.flush
#       File.read(path)
#       # => "WWWst lineXXXecond line\nFourth line\nFifth line\n"
#
#       f.seek(-6, :END)
#       # => 0
#       f.write('YYY')
#       # => 3
#       f.flush
#       # => #<File:t.tmp>
#       File.read(path)
#       # => "WWWst lineXXXecond line\nFourth line\nFifth YYYe\n"
#
#       f.seek(2, :END)
#       f.write('ZZZ') # Zero padding as needed.
#       f.flush
#       File.read(path)
#       # => "WWWst lineXXXecond line\nFourth line\nFifth YYYe\n\u0000\u0000ZZZ"
#
# - <tt>'a+'</tt>:
#
#   - \File is not initially truncated:
#
#       path = 't.tmp'
#       File.write(path, 'foo')
#       f = File.new(path, 'a+')
#       f.size == 0 # => false
#
#   - File's initial read position is 0:
#
#       f.pos # => 0
#
#   - \File may be written only at end-of-file;
#     IO#rewind, IO#pos=, IO#seek do not affect writing:
#
#       f.write('bar')
#       f.flush
#       File.read(path)      # => "foobar"
#       f.write('baz')
#       f.flush
#       File.read(path)      # => "foobarbaz"
#
#       f.rewind
#       f.write('bat')
#       f.flush
#       File.read(path) # => "foobarbazbat"
#
#   - \File may be read anywhere; see IO#rewind, IO#pos=, IO#seek:
#
#       f.rewind
#       f.read # => "foobarbazbat"
#
#       f.pos = 3
#       f.read # => "barbazbat"
#
#       f.seek(-3, :END)
#       f.read # => "bat"
#
# ===== Read/Write Modes for \File To Be Created
#
# Note that modes <tt>'r'</tt> and <tt>'r+'</tt> are not allowed
# for a non-existent file (exception raised).
#
# - <tt>'w'</tt>:
#
#   - File's initial write position is 0:
#
#       path = 't.tmp'
#       FileUtils.rm_f(path)
#       f = File.new(path, 'w')
#       f.pos # => 0
#
#   - \File may be written anywhere (even past end-of-file);
#     see IO#rewind, IO#pos=, IO#seek:
#
#       f.write('foo')
#       f.flush
#       File.read(path) # => "foo"
#       f.pos # => 3
#
#       f.write('bar')
#       f.flush
#       File.read(path) # => "foobar"
#       f.pos # => 6
#
#       f.rewind
#       f.write('baz')
#       f.flush
#       File.read(path) # => "bazbar"
#       f.pos # => 3
#
#       f.pos = 3
#       f.write('foo')
#       f.flush
#       File.read(path) # => "bazfoo"
#       f.pos # => 6
#
#       f.seek(-3, :END)
#       f.write('bam')
#       f.flush
#       File.read(path) # => "bazbam"
#       f.pos # => 6
#
#       f.pos = 8
#       f.write('bah')  # Zero padding as needed.
#       f.flush
#       File.read(path) # => "bazbam\u0000\u0000bah"
#       f.pos # => 11
#
#   - Reading is not allowed:
#
#       f.read # Raises IOError.
#
# - <tt>'a'</tt>:
#
#   - File's initial write position is 0:
#
#       path = 't.tmp'
#       FileUtils.rm_f(path)
#       f = File.new(path, 'a')
#       f.pos # => 0
#
#   - Writing occurs only at end-of-file:
#
#       f.write('foo')
#       f.pos # => 3
#       f.write('bar')
#       f.pos # => 6
#       f.flush
#       File.read(path) # => "foobar"
#
#       f.rewind
#       f.write('baz')
#       f.flush
#       File.read(path) # => "foobarbaz"
#
#   - Reading is not allowed:
#
#       f.read # Raises IOError.
#
# - <tt>'w+'</tt>:
#
#   - File's initial position is 0:
#
#       path = 't.tmp'
#       FileUtils.rm_f(path)
#       f = File.new(path, 'w+')
#       f.pos # => 0
#
#   - \File may be written anywhere (even past end-of-file);
#     see IO#rewind, IO#pos=, IO#seek:
#
#       f.write('foo')
#       f.flush
#       File.read(path) # => "foo"
#       f.pos # => 3
#
#       f.write('bar')
#       f.flush
#       File.read(path) # => "foobar"
#       f.pos # => 6
#
#       f.rewind
#       f.write('baz')
#       f.flush
#       File.read(path) # => "bazbar"
#       f.pos # => 3
#
#       f.pos = 3
#       f.write('foo')
#       f.flush
#       File.read(path) # => "bazfoo"
#       f.pos # => 6
#
#       f.seek(-3, :END)
#       f.write('bam')
#       f.flush
#       File.read(path) # => "bazbam"
#       f.pos # => 6
#
#       f.pos = 8
#       f.write('bah')  # Zero padding as needed.
#       f.flush
#       File.read(path) # => "bazbam\u0000\u0000bah"
#       f.pos # => 11
#
#   - \File may be read anywhere (even past end-of-file);
#     see IO#rewind, IO#pos=, IO#seek:
#
#       f.rewind
#       # => 0
#       f.read
#       # => "bazbam\u0000\u0000bah"
#
#       f.pos = 3
#       # => 3
#       f.read
#       # => "bam\u0000\u0000bah"
#
#       f.seek(-3, :END)
#       # => 0
#       f.read
#       # => "bah"
#
# - <tt>'a+'</tt>:
#
#   - File's initial write position is 0:
#
#       path = 't.tmp'
#       FileUtils.rm_f(path)
#       f = File.new(path, 'a+')
#       f.pos # => 0
#
#   - Writing occurs only at end-of-file:
#
#       f.write('foo')
#       f.pos # => 3
#       f.write('bar')
#       f.pos # => 6
#       f.flush
#       File.read(path) # => "foobar"
#
#       f.rewind
#       f.write('baz')
#       f.flush
#       File.read(path) # => "foobarbaz"
#
#   - \File may be read anywhere (even past end-of-file);
#     see IO#rewind, IO#pos=, IO#seek:
#
#       f.rewind
#       f.read # => "foobarbaz"
#
#       f.pos = 3
#       f.read # => "barbaz"
#
#       f.seek(-3, :END)
#       f.read # => "baz"
#
#       f.pos = 800
#       f.read # => ""
#
# ==== \Data Mode
#
# To specify whether data is to be treated as text or as binary data,
# either of the following may be suffixed to any of the string read/write modes
# above:
#
# - <tt>'t'</tt>: Text data; sets the default external encoding
#   to <tt>Encoding::UTF_8</tt>;
#   on Windows, enables conversion between EOL and CRLF
#   and enables interpreting <tt>0x1A</tt> as an end-of-file marker.
# - <tt>'b'</tt>: Binary data; sets the default external encoding
#   to <tt>Encoding::ASCII_8BIT</tt>;
#   on Windows, suppresses conversion between EOL and CRLF
#   and disables interpreting <tt>0x1A</tt> as an end-of-file marker.
#
# If neither is given, the stream defaults to text data.
#
# Examples:
#
#   File.new('t.txt', 'rt')
#   File.new('t.dat', 'rb')
#
# When the data mode is specified, the read/write mode may not be omitted,
# and the data mode must precede the file-create mode, if given:
#
#   File.new('t.dat', 'b')   # Raises an exception.
#   File.new('t.dat', 'rxb') # Raises an exception.
#
# ==== \File-Create Mode
#
# The following may be suffixed to any writable string mode above:
#
# - <tt>'x'</tt>: Creates the file if it does not exist;
#   raises an exception if the file exists.
#
# Example:
#
#   File.new('t.tmp', 'wx')
#
# When the file-create mode is specified, the read/write mode may not be omitted,
# and the file-create mode must follow the data mode:
#
#   File.new('t.dat', 'x')   # Raises an exception.
#   File.new('t.dat', 'rxb') # Raises an exception.
#
# === \Integer Access Modes
#
# When mode is an integer it must be one or more of the following constants,
# which may be combined by the bitwise OR operator <tt>|</tt>:
#
# - +File::RDONLY+: Open for reading only.
# - +File::WRONLY+: Open for writing only.
# - +File::RDWR+: Open for reading and writing.
# - +File::APPEND+: Open for appending only.
#
# Examples:
#
#   File.new('t.txt', File::RDONLY)
#   File.new('t.tmp', File::RDWR | File::CREAT | File::EXCL)
#
# Note: Method IO#set_encoding does not allow the mode to be specified as an integer.
#
# === File-Create Mode Specified as an \Integer
#
# These constants may also be ORed into the integer mode:
#
# - +File::CREAT+: Create file if it does not exist.
# - +File::EXCL+: Raise an exception if +File::CREAT+ is given and the file exists.
#
# === \Data Mode Specified as an \Integer
#
# \Data mode cannot be specified as an integer.
# When the stream access mode is given as an integer,
# the data mode is always text, never binary.
#
# Note that although there is a constant +File::BINARY+,
# setting its value in an integer stream mode has no effect;
# this is because, as documented in File::Constants,
# the +File::BINARY+ value disables line code conversion,
# but does not change the external encoding.
#
# === Encodings
#
# Any of the string modes above may specify encodings -
# either external encoding only or both external and internal encodings -
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
#   f.close
#
# The numerous encoding names are available in array Encoding.name_list:
#
#   Encoding.name_list.take(3) # => ["ASCII-8BIT", "UTF-8", "US-ASCII"]
#
# When the external encoding is set, strings read are tagged by that encoding
# when reading, and strings written are converted to that encoding when
# writing.
#
# When both external and internal encodings are set,
# strings read are converted from external to internal encoding,
# and strings written are converted from internal to external encoding.
# For further details about transcoding input and output,
# see {Encodings}[rdoc-ref:encodings.rdoc@Encodings].
#
# If the external encoding is <tt>'BOM|UTF-8'</tt>, <tt>'BOM|UTF-16LE'</tt>
# or <tt>'BOM|UTF16-BE'</tt>,
# Ruby checks for a Unicode BOM in the input document
# to help determine the encoding.
# For UTF-16 encodings the file open mode must be binary.
# If the BOM is found,
# it is stripped and the external encoding from the BOM is used.
#
# Note that the BOM-style encoding option is case insensitive,
# so <tt>'bom|utf-8'</tt> is also valid.
#
# == \File Permissions
#
# A \File object has _permissions_, an octal integer representing
# the permissions of an actual file in the underlying platform.
#
# Note that file permissions are quite different from the _mode_
# of a file stream (\File object).
#
# In a \File object, the permissions are available thus,
# where method +mode+, despite its name, returns permissions:
#
#   f = File.new('t.txt')
#   f.lstat.mode.to_s(8) # => "100644"
#
# On a Unix-based operating system,
# the three low-order octal digits represent the permissions
# for owner (6), group (4), and world (4).
# The triplet of bits in each octal digit represent, respectively,
# read, write, and execute permissions.
#
# Permissions <tt>0644</tt> thus represent read-write access for owner
# and read-only access for group and world.
# See man pages {open(2)}[https://www.unix.com/man-page/bsd/2/open]
# and {chmod(2)}[https://www.unix.com/man-page/bsd/2/chmod].
#
# For a directory, the meaning of the execute bit changes:
# when set, the directory can be searched.
#
# Higher-order bits in permissions may indicate the type of file
# (plain, directory, pipe, socket, etc.) and various other special features.
#
# On non-Posix operating systems, permissions may include only read-only or read-write,
# in which case, the remaining permission will resemble typical values.
# On Windows, for instance, the default permissions are <code>0644</code>;
# The only change that can be made is to make the file
# read-only, which is reported as <code>0444</code>.
#
# For a method that actually creates a file in the underlying platform
# (as opposed to merely creating a \File object),
# permissions may be specified:
#
#   File.new('t.tmp', File::CREAT, 0644)
#   File.new('t.tmp', File::CREAT, 0444)
#
# Permissions may also be changed:
#
#   f = File.new('t.tmp', File::CREAT, 0444)
#   f.chmod(0644)
#   f.chmod(0444)
#
# == \File \Constants
#
# Various constants for use in \File and IO methods
# may be found in module File::Constants;
# an array of their names is returned by <tt>File::Constants.constants</tt>.
#
# == What's Here
#
# First, what's elsewhere. \Class \File:
#
# - Inherits from {class IO}[rdoc-ref:IO@What-27s+Here],
#   in particular, methods for creating, reading, and writing files
# - Includes module FileTest,
#   which provides dozens of additional methods.
#
# Here, class \File provides methods that are useful for:
#
# - {Creating}[rdoc-ref:File@Creating]
# - {Querying}[rdoc-ref:File@Querying]
# - {Settings}[rdoc-ref:File@Settings]
# - {Other}[rdoc-ref:File@Other]
#
# === Creating
#
# - ::new: Opens the file at the given path; returns the file.
# - ::open: Same as ::new, but when given a block will yield the file to the block,
#   and close the file upon exiting the block.
# - ::link: Creates a new name for an existing file using a hard link.
# - ::mkfifo: Returns the FIFO file created at the given path.
# - ::symlink: Creates a symbolic link for the given file path.
#
# === Querying
#
# _Paths_
#
# - ::absolute_path: Returns the absolute file path for the given path.
# - ::absolute_path?: Returns whether the given path is the absolute file path.
# - ::basename: Returns the last component of the given file path.
# - ::dirname: Returns all but the last component of the given file path.
# - ::expand_path: Returns the absolute file path for the given path,
#   expanding <tt>~</tt> for a home directory.
# - ::extname: Returns the file extension for the given file path.
# - ::fnmatch? (aliased as ::fnmatch): Returns whether the given file path
#   matches the given pattern.
# - ::join: Joins path components into a single path string.
# - ::path: Returns the string representation of the given path.
# - ::readlink: Returns the path to the file at the given symbolic link.
# - ::realdirpath: Returns the real path for the given file path,
#   where the last component need not exist.
# - ::realpath: Returns the real path for the given file path,
#   where all components must exist.
# - ::split: Returns an array of two strings: the directory name and basename
#   of the file at the given path.
# - #path (aliased as #to_path):  Returns the string representation of the given path.
#
# _Times_
#
# - ::atime: Returns a Time for the most recent access to the given file.
# - ::birthtime: Returns a Time  for the creation of the given file.
# - ::ctime: Returns a Time  for the metadata change of the given file.
# - ::mtime: Returns a Time for the most recent data modification to
#   the content of the given file.
# - #atime: Returns a Time for the most recent access to +self+.
# - #birthtime: Returns a Time  the creation for +self+.
# - #ctime: Returns a Time for the metadata change of +self+.
# - #mtime: Returns a Time for the most recent data modification
#   to the content of +self+.
#
# _Types_
#
# - ::blockdev?: Returns whether the file at the given path is a block device.
# - ::chardev?: Returns whether the file at the given path is a character device.
# - ::directory?: Returns whether the file at the given path is a directory.
# - ::executable?: Returns whether the file at the given path is executable
#   by the effective user and group of the current process.
# - ::executable_real?: Returns whether the file at the given path is executable
#   by the real user and group of the current process.
# - ::exist?: Returns whether the file at the given path exists.
# - ::file?: Returns whether the file at the given path is a regular file.
# - ::ftype: Returns a string giving the type of the file at the given path.
# - ::grpowned?: Returns whether the effective group of the current process
#   owns the file at the given path.
# - ::identical?: Returns whether the files at two given paths are identical.
# - ::lstat: Returns the File::Stat object for the last symbolic link
#   in the given path.
# - ::owned?: Returns whether the effective user of the current process
#   owns the file at the given path.
# - ::pipe?: Returns whether the file at the given path is a pipe.
# - ::readable?: Returns whether the file at the given path is readable
#   by the effective user and group of the current process.
# - ::readable_real?: Returns whether the file at the given path is readable
#   by the real user and group of the current process.
# - ::setgid?: Returns whether the setgid bit is set for the file at the given path.
# - ::setuid?: Returns whether the setuid bit is set for the file at the given path.
# - ::socket?: Returns whether the file at the given path is a socket.
# - ::stat: Returns the File::Stat object for the file at the given path.
# - ::sticky?: Returns whether the file at the given path has its sticky bit set.
# - ::symlink?: Returns whether the file at the given path is a symbolic link.
# - ::umask: Returns the umask value for the current process.
# - ::world_readable?: Returns whether the file at the given path is readable
#   by others.
# - ::world_writable?: Returns whether the file at the given path is writable
#   by others.
# - ::writable?: Returns whether the file at the given path is writable
#   by the effective user and group of the current process.
# - ::writable_real?: Returns whether the file at the given path is writable
#   by the real user and group of the current process.
# - #lstat: Returns the File::Stat object for the last symbolic link
#   in the path for +self+.
#
# _Contents_
#
# - ::empty? (aliased as ::zero?): Returns whether the file at the given path
#   exists and is empty.
# - ::size: Returns the size (bytes) of the file at the given path.
# - ::size?: Returns +nil+ if there is no file at the given path,
#   or if that file is empty; otherwise returns the file size (bytes).
# - #size: Returns the size (bytes) of +self+.
#
# === Settings
#
# - ::chmod: Changes permissions of the file at the given path.
# - ::chown: Change ownership of the file at the given path.
# - ::lchmod: Changes permissions of the last symbolic link in the given path.
# - ::lchown: Change ownership of the last symbolic in the given path.
# - ::lutime: For each given file path, sets the access time and modification time
#   of the last symbolic link in the path.
# - ::rename: Moves the file at one given path to another given path.
# - ::utime: Sets the access time and modification time of each file
#   at the given paths.
# - #flock: Locks or unlocks +self+.
#
# === Other
#
# - ::truncate: Truncates the file at the given file path to the given size.
# - ::unlink (aliased as ::delete): Deletes the file for each given file path.
# - #truncate: Truncates +self+ to the given size.
class File < IO
  # platform specific alternative separator
  ALT_SEPARATOR = _
  # path list separator
  PATH_SEPARATOR = _
  # separates directory parts in path
  SEPARATOR = _
  # separates directory parts in path
  Separator = _

  # Converts a pathname to an absolute pathname. Relative paths are
  # referenced from the current working directory of the process unless
  # <i>dir_string</i> is given, in which case it will be used as the
  # starting point. If the given pathname starts with a ``<code>~</code>''
  # it is NOT expanded, it is treated as a normal directory name.
  #
  #    File.absolute_path("~oracle/bin")       #=> "<relative_path>/~oracle/bin"
  def self.absolute_path(*args) end

  # Returns <code>true</code> if +file_name+ is an absolute path, and
  # <code>false</code> otherwise.
  #
  #    File.absolute_path?("c:/foo")     #=> false (on Linux), true (on Windows)
  def self.absolute_path?(file_name) end

  # Returns the last access time for the named file as a Time object.
  #
  # _file_name_ can be an IO object.
  #
  #    File.atime("testfile")   #=> Wed Apr 09 08:51:48 CDT 2003
  def self.atime(file_name) end

  # Returns the last component of the filename given in
  # <i>file_name</i> (after first stripping trailing separators),
  # which can be formed using both File::SEPARATOR and
  # File::ALT_SEPARATOR as the separator when File::ALT_SEPARATOR is
  # not <code>nil</code>. If <i>suffix</i> is given and present at the
  # end of <i>file_name</i>, it is removed. If <i>suffix</i> is ".*",
  # any extension will be removed.
  #
  #    File.basename("/home/gumby/work/ruby.rb")          #=> "ruby.rb"
  #    File.basename("/home/gumby/work/ruby.rb", ".rb")   #=> "ruby"
  #    File.basename("/home/gumby/work/ruby.rb", ".*")    #=> "ruby"
  def self.basename(*args) end

  # Returns the birth time for the named file.
  #
  # _file_name_ can be an IO object.
  #
  #    File.birthtime("testfile")   #=> Wed Apr 09 08:53:13 CDT 2003
  #
  # If the platform doesn't have birthtime, raises NotImplementedError.
  def self.birthtime(file_name) end

  # Returns +true+ if +filepath+ points to a block device, +false+ otherwise:
  #
  #   File.blockdev?('/dev/sda1')       # => true
  #   File.blockdev?(File.new('t.tmp')) # => false
  def self.blockdev?(filepath) end

  # Returns +true+ if +filepath+ points to a character device, +false+ otherwise.
  #
  #   File.chardev?($stdin)     # => true
  #   File.chardev?('t.txt')     # => false
  def self.chardev?(filepath) end

  # Changes permission bits on the named file(s) to the bit pattern
  # represented by <i>mode_int</i>. Actual effects are operating system
  # dependent (see the beginning of this section). On Unix systems, see
  # <code>chmod(2)</code> for details. Returns the number of files
  # processed.
  #
  #    File.chmod(0644, "testfile", "out")   #=> 2
  def self.chmod(mode_int, file_name, *args) end

  # Changes the owner and group of the named file(s) to the given
  # numeric owner and group id's. Only a process with superuser
  # privileges may change the owner of a file. The current owner of a
  # file may change the file's group to any group to which the owner
  # belongs. A <code>nil</code> or -1 owner or group id is ignored.
  # Returns the number of files processed.
  #
  #    File.chown(nil, 100, "testfile")
  def self.chown(owner_int, group_int, file_name, *args) end

  # Returns the change time for the named file (the time at which
  # directory information about the file was changed, not the file
  # itself).
  #
  # _file_name_ can be an IO object.
  #
  # Note that on Windows (NTFS), returns creation time (birth time).
  #
  #    File.ctime("testfile")   #=> Wed Apr 09 08:53:13 CDT 2003
  def self.ctime(file_name) end

  # Deletes the named files, returning the number of names
  # passed as arguments. Raises an exception on any error.
  # Since the underlying implementation relies on the
  # <code>unlink(2)</code> system call, the type of
  # exception raised depends on its error type (see
  # https://linux.die.net/man/2/unlink) and has the form of
  # e.g. Errno::ENOENT.
  #
  # See also Dir::rmdir.
  def self.delete(file_name, *args) end

  # With string +object+ given, returns +true+ if +path+ is a string path
  # leading to a directory, or to a symbolic link to a directory; +false+ otherwise:
  #
  #   File.directory?('.')              # => true
  #   File.directory?('foo')            # => false
  #   File.symlink('.', 'dirlink')      # => 0
  #   File.directory?('dirlink')        # => true
  #   File.symlink('t,txt', 'filelink') # => 0
  #   File.directory?('filelink')       # => false
  #
  # Argument +path+ can be an IO object.
  def self.directory?(path) end

  # Returns all components of the filename given in <i>file_name</i>
  # except the last one (after first stripping trailing separators).
  # The filename can be formed using both File::SEPARATOR and
  # File::ALT_SEPARATOR as the separator when File::ALT_SEPARATOR is
  # not <code>nil</code>.
  #
  #    File.dirname("/home/gumby/work/ruby.rb")   #=> "/home/gumby/work"
  #
  # If +level+ is given, removes the last +level+ components, not only
  # one.
  #
  #    File.dirname("/home/gumby/work/ruby.rb", 2) #=> "/home/gumby"
  #    File.dirname("/home/gumby/work/ruby.rb", 4) #=> "/"
  def self.dirname(file_name, level = 1) end

  # Returns <code>true</code> if the named file exists and has
  # a zero size.
  #
  # _file_name_ can be an IO object.
  def self.empty?(p1) end

  # Returns <code>true</code> if the named file is executable by the effective
  # user and group id of this process. See eaccess(3).
  #
  # Windows does not support execute permissions separately from read
  # permissions. On Windows, a file is only considered executable if it ends in
  # .bat, .cmd, .com, or .exe.
  #
  # Note that some OS-level security features may cause this to return true
  # even though the file is not executable by the effective user/group.
  def self.executable?(file_name) end

  # Returns <code>true</code> if the named file is executable by the real
  # user and group id of this process. See access(3).
  #
  # Windows does not support execute permissions separately from read
  # permissions. On Windows, a file is only considered executable if it ends in
  # .bat, .cmd, .com, or .exe.
  #
  # Note that some OS-level security features may cause this to return true
  # even though the file is not executable by the real user/group.
  def self.executable_real?(file_name) end

  # Return <code>true</code> if the named file exists.
  #
  # _file_name_ can be an IO object.
  #
  # "file exists" means that stat() or fstat() system call is successful.
  def self.exist?(file_name) end

  # Converts a pathname to an absolute pathname. Relative paths are
  # referenced from the current working directory of the process unless
  # +dir_string+ is given, in which case it will be used as the
  # starting point. The given pathname may start with a
  # ``<code>~</code>'', which expands to the process owner's home
  # directory (the environment variable +HOME+ must be set
  # correctly). ``<code>~</code><i>user</i>'' expands to the named
  # user's home directory.
  #
  #    File.expand_path("~oracle/bin")           #=> "/home/oracle/bin"
  #
  # A simple example of using +dir_string+ is as follows.
  #    File.expand_path("ruby", "/usr/bin")      #=> "/usr/bin/ruby"
  #
  # A more complex example which also resolves parent directory is as follows.
  # Suppose we are in bin/mygem and want the absolute path of lib/mygem.rb.
  #
  #    File.expand_path("../../lib/mygem.rb", __FILE__)
  #    #=> ".../path/to/project/lib/mygem.rb"
  #
  # So first it resolves the parent of __FILE__, that is bin/, then go to the
  # parent, the root of the project and appends +lib/mygem.rb+.
  def self.expand_path(*args) end

  # Returns the extension (the portion of file name in +path+
  # starting from the last period).
  #
  # If +path+ is a dotfile, or starts with a period, then the starting
  # dot is not dealt with the start of the extension.
  #
  # An empty string will also be returned when the period is the last character
  # in +path+.
  #
  # On Windows, trailing dots are truncated.
  #
  #    File.extname("test.rb")         #=> ".rb"
  #    File.extname("a/b/d/test.rb")   #=> ".rb"
  #    File.extname(".a/b/d/test.rb")  #=> ".rb"
  #    File.extname("foo.")            #=> "" on Windows
  #    File.extname("foo.")            #=> "." on non-Windows
  #    File.extname("test")            #=> ""
  #    File.extname(".profile")        #=> ""
  #    File.extname(".profile.sh")     #=> ".sh"
  def self.extname(path) end

  # Returns +true+ if the named +file+ exists and is a regular file.
  #
  # +file+ can be an IO object.
  #
  # If the +file+ argument is a symbolic link, it will resolve the symbolic link
  # and use the file referenced by the link.
  def self.file?(file) end

  # Returns true if +path+ matches against +pattern+.  The pattern is not a
  # regular expression; instead it follows rules similar to shell filename
  # globbing.  It may contain the following metacharacters:
  #
  # <code>*</code>::
  #   Matches any file. Can be restricted by other values in the glob.
  #   Equivalent to <code>/.*/x</code> in regexp.
  #
  #   <code>*</code>::    Matches all regular files
  #   <code>c*</code>::   Matches all files beginning with <code>c</code>
  #   <code>*c</code>::   Matches all files ending with <code>c</code>
  #   <code>\*c*</code>:: Matches all files that have <code>c</code> in them
  #                       (including at the beginning or end).
  #
  #   To match hidden files (that start with a <code>.</code>) set the
  #   File::FNM_DOTMATCH flag.
  #
  # <code>**</code>::
  #   Matches directories recursively or files expansively.
  #
  # <code>?</code>::
  #   Matches any one character. Equivalent to <code>/.{1}/</code> in regexp.
  #
  # <code>[set]</code>::
  #   Matches any one character in +set+.  Behaves exactly like character sets
  #   in Regexp, including set negation (<code>[^a-z]</code>).
  #
  # <code>\\</code>::
  #   Escapes the next metacharacter.
  #
  # <code>{a,b}</code>::
  #   Matches pattern a and pattern b if File::FNM_EXTGLOB flag is enabled.
  #   Behaves like a Regexp union (<code>(?:a|b)</code>).
  #
  # +flags+ is a bitwise OR of the <code>FNM_XXX</code> constants. The same
  # glob pattern and flags are used by Dir::glob.
  #
  # Examples:
  #
  #    File.fnmatch('cat',       'cat')        #=> true  # match entire string
  #    File.fnmatch('cat',       'category')   #=> false # only match partial string
  #
  #    File.fnmatch('c{at,ub}s', 'cats')                    #=> false # { } isn't supported by default
  #    File.fnmatch('c{at,ub}s', 'cats', File::FNM_EXTGLOB) #=> true  # { } is supported on FNM_EXTGLOB
  #
  #    File.fnmatch('c?t',     'cat')          #=> true  # '?' match only 1 character
  #    File.fnmatch('c??t',    'cat')          #=> false # ditto
  #    File.fnmatch('c*',      'cats')         #=> true  # '*' match 0 or more characters
  #    File.fnmatch('c*t',     'c/a/b/t')      #=> true  # ditto
  #    File.fnmatch('ca[a-z]', 'cat')          #=> true  # inclusive bracket expression
  #    File.fnmatch('ca[^t]',  'cat')          #=> false # exclusive bracket expression ('^' or '!')
  #
  #    File.fnmatch('cat', 'CAT')                     #=> false # case sensitive
  #    File.fnmatch('cat', 'CAT', File::FNM_CASEFOLD) #=> true  # case insensitive
  #    File.fnmatch('cat', 'CAT', File::FNM_SYSCASE)  #=> true or false # depends on the system default
  #
  #    File.fnmatch('?',   '/', File::FNM_PATHNAME)  #=> false # wildcard doesn't match '/' on FNM_PATHNAME
  #    File.fnmatch('*',   '/', File::FNM_PATHNAME)  #=> false # ditto
  #    File.fnmatch('[/]', '/', File::FNM_PATHNAME)  #=> false # ditto
  #
  #    File.fnmatch('\?',   '?')                       #=> true  # escaped wildcard becomes ordinary
  #    File.fnmatch('\a',   'a')                       #=> true  # escaped ordinary remains ordinary
  #    File.fnmatch('\a',   '\a', File::FNM_NOESCAPE)  #=> true  # FNM_NOESCAPE makes '\' ordinary
  #    File.fnmatch('[\?]', '?')                       #=> true  # can escape inside bracket expression
  #
  #    File.fnmatch('*',   '.profile')                      #=> false # wildcard doesn't match leading
  #    File.fnmatch('*',   '.profile', File::FNM_DOTMATCH)  #=> true  # period by default.
  #    File.fnmatch('.*',  '.profile')                      #=> true
  #
  #    File.fnmatch('**/*.rb', 'main.rb')                  #=> false
  #    File.fnmatch('**/*.rb', './main.rb')                #=> false
  #    File.fnmatch('**/*.rb', 'lib/song.rb')              #=> true
  #    File.fnmatch('**.rb', 'main.rb')                    #=> true
  #    File.fnmatch('**.rb', './main.rb')                  #=> false
  #    File.fnmatch('**.rb', 'lib/song.rb')                #=> true
  #    File.fnmatch('*',     'dave/.profile')              #=> true
  #
  #    File.fnmatch('**/foo', 'a/b/c/foo', File::FNM_PATHNAME)     #=> true
  #    File.fnmatch('**/foo', '/a/b/c/foo', File::FNM_PATHNAME)    #=> true
  #    File.fnmatch('**/foo', 'c:/a/b/c/foo', File::FNM_PATHNAME)  #=> true
  #    File.fnmatch('**/foo', 'a/.b/c/foo', File::FNM_PATHNAME)    #=> false
  #    File.fnmatch('**/foo', 'a/.b/c/foo', File::FNM_PATHNAME | File::FNM_DOTMATCH) #=> true
  def self.fnmatch(pattern, path, *flags) end
  class << self
    alias fnmatch? fnmatch
  end

  # Identifies the type of the named file; the return string is one of
  # ``<code>file</code>'', ``<code>directory</code>'',
  # ``<code>characterSpecial</code>'', ``<code>blockSpecial</code>'',
  # ``<code>fifo</code>'', ``<code>link</code>'',
  # ``<code>socket</code>'', or ``<code>unknown</code>''.
  #
  #    File.ftype("testfile")            #=> "file"
  #    File.ftype("/dev/tty")            #=> "characterSpecial"
  #    File.ftype("/tmp/.X11-unix/X0")   #=> "socket"
  def self.ftype(file_name) end

  # Returns <code>true</code> if the named file exists and the
  # effective group id of the calling process is the owner of
  # the file. Returns <code>false</code> on Windows.
  #
  # _file_name_ can be an IO object.
  def self.grpowned?(file_name) end

  # Returns <code>true</code> if the named files are identical.
  #
  # _file_1_ and _file_2_ can be an IO object.
  #
  #     open("a", "w") {}
  #     p File.identical?("a", "a")      #=> true
  #     p File.identical?("a", "./a")    #=> true
  #     File.link("a", "b")
  #     p File.identical?("a", "b")      #=> true
  #     File.symlink("a", "c")
  #     p File.identical?("a", "c")      #=> true
  #     open("d", "w") {}
  #     p File.identical?("a", "d")      #=> false
  def self.identical?(file1, file2) end

  # Returns a new string formed by joining the strings using
  # <code>"/"</code>.
  #
  #    File.join("usr", "mail", "gumby")   #=> "usr/mail/gumby"
  def self.join(string, *args) end

  # Equivalent to File::chmod, but does not follow symbolic links (so
  # it will change the permissions associated with the link, not the
  # file referenced by the link). Often not available.
  def self.lchmod(mode_int, file_name, *args) end

  # Equivalent to File::chown, but does not follow symbolic
  # links (so it will change the owner associated with the link, not the
  # file referenced by the link). Often not available. Returns number
  # of files in the argument list.
  def self.lchown(*args) end

  # Creates a new name for an existing file using a hard link. Will not
  # overwrite <i>new_name</i> if it already exists (raising a subclass
  # of SystemCallError). Not available on all platforms.
  #
  #    File.link("testfile", ".testfile")   #=> 0
  #    IO.readlines(".testfile")[0]         #=> "This is line one\n"
  def self.link(old_name, new_name) end

  # Like File::stat, but does not follow the last symbolic link;
  # instead, returns a File::Stat object for the link itself.
  #
  #   File.symlink('t.txt', 'symlink')
  #   File.stat('symlink').size  # => 47
  #   File.lstat('symlink').size # => 5
  def self.lstat(filepath) end

  # Sets the access and modification times of each named file to the
  # first two arguments. If a file is a symlink, this method acts upon
  # the link itself as opposed to its referent; for the inverse
  # behavior, see File.utime. Returns the number of file
  # names in the argument list.
  def self.lutime(atime, mtime, file_name, *args) end

  # Creates a FIFO special file with name _file_name_.  _mode_
  # specifies the FIFO's permissions. It is modified by the process's
  # umask in the usual way: the permissions of the created file are
  # (mode & ~umask).
  def self.mkfifo(file_name, mode = 0o666) end

  # Returns the modification time for the named file as a Time object.
  #
  # _file_name_ can be an IO object.
  #
  #    File.mtime("testfile")   #=> Tue Apr 08 12:58:04 CDT 2003
  def self.mtime(file_name) end

  # Creates a new File object, via File.new with the given arguments.
  #
  # With no block given, returns the File object.
  #
  # With a block given, calls the block with the File object
  # and returns the block's value.
  def self.open(path, mode = 'r', perm = 0o666, **opts) end

  # Returns <code>true</code> if the named file exists and the
  # effective used id of the calling process is the owner of
  # the file.
  #
  # _file_name_ can be an IO object.
  def self.owned?(file_name) end

  # Returns the string representation of the path
  #
  #    File.path(File::NULL)           #=> "/dev/null"
  #    File.path(Pathname.new("/tmp")) #=> "/tmp"
  def self.path(path) end

  # Returns +true+ if +filepath+ points to a pipe, +false+ otherwise:
  #
  #   File.mkfifo('tmp/fifo')
  #   File.pipe?('tmp/fifo') # => true
  #   File.pipe?('t.txt')    # => false
  def self.pipe?(filepath) end

  # Returns <code>true</code> if the named file is readable by the effective
  # user and group id of this process. See eaccess(3).
  #
  # Note that some OS-level security features may cause this to return true
  # even though the file is not readable by the effective user/group.
  def self.readable?(file_name) end

  # Returns <code>true</code> if the named file is readable by the real
  # user and group id of this process. See access(3).
  #
  # Note that some OS-level security features may cause this to return true
  # even though the file is not readable by the real user/group.
  def self.readable_real?(file_name) end

  # Returns the name of the file referenced by the given link.
  # Not available on all platforms.
  #
  #    File.symlink("testfile", "link2test")   #=> 0
  #    File.readlink("link2test")              #=> "testfile"
  def self.readlink(link_name) end

  #  Returns the real (absolute) pathname of _pathname_ in the actual filesystem.
  #  The real pathname doesn't contain symlinks or useless dots.
  #
  #  If _dir_string_ is given, it is used as a base directory
  #  for interpreting relative pathname instead of the current directory.
  #
  #  The last component of the real pathname can be nonexistent.
  def self.realdirpath(*args) end

  #  Returns the real (absolute) pathname of _pathname_ in the actual
  #  filesystem not containing symlinks or useless dots.
  #
  #  If _dir_string_ is given, it is used as a base directory
  #  for interpreting relative pathname instead of the current directory.
  #
  #  All components of the pathname must exist when this method is
  #  called.
  def self.realpath(*args) end

  # Renames the given file to the new name. Raises a SystemCallError
  # if the file cannot be renamed.
  #
  #    File.rename("afile", "afile.bak")   #=> 0
  def self.rename(old_name, new_name) end

  # Returns <code>true</code> if the named file has the setgid bit set.
  #
  # _file_name_ can be an IO object.
  def self.setgid?(file_name) end

  # Returns <code>true</code> if the named file has the setuid bit set.
  #
  # _file_name_ can be an IO object.
  def self.setuid?(file_name) end

  # Returns the size of <code>file_name</code>.
  #
  # _file_name_ can be an IO object.
  def self.size(file_name) end

  # Returns +nil+ if +file_name+ doesn't exist or has zero size, the size of the
  # file otherwise.
  #
  # _file_name_ can be an IO object.
  def self.size?(file_name) end

  # Returns +true+ if +filepath+ points to a socket, +false+ otherwise:
  #
  #   require 'socket'
  #   File.socket?(Socket.new(:INET, :STREAM)) # => true
  #   File.socket?(File.new('t.txt'))          # => false
  def self.socket?(filepath) end

  # Splits the given string into a directory and a file component and
  # returns them in a two-element array. See also File::dirname and
  # File::basename.
  #
  #    File.split("/home/gumby/.profile")   #=> ["/home/gumby", ".profile"]
  def self.split(file_name) end

  # Returns a File::Stat object for the file at +filepath+ (see File::Stat):
  #
  #   File.stat('t.txt').class # => File::Stat
  def self.stat(filepath) end

  # Returns <code>true</code> if the named file has the sticky bit set.
  #
  # _file_name_ can be an IO object.
  def self.sticky?(file_name) end

  # Creates a symbolic link called <i>new_name</i> for the existing file
  # <i>old_name</i>. Raises a NotImplemented exception on
  # platforms that do not support symbolic links.
  #
  #    File.symlink("testfile", "link2test")   #=> 0
  def self.symlink(old_name, new_name) end

  # Returns +true+ if +filepath+ points to a symbolic link, +false+ otherwise:
  #
  #   symlink = File.symlink('t.txt', 'symlink')
  #   File.symlink?('symlink') # => true
  #   File.symlink?('t.txt')   # => false
  def self.symlink?(filepath) end

  # Truncates the file <i>file_name</i> to be at most <i>integer</i>
  # bytes long. Not available on all platforms.
  #
  #    f = File.new("out", "w")
  #    f.write("1234567890")     #=> 10
  #    f.close                   #=> nil
  #    File.truncate("out", 5)   #=> 0
  #    File.size("out")          #=> 5
  def self.truncate(file_name, integer) end

  # Returns the current umask value for this process. If the optional
  # argument is given, set the umask to that value and return the
  # previous value. Umask values are <em>subtracted</em> from the
  # default permissions, so a umask of <code>0222</code> would make a
  # file read-only for everyone.
  #
  #    File.umask(0006)   #=> 18
  #    File.umask         #=> 6
  def self.umask(...) end

  # Deletes the named files, returning the number of names
  # passed as arguments. Raises an exception on any error.
  # Since the underlying implementation relies on the
  # <code>unlink(2)</code> system call, the type of
  # exception raised depends on its error type (see
  # https://linux.die.net/man/2/unlink) and has the form of
  # e.g. Errno::ENOENT.
  #
  # See also Dir::rmdir.
  def self.unlink(file_name, *args) end

  # Sets the access and modification times of each named file to the
  # first two arguments. If a file is a symlink, this method acts upon
  # its referent rather than the link itself; for the inverse
  # behavior see File.lutime. Returns the number of file
  # names in the argument list.
  def self.utime(atime, mtime, file_name, *args) end

  # If <i>file_name</i> is readable by others, returns an integer
  # representing the file permission bits of <i>file_name</i>. Returns
  # <code>nil</code> otherwise. The meaning of the bits is platform
  # dependent; on Unix systems, see <code>stat(2)</code>.
  #
  # _file_name_ can be an IO object.
  #
  #    File.world_readable?("/etc/passwd")           #=> 420
  #    m = File.world_readable?("/etc/passwd")
  #    sprintf("%o", m)                              #=> "644"
  def self.world_readable?(file_name) end

  # If <i>file_name</i> is writable by others, returns an integer
  # representing the file permission bits of <i>file_name</i>. Returns
  # <code>nil</code> otherwise. The meaning of the bits is platform
  # dependent; on Unix systems, see <code>stat(2)</code>.
  #
  # _file_name_ can be an IO object.
  #
  #    File.world_writable?("/tmp")                  #=> 511
  #    m = File.world_writable?("/tmp")
  #    sprintf("%o", m)                              #=> "777"
  def self.world_writable?(file_name) end

  # Returns <code>true</code> if the named file is writable by the effective
  # user and group id of this process. See eaccess(3).
  #
  # Note that some OS-level security features may cause this to return true
  # even though the file is not writable by the effective user/group.
  def self.writable?(file_name) end

  # Returns <code>true</code> if the named file is writable by the real
  # user and group id of this process. See access(3).
  #
  # Note that some OS-level security features may cause this to return true
  # even though the file is not writable by the real user/group.
  def self.writable_real?(file_name) end

  # Returns <code>true</code> if the named file exists and has
  # a zero size.
  #
  # _file_name_ can be an IO object.
  def self.zero?(file_name) end

  # Opens the file at the given +path+ according to the given +mode+;
  # creates and returns a new File object for that file.
  #
  # The new File object is buffered mode (or non-sync mode), unless
  # +filename+ is a tty.
  # See IO#flush, IO#fsync, IO#fdatasync, and IO#sync=.
  #
  # Argument +path+ must be a valid file path:
  #
  #   f = File.new('/etc/fstab')
  #   f.close
  #   f = File.new('t.txt')
  #   f.close
  #
  # Optional argument +mode+ (defaults to 'r') must specify a valid mode;
  # see {Access Modes}[rdoc-ref:File@Access+Modes]:
  #
  #   f = File.new('t.tmp', 'w')
  #   f.close
  #   f = File.new('t.tmp', File::RDONLY)
  #   f.close
  #
  # Optional argument +perm+ (defaults to 0666) must specify valid permissions
  # see {File Permissions}[rdoc-ref:File@File+Permissions]:
  #
  #   f = File.new('t.tmp', File::CREAT, 0644)
  #   f.close
  #   f = File.new('t.tmp', File::CREAT, 0444)
  #   f.close
  #
  # Optional keyword arguments +opts+ specify:
  #
  # - {Open Options}[rdoc-ref:IO@Open+Options].
  # - {Encoding options}[rdoc-ref:encodings.rdoc@Encoding+Options].
  def initialize(path, mode = 'r', perm = 0o666, **opts) end

  # Returns the last access time (a Time object) for <i>file</i>, or
  # epoch if <i>file</i> has not been accessed.
  #
  #    File.new("testfile").atime   #=> Wed Dec 31 18:00:00 CST 1969
  def atime; end

  # Returns the birth time for <i>file</i>.
  #
  #    File.new("testfile").birthtime   #=> Wed Apr 09 08:53:14 CDT 2003
  #
  # If the platform doesn't have birthtime, raises NotImplementedError.
  def birthtime; end

  # Changes permission bits on <i>file</i> to the bit pattern
  # represented by <i>mode_int</i>. Actual effects are platform
  # dependent; on Unix systems, see <code>chmod(2)</code> for details.
  # Follows symbolic links. Also see File#lchmod.
  #
  #    f = File.new("out", "w");
  #    f.chmod(0644)   #=> 0
  def chmod(mode_int) end

  # Changes the owner and group of <i>file</i> to the given numeric
  # owner and group id's. Only a process with superuser privileges may
  # change the owner of a file. The current owner of a file may change
  # the file's group to any group to which the owner belongs. A
  # <code>nil</code> or -1 owner or group id is ignored. Follows
  # symbolic links. See also File#lchown.
  #
  #    File.new("testfile").chown(502, 1000)
  def chown(owner_int, group_int) end

  # Returns the change time for <i>file</i> (that is, the time directory
  # information about the file was changed, not the file itself).
  #
  # Note that on Windows (NTFS), returns creation time (birth time).
  #
  #    File.new("testfile").ctime   #=> Wed Apr 09 08:53:14 CDT 2003
  def ctime; end

  # Locks or unlocks file +self+ according to the given `locking_constant`,
  # a bitwise OR of the values in the table below.
  #
  # Not available on all platforms.
  #
  # Returns `false` if `File::LOCK_NB` is specified and the operation would have blocked;
  # otherwise returns `0`.
  #
  # | Constant        | Lock         | Effect
  # |-----------------|--------------|-----------------------------------------------------------------------------------------------------------------|
  # | +File::LOCK_EX+ | Exclusive    | Only one process may hold an exclusive lock for +self+ at a time.                                               |
  # | +File::LOCK_NB+ | Non-blocking | No blocking; may be combined with +File::LOCK_SH+ or +File::LOCK_EX+ using the bitwise OR operator <tt>\|</tt>. |
  # | +File::LOCK_SH+ | Shared       | Multiple processes may each hold a shared lock for +self+ at the same time.                                     |
  # | +File::LOCK_UN+ | Unlock       | Remove an existing lock held by this process.                                                                   |
  #
  # Example:
  #
  # ```ruby
  # # Update a counter using an exclusive lock.
  # # Don't use File::WRONLY because it truncates the file.
  # File.open('counter', File::RDWR | File::CREAT, 0644) do |f|
  #   f.flock(File::LOCK_EX)
  #   value = f.read.to_i + 1
  #   f.rewind
  #   f.write("#{value}\n")
  #   f.flush
  #   f.truncate(f.pos)
  # end
  #
  # # Read the counter using a shared lock.
  # File.open('counter', 'r') do |f|
  #   f.flock(File::LOCK_SH)
  #   f.read
  # end
  # ```
  def flock(locking_constant) end

  # Like File#stat, but does not follow the last symbolic link;
  # instead, returns a File::Stat object for the link itself:
  #
  #   File.symlink('t.txt', 'symlink')
  #   f = File.new('symlink')
  #   f.stat.size  # => 47
  #   f.lstat.size # => 11
  def lstat; end

  # Returns the modification time for <i>file</i>.
  #
  #    File.new("testfile").mtime   #=> Wed Apr 09 08:53:14 CDT 2003
  def mtime; end

  # Returns the size of <i>file</i> in bytes.
  #
  #    File.new("testfile").size   #=> 66
  def size; end

  # Truncates <i>file</i> to at most <i>integer</i> bytes. The file
  # must be opened for writing. Not available on all platforms.
  #
  #    f = File.new("out", "w")
  #    f.syswrite("1234567890")   #=> 10
  #    f.truncate(5)              #=> 0
  #    f.close()                  #=> nil
  #    File.size("out")           #=> 5
  def truncate(integer) end

  # \Module +File::Constants+ defines file-related constants.
  #
  # There are two families of constants here:
  #
  # - Those having to do with {file access}[rdoc-ref:File::Constants@File+Access].
  # - Those having to do with {filename globbing}[rdoc-ref:File::Constants@Filename+Globbing+Constants+-28File-3A-3AFNM_-2A-29].
  #
  # \File constants defined for the local process may be retrieved
  # with method File::Constants.constants:
  #
  #   File::Constants.constants.take(5)
  #   # => [:RDONLY, :WRONLY, :RDWR, :APPEND, :CREAT]
  #
  # == \File Access
  #
  # \File-access constants may be used with optional argument +mode+ in calls
  # to the following methods:
  #
  # - File.new.
  # - File.open.
  # - IO.for_fd.
  # - IO.new.
  # - IO.open.
  # - IO.popen.
  # - IO.reopen.
  # - IO.sysopen.
  # - StringIO.new.
  # - StringIO.open.
  # - StringIO#reopen.
  #
  # === Read/Write Access
  #
  # Read-write access for a stream
  # may be specified by a file-access constant.
  #
  # The constant may be specified as part of a bitwise OR of other such constants.
  #
  # Any combination of the constants in this section may be specified.
  #
  # ==== File::RDONLY
  #
  # Flag File::RDONLY specifies the stream should be opened for reading only:
  #
  #   filepath = '/tmp/t.tmp'
  #   f = File.new(filepath, File::RDONLY)
  #   f.write('Foo') # Raises IOError (not opened for writing).
  #
  # ==== File::WRONLY
  #
  # Flag File::WRONLY specifies that the stream should be opened for writing only:
  #
  #   f = File.new(filepath, File::WRONLY)
  #   f.read # Raises IOError (not opened for reading).
  #
  # ==== File::RDWR
  #
  # Flag File::RDWR specifies that the stream should be opened
  # for both reading and writing:
  #
  #   f = File.new(filepath, File::RDWR)
  #   f.write('Foo') # => 3
  #   f.rewind       # => 0
  #   f.read         # => "Foo"
  #
  # === \File Positioning
  #
  # ==== File::APPEND
  #
  # Flag File::APPEND specifies that the stream should be opened
  # in append mode.
  #
  # Before each write operation, the position is set to end-of-stream.
  # The modification of the position and the following write operation
  # are performed as a single atomic step.
  #
  # ==== File::TRUNC
  #
  # Flag File::TRUNC specifies that the stream should be truncated
  # at its beginning.
  # If the file exists and is successfully opened for writing,
  # it is to be truncated to position zero;
  # its ctime and mtime are updated.
  #
  # There is no effect on a FIFO special file or a terminal device.
  # The effect on other file types is implementation-defined.
  # The result of using File::TRUNC with File::RDONLY is undefined.
  #
  # === Creating and Preserving
  #
  # ==== File::CREAT
  #
  # Flag File::CREAT specifies that the stream should be created
  # if it does not already exist.
  #
  # If the file exists:
  #
  #   - Raise an exception if File::EXCL is also specified.
  #   - Otherwise, do nothing.
  #
  # If the file does not exist, then it is created.
  # Upon successful completion, the atime, ctime, and mtime of the file are updated,
  # and the ctime and mtime of the parent directory are updated.
  #
  # ==== File::EXCL
  #
  # Flag File::EXCL specifies that the stream should not already exist;
  # If flags File::CREAT and File::EXCL are both specified
  # and the stream already exists, an exception is raised.
  #
  # The check for the existence and creation of the file is performed as an
  # atomic operation.
  #
  # If both File::EXCL and File::CREAT are specified and the path names a symbolic link,
  # an exception is raised regardless of the contents of the symbolic link.
  #
  # If File::EXCL is specified and File::CREAT is not specified,
  # the result is undefined.
  #
  # === POSIX \File \Constants
  #
  # Some file-access constants are defined only on POSIX-compliant systems;
  # those are:
  #
  # - File::SYNC.
  # - File::DSYNC.
  # - File::RSYNC.
  # - File::DIRECT.
  # - File::NOATIME.
  # - File::NOCTTY.
  # - File::NOFOLLOW.
  # - File::TMPFILE.
  #
  # ==== File::SYNC, File::RSYNC, and File::DSYNC
  #
  # Flag File::SYNC, File::RSYNC, or File::DSYNC
  # specifies synchronization of I/O operations with the underlying file system.
  #
  # These flags are valid only for POSIX-compliant systems.
  #
  # - File::SYNC specifies that all write operations (both data and metadata)
  #   are immediately to be flushed to the underlying storage device.
  #   This means that the data is written to the storage device,
  #   and the file's metadata (e.g., file size, timestamps, permissions)
  #   are also synchronized.
  #   This guarantees that data is safely stored on the storage medium
  #   before returning control to the calling program.
  #   This flag can have a significant impact on performance
  #   since it requires synchronous writes, which can be slower
  #   compared to asynchronous writes.
  #
  # - File::RSYNC specifies that any read operations on the file will not return
  #   until all outstanding write operations
  #   (those that have been issued but not completed) are also synchronized.
  #   This is useful when you want to read the most up-to-date data,
  #   which may still be in the process of being written.
  #
  # - File::DSYNC specifies that all _data_ write operations
  #   are immediately to be flushed to the underlying storage device;
  #   this differs from File::SYNC, which requires that _metadata_
  #   also be synchronized.
  #
  # Note that the behavior of these flags may vary slightly
  # depending on the operating system and filesystem being used.
  # Additionally, using these flags can have an impact on performance
  # due to the synchronous nature of the I/O operations,
  # so they should be used judiciously,
  # especially in performance-critical applications.
  #
  # ==== File::NOCTTY
  #
  # Flag File::NOCTTY specifies that if the stream is a terminal device,
  # that device does not become the controlling terminal for the process.
  #
  # Defined only for POSIX-compliant systems.
  #
  # ==== File::DIRECT
  #
  # Flag File::DIRECT requests that cache effects of the I/O to and from the stream
  # be minimized.
  #
  # Defined only for POSIX-compliant systems.
  #
  # ==== File::NOATIME
  #
  # Flag File::NOATIME specifies that act of opening the stream
  # should not modify its access time (atime).
  #
  # Defined only for POSIX-compliant systems.
  #
  # ==== File::NOFOLLOW
  #
  # Flag File::NOFOLLOW specifies that if path is a symbolic link,
  # it should not be followed.
  #
  # Defined only for POSIX-compliant systems.
  #
  # ==== File::TMPFILE
  #
  # Flag File::TMPFILE specifies that the opened stream
  # should be a new temporary file.
  #
  # Defined only for POSIX-compliant systems.
  #
  # === Other File-Access \Constants
  #
  # ==== File::NONBLOCK
  #
  # When possible, the file is opened in nonblocking mode.
  # Neither the open operation nor any subsequent I/O operations on
  # the file will cause the calling process to wait.
  #
  # ==== File::BINARY
  #
  # Flag File::BINARY specifies that the stream is to be accessed in binary mode.
  #
  # ==== File::SHARE_DELETE
  #
  # Flag File::SHARE_DELETE enables other processes to open the stream
  # with delete access.
  #
  # Windows only.
  #
  # If the stream is opened for (local) delete access without File::SHARE_DELETE,
  # and another process attempts to open it with delete access,
  # the attempt fails and the stream is not opened for that process.
  #
  # == Locking
  #
  # Four file constants relate to stream locking;
  # see File#flock:
  #
  # ==== File::LOCK_EX
  #
  # Flag File::LOCK_EX specifies an exclusive lock;
  # only one process a a time may lock the stream.
  #
  # ==== File::LOCK_NB
  #
  # Flag File::LOCK_NB specifies non-blocking locking for the stream;
  # may be combined with File::LOCK_EX or File::LOCK_SH.
  #
  # ==== File::LOCK_SH
  #
  # Flag File::LOCK_SH specifies that multiple processes may lock
  # the stream at the same time.
  #
  # ==== File::LOCK_UN
  #
  # Flag File::LOCK_UN specifies that the stream is not to be locked.
  #
  # == Filename Globbing \Constants (File::FNM_*)
  #
  # Filename-globbing constants may be used with optional argument +flags+
  # in calls to the following methods:
  #
  # - Dir.glob.
  # - File.fnmatch.
  # - Pathname#fnmatch.
  # - Pathname.glob.
  # - Pathname#glob.
  #
  # The constants are:
  #
  # ==== File::FNM_CASEFOLD
  #
  # Flag File::FNM_CASEFOLD makes patterns case insensitive
  # for File.fnmatch (but not Dir.glob).
  #
  # ==== File::FNM_DOTMATCH
  #
  # Flag File::FNM_DOTMATCH makes the <tt>'*'</tt> pattern
  # match a filename starting with <tt>'.'</tt>.
  #
  # ==== File::FNM_EXTGLOB
  #
  # Flag File::FNM_EXTGLOB enables pattern <tt>'{_a_,_b_}'</tt>,
  # which matches pattern '_a_' and pattern '_b_';
  # behaves like
  # a {regexp union}[rdoc-ref:Regexp.union]
  # (e.g., <tt>'(?:_a_|_b_)'</tt>):
  #
  #   pattern = '{LEGAL,BSDL}'
  #   Dir.glob(pattern)      # => ["LEGAL", "BSDL"]
  #   Pathname.glob(pattern) # => [#<Pathname:LEGAL>, #<Pathname:BSDL>]
  #   pathname.glob(pattern) # => [#<Pathname:LEGAL>, #<Pathname:BSDL>]
  #
  # ==== File::FNM_NOESCAPE
  #
  # Flag File::FNM_NOESCAPE disables <tt>'\'</tt> escaping.
  #
  # ==== File::FNM_PATHNAME
  #
  # Flag File::FNM_PATHNAME specifies that patterns <tt>'*'</tt> and <tt>'?'</tt>
  # do not match the directory separator
  # (the value of constant File::SEPARATOR).
  #
  # ==== File::FNM_SHORTNAME
  #
  # Flag File::FNM_SHORTNAME allows patterns to match short names if they exist.
  #
  # Windows only.
  #
  # ==== File::FNM_SYSCASE
  #
  # Flag File::FNM_SYSCASE specifies that case sensitivity
  # is the same as in the underlying operating system;
  # effective for File.fnmatch, but not Dir.glob.
  #
  # == Other \Constants
  #
  # ==== File::NULL
  #
  # Flag File::NULL contains the string value of the null device:
  #
  # - On a Unix-like OS, <tt>'/dev/null'</tt>.
  # - On Windows, <tt>'NUL'</tt>.
  module Constants
    # {File::APPEND}[rdoc-ref:File::Constants@File-3A-3AAPPEND]
    APPEND = _
    # {File::BINARY}[rdoc-ref:File::Constants@File-3A-3ABINARY]
    BINARY = _
    # {File::CREAT}[rdoc-ref:File::Constants@File-3A-3ACREAT]
    CREAT = _
    # {File::DIRECT}[rdoc-ref:File::Constants@File-3A-3ADIRECT]
    DIRECT = _
    # {File::DSYNC}[rdoc-ref:File::Constants@File-3A-3ASYNC-2C+File-3A-3ARSYNC-2C+and+File-3A-3ADSYNC]
    DSYNC = _
    # {File::EXCL}[rdoc-ref:File::Constants@File-3A-3AEXCL]
    EXCL = _
    # {File::FNM_CASEFOLD}[rdoc-ref:File::Constants@File-3A-3AFNM_CASEFOLD]
    FNM_CASEFOLD = _
    # {File::FNM_DOTMATCH}[rdoc-ref:File::Constants@File-3A-3AFNM_DOTMATCH]
    FNM_DOTMATCH = _
    # {File::FNM_EXTGLOB}[rdoc-ref:File::Constants@File-3A-3AFNM_EXTGLOB]
    FNM_EXTGLOB = _
    # {File::FNM_NOESCAPE}[rdoc-ref:File::Constants@File-3A-3AFNM_NOESCAPE]
    FNM_NOESCAPE = _
    # {File::FNM_PATHNAME}[rdoc-ref:File::Constants@File-3A-3AFNM_PATHNAME]
    FNM_PATHNAME = _
    # {File::FNM_SHORTNAME}[rdoc-ref:File::Constants@File-3A-3AFNM_SHORTNAME]
    FNM_SHORTNAME = _
    # {File::FNM_SYSCASE}[rdoc-ref:File::Constants@File-3A-3AFNM_SYSCASE]
    FNM_SYSCASE = _
    # {File::LOCK_EX}[rdoc-ref:File::Constants@File-3A-3ALOCK_EX]
    LOCK_EX = _
    # {File::LOCK_NB}[rdoc-ref:File::Constants@File-3A-3ALOCK_NB]
    LOCK_NB = _
    # {File::LOCK_SH}[rdoc-ref:File::Constants@File-3A-3ALOCK_SH]
    LOCK_SH = _
    # {File::LOCK_UN}[rdoc-ref:File::Constants@File-3A-3ALOCK_UN]
    LOCK_UN = _
    # {File::NOATIME}[rdoc-ref:File::Constants@File-3A-3ANOATIME]
    NOATIME = _
    # {File::NOCTTY}[rdoc-ref:File::Constants@File-3A-3ANOCTTY]
    NOCTTY = _
    # {File::NOFOLLOW}[rdoc-ref:File::Constants@File-3A-3ANOFOLLOW]
    NOFOLLOW = _
    # {File::NONBLOCK}[rdoc-ref:File::Constants@File-3A-3ANONBLOCK]
    NONBLOCK = _
    # {File::NULL}[rdoc-ref:File::Constants@File-3A-3ANULL]
    NULL = _
    # {File::RDONLY}[rdoc-ref:File::Constants@File-3A-3ARDONLY]
    RDONLY = _
    # {File::RDWR}[rdoc-ref:File::Constants@File-3A-3ARDWR]
    RDWR = _
    # {File::RSYNC}[rdoc-ref:File::Constants@File-3A-3ASYNC-2C+File-3A-3ARSYNC-2C+and+File-3A-3ADSYNC]
    RSYNC = _
    # {File::SHARE_DELETE}[rdoc-ref:File::Constants@File-3A-3ASHARE_DELETE]
    SHARE_DELETE = _
    # {File::SYNC}[rdoc-ref:File::Constants@File-3A-3ASYNC-2C+File-3A-3ARSYNC-2C+and+File-3A-3ADSYNC]
    SYNC = _
    # {File::TMPFILE}[rdoc-ref:File::Constants@File-3A-3ATMPFILE]
    TMPFILE = _
    # {File::TRUNC}[rdoc-ref:File::Constants@File-3A-3ATRUNC]
    TRUNC = _
    # {File::WRONLY}[rdoc-ref:File::Constants@File-3A-3AWRONLY]
    WRONLY = _
  end

  # Objects of class File::Stat encapsulate common status information
  # for File objects. The information is recorded at the moment the
  # File::Stat object is created; changes made to the file after that
  # point will not be reflected. File::Stat objects are returned by
  # IO#stat, File::stat, File#lstat, and File::lstat. Many of these
  # methods return platform-specific values, and not all values are
  # meaningful on all systems. See also Kernel#test.
  class Stat
    include Comparable

    #   File::Stat.new(file_name)  -> stat
    #
    # Create a File::Stat object for the given file name (raising an
    # exception if the file doesn't exist).
    def initialize(p1) end

    # Compares File::Stat objects by comparing their respective modification
    # times.
    #
    # +nil+ is returned if +other_stat+ is not a File::Stat object
    #
    #    f1 = File.new("f1", "w")
    #    sleep 1
    #    f2 = File.new("f2", "w")
    #    f1.stat <=> f2.stat   #=> -1
    def <=>(other) end

    # Returns the last access time for this file as an object of class
    # Time.
    #
    #    File.stat("testfile").atime   #=> Wed Dec 31 18:00:00 CST 1969
    def atime; end

    # Returns the birth time for <i>stat</i>.
    #
    # If the platform doesn't have birthtime, raises NotImplementedError.
    #
    #    File.write("testfile", "foo")
    #    sleep 10
    #    File.write("testfile", "bar")
    #    sleep 10
    #    File.chmod(0644, "testfile")
    #    sleep 10
    #    File.read("testfile")
    #    File.stat("testfile").birthtime   #=> 2014-02-24 11:19:17 +0900
    #    File.stat("testfile").mtime       #=> 2014-02-24 11:19:27 +0900
    #    File.stat("testfile").ctime       #=> 2014-02-24 11:19:37 +0900
    #    File.stat("testfile").atime       #=> 2014-02-24 11:19:47 +0900
    def birthtime; end

    # Returns the native file system's block size. Will return <code>nil</code>
    # on platforms that don't support this information.
    #
    #    File.stat("testfile").blksize   #=> 4096
    def blksize; end

    # Returns <code>true</code> if the file is a block device,
    # <code>false</code> if it isn't or if the operating system doesn't
    # support this feature.
    #
    #    File.stat("testfile").blockdev?    #=> false
    #    File.stat("/dev/hda1").blockdev?   #=> true
    def blockdev?; end

    # Returns the number of native file system blocks allocated for this
    # file, or <code>nil</code> if the operating system doesn't
    # support this feature.
    #
    #    File.stat("testfile").blocks   #=> 2
    def blocks; end

    # Returns <code>true</code> if the file is a character device,
    # <code>false</code> if it isn't or if the operating system doesn't
    # support this feature.
    #
    #    File.stat("/dev/tty").chardev?   #=> true
    def chardev?; end

    # Returns the change time for <i>stat</i> (that is, the time
    # directory information about the file was changed, not the file
    # itself).
    #
    # Note that on Windows (NTFS), returns creation time (birth time).
    #
    #    File.stat("testfile").ctime   #=> Wed Apr 09 08:53:14 CDT 2003
    def ctime; end

    # Returns an integer representing the device on which <i>stat</i>
    # resides.
    #
    #    File.stat("testfile").dev   #=> 774
    def dev; end

    # Returns the major part of <code>File_Stat#dev</code> or
    # <code>nil</code>.
    #
    #    File.stat("/dev/fd1").dev_major   #=> 2
    #    File.stat("/dev/tty").dev_major   #=> 5
    def dev_major; end

    # Returns the minor part of <code>File_Stat#dev</code> or
    # <code>nil</code>.
    #
    #    File.stat("/dev/fd1").dev_minor   #=> 1
    #    File.stat("/dev/tty").dev_minor   #=> 0
    def dev_minor; end

    # Returns <code>true</code> if <i>stat</i> is a directory,
    # <code>false</code> otherwise.
    #
    #    File.stat("testfile").directory?   #=> false
    #    File.stat(".").directory?          #=> true
    def directory?; end

    # Returns <code>true</code> if <i>stat</i> is executable or if the
    # operating system doesn't distinguish executable files from
    # nonexecutable files. The tests are made using the effective owner of
    # the process.
    #
    #    File.stat("testfile").executable?   #=> false
    def executable?; end

    # Same as <code>executable?</code>, but tests using the real owner of
    # the process.
    def executable_real?; end

    # Returns <code>true</code> if <i>stat</i> is a regular file (not
    # a device file, pipe, socket, etc.).
    #
    #    File.stat("testfile").file?   #=> true
    def file?; end

    # Identifies the type of <i>stat</i>. The return string is one of:
    # ``<code>file</code>'', ``<code>directory</code>'',
    # ``<code>characterSpecial</code>'', ``<code>blockSpecial</code>'',
    # ``<code>fifo</code>'', ``<code>link</code>'',
    # ``<code>socket</code>'', or ``<code>unknown</code>''.
    #
    #    File.stat("/dev/tty").ftype   #=> "characterSpecial"
    def ftype; end

    # Returns the numeric group id of the owner of <i>stat</i>.
    #
    #    File.stat("testfile").gid   #=> 500
    def gid; end

    # Returns true if the effective group id of the process is the same as
    # the group id of <i>stat</i>. On Windows, returns <code>false</code>.
    #
    #    File.stat("testfile").grpowned?      #=> true
    #    File.stat("/etc/passwd").grpowned?   #=> false
    def grpowned?; end

    # Returns the inode number for <i>stat</i>.
    #
    #    File.stat("testfile").ino   #=> 1083669
    def ino; end

    # Produce a nicely formatted description of <i>stat</i>.
    #
    #   File.stat("/etc/passwd").inspect
    #      #=> "#<File::Stat dev=0xe000005, ino=1078078, mode=0100644,
    #      #    nlink=1, uid=0, gid=0, rdev=0x0, size=1374, blksize=4096,
    #      #    blocks=8, atime=Wed Dec 10 10:16:12 CST 2003,
    #      #    mtime=Fri Sep 12 15:41:41 CDT 2003,
    #      #    ctime=Mon Oct 27 11:20:27 CST 2003,
    #      #    birthtime=Mon Aug 04 08:13:49 CDT 2003>"
    def inspect; end

    # Returns an integer representing the permission bits of
    # <i>stat</i>. The meaning of the bits is platform dependent; on
    # Unix systems, see <code>stat(2)</code>.
    #
    #    File.chmod(0644, "testfile")   #=> 1
    #    s = File.stat("testfile")
    #    sprintf("%o", s.mode)          #=> "100644"
    def mode; end

    # Returns the modification time of <i>stat</i>.
    #
    #    File.stat("testfile").mtime   #=> Wed Apr 09 08:53:14 CDT 2003
    def mtime; end

    # Returns the number of hard links to <i>stat</i>.
    #
    #    File.stat("testfile").nlink             #=> 1
    #    File.link("testfile", "testfile.bak")   #=> 0
    #    File.stat("testfile").nlink             #=> 2
    def nlink; end

    # Returns <code>true</code> if the effective user id of the process is
    # the same as the owner of <i>stat</i>.
    #
    #    File.stat("testfile").owned?      #=> true
    #    File.stat("/etc/passwd").owned?   #=> false
    def owned?; end

    # Returns <code>true</code> if the operating system supports pipes and
    # <i>stat</i> is a pipe; <code>false</code> otherwise.
    def pipe?; end

    # Returns an integer representing the device type on which
    # <i>stat</i> resides. Returns <code>nil</code> if the operating
    # system doesn't support this feature.
    #
    #    File.stat("/dev/fd1").rdev   #=> 513
    #    File.stat("/dev/tty").rdev   #=> 1280
    def rdev; end

    # Returns the major part of <code>File_Stat#rdev</code> or
    # <code>nil</code>.
    #
    #    File.stat("/dev/fd1").rdev_major   #=> 2
    #    File.stat("/dev/tty").rdev_major   #=> 5
    def rdev_major; end

    # Returns the minor part of <code>File_Stat#rdev</code> or
    # <code>nil</code>.
    #
    #    File.stat("/dev/fd1").rdev_minor   #=> 1
    #    File.stat("/dev/tty").rdev_minor   #=> 0
    def rdev_minor; end

    # Returns <code>true</code> if <i>stat</i> is readable by the
    # effective user id of this process.
    #
    #    File.stat("testfile").readable?   #=> true
    def readable?; end

    # Returns <code>true</code> if <i>stat</i> is readable by the real
    # user id of this process.
    #
    #    File.stat("testfile").readable_real?   #=> true
    def readable_real?; end

    # Returns <code>true</code> if <i>stat</i> has the set-group-id
    # permission bit set, <code>false</code> if it doesn't or if the
    # operating system doesn't support this feature.
    #
    #    File.stat("/usr/sbin/lpc").setgid?   #=> true
    def setgid?; end

    # Returns <code>true</code> if <i>stat</i> has the set-user-id
    # permission bit set, <code>false</code> if it doesn't or if the
    # operating system doesn't support this feature.
    #
    #    File.stat("/bin/su").setuid?   #=> true
    def setuid?; end

    # Returns the size of <i>stat</i> in bytes.
    #
    #    File.stat("testfile").size   #=> 66
    def size; end

    # Returns +nil+ if <i>stat</i> is a zero-length file, the size of
    # the file otherwise.
    #
    #    File.stat("testfile").size?   #=> 66
    #    File.stat(File::NULL).size?   #=> nil
    def size?; end

    # Returns <code>true</code> if <i>stat</i> is a socket,
    # <code>false</code> if it isn't or if the operating system doesn't
    # support this feature.
    #
    #    File.stat("testfile").socket?   #=> false
    def socket?; end

    # Returns <code>true</code> if <i>stat</i> has its sticky bit set,
    # <code>false</code> if it doesn't or if the operating system doesn't
    # support this feature.
    #
    #    File.stat("testfile").sticky?   #=> false
    def sticky?; end

    # Returns <code>true</code> if <i>stat</i> is a symbolic link,
    # <code>false</code> if it isn't or if the operating system doesn't
    # support this feature. As File::stat automatically follows symbolic
    # links, #symlink? will always be <code>false</code> for an object
    # returned by File::stat.
    #
    #    File.symlink("testfile", "alink")   #=> 0
    #    File.stat("alink").symlink?         #=> false
    #    File.lstat("alink").symlink?        #=> true
    def symlink?; end

    # Returns the numeric user id of the owner of <i>stat</i>.
    #
    #    File.stat("testfile").uid   #=> 501
    def uid; end

    # If <i>stat</i> is readable by others, returns an integer
    # representing the file permission bits of <i>stat</i>. Returns
    # <code>nil</code> otherwise. The meaning of the bits is platform
    # dependent; on Unix systems, see <code>stat(2)</code>.
    #
    #    m = File.stat("/etc/passwd").world_readable?  #=> 420
    #    sprintf("%o", m)                              #=> "644"
    def world_readable?; end

    # If <i>stat</i> is writable by others, returns an integer
    # representing the file permission bits of <i>stat</i>. Returns
    # <code>nil</code> otherwise. The meaning of the bits is platform
    # dependent; on Unix systems, see <code>stat(2)</code>.
    #
    #    m = File.stat("/tmp").world_writable?         #=> 511
    #    sprintf("%o", m)                              #=> "777"
    def world_writable?; end

    # Returns <code>true</code> if <i>stat</i> is writable by the
    # effective user id of this process.
    #
    #    File.stat("testfile").writable?   #=> true
    def writable?; end

    # Returns <code>true</code> if <i>stat</i> is writable by the real
    # user id of this process.
    #
    #    File.stat("testfile").writable_real?   #=> true
    def writable_real?; end

    # Returns <code>true</code> if <i>stat</i> is a zero-length file;
    # <code>false</code> otherwise.
    #
    #    File.stat("testfile").zero?   #=> false
    def zero?; end
  end
end
