# frozen_string_literal: true

class SystemCallError < StandardError
  # Return +true+ if the receiver is a generic +SystemCallError+, or
  # if the error numbers _self_ and _other_ are the same.
  def self.===(other) end

  # If _errno_ corresponds to a known system error code, constructs
  # the appropriate <code>Errno</code> class for that error, otherwise
  # constructs a generic <code>SystemCallError</code> object. The
  # error number is subsequently available via the <code>errno</code>
  # method.
  def initialize(msg, errno) end

  # Return this SystemCallError's error number.
  def errno; end
end
