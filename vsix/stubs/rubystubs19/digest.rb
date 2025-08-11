# frozen_string_literal: true

# This module provides a framework for message digest libraries.
#
# You may want to look at OpenSSL::Digest as it supports support more
# algorithms.
#
# A cryptographic hash function is a procedure that takes data and return a
# fixed bit string : the hash value, also known as _digest_. Hash functions
# are also called one-way functions, it is easy to compute a digest from
# a message, but it is infeasible to generate a message from a digest.
#
# == Example
#
#   require 'digest'
#
#   # Compute a complete digest
#   sha256 = Digest::SHA256.new
#   digest = sha256.digest message
#
#   # Compute digest by chunks
#   sha256 = Digest::SHA256.new
#   sha256.update message1
#   sha256 << message2 # << is an alias for update
#
#   digest = sha256.digest
#
# == Digest algorithms
#
# Different digest algorithms (or hash functions) are available :
#
# HMAC::
#   See FIPS PUB 198 The Keyed-Hash Message Authentication Code (HMAC)
# RIPEMD-160::
#   (as Digest::RMD160) see
#   http://homes.esat.kuleuven.be/~bosselae/ripemd160.html
# SHA1::
#   See FIPS 180 Secure Hash Standard
# SHA2 family::
#   See FIPS 180 Secure Hash Standard which defines the following algorithms:
#   * SHA512
#   * SHA384
#   * SHA256
#
# The latest versions of the FIPS publications can be found here:
# http://csrc.nist.gov/publications/PubsFIPS.html
#
# Additionally Digest::BubbleBabble encodes a digest as a sequence of
# consonants and vowels which is more recognizable and comparable than a
# hexadecimal digest.  See http://en.wikipedia.org/wiki/Bubblebabble
module Digest
  # Generates a hex-encoded version of a given _string_.
  def self.hexencode(string) end

  private

  # Generates a hex-encoded version of a given _string_.
  def hexencode(string) end

  # This abstract class provides a common interface to message digest
  # implementation classes written in C.
  class Base < Class
  end

  # This module stands as a base class for digest implementation
  # classes.
  class Class
    include Digest::Instance

    # Returns the hash value of a given _string_.  This is equivalent to
    # Digest::Class.new(*parameters).digest(string), where extra
    # _parameters_, if any, are passed through to the constructor and the
    # _string_ is passed to #digest().
    def self.digest(string, *parameters) end

    # Returns the hex-encoded hash value of a given _string_.  This is
    # almost equivalent to
    # Digest.hexencode(Digest::Class.new(*parameters).digest(string)).
    def self.hexdigest(*args) end
  end

  # This module provides instance methods for a digest implementation
  # object to calculate message digest values.
  module Instance
    # If a string is given, checks whether it is equal to the hex-encoded
    # hash value of the digest object.  If another digest instance is
    # given, checks whether they have the same hash value.  Otherwise
    # returns false.
    def ==(*several_variants) end

    # Returns the block length of the digest.
    #
    # This method is overridden by each implementation subclass.
    def block_length; end

    # If none is given, returns the resulting hash value of the digest,
    # keeping the digest's state.
    #
    # If a _string_ is given, returns the hash value for the given
    # _string_, resetting the digest to the initial state before and
    # after the process.
    def digest(*several_variants) end

    # Returns the resulting hash value and resets the digest to the
    # initial state.
    def digest!; end

    # Returns the length of the hash value of the digest.
    #
    # This method should be overridden by each implementation subclass.
    # If not, digest_obj.digest().length() is returned.
    def digest_length; end

    # If none is given, returns the resulting hash value of the digest in
    # a hex-encoded form, keeping the digest's state.
    #
    # If a _string_ is given, returns the hash value for the given
    # _string_ in a hex-encoded form, resetting the digest to the initial
    # state before and after the process.
    def hexdigest(*several_variants) end

    # Returns the resulting hash value in a hex-encoded form and resets
    # the digest to the initial state.
    def hexdigest!; end

    # Creates a printable version of the digest object.
    def inspect; end

    # Returns digest_obj.digest_length().
    def length; end
    alias size length

    # Returns a new, initialized copy of the digest object.  Equivalent
    # to digest_obj.clone().reset().
    def new; end

    # Resets the digest to the initial state and returns self.
    #
    # This method is overridden by each implementation subclass.
    def reset; end

    # Returns digest_obj.hexdigest().
    def to_s; end

    # Updates the digest using a given _string_ and returns self.
    #
    # The update() method and the left-shift operator are overridden by
    # each implementation subclass. (One should be an alias for the
    # other)
    def update(string) end
    alias << update

    private

    # Finishes the digest and returns the resulting hash value.
    #
    # This method is overridden by each implementation subclass and often
    # made private, because some of those subclasses may leave internal
    # data uninitialized.  Do not call this method from outside.  Use
    # #digest!() instead, which ensures that internal data be reset for
    # security reasons.
    def finish; end
  end
end
