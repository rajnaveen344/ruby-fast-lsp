# frozen_string_literal: true

# Pathname represents the name of a file or directory on the filesystem,
# but not the file itself.
#
# The pathname depends on the Operating System: Unix, Windows, etc.
# This library works with pathnames of local OS, however non-Unix pathnames
# are supported experimentally.
#
# A Pathname can be relative or absolute.  It's not until you try to
# reference the file that it even matters whether the file exists or not.
#
# Pathname is immutable.  It has no method for destructive update.
#
# The goal of this class is to manipulate file path information in a neater
# way than standard Ruby provides.  The examples below demonstrate the
# difference.
#
# *All* functionality from File, FileTest, and some from Dir and FileUtils is
# included, in an unsurprising way.  It is essentially a facade for all of
# these, and more.
#
# == Examples
#
# === Example 1: Using Pathname
#
#   require 'pathname'
#   pn = Pathname.new("/usr/bin/ruby")
#   size = pn.size              # 27662
#   isdir = pn.directory?       # false
#   dir  = pn.dirname           # Pathname:/usr/bin
#   base = pn.basename          # Pathname:ruby
#   dir, base = pn.split        # [Pathname:/usr/bin, Pathname:ruby]
#   data = pn.read
#   pn.open { |f| _ }
#   pn.each_line { |line| _ }
#
# === Example 2: Using standard Ruby
#
#   pn = "/usr/bin/ruby"
#   size = File.size(pn)        # 27662
#   isdir = File.directory?(pn) # false
#   dir  = File.dirname(pn)     # "/usr/bin"
#   base = File.basename(pn)    # "ruby"
#   dir, base = File.split(pn)  # ["/usr/bin", "ruby"]
#   data = File.read(pn)
#   File.open(pn) { |f| _ }
#   File.foreach(pn) { |line| _ }
#
# === Example 3: Special features
#
#   p1 = Pathname.new("/usr/lib")   # Pathname:/usr/lib
#   p2 = p1 + "ruby/1.8"            # Pathname:/usr/lib/ruby/1.8
#   p3 = p1.parent                  # Pathname:/usr
#   p4 = p2.relative_path_from(p3)  # Pathname:lib/ruby/1.8
#   pwd = Pathname.pwd              # Pathname:/home/gavin
#   pwd.absolute?                   # true
#   p5 = Pathname.new "."           # Pathname:.
#   p5 = p5 + "music/../articles"   # Pathname:music/../articles
#   p5.cleanpath                    # Pathname:articles
#   p5.realpath                     # Pathname:/home/gavin/articles
#   p5.children                     # [Pathname:/home/gavin/articles/linux, ...]
#
# == Breakdown of functionality
#
# === Core methods
#
# These methods are effectively manipulating a String, because that's
# all a path is.  None of these access the file system except for
# #mountpoint?, #children, #each_child, #realdirpath and #realpath.
#
# - +
# - #join
# - #parent
# - #root?
# - #absolute?
# - #relative?
# - #relative_path_from
# - #each_filename
# - #cleanpath
# - #realpath
# - #realdirpath
# - #children
# - #each_child
# - #mountpoint?
#
# === File status predicate methods
#
# These methods are a facade for FileTest:
# - #blockdev?
# - #chardev?
# - #directory?
# - #executable?
# - #executable_real?
# - #exist?
# - #file?
# - #grpowned?
# - #owned?
# - #pipe?
# - #readable?
# - #world_readable?
# - #readable_real?
# - #setgid?
# - #setuid?
# - #size
# - #size?
# - #socket?
# - #sticky?
# - #symlink?
# - #writable?
# - #world_writable?
# - #writable_real?
# - #zero?
#
# === File property and manipulation methods
#
# These methods are a facade for File:
# - #atime
# - #ctime
# - #mtime
# - #chmod(mode)
# - #lchmod(mode)
# - #chown(owner, group)
# - #lchown(owner, group)
# - #fnmatch(pattern, *args)
# - #fnmatch?(pattern, *args)
# - #ftype
# - #make_link(old)
# - #open(*args, &block)
# - #readlink
# - #rename(to)
# - #stat
# - #lstat
# - #make_symlink(old)
# - #truncate(length)
# - #utime(atime, mtime)
# - #basename(*args)
# - #dirname
# - #extname
# - #expand_path(*args)
# - #split
#
# === Directory methods
#
# These methods are a facade for Dir:
# - Pathname.glob(*args)
# - Pathname.getwd / Pathname.pwd
# - #rmdir
# - #entries
# - #each_entry(&block)
# - #mkdir(*args)
# - #opendir(*args)
#
# === IO
#
# These methods are a facade for IO:
# - #each_line(*args, &block)
# - #read(*args)
# - #binread(*args)
# - #readlines(*args)
# - #sysopen(*args)
#
# === Utilities
#
# These methods are a mixture of Find, FileUtils, and others:
# - #find(&block)
# - #mkpath
# - #rmtree
# - #unlink / #delete
#
# == Method documentation
#
# As the above section shows, most of the methods in Pathname are facades.  The
# documentation for these methods generally just says, for instance, "See
# FileTest.writable?", as you should be familiar with the original method
# anyway, and its documentation (e.g. through +ri+) will contain more
# information.  In some cases, a brief description will follow.
class Pathname
  # Returns the current working directory as a Pathname.
  #
  #      Pathname.getwd
  #          #=> #<Pathname:/home/zzak/projects/ruby>
  #
  # See Dir.getwd.
  def self.getwd; end

  # Returns or yields Pathname objects.
  #
  #  Pathname.glob("config/" "*.rb")
  #      #=> [#<Pathname:config/environment.rb>, #<Pathname:config/routes.rb>, ..]
  #
  # See Dir.glob.
  def self.glob(p1, p2 = v2) end

  # Returns the current working directory as a Pathname.
  #
  #      Pathname.getwd
  #          #=> #<Pathname:/home/zzak/projects/ruby>
  #
  # See Dir.getwd.
  def self.pwd; end

  # Create a Pathname object from the given String (or String-like object).
  # If +path+ contains a NULL character (<tt>\0</tt>), an ArgumentError is raised.
  def initialize(p1) end

  # Provides a case-sensitive comparison operator for pathnames.
  #
  #     Pathname.new('/usr') <=> Pathname.new('/usr/bin')
  #         #=> -1
  #     Pathname.new('/usr/bin') <=> Pathname.new('/usr/bin')
  #         #=> 0
  #     Pathname.new('/usr/bin') <=> Pathname.new('/USR/BIN')
  #         #=> 1
  #
  # It will return +-1+, +0+ or +1+ depending on the value of the left argument
  # relative to the right argument. Or it will return +nil+ if the arguments
  # are not comparable.
  def <=>(other) end

  # Compare this pathname with +other+.  The comparison is string-based.
  # Be aware that two different paths (<tt>foo.txt</tt> and <tt>./foo.txt</tt>)
  # can refer to the same file.
  def ==(other) end
  alias === ==
  alias eql? ==

  # Returns the last access time for the file.
  #
  # See File.atime.
  def atime; end

  # Returns the last component of the path.
  #
  # See File.basename.
  def basename(p1 = v1) end

  # Returns all the bytes from the file, or the first +N+ if specified.
  #
  # See IO.binread.
  def binread(p1 = v1, p2 = v2) end

  # See FileTest.blockdev?.
  def blockdev?; end

  # See FileTest.chardev?.
  def chardev?; end

  # Changes file permissions.
  #
  # See File.chmod.
  def chmod; end

  # Change owner and group of the file.
  #
  # See File.chown.
  def chown; end

  # Returns the last change time, using directory information, not the file itself.
  #
  # See File.ctime.
  def ctime; end

  # See FileTest.directory?.
  def directory?; end

  # Returns all but the last component of the path.
  #
  # See File.dirname.
  def dirname; end

  # Iterates over the entries (files and subdirectories) in the directory,
  # yielding a Pathname object for each entry.
  def each_entry; end

  # Iterates over each line in the file and yields a String object for each.
  def each_line(*several_variants) end

  # Return the entries (files and subdirectories) in the directory, each as a
  # Pathname object.
  #
  # The results contains just the names in the directory, without any trailing
  # slashes or recursive look-up.
  #
  #   pp Pathname.new('/usr/local').entries
  #   #=> [#<Pathname:share>,
  #   #    #<Pathname:lib>,
  #   #    #<Pathname:..>,
  #   #    #<Pathname:include>,
  #   #    #<Pathname:etc>,
  #   #    #<Pathname:bin>,
  #   #    #<Pathname:man>,
  #   #    #<Pathname:games>,
  #   #    #<Pathname:.>,
  #   #    #<Pathname:sbin>,
  #   #    #<Pathname:src>]
  #
  # The result may contain the current directory <code>#<Pathname:.></code> and
  # the parent directory <code>#<Pathname:..></code>.
  #
  # If you don't want +.+ and +..+ and
  # want directories, consider Pathname#children.
  def entries; end

  # See FileTest.executable?.
  def executable?; end

  # See FileTest.executable_real?.
  def executable_real?; end

  # See FileTest.exist?.
  def exist?; end

  # Returns the absolute path for the file.
  #
  # See File.expand_path.
  def expand_path(p1 = v1) end

  # Returns the file's extension.
  #
  # See File.extname.
  def extname; end

  # See FileTest.file?.
  def file?; end

  # Return +true+ if the receiver matches the given pattern.
  #
  # See File.fnmatch.
  def fnmatch(pattern, *flags) end
  alias fnmatch? fnmatch

  # Freezes this Pathname.
  #
  # See Object.freeze.
  def freeze; end

  # Returns "type" of file ("file", "directory", etc).
  #
  # See File.ftype.
  def ftype; end

  # See FileTest.grpowned?.
  def grpowned?; end

  # Same as Pathname.chmod, but does not follow symbolic links.
  #
  # See File.lchmod.
  def lchmod; end

  # Same as Pathname.chown, but does not follow symbolic links.
  #
  # See File.lchown.
  def lchown; end

  # See File.lstat.
  def lstat; end

  # Creates a hard link at _pathname_.
  #
  # See File.link.
  def make_link(old) end

  # Creates a symbolic link.
  #
  # See File.symlink.
  def make_symlink(old) end

  # Create the referenced directory.
  #
  # See Dir.mkdir.
  def mkdir(p1 = v1) end

  # Returns the last modified time of the file.
  #
  # See File.mtime.
  def mtime; end

  # Opens the file for reading or writing.
  #
  # See File.open.
  def open(p1 = v1, p2 = v2, p3 = v3) end

  # Opens the referenced directory.
  #
  # See Dir.open.
  def opendir; end

  # See FileTest.owned?.
  def owned?; end

  # See FileTest.pipe?.
  def pipe?; end

  # Returns all data from the file, or the first +N+ bytes if specified.
  #
  # See IO.read.
  def read(*several_variants) end

  # See FileTest.readable?.
  def readable?; end

  # See FileTest.readable_real?.
  def readable_real?; end

  # Returns all the lines from the file.
  #
  # See IO.readlines.
  def readlines(*several_variants) end

  # Read symbolic link.
  #
  # See File.readlink.
  def readlink; end

  # Returns the real (absolute) pathname of +self+ in the actual filesystem.
  #
  # Does not contain symlinks or useless dots, +..+ and +.+.
  #
  # The last component of the real pathname can be nonexistent.
  def realdirpath(p1 = v1) end

  # Returns the real (absolute) pathname for +self+ in the actual
  # filesystem.
  #
  # Does not contain symlinks or useless dots, +..+ and +.+.
  #
  # All components of the pathname must exist when this method is
  # called.
  def realpath(p1 = v1) end

  # Rename the file.
  #
  # See File.rename.
  def rename(p1) end

  # Remove the referenced directory.
  #
  # See Dir.rmdir.
  def rmdir; end

  # See FileTest.setgid?.
  def setgid?; end

  # See FileTest.setuid?.
  def setuid?; end

  # See FileTest.size.
  def size; end

  # See FileTest.size?.
  def size?; end

  # See FileTest.socket?.
  def socket?; end

  # Returns the #dirname and the #basename in an Array.
  #
  # See File.split.
  def split; end

  # Returns a File::Stat object.
  #
  # See File.stat.
  def stat; end

  # See FileTest.sticky?.
  def sticky?; end

  # Return a pathname which is substituted by String#sub.
  #
  #      path1 = Pathname.new('/usr/bin/perl')
  #      path1.sub('perl', 'ruby')
  #          #=> #<Pathname:/usr/bin/ruby>
  def sub(*args) end

  # Return a pathname with +repl+ added as a suffix to the basename.
  #
  # If self has no extension part, +repl+ is appended.
  #
  #      Pathname.new('/usr/bin/shutdown').sub_ext('.rb')
  #          #=> #<Pathname:/usr/bin/shutdown.rb>
  def sub_ext(p1) end

  # See FileTest.symlink?.
  def symlink?; end

  # See IO.sysopen.
  def sysopen(p1 = v1, p2 = v2) end

  # Taints this Pathname.
  #
  # See Object.taint.
  def taint; end

  # Return the path as a String.
  #
  # to_path is implemented so Pathname objects are usable with File.open, etc.
  def to_s; end
  alias to_path to_s

  # Truncates the file to +length+ bytes.
  #
  # See File.truncate.
  def truncate(p1) end

  # Removes a file or directory, using File.unlink if +self+ is a file, or
  # Dir.unlink as necessary.
  def unlink; end
  alias delete unlink

  # Untaints this Pathname.
  #
  # See Object.untaint.
  def untaint; end

  # Update the access and modification times of the file.
  #
  # See File.utime.
  def utime(p1, p2) end

  # See FileTest.world_readable?.
  def world_readable?; end

  # See FileTest.world_writable?.
  def world_writable?; end

  # See FileTest.writable?.
  def writable?; end

  # See FileTest.writable_real?.
  def writable_real?; end

  # See FileTest.zero?.
  def zero?; end
end
