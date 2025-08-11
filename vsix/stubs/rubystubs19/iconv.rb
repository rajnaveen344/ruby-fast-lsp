# frozen_string_literal: true

# == Summary
#
# Ruby extension for charset conversion.
#
# == Abstract
#
# Iconv is a wrapper class for the UNIX 95 <tt>iconv()</tt> function family,
# which translates string between various encoding systems.
#
# See Open Group's on-line documents for more details.
# * <tt>iconv.h</tt>:       http://www.opengroup.org/onlinepubs/007908799/xsh/iconv.h.html
# * <tt>iconv_open()</tt>:  http://www.opengroup.org/onlinepubs/007908799/xsh/iconv_open.html
# * <tt>iconv()</tt>:       http://www.opengroup.org/onlinepubs/007908799/xsh/iconv.html
# * <tt>iconv_close()</tt>: http://www.opengroup.org/onlinepubs/007908799/xsh/iconv_close.html
#
# Which coding systems are available is platform-dependent.
#
# == Examples
#
# 1. Simple conversion between two charsets.
#
#      converted_text = Iconv.conv('iso-8859-15', 'utf-8', text)
#
# 2. Instantiate a new Iconv and use method Iconv#iconv.
#
#      cd = Iconv.new(to, from)
#      begin
#        input.each { |s| output << cd.iconv(s) }
#        output << cd.iconv(nil)                   # Don't forget this!
#      ensure
#        cd.close
#      end
#
# 3. Invoke Iconv.open with a block.
#
#      Iconv.open(to, from) do |cd|
#        input.each { |s| output << cd.iconv(s) }
#        output << cd.iconv(nil)
#      end
#
# 4. Shorthand for (3).
#
#      Iconv.iconv(to, from, *input.to_a)
#
# == Attentions
#
# Even if some extentions of implementation dependent are useful,
# DON'T USE those extentions in libraries and scripts to widely distribute.
# If you want to use those feature, use String#encode.
class Iconv < Data
  # Returns the map from canonical name to system dependent name.
  def self.charset_map; end

  # Shorthand for
  #   Iconv.iconv(to, from, str).join
  # See Iconv.iconv.
  def self.conv(to, from, str) end

  # Returns available iconvctl() method list.
  def self.ctlmethods; end

  # Shorthand for
  #   Iconv.open(to, from) { |cd|
  #     (strs + [nil]).collect { |s| cd.iconv(s) }
  #   }
  #
  # === Parameters
  #
  # <tt>to, from</tt>:: see Iconv.new
  # <tt>strs</tt>:: strings to be converted
  #
  # === Exceptions
  #
  # Exceptions thrown by Iconv.new, Iconv.open and Iconv#iconv.
  def self.iconv(to, from, *strs) end

  # Iterates each alias sets.
  def self.list; end

  # Equivalent to Iconv.new except that when it is called with a block, it
  # yields with the new instance and closes it, and returns the result which
  # returned from the block.
  def self.open(to, from) end

  # Creates new code converter from a coding-system designated with +from+
  # to another one designated with +to+.
  #
  # === Parameters
  #
  # +to+::   encoding name for destination
  # +from+:: encoding name for source
  # +options+:: options for converter
  #
  # === Exceptions
  #
  # TypeError::       if +to+ or +from+ aren't String
  # InvalidEncoding:: if designated converter couldn't find out
  # SystemCallError:: if <tt>iconv_open(3)</tt> fails
  def initialize(to, from, *options) end

  # Finishes conversion.
  #
  # After calling this, calling Iconv#iconv will cause an exception, but
  # multiple calls of #close are guaranteed to end successfully.
  #
  # Returns a string containing the byte sequence to change the output buffer to
  # its initial shift state.
  def close; end

  # Equivalent to
  #
  #   iconv(nil, str..., nil).join
  def conv(*args) end

  # Sets discard_ilseq flag.
  def discard_ilseq=(flag) end

  # Returns discard_ilseq flag.
  def discard_ilseq?; end

  # Converts string and returns the result.
  # * If +str+ is a String, converts <tt>str[start, length]</tt> and returns the converted string.
  # * If +str+ is +nil+, places converter itself into initial shift state and
  #   just returns a string containing the byte sequence to change the output
  #   buffer to its initial shift state.
  # * Otherwise, raises an exception.
  #
  # === Parameters
  #
  # str::    string to be converted, or nil
  # start::  starting offset
  # length:: conversion length; nil or -1 means whole the string from start
  #
  # === Exceptions
  #
  # * IconvIllegalSequence
  # * IconvInvalidCharacter
  # * IconvOutOfRange
  #
  # === Examples
  #
  # See the Iconv documentation.
  def iconv(str, start = 0, length = -1) end

  # Sets transliterate flag.
  def transliterate=(flag) end

  # Returns transliterate flag.
  def transliterate?; end

  # Returns trivial flag.
  def trivial?; end

  # Detected a bug of underlying iconv(3) libray.
  # * returns an error without setting errno properly
  class BrokenLibrary < RuntimeError
    include Iconv::Failure
  end

  # Base attributes for Iconv exceptions.
  module Failure
    # Creates new code converter from a coding-system designated with +from+
    # to another one designated with +to+.
    #
    # === Parameters
    #
    # +to+::   encoding name for destination
    # +from+:: encoding name for source
    # +options+:: options for converter
    #
    # === Exceptions
    #
    # TypeError::       if +to+ or +from+ aren't String
    # InvalidEncoding:: if designated converter couldn't find out
    # SystemCallError:: if <tt>iconv_open(3)</tt> fails
    def initialize(to, from, *options) end

    # Returns substring of the original string passed to Iconv that starts at the
    # character caused the exception.
    def failed; end

    # Returns inspected string like as: #<_class_: _success_, _failed_>
    def inspect; end

    # Returns string(s) translated successfully until the exception occurred.
    # * In the case of failure occurred within Iconv.iconv, returned
    #   value is an array of strings translated successfully preceding
    #   failure and the last element is string on the way.
    def success; end
  end

  # Input conversion stopped due to an input byte that does not belong to
  # the input codeset, or the output codeset does not contain the
  # character.
  class IllegalSequence < ArgumentError
    include Iconv::Failure
  end

  # Input conversion stopped due to an incomplete character or shift
  # sequence at the end of the input buffer.
  class InvalidCharacter < ArgumentError
    include Iconv::Failure
  end

  # Requested coding-system is not available on this system.
  class InvalidEncoding < ArgumentError
    include Iconv::Failure
  end

  # Iconv library internal error.  Must not occur.
  class OutOfRange < RuntimeError
    include Iconv::Failure
  end
end
