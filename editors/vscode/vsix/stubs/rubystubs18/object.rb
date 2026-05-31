# frozen_string_literal: true

class Object < BasicObject
  include Kernel

  ARGF = _
  ARGV = _
  DATA = _
  ENV = _
  FALSE = _
  IPsocket = _
  MatchingData = _
  NIL = _
  PLATFORM = _
  RELEASE_DATE = _
  RUBY_COPYRIGHT = _
  RUBY_DESCRIPTION = _
  RUBY_PATCHLEVEL = _
  RUBY_PLATFORM = _
  RUBY_RELEASE_DATE = _
  RUBY_VERSION = _
  SOCKSsocket = _
  STDERR = _
  # constants to hold original stdin/stdout/stderr
  STDIN = _
  STDOUT = _
  TCPserver = _
  TCPsocket = _
  TOPLEVEL_BINDING = _
  TRUE = _
  UDPsocket = _
  UNIXserver = _
  UNIXsocket = _
  # obsolete constants
  VERSION = _

  # Not documented
  def initialize; end
end
