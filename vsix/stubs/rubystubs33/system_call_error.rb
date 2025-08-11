# frozen_string_literal: true

# SystemCallError is the base class for all low-level
# platform-dependent errors.
#
# The errors available on the current platform are subclasses of
# SystemCallError and are defined in the Errno module.
#
#    File.open("does/not/exist")
#
# <em>raises the exception:</em>
#
#    Errno::ENOENT: No such file or directory - does/not/exist
class SystemCallError < StandardError
  # Return +true+ if the receiver is a generic +SystemCallError+, or
  # if the error numbers +self+ and _other_ are the same.
  def self.===(other) end

  # If _errno_ corresponds to a known system error code, constructs the
  # appropriate Errno class for that error, otherwise constructs a
  # generic SystemCallError object. The error number is subsequently
  # available via the #errno method.
  def initialize(msg, errno) end

  # Return this SystemCallError's error number.
  def errno; end
end
