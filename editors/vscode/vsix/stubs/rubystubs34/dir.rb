# frozen_string_literal: true

# An object of class \Dir represents a directory in the underlying file system.
#
# It consists mainly of:
#
# - A string _path_, given when the object is created,
#   that specifies a directory in the underlying file system;
#   method #path returns the path.
# - A collection of string <i>entry names</i>,
#   each of which is the name of a directory or file in the underlying file system;
#   the entry names may be retrieved
#   in an {array-like fashion}[rdoc-ref:Dir@Dir+As+Array-Like]
#   or in a {stream-like fashion}[rdoc-ref:Dir@Dir+As+Stream-Like].
#
# == About the Examples
#
# Some examples on this page use this simple file tree:
#
#   example/
#   ├── config.h
#   ├── lib/
#   │   ├── song/
#   │   │   └── karaoke.rb
#   │   └── song.rb
#   └── main.rb
#
# Others use the file tree for the
# {Ruby project itself}[https://github.com/ruby/ruby].
#
# == \Dir As \Array-Like
#
# A \Dir object is in some ways array-like:
#
# - It has instance methods #children, #each, and #each_child.
# - It includes {module Enumerable}[rdoc-ref:Enumerable@What-27s+Here].
#
# == \Dir As Stream-Like
#
# A \Dir object is in some ways stream-like.
#
# The stream is initially open for reading,
# but may be closed manually (using method #close),
# and will be closed on block exit if created by Dir.open called with a block.
# The closed stream may not be further manipulated,
# and may not be reopened.
#
# The stream has a _position_, which is the index of an entry in the directory:
#
# - The initial position is zero (before the first entry).
# - \Method #tell (aliased as #pos) returns the position.
# - \Method #pos= sets the position (but ignores a value outside the stream),
#   and returns the position.
# - \Method #seek is like #pos=, but returns +self+ (convenient for chaining).
# - \Method #read, if not at end-of-stream, reads the next entry and increments
#   the position;
#   if at end-of-stream, does not increment the position.
# - \Method #rewind sets the position to zero.
#
# Examples (using the {simple file tree}[rdoc-ref:Dir@About+the+Examples]):
#
#   dir = Dir.new('example') # => #<Dir:example>
#   dir.pos                  # => 0
#
#   dir.read # => "."
#   dir.read # => ".."
#   dir.read # => "config.h"
#   dir.read # => "lib"
#   dir.read # => "main.rb"
#   dir.pos  # => 5
#   dir.read # => nil
#   dir.pos  # => 5
#
#   dir.rewind # => #<Dir:example>
#   dir.pos    # => 0
#
#   dir.pos = 3 # => 3
#   dir.pos     # => 3
#
#   dir.seek(4) # => #<Dir:example>
#   dir.pos     # => 4
#
#   dir.close # => nil
#   dir.read  # Raises IOError.
#
# == What's Here
#
# First, what's elsewhere. \Class \Dir:
#
# - Inherits from {class Object}[rdoc-ref:Object@What-27s+Here].
# - Includes {module Enumerable}[rdoc-ref:Enumerable@What-27s+Here],
#   which provides dozens of additional methods.
#
# Here, class \Dir provides methods that are useful for:
#
# - {Reading}[rdoc-ref:Dir@Reading]
# - {Setting}[rdoc-ref:Dir@Setting]
# - {Querying}[rdoc-ref:Dir@Querying]
# - {Iterating}[rdoc-ref:Dir@Iterating]
# - {Other}[rdoc-ref:Dir@Other]
#
# === Reading
#
# - #close: Closes the directory stream for +self+.
# - #pos=: Sets the position in the directory stream for +self+.
# - #read: Reads and returns the next entry in the directory stream for +self+.
# - #rewind: Sets the position in the directory stream for +self+ to the first entry.
# - #seek: Sets the position in the directory stream for +self+
#   the entry at the given offset.
#
# === Setting
#
# - ::chdir: Changes the working directory of the current process
#   to the given directory.
# - ::chroot: Changes the file-system root for the current process
#   to the given directory.
#
# === Querying
#
# - ::[]: Same as ::glob without the ability to pass flags.
# - ::children: Returns an array of names of the children
#   (both files and directories) of the given directory,
#   but not including <tt>.</tt> or <tt>..</tt>.
# - ::empty?: Returns whether the given path is an empty directory.
# - ::entries: Returns an array of names of the children
#   (both files and directories) of the given directory,
#   including <tt>.</tt> and <tt>..</tt>.
# - ::exist?: Returns whether the given path is a directory.
# - ::getwd (aliased as #pwd): Returns the path to the current working directory.
# - ::glob: Returns an array of file paths matching the given pattern and flags.
# - ::home: Returns the home directory path for a given user or the current user.
# - #children: Returns an array of names of the children
#   (both files and directories) of +self+,
#   but not including <tt>.</tt> or <tt>..</tt>.
# - #fileno: Returns the integer file descriptor for +self+.
# - #path (aliased as #to_path): Returns the path used to create +self+.
# - #tell (aliased as #pos): Returns the integer position
#   in the directory stream for +self+.
#
# === Iterating
#
# - ::each_child: Calls the given block with each entry in the given directory,
#   but not including <tt>.</tt> or <tt>..</tt>.
# - ::foreach: Calls the given block with each entry in the given directory,
#   including <tt>.</tt> and <tt>..</tt>.
# - #each: Calls the given block with each entry in +self+,
#   including <tt>.</tt> and <tt>..</tt>.
# - #each_child: Calls the given block with each entry in +self+,
#   but not including <tt>.</tt> or <tt>..</tt>.
#
# === Other
#
# - ::mkdir: Creates a directory at the given path, with optional permissions.
# - ::new: Returns a new \Dir for the given path, with optional encoding.
# - ::open: Same as ::new, but if a block is given, yields the \Dir to the block,
#   closing it upon block exit.
# - ::unlink (aliased as ::delete and ::rmdir): Removes the given directory.
# - #inspect: Returns a string description of +self+.
class Dir
  include Enumerable

  # Calls Dir.glob with argument +patterns+
  # and the values of keyword arguments +base+ and +sort+;
  # returns the array of selected entry names.
  def self.[](*patterns, base: nil, sort: true) end

  # Changes the current working directory.
  #
  # With argument +new_dirpath+ and no block,
  # changes to the given +dirpath+:
  #
  #   Dir.pwd         # => "/example"
  #   Dir.chdir('..') # => 0
  #   Dir.pwd         # => "/"
  #
  # With no argument and no block:
  #
  # - Changes to the value of environment variable +HOME+ if defined.
  # - Otherwise changes to the value of environment variable +LOGDIR+ if defined.
  # - Otherwise makes no change.
  #
  # With argument +new_dirpath+ and a block, temporarily changes the working directory:
  #
  # - Calls the block with the argument.
  # - Changes to the given directory.
  # - Executes the block (yielding the new path).
  # - Restores the previous working directory.
  # - Returns the block's return value.
  #
  # Example:
  #
  #   Dir.chdir('/var/spool/mail')
  #   Dir.pwd   # => "/var/spool/mail"
  #   Dir.chdir('/tmp') do
  #     Dir.pwd # => "/tmp"
  #   end
  #   Dir.pwd   # => "/var/spool/mail"
  #
  # With no argument and a block,
  # calls the block with the current working directory (string)
  # and returns the block's return value.
  #
  # Calls to \Dir.chdir with blocks may be nested:
  #
  #   Dir.chdir('/var/spool/mail')
  #   Dir.pwd     # => "/var/spool/mail"
  #   Dir.chdir('/tmp') do
  #     Dir.pwd   # => "/tmp"
  #     Dir.chdir('/usr') do
  #       Dir.pwd # => "/usr"
  #     end
  #     Dir.pwd   # => "/tmp"
  #   end
  #   Dir.pwd     # => "/var/spool/mail"
  #
  # In a multi-threaded program an error is raised if a thread attempts
  # to open a +chdir+ block while another thread has one open,
  # or a call to +chdir+ without a block occurs inside
  # a block passed to +chdir+ (even in the same thread).
  #
  # Raises an exception if the target directory does not exist.
  def self.chdir(...) end

  # Returns an array of the entry names in the directory at +dirpath+
  # except for <tt>'.'</tt> and <tt>'..'</tt>;
  # sets the given encoding onto each returned entry name:
  #
  #   Dir.children('/example') # => ["config.h", "lib", "main.rb"]
  #   Dir.children('/example').first.encoding
  #   # => #<Encoding:UTF-8>
  #   Dir.children('/example', encoding: 'US-ASCII').first.encoding
  #   # => #<Encoding:US-ASCII>
  #
  # See {String Encoding}[rdoc-ref:encodings.rdoc@String+Encoding].
  #
  # Raises an exception if the directory does not exist.
  def self.children(...) end

  # Changes the root directory of the calling process to that specified in +dirpath+.
  # The new root directory is used for pathnames beginning with <tt>'/'</tt>.
  # The root directory is inherited by all children of the calling process.
  #
  # Only a privileged process may call +chroot+.
  #
  # See {Linux chroot}[https://man7.org/linux/man-pages/man2/chroot.2.html].
  def self.chroot(dirpath) end

  # Removes the directory at +dirpath+ from the underlying file system:
  #
  #   Dir.rmdir('foo') # => 0
  #
  # Raises an exception if the directory is not empty.
  def self.delete(p1) end

  # Like Dir.foreach, except that entries <tt>'.'</tt> and <tt>'..'</tt>
  # are not included.
  def self.each_child(...) end

  # Returns whether +dirpath+ specifies an empty directory:
  #
  #   dirpath = '/tmp/foo'
  #   Dir.mkdir(dirpath)
  #   Dir.empty?(dirpath)            # => true
  #   Dir.empty?('/example')         # => false
  #   Dir.empty?('/example/main.rb') # => false
  #
  # Raises an exception if +dirpath+ does not specify a directory or file
  # in the underlying file system.
  def self.empty?(dirpath) end

  # Returns an array of the entry names in the directory at +dirpath+;
  # sets the given encoding onto each returned entry name:
  #
  #   Dir.entries('/example') # => ["config.h", "lib", "main.rb", "..", "."]
  #   Dir.entries('/example').first.encoding
  #   # => #<Encoding:UTF-8>
  #   Dir.entries('/example', encoding: 'US-ASCII').first.encoding
  #   # => #<Encoding:US-ASCII>
  #
  # See {String Encoding}[rdoc-ref:encodings.rdoc@String+Encoding].
  #
  # Raises an exception if the directory does not exist.
  def self.entries(dirname, encoding: 'UTF-8') end

  # Returns whether +dirpath+ is a directory in the underlying file system:
  #
  #   Dir.exist?('/example')         # => true
  #   Dir.exist?('/nosuch')          # => false
  #   Dir.exist?('/example/main.rb') # => false
  #
  # Same as File.directory?.
  def self.exist?(dirpath) end

  # Changes the current working directory to the directory
  # specified by the integer file descriptor +fd+.
  #
  # When passing a file descriptor over a UNIX socket or to a child process,
  # using +fchdir+ instead of +chdir+ avoids the
  # {time-of-check to time-of-use vulnerability}[https://en.wikipedia.org/wiki/Time-of-check_to_time-of-use]
  #
  # With no block, changes to the directory given by +fd+:
  #
  #   Dir.chdir('/var/spool/mail')
  #   Dir.pwd # => "/var/spool/mail"
  #   dir  = Dir.new('/usr')
  #   fd = dir.fileno
  #   Dir.fchdir(fd)
  #   Dir.pwd # => "/usr"
  #
  # With a block, temporarily changes the working directory:
  #
  # - Calls the block with the argument.
  # - Changes to the given directory.
  # - Executes the block (yields no args).
  # - Restores the previous working directory.
  # - Returns the block's return value.
  #
  # Example:
  #
  #   Dir.chdir('/var/spool/mail')
  #   Dir.pwd # => "/var/spool/mail"
  #   dir  = Dir.new('/tmp')
  #   fd = dir.fileno
  #   Dir.fchdir(fd) do
  #     Dir.pwd # => "/tmp"
  #   end
  #   Dir.pwd # => "/var/spool/mail"
  #
  # This method uses the
  # {fchdir()}[https://www.man7.org/linux/man-pages/man3/fchdir.3p.html]
  # function defined by POSIX 2008;
  # the method is not implemented on non-POSIX platforms (raises NotImplementedError).
  #
  # Raises an exception if the file descriptor is not valid.
  #
  # In a multi-threaded program an error is raised if a thread attempts
  # to open a +chdir+ block while another thread has one open,
  # or a call to +chdir+ without a block occurs inside
  # a block passed to +chdir+ (even in the same thread).
  def self.fchdir(fd) end

  # Returns a new \Dir object representing the directory specified by the given
  # integer directory file descriptor +fd+:
  #
  #   d0 = Dir.new('..')
  #   d1 = Dir.for_fd(d0.fileno)
  #
  # Note that the returned +d1+ does not have an associated path:
  #
  #   d0.path # => '..'
  #   d1.path # => nil
  #
  # This method uses the
  # {fdopendir()}[https://www.man7.org/linux/man-pages/man3/fdopendir.3p.html]
  # function defined by POSIX 2008;
  # the method is not implemented on non-POSIX platforms (raises NotImplementedError).
  def self.for_fd(fd) end

  # Calls the block with each entry name in the directory at +dirpath+;
  # sets the given encoding onto each passed +entry_name+:
  #
  #   Dir.foreach('/example') {|entry_name| p entry_name }
  #
  # Output:
  #
  #   "config.h"
  #   "lib"
  #   "main.rb"
  #   ".."
  #   "."
  #
  # Encoding:
  #
  #   Dir.foreach('/example') {|entry_name| p entry_name.encoding; break }
  #   Dir.foreach('/example', encoding: 'US-ASCII') {|entry_name| p entry_name.encoding; break }
  #
  # Output:
  #
  #   #<Encoding:UTF-8>
  #   #<Encoding:US-ASCII>
  #
  # See {String Encoding}[rdoc-ref:encodings.rdoc@String+Encoding].
  #
  # Returns an enumerator if no block is given.
  def self.foreach(dirpath, encoding: 'UTF-8') end

  # Returns the path to the current working directory:
  #
  #   Dir.chdir("/tmp") # => 0
  #   Dir.pwd           # => "/tmp"
  def self.getwd; end

  # Forms an array _entry_names_ of the entry names selected by the arguments.
  #
  # Argument +patterns+ is a string pattern or an array of string patterns;
  # note that these are not regexps; see below.
  #
  # Notes for the following examples:
  #
  # - <tt>'*'</tt> is the pattern that matches any entry name
  #   except those that begin with <tt>'.'</tt>.
  # - We use method Array#take to shorten returned arrays
  #   that otherwise would be very large.
  #
  # With no block, returns array _entry_names_;
  # example (using the {simple file tree}[rdoc-ref:Dir@About+the+Examples]):
  #
  #   Dir.glob('*') # => ["config.h", "lib", "main.rb"]
  #
  # With a block, calls the block with each of the _entry_names_
  # and returns +nil+:
  #
  #   Dir.glob('*') {|entry_name| puts entry_name } # => nil
  #
  # Output:
  #
  #   config.h
  #   lib
  #   main.rb
  #
  # If optional keyword argument +flags+ is given,
  # the value modifies the matching; see below.
  #
  # If optional keyword argument +base+ is given,
  # its value specifies the base directory.
  # Each pattern string specifies entries relative to the base directory;
  # the default is <tt>'.'</tt>.
  # The base directory is not prepended to the entry names in the result:
  #
  #   Dir.glob(pattern, base: 'lib').take(5)
  #   # => ["abbrev.gemspec", "abbrev.rb", "base64.gemspec", "base64.rb", "benchmark.gemspec"]
  #   Dir.glob(pattern, base: 'lib/irb').take(5)
  #   # => ["cmd", "color.rb", "color_printer.rb", "completion.rb", "context.rb"]
  #
  # If optional keyword +sort+ is given, its value specifies whether
  # the array is to be sorted; the default is +true+.
  # Passing value +false+ with that keyword disables sorting
  # (though the underlying file system may already have sorted the array).
  #
  # <b>Patterns</b>
  #
  # Each pattern string is expanded
  # according to certain metacharacters;
  # examples below use the {Ruby file tree}[rdoc-ref:Dir@About+the+Examples]:
  #
  # - <tt>'*'</tt>: Matches any substring in an entry name,
  #   similar in meaning to regexp <tt>/.*/mx</tt>;
  #   may be restricted by other values in the pattern strings:
  #
  #   - <tt>'*'</tt> matches all entry names:
  #
  #       Dir.glob('*').take(3)  # => ["BSDL", "CONTRIBUTING.md", "COPYING"]
  #
  #   - <tt>'c*'</tt> matches entry names beginning with <tt>'c'</tt>:
  #
  #       Dir.glob('c*').take(3) # => ["CONTRIBUTING.md", "COPYING", "COPYING.ja"]
  #
  #   - <tt>'*c'</tt> matches entry names ending with <tt>'c'</tt>:
  #
  #       Dir.glob('*c').take(3) # => ["addr2line.c", "array.c", "ast.c"]
  #
  #   - <tt>'\*c\*'</tt> matches entry names that contain <tt>'c'</tt>,
  #     even at the beginning or end:
  #
  #       Dir.glob('*c*').take(3) # => ["CONTRIBUTING.md", "COPYING", "COPYING.ja"]
  #
  #   Does not match Unix-like hidden entry names ("dot files").
  #   To include those in the matched entry names,
  #   use flag IO::FNM_DOTMATCH or something like <tt>'{*,.*}'</tt>.
  #
  #  - <tt>'**'</tt>: Matches entry names recursively
  #    if followed by  the slash character <tt>'/'</tt>:
  #
  #      Dir.glob('**/').take(3) # => ["basictest/", "benchmark/", "benchmark/gc/"]
  #
  #    If the string pattern contains other characters
  #    or is not followed by a slash character,
  #    it is equivalent to <tt>'*'</tt>.
  #
  # - <tt>'?'</tt> Matches any single character;
  #   similar in meaning to regexp <tt>/./</tt>:
  #
  #     Dir.glob('io.?') # => ["io.c"]
  #
  # - <tt>'[_set_]'</tt>: Matches any one character in the string _set_;
  #   behaves like a {Regexp character class}[rdoc-ref:Regexp@Character+Classes],
  #   including set negation (<tt>'[^a-z]'</tt>):
  #
  #     Dir.glob('*.[a-z][a-z]').take(3)
  #     # => ["CONTRIBUTING.md", "COPYING.ja", "KNOWNBUGS.rb"]
  #
  # - <tt>'{_abc_,_xyz_}'</tt>:
  #   Matches either string _abc_ or string _xyz_;
  #   behaves like {Regexp alternation}[rdoc-ref:Regexp@Alternation]:
  #
  #     Dir.glob('{LEGAL,BSDL}') # => ["LEGAL", "BSDL"]
  #
  #   More than two alternatives may be given.
  #
  # - <tt>\\</tt>: Escapes the following metacharacter.
  #
  #   Note that on Windows, the backslash character may not be used
  #   in a string pattern:
  #   <tt>Dir['c:\\foo*']</tt> will not work, use <tt>Dir['c:/foo*']</tt> instead.
  #
  # More examples (using the {simple file tree}[rdoc-ref:Dir@About+the+Examples]):
  #
  #   # We're in the example directory.
  #   File.basename(Dir.pwd) # => "example"
  #   Dir.glob('config.?')              # => ["config.h"]
  #   Dir.glob('*.[a-z][a-z]')          # => ["main.rb"]
  #   Dir.glob('*.[^r]*')               # => ["config.h"]
  #   Dir.glob('*.{rb,h}')              # => ["main.rb", "config.h"]
  #   Dir.glob('*')                     # => ["config.h", "lib", "main.rb"]
  #   Dir.glob('*', File::FNM_DOTMATCH) # => [".", "config.h", "lib", "main.rb"]
  #   Dir.glob(["*.rb", "*.h"])         # => ["main.rb", "config.h"]
  #
  #   Dir.glob('**/*.rb')
  #   => ["lib/song/karaoke.rb", "lib/song.rb", "main.rb"]
  #
  #   Dir.glob('**/*.rb', base: 'lib')  #   => ["song/karaoke.rb", "song.rb"]
  #
  #   Dir.glob('**/lib')                # => ["lib"]
  #
  #   Dir.glob('**/lib/**/*.rb')        # => ["lib/song/karaoke.rb", "lib/song.rb"]
  #
  #   Dir.glob('**/lib/*.rb')           # => ["lib/song.rb"]
  #
  # <b>Flags</b>
  #
  # If optional keyword argument +flags+ is given (the default is zero -- no flags),
  # its value should be the bitwise OR of one or more of the constants
  # defined in module File::Constants.
  #
  # Example:
  #
  #   flags = File::FNM_EXTGLOB | File::FNM_DOTMATCH
  #
  # Specifying flags can extend, restrict, or otherwise modify the matching.
  #
  # The flags for this method (other constants in File::Constants do not apply):
  #
  # - File::FNM_DOTMATCH:
  #   specifies that entry names beginning with <tt>'.'</tt>
  #   should be considered for matching:
  #
  #     Dir.glob('*').take(5)
  #     # => ["BSDL", "CONTRIBUTING.md", "COPYING", "COPYING.ja", "GPL"]
  #     Dir.glob('*', flags: File::FNM_DOTMATCH).take(5)
  #     # => [".", ".appveyor.yml", ".cirrus.yml", ".dir-locals.el", ".document"]
  #
  # - File::FNM_EXTGLOB:
  #   enables the pattern extension
  #   <tt>'{_a_,_b_}'</tt>, which matches pattern _a_ and pattern _b_;
  #   behaves like a
  #   {regexp union}[rdoc-ref:Regexp.union]
  #   (e.g., <tt>'(?:_a_|_b_)'</tt>):
  #
  #     pattern = '{LEGAL,BSDL}'
  #     Dir.glob(pattern)      # => ["LEGAL", "BSDL"]
  #
  # - File::FNM_NOESCAPE:
  #   specifies that escaping with the backslash character <tt>'\'</tt>
  #   is disabled; the character is not an escape character.
  #
  # - File::FNM_PATHNAME:
  #   specifies that metacharacters <tt>'*'</tt> and <tt>'?'</tt>
  #   do not match directory separators.
  #
  # - File::FNM_SHORTNAME:
  #   specifies that patterns may match short names if they exist; Windows only.
  def self.glob(pattern, flags = _, base: _) end

  # Returns the home directory path of the user specified with +user_name+
  # if it is not +nil+, or the current login user:
  #
  #   Dir.home         # => "/home/me"
  #   Dir.home('root') # => "/root"
  #
  # Raises ArgumentError if +user_name+ is not a user name.
  def self.home(user_name = nil) end

  # Creates a directory in the underlying file system
  # at +dirpath+ with the given +permissions+;
  # returns zero:
  #
  #   Dir.mkdir('foo')
  #   File.stat(Dir.new('foo')).mode.to_s(8)[1..4] # => "0755"
  #   Dir.mkdir('bar', 0644)
  #   File.stat(Dir.new('bar')).mode.to_s(8)[1..4] # => "0644"
  #
  # See {File Permissions}[rdoc-ref:File@File+Permissions].
  # Note that argument +permissions+ is ignored on Windows.
  def self.mkdir(string, *permissions) end

  # Creates a new \Dir object _dir_ for the directory at +dirpath+.
  #
  # With no block, the method equivalent to Dir.new(dirpath, encoding):
  #
  #   Dir.open('.') # => #<Dir:.>
  #
  # With a block given, the block is called with the created _dir_;
  # on block exit _dir_ is closed and the block's value is returned:
  #
  #   Dir.open('.') {|dir| dir.inspect } # => "#<Dir:.>"
  #
  # The value given with optional keyword argument +encoding+
  # specifies the encoding for the directory entry names;
  # if +nil+ (the default), the file system's encoding is used:
  #
  #   Dir.open('.').read.encoding                       # => #<Encoding:UTF-8>
  #   Dir.open('.', encoding: 'US-ASCII').read.encoding # => #<Encoding:US-ASCII>
  def self.open(...) end

  # Returns the path to the current working directory:
  #
  #   Dir.chdir("/tmp") # => 0
  #   Dir.pwd           # => "/tmp"
  def self.pwd; end

  # Removes the directory at +dirpath+ from the underlying file system:
  #
  #   Dir.rmdir('foo') # => 0
  #
  # Raises an exception if the directory is not empty.
  def self.rmdir(dirpath) end

  # Removes the directory at +dirpath+ from the underlying file system:
  #
  #   Dir.rmdir('foo') # => 0
  #
  # Raises an exception if the directory is not empty.
  def self.unlink(p1) end

  # Returns a new \Dir object for the directory at +dirpath+:
  #
  #   Dir.new('.') # => #<Dir:.>
  #
  # The value given with optional keyword argument +encoding+
  # specifies the encoding for the directory entry names;
  # if +nil+ (the default), the file system's encoding is used:
  #
  #   Dir.new('.').read.encoding                       # => #<Encoding:UTF-8>
  #   Dir.new('.', encoding: 'US-ASCII').read.encoding # => #<Encoding:US-ASCII>
  def initialize(...) end

  # Changes the current working directory to +self+:
  #
  #   Dir.pwd # => "/"
  #   dir = Dir.new('example')
  #   dir.chdir
  #   Dir.pwd # => "/example"
  #
  # With a block, temporarily changes the working directory:
  #
  # - Calls the block.
  # - Changes to the given directory.
  # - Executes the block (yields no args).
  # - Restores the previous working directory.
  # - Returns the block's return value.
  #
  # Uses Dir.fchdir if available, and Dir.chdir if not, see those
  # methods for caveats.
  def chdir; end

  # Returns an array of the entry names in +self+
  # except for <tt>'.'</tt> and <tt>'..'</tt>:
  #
  #   dir = Dir.new('/example')
  #   dir.children # => ["config.h", "lib", "main.rb"]
  def children; end

  # Closes the stream in +self+, if it is open, and returns +nil+;
  # ignored if +self+ is already closed:
  #
  #   dir = Dir.new('example')
  #   dir.read     # => "."
  #   dir.close     # => nil
  #   dir.close     # => nil
  #   dir.read # Raises IOError.
  def close; end

  # Calls the block with each entry name in +self+:
  #
  #   Dir.new('example').each {|entry_name| p entry_name }
  #
  # Output:
  #
  #   "."
  #   ".."
  #   "config.h"
  #   "lib"
  #   "main.rb"
  #
  # With no block given, returns an Enumerator.
  def each; end

  # Calls the block with each entry name in +self+
  # except <tt>'.'</tt> and <tt>'..'</tt>:
  #
  #   dir = Dir.new('/example')
  #   dir.each_child {|entry_name| p entry_name }
  #
  # Output:
  #
  #   "config.h"
  #   "lib"
  #   "main.rb"
  #
  # If no block is given, returns an enumerator.
  def each_child; end

  # Returns the file descriptor used in <em>dir</em>.
  #
  #   d = Dir.new('..')
  #   d.fileno # => 8
  #
  # This method uses the
  # {dirfd()}[https://www.man7.org/linux/man-pages/man3/dirfd.3.html]
  # function defined by POSIX 2008;
  # the method is not implemented on non-POSIX platforms (raises NotImplementedError).
  def fileno; end

  # Returns a string description of +self+:
  #
  #   Dir.new('example').inspect # => "#<Dir:example>"
  def inspect; end

  # Returns the +dirpath+ string that was used to create +self+
  # (or +nil+ if created by method Dir.for_fd):
  #
  #   Dir.new('example').path # => "example"
  def path; end
  alias to_path path

  # Sets the position in +self+ and returns +position+.
  # The value of +position+ should have been returned from an earlier call to #tell;
  # if not, the return values from subsequent calls to #read are unspecified.
  #
  # See {Dir As Stream-Like}[rdoc-ref:Dir@Dir+As+Stream-Like].
  #
  # Examples:
  #
  #   dir = Dir.new('example')
  #   dir.pos      # => 0
  #   dir.pos = 3  # => 3
  #   dir.pos      # => 3
  #   dir.pos = 30 # => 30
  #   dir.pos      # => 5
  def pos=(position) end

  # Reads and returns the next entry name from +self+;
  # returns +nil+ if at end-of-stream;
  # see {Dir As Stream-Like}[rdoc-ref:Dir@Dir+As+Stream-Like]:
  #
  #   dir = Dir.new('example')
  #   dir.read # => "."
  #   dir.read # => ".."
  #   dir.read # => "config.h"
  def read; end

  # Sets the position in +self+ to zero;
  # see {Dir As Stream-Like}[rdoc-ref:Dir@Dir+As+Stream-Like]:
  #
  #   dir = Dir.new('example')
  #   dir.read    # => "."
  #   dir.read    # => ".."
  #   dir.pos     # => 2
  #   dir.rewind  # => #<Dir:example>
  #   dir.pos     # => 0
  def rewind; end

  # Sets the position in +self+ and returns +self+.
  # The value of +position+ should have been returned from an earlier call to #tell;
  # if not, the return values from subsequent calls to #read are unspecified.
  #
  # See {Dir As Stream-Like}[rdoc-ref:Dir@Dir+As+Stream-Like].
  #
  # Examples:
  #
  #   dir = Dir.new('example')
  #   dir.pos      # => 0
  #   dir.seek(3)  # => #<Dir:example>
  #   dir.pos      # => 3
  #   dir.seek(30) # => #<Dir:example>
  #   dir.pos      # => 5
  def seek(position) end

  # Returns the current position of +self+;
  # see {Dir As Stream-Like}[rdoc-ref:Dir@Dir+As+Stream-Like]:
  #
  #   dir = Dir.new('example')
  #   dir.tell  # => 0
  #   dir.read  # => "."
  #   dir.tell  # => 1
  def tell; end
  alias pos tell
end
