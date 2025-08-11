# frozen_string_literal: true

# Objects of class Dir are directory streams representing
# directories in the underlying file system. They provide a variety
# of ways to list directories and their contents. See also File.
#
# The directory used in these examples contains the two regular files
# (<code>config.h</code> and <code>main.rb</code>), the parent
# directory (<code>..</code>), and the directory itself
# (<code>.</code>).
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

  # Equivalent to calling
  # <code>Dir.glob([</code><i>string,...</i><code>], 0)</code>.
  def self.[](*args, base: nil, sort: true) end

  # Changes the current working directory of the process to the given
  # string. When called without an argument, changes the directory to
  # the value of the environment variable <code>HOME</code>, or
  # <code>LOGDIR</code>. SystemCallError (probably Errno::ENOENT) if
  # the target directory does not exist.
  #
  # If a block is given, it is passed the name of the new current
  # directory, and the block is executed with that as the current
  # directory. The original working directory is restored when the block
  # exits. The return value of <code>chdir</code> is the value of the
  # block. <code>chdir</code> blocks can be nested, but in a
  # multi-threaded program an error will be raised if a thread attempts
  # to open a <code>chdir</code> block while another thread has one
  # open or a call to <code>chdir</code> without a block occurs inside
  # a block passed to <code>chdir</code> (even in the same thread).
  #
  #    Dir.chdir("/var/spool/mail")
  #    puts Dir.pwd
  #    Dir.chdir("/tmp") do
  #      puts Dir.pwd
  #      Dir.chdir("/usr") do
  #        puts Dir.pwd
  #      end
  #      puts Dir.pwd
  #    end
  #    puts Dir.pwd
  #
  # <em>produces:</em>
  #
  #    /var/spool/mail
  #    /tmp
  #    /usr
  #    /tmp
  #    /var/spool/mail
  def self.chdir(*string) end

  # Returns an array containing all of the filenames except for "."
  # and ".." in the given directory. Will raise a SystemCallError if
  # the named directory doesn't exist.
  #
  # The optional <i>encoding</i> keyword argument specifies the encoding of the
  # directory. If not specified, the filesystem encoding is used.
  #
  #    Dir.children("testdir")   #=> ["config.h", "main.rb"]
  def self.children(...) end

  # Changes this process's idea of the file system root. Only a
  # privileged process may make this call. Not available on all
  # platforms. On Unix systems, see <code>chroot(2)</code> for more
  # information.
  def self.chroot(string) end

  # Deletes the named directory. Raises a subclass of SystemCallError
  # if the directory isn't empty.
  def self.delete(string) end

  # Calls the block once for each entry except for "." and ".." in the
  # named directory, passing the filename of each entry as a parameter
  # to the block.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    Dir.each_child("testdir") {|x| puts "Got #{x}" }
  #
  # <em>produces:</em>
  #
  #    Got config.h
  #    Got main.rb
  def self.each_child(...) end

  # Returns <code>true</code> if the named file is an empty directory,
  # <code>false</code> if it is not a directory or non-empty.
  def self.empty?(path_name) end

  # Returns an array containing all of the filenames in the given
  # directory. Will raise a SystemCallError if the named directory
  # doesn't exist.
  #
  # The optional <i>encoding</i> keyword argument specifies the encoding of the
  # directory. If not specified, the filesystem encoding is used.
  #
  #    Dir.entries("testdir")   #=> [".", "..", "config.h", "main.rb"]
  def self.entries(...) end

  # Returns <code>true</code> if the named file is a directory,
  # <code>false</code> otherwise.
  def self.exist?(file_name) end

  # Calls the block once for each entry in the named directory, passing
  # the filename of each entry as a parameter to the block.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    Dir.foreach("testdir") {|x| puts "Got #{x}" }
  #
  # <em>produces:</em>
  #
  #    Got .
  #    Got ..
  #    Got config.h
  #    Got main.rb
  def self.foreach(...) end

  # Returns the path to the current working directory of this process as
  # a string.
  #
  #    Dir.chdir("/tmp")   #=> 0
  #    Dir.getwd           #=> "/tmp"
  #    Dir.pwd             #=> "/tmp"
  def self.getwd; end

  # Expands +pattern+, which is a pattern string or an Array of pattern
  # strings, and returns an array containing the matching filenames.
  # If a block is given, calls the block once for each matching filename,
  # passing the filename as a parameter to the block.
  #
  # The optional +base+ keyword argument specifies the base directory for
  # interpreting relative pathnames instead of the current working directory.
  # As the results are not prefixed with the base directory name in this
  # case, you will need to prepend the base directory name if you want real
  # paths.
  #
  # The results which matched single wildcard or character set are sorted in
  # binary ascending order, unless +false+ is given as the optional +sort+
  # keyword argument.  The order of an Array of pattern strings and braces
  # are preserved.
  #
  # Note that the pattern is not a regexp, it's closer to a shell glob.
  # See File::fnmatch for the meaning of the +flags+ parameter.
  # Case sensitivity depends on your system (+File::FNM_CASEFOLD+ is ignored).
  #
  # <code>*</code>::
  #   Matches any file. Can be restricted by other values in the glob.
  #   Equivalent to <code>/.*/mx</code> in regexp.
  #
  #   <code>*</code>::     Matches all files
  #   <code>c*</code>::    Matches all files beginning with <code>c</code>
  #   <code>*c</code>::    Matches all files ending with <code>c</code>
  #   <code>\*c\*</code>:: Match all files that have <code>c</code> in them
  #                        (including at the beginning or end).
  #
  #   Note, this will not match Unix-like hidden files (dotfiles).  In order
  #   to include those in the match results, you must use the
  #   File::FNM_DOTMATCH flag or something like <code>"{*,.*}"</code>.
  #
  # <code>**</code>::
  #   Matches directories recursively if followed by <code>/</code>.  If
  #   this path segment contains any other characters, it is the same as the
  #   usual <code>*</code>.
  #
  # <code>?</code>::
  #   Matches any one character. Equivalent to <code>/.{1}/</code> in regexp.
  #
  # <code>[set]</code>::
  #   Matches any one character in +set+.  Behaves exactly like character sets
  #   in Regexp, including set negation (<code>[^a-z]</code>).
  #
  # <code>{p,q}</code>::
  #   Matches either literal <code>p</code> or literal <code>q</code>.
  #   Equivalent to pattern alternation in regexp.
  #
  #   Matching literals may be more than one character in length.  More than
  #   two literals may be specified.
  #
  # <code>\\</code>::
  #   Escapes the next metacharacter.
  #
  #   Note that this means you cannot use backslash on windows as part of a
  #   glob, i.e.  <code>Dir["c:\\foo*"]</code> will not work, use
  #   <code>Dir["c:/foo*"]</code> instead.
  #
  # Examples:
  #
  #    Dir["config.?"]                     #=> ["config.h"]
  #    Dir.glob("config.?")                #=> ["config.h"]
  #    Dir.glob("*.[a-z][a-z]")            #=> ["main.rb"]
  #    Dir.glob("*.[^r]*")                 #=> ["config.h"]
  #    Dir.glob("*.{rb,h}")                #=> ["main.rb", "config.h"]
  #    Dir.glob("*")                       #=> ["config.h", "main.rb"]
  #    Dir.glob("*", File::FNM_DOTMATCH)   #=> [".", "config.h", "main.rb"]
  #    Dir.glob(["*.rb", "*.h"])           #=> ["main.rb", "config.h"]
  #
  #    Dir.glob("**/*.rb")                 #=> ["main.rb",
  #                                        #    "lib/song.rb",
  #                                        #    "lib/song/karaoke.rb"]
  #
  #    Dir.glob("**/*.rb", base: "lib")    #=> ["song.rb",
  #                                        #    "song/karaoke.rb"]
  #
  #    Dir.glob("**/lib")                  #=> ["lib"]
  #
  #    Dir.glob("**/lib/**/*.rb")          #=> ["lib/song.rb",
  #                                        #    "lib/song/karaoke.rb"]
  #
  #    Dir.glob("**/lib/*.rb")             #=> ["lib/song.rb"]
  def self.glob(pattern, flags = _, base: _) end

  # Returns the home directory of the current user or the named user
  # if given.
  def self.home(...) end

  # Makes a new directory named by <i>string</i>, with permissions
  # specified by the optional parameter <i>anInteger</i>. The
  # permissions may be modified by the value of File::umask, and are
  # ignored on NT. Raises a SystemCallError if the directory cannot be
  # created. See also the discussion of permissions in the class
  # documentation for File.
  #
  #   Dir.mkdir(File.join(Dir.home, ".foo"), 0700) #=> 0
  def self.mkdir(string, *permissions) end

  # The optional <i>encoding</i> keyword argument specifies the encoding of the directory.
  # If not specified, the filesystem encoding is used.
  #
  # With no block, <code>open</code> is a synonym for Dir::new. If a
  # block is present, it is passed <i>aDir</i> as a parameter. The
  # directory is closed at the end of the block, and Dir::open returns
  # the value of the block.
  def self.open(...) end

  # Returns the path to the current working directory of this process as
  # a string.
  #
  #    Dir.chdir("/tmp")   #=> 0
  #    Dir.getwd           #=> "/tmp"
  #    Dir.pwd             #=> "/tmp"
  def self.pwd; end

  # Deletes the named directory. Raises a subclass of SystemCallError
  # if the directory isn't empty.
  def self.rmdir(string) end

  # Deletes the named directory. Raises a subclass of SystemCallError
  # if the directory isn't empty.
  def self.unlink(string) end

  # Returns a new directory object for the named directory.
  #
  # The optional <i>encoding</i> keyword argument specifies the encoding of the directory.
  # If not specified, the filesystem encoding is used.
  def initialize(...) end

  # Returns an array containing all of the filenames except for "."
  # and ".." in this directory.
  #
  #    d = Dir.new("testdir")
  #    d.children   #=> ["config.h", "main.rb"]
  def children; end

  # Closes the directory stream.
  # Calling this method on closed Dir object is ignored since Ruby 2.3.
  #
  #    d = Dir.new("testdir")
  #    d.close   #=> nil
  def close; end

  # Calls the block once for each entry in this directory, passing the
  # filename of each entry as a parameter to the block.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    d = Dir.new("testdir")
  #    d.each  {|x| puts "Got #{x}" }
  #
  # <em>produces:</em>
  #
  #    Got .
  #    Got ..
  #    Got config.h
  #    Got main.rb
  def each; end

  # Calls the block once for each entry except for "." and ".." in
  # this directory, passing the filename of each entry as a parameter
  # to the block.
  #
  # If no block is given, an enumerator is returned instead.
  #
  #    d = Dir.new("testdir")
  #    d.each_child  {|x| puts "Got #{x}" }
  #
  # <em>produces:</em>
  #
  #    Got config.h
  #    Got main.rb
  def each_child; end

  # Returns the file descriptor used in <em>dir</em>.
  #
  #    d = Dir.new("..")
  #    d.fileno   #=> 8
  #
  # This method uses dirfd() function defined by POSIX 2008.
  # NotImplementedError is raised on other platforms, such as Windows,
  # which doesn't provide the function.
  def fileno; end

  # Return a string describing this Dir object.
  def inspect; end

  # Returns the path parameter passed to <em>dir</em>'s constructor.
  #
  #    d = Dir.new("..")
  #    d.path   #=> ".."
  def path; end
  alias to_path path

  # Synonym for Dir#seek, but returns the position parameter.
  #
  #    d = Dir.new("testdir")   #=> #<Dir:0x401b3c40>
  #    d.read                   #=> "."
  #    i = d.pos                #=> 12
  #    d.read                   #=> ".."
  #    d.pos = i                #=> 12
  #    d.read                   #=> ".."
  def pos=(integer) end

  # Reads the next entry from <em>dir</em> and returns it as a string.
  # Returns <code>nil</code> at the end of the stream.
  #
  #    d = Dir.new("testdir")
  #    d.read   #=> "."
  #    d.read   #=> ".."
  #    d.read   #=> "config.h"
  def read; end

  # Repositions <em>dir</em> to the first entry.
  #
  #    d = Dir.new("testdir")
  #    d.read     #=> "."
  #    d.rewind   #=> #<Dir:0x401b3fb0>
  #    d.read     #=> "."
  def rewind; end

  # Seeks to a particular location in <em>dir</em>. <i>integer</i>
  # must be a value returned by Dir#tell.
  #
  #    d = Dir.new("testdir")   #=> #<Dir:0x401b3c40>
  #    d.read                   #=> "."
  #    i = d.tell               #=> 12
  #    d.read                   #=> ".."
  #    d.seek(i)                #=> #<Dir:0x401b3c40>
  #    d.read                   #=> ".."
  def seek(integer) end

  # Returns the current position in <em>dir</em>. See also Dir#seek.
  #
  #    d = Dir.new("testdir")
  #    d.tell   #=> 0
  #    d.read   #=> "."
  #    d.tell   #=> 12
  def tell; end
  alias pos tell
end
