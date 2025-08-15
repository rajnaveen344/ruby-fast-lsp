# frozen_string_literal: true

# Objects of class <code>Dir</code> are directory streams representing
# directories in the underlying file system. They provide a variety of
# ways to list directories and their contents. See also
# <code>File</code>.
#
# The directory used in these examples contains the two regular files
# (<code>config.h</code> and <code>main.rb</code>), the parent
# directory (<code>..</code>), and the directory itself
# (<code>.</code>).
class Dir
  include Enumerable

  # Equivalent to calling
  # <code>Dir.glob([</code><i>string,...</i><code>],0)</code>.
  def self.[](*args) end

  # Changes the current working directory of the process to the given
  # string. When called without an argument, changes the directory to
  # the value of the environment variable <code>HOME</code>, or
  # <code>LOGDIR</code>. <code>SystemCallError</code> (probably
  # <code>Errno::ENOENT</code>) if the target directory does not exist.
  #
  # If a block is given, it is passed the name of the new current
  # directory, and the block is executed with that as the current
  # directory. The original working directory is restored when the block
  # exits. The return value of <code>chdir</code> is the value of the
  # block. <code>chdir</code> blocks can be nested, but in a
  # multi-threaded program an error will be raised if a thread attempts
  # to open a <code>chdir</code> block while another thread has one
  # open.
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

  # Changes this process's idea of the file system root. Only a
  # privileged process may make this call. Not available on all
  # platforms. On Unix systems, see <code>chroot(2)</code> for more
  # information.
  def self.chroot(string) end

  # Deletes the named directory. Raises a subclass of
  # <code>SystemCallError</code> if the directory isn't empty.
  def self.delete(string) end

  # Returns <code>true</code> if the named file is an empty directory,
  # <code>false</code> if it is not a directory or non-empty.
  def self.empty?(path_name) end

  # Returns an array containing all of the filenames in the given
  # directory. Will raise a <code>SystemCallError</code> if the named
  # directory doesn't exist.
  #
  # The optional <i>enc</i> argument specifies the encoding of the directory.
  # If not specified, the filesystem encoding is used.
  #
  #    Dir.entries("testdir")   #=> [".", "..", "config.h", "main.rb"]
  def self.entries(*several_variants) end

  # Returns <code>true</code> if the named file is a directory,
  # <code>false</code> otherwise.
  def self.exist?(file_name) end

  # Deprecated method. Don't use.
  def self.exists?(file_name) end

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
  def self.foreach(*several_variants) end

  # Returns the path to the current working directory of this process as
  # a string.
  #
  #    Dir.chdir("/tmp")   #=> 0
  #    Dir.getwd           #=> "/tmp"
  #    Dir.pwd             #=> "/tmp"
  def self.getwd; end

  # Expands +pattern+, which is an Array of patterns or a pattern String, and
  # returns the results as +matches+ or as arguments given to the block.
  #
  # Note that this pattern is not a regexp, it's closer to a shell glob.  See
  # File::fnmatch for the meaning of the +flags+ parameter.  Note that case
  # sensitivity depends on your system (so File::FNM_CASEFOLD is ignored), as
  # does the order in which the results are returned.
  #
  # <code>*</code>::
  #   Matches any file. Can be restricted by other values in the glob.
  #   Equivalent to <code>/ .* /x</code> in regexp.
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
  #   Matches directories recursively.
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
  # <code> \\ </code>::
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
  #    Dir.glob("*", File::FNM_DOTMATCH)   #=> [".", "..", "config.h", "main.rb"]
  #
  #    rbfiles = File.join("**", "*.rb")
  #    Dir.glob(rbfiles)                   #=> ["main.rb",
  #                                        #    "lib/song.rb",
  #                                        #    "lib/song/karaoke.rb"]
  #    libdirs = File.join("**", "lib")
  #    Dir.glob(libdirs)                   #=> ["lib"]
  #
  #    librbfiles = File.join("**", "lib", "**", "*.rb")
  #    Dir.glob(librbfiles)                #=> ["lib/song.rb",
  #                                        #    "lib/song/karaoke.rb"]
  #
  #    librbfiles = File.join("**", "lib", "*.rb")
  #    Dir.glob(librbfiles)                #=> ["lib/song.rb"]
  def self.glob(pattern, *flags) end

  # Returns the home directory of the current user or the named user
  # if given.
  def self.home(*several_variants) end

  # Makes a new directory named by <i>string</i>, with permissions
  # specified by the optional parameter <i>anInteger</i>. The
  # permissions may be modified by the value of
  # <code>File::umask</code>, and are ignored on NT. Raises a
  # <code>SystemCallError</code> if the directory cannot be created. See
  # also the discussion of permissions in the class documentation for
  # <code>File</code>.
  #
  #   Dir.mkdir(File.join(Dir.home, ".foo"), 0700) #=> 0
  def self.mkdir(string, *permissions_int) end

  # The optional <i>enc</i> argument specifies the encoding of the directory.
  # If not specified, the filesystem encoding is used.
  #
  # With no block, <code>open</code> is a synonym for
  # <code>Dir::new</code>. If a block is present, it is passed
  # <i>aDir</i> as a parameter. The directory is closed at the end of
  # the block, and <code>Dir::open</code> returns the value of the
  # block.
  def self.open(*several_variants) end

  # Returns the path to the current working directory of this process as
  # a string.
  #
  #    Dir.chdir("/tmp")   #=> 0
  #    Dir.getwd           #=> "/tmp"
  #    Dir.pwd             #=> "/tmp"
  def self.pwd; end

  # Deletes the named directory. Raises a subclass of
  # <code>SystemCallError</code> if the directory isn't empty.
  def self.rmdir(string) end

  # Deletes the named directory. Raises a subclass of
  # <code>SystemCallError</code> if the directory isn't empty.
  def self.unlink(string) end

  # Returns a new directory object for the named directory.
  #
  # The optional <i>enc</i> argument specifies the encoding of the directory.
  # If not specified, the filesystem encoding is used.
  def initialize(*several_variants) end

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

  # Synonym for <code>Dir#seek</code>, but returns the position
  # parameter.
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
  # must be a value returned by <code>Dir#tell</code>.
  #
  #    d = Dir.new("testdir")   #=> #<Dir:0x401b3c40>
  #    d.read                   #=> "."
  #    i = d.tell               #=> 12
  #    d.read                   #=> ".."
  #    d.seek(i)                #=> #<Dir:0x401b3c40>
  #    d.read                   #=> ".."
  def seek(integer) end

  # Returns the current position in <em>dir</em>. See also
  # <code>Dir#seek</code>.
  #
  #    d = Dir.new("testdir")
  #    d.tell   #=> 0
  #    d.read   #=> "."
  #    d.tell   #=> 12
  def tell; end
  alias pos tell
end
