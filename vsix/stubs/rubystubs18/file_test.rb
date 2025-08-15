# frozen_string_literal: true

# <code>FileTest</code> implements file test operations similar to
# those used in <code>File::Stat</code>. It exists as a standalone
# module, and its methods are also insinuated into the <code>File</code>
# class. (Note that this is not done by inclusion: the interpreter cheats).
module FileTest
  # Returns <code>true</code> if the named file is a block device.
  def blockdev?(file_name) end

  # Returns <code>true</code> if the named file is a character device.
  def chardev?(file_name) end

  # Returns <code>true</code> if the named file is a directory,
  # <code>false</code> otherwise.
  #
  #    File.directory?(".")
  def directory?(file_name) end

  # Returns <code>true</code> if the named file is executable by the effective
  # user id of this process.
  def executable?(file_name) end

  # Returns <code>true</code> if the named file is executable by the real
  # user id of this process.
  def executable_real?(file_name) end

  # Return <code>true</code> if the named file exists.
  def exist?(file_name) end
  alias exists? exist?

  # Returns <code>true</code> if the named file exists and is a
  # regular file.
  def file?(file_name) end

  # Returns <code>true</code> if the named file exists and the
  # effective group id of the calling process is the owner of
  # the file. Returns <code>false</code> on Windows.
  def grpowned?(file_name) end

  # Returns <code>true</code> if the named files are identical.
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
  def identical?(file1, file2) end

  # Returns <code>true</code> if the named file exists and the
  # effective used id of the calling process is the owner of
  # the file.
  def owned?(file_name) end

  # Returns <code>true</code> if the named file is a pipe.
  def pipe?(file_name) end

  # Returns <code>true</code> if the named file is readable by the effective
  # user id of this process.
  def readable?(file_name) end

  # Returns <code>true</code> if the named file is readable by the real
  # user id of this process.
  def readable_real?(file_name) end

  # Returns <code>true</code> if the named file has the setgid bit set.
  def setgid?(file_name) end

  # Returns <code>true</code> if the named file has the setuid bit set.
  def setuid?(file_name) end

  # Returns the size of <code>file_name</code>.
  def size(file_name) end

  # Returns +nil+ if +file_name+ doesn't exist or has zero size, the size of the
  # file otherwise.
  def size?(file_name) end

  # Returns <code>true</code> if the named file is a socket.
  def socket?(file_name) end

  # Returns <code>true</code> if the named file has the sticky bit set.
  def sticky?(file_name) end

  # Returns <code>true</code> if the named file is a symbolic link.
  def symlink?(file_name) end

  # Returns <code>true</code> if the named file is writable by the effective
  # user id of this process.
  def writable?(file_name) end

  # Returns <code>true</code> if the named file is writable by the real
  # user id of this process.
  def writable_real?(file_name) end

  # Returns <code>true</code> if the named file exists and has
  # a zero size.
  def zero?(file_name) end
end
