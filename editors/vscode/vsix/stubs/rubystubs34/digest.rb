# frozen_string_literal: true

# This module provides a framework for message digest libraries.
#
# You may want to look at OpenSSL::Digest as it supports more algorithms.
#
# A cryptographic hash function is a procedure that takes data and returns a
# fixed bit string: the hash value, also known as _digest_. Hash functions
# are also called one-way functions, it is easy to compute a digest from
# a message, but it is infeasible to generate a message from a digest.
#
# == Examples
#
#   require 'digest'
#
#   # Compute a complete digest
#   Digest::SHA256.digest 'message'       #=> "\xABS\n\x13\xE4Y..."
#
#   sha256 = Digest::SHA256.new
#   sha256.digest 'message'               #=> "\xABS\n\x13\xE4Y..."
#
#   # Other encoding formats
#   Digest::SHA256.hexdigest 'message'    #=> "ab530a13e459..."
#   Digest::SHA256.base64digest 'message' #=> "q1MKE+RZFJgr..."
#
#   # Compute digest by chunks
#   md5 = Digest::MD5.new
#   md5.update 'message1'
#   md5 << 'message2'                     # << is an alias for update
#
#   md5.hexdigest                         #=> "94af09c09bb9..."
#
#   # Compute digest for a file
#   sha256 = Digest::SHA256.file 'testfile'
#   sha256.hexdigest
#
# Additionally digests can be encoded in "bubble babble" format as a sequence
# of consonants and vowels which is more recognizable and comparable than a
# hexadecimal digest.
#
#   require 'digest/bubblebabble'
#
#   Digest::SHA256.bubblebabble 'message' #=> "xopoh-fedac-fenyh-..."
#
# See the bubble babble specification at
# http://web.mit.edu/kenta/www/one/bubblebabble/spec/jrtrjwzi/draft-huima-01.txt.
#
# == Digest algorithms
#
# Different digest algorithms (or hash functions) are available:
#
# MD5::
#  See RFC 1321 The MD5 Message-Digest Algorithm
# RIPEMD-160::
#   As Digest::RMD160.
#   See http://homes.esat.kuleuven.be/~bosselae/ripemd160.html.
# SHA1::
#   See FIPS 180 Secure Hash Standard.
# SHA2 family::
#   See FIPS 180 Secure Hash Standard which defines the following algorithms:
#   * SHA512
#   * SHA384
#   * SHA256
#
# The latest versions of the FIPS publications can be found here:
# http://csrc.nist.gov/publications/PubsFIPS.html.
module Digest
  # Returns a BubbleBabble encoded version of a given _string_.
  def self.bubblebabble(string) end

  # Generates a hex-encoded version of a given _string_.
  def self.hexencode(string) end

  private

  # Returns a BubbleBabble encoded version of a given _string_.
  def bubblebabble(string) end

  # Generates a hex-encoded version of a given _string_.
  def hexencode(string) end

  # This abstract class provides a common interface to message digest
  # implementation classes written in C.
  #
  # ==Write a Digest subclass in C
  # Digest::Base provides a common interface to message digest
  # classes written in C. These classes must provide a struct
  # of type rb_digest_metadata_t:
  #  typedef int (*rb_digest_hash_init_func_t)(void *);
  #  typedef void (*rb_digest_hash_update_func_t)(void *, unsigned char *, size_t);
  #  typedef int (*rb_digest_hash_finish_func_t)(void *, unsigned char *);
  #
  #  typedef struct {
  #    int api_version;
  #    size_t digest_len;
  #    size_t block_len;
  #    size_t ctx_size;
  #    rb_digest_hash_init_func_t init_func;
  #    rb_digest_hash_update_func_t update_func;
  #    rb_digest_hash_finish_func_t finish_func;
  #  } rb_digest_metadata_t;
  #
  # This structure must be set as an instance variable named +metadata+
  # (without the +@+ in front of the name). By example:
  #   static const rb_digest_metadata_t sha1 = {
  #      RUBY_DIGEST_API_VERSION,
  #      SHA1_DIGEST_LENGTH,
  #      SHA1_BLOCK_LENGTH,
  #      sizeof(SHA1_CTX),
  #      (rb_digest_hash_init_func_t)SHA1_Init,
  #      (rb_digest_hash_update_func_t)SHA1_Update,
  #      (rb_digest_hash_finish_func_t)SHA1_Finish,
  #  };
  #
  #  rb_ivar_set(cDigest_SHA1, rb_intern("metadata"),
  #              rb_digest_make_metadata(&sha1));
  class Base < Class
    # Return the block length of the digest in bytes.
    def block_length; end

    # Return the length of the hash value in bytes.
    def digest_length; end

    # Reset the digest to its initial state and return +self+.
    def reset; end

    # Update the digest using given _string_ and return +self+.
    def update(string) end
    alias << update
  end

  # This module stands as a base class for digest implementation
  # classes.
  class Class
    include Digest::Instance

    # Returns the BubbleBabble encoded hash value of a given _string_.
    def self.bubblebabble(string, *args) end

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
    def ==(...) end

    # Returns the block length of the digest.
    #
    # This method is overridden by each implementation subclass.
    def block_length; end

    # Returns the resulting hash value in a Bubblebabble encoded form.
    def bubblebabble; end

    # If none is given, returns the resulting hash value of the digest,
    # keeping the digest's state.
    #
    # If a _string_ is given, returns the hash value for the given
    # _string_, resetting the digest to the initial state before and
    # after the process.
    def digest(...) end

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
    def hexdigest(...) end

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

  # A class for calculating message digests using the MD5
  # Message-Digest Algorithm by RSA Data Security, Inc., described in
  # RFC1321.
  #
  # MD5 calculates a digest of 128 bits (16 bytes).
  #
  # == Examples
  #  require 'digest'
  #
  #  # Compute a complete digest
  #  Digest::MD5.hexdigest 'abc'      #=> "90015098..."
  #
  #  # Compute digest by chunks
  #  md5 = Digest::MD5.new               # =>#<Digest::MD5>
  #  md5.update "ab"
  #  md5 << "c"                           # alias for #update
  #  md5.hexdigest                        # => "90015098..."
  #
  #  # Use the same object to compute another digest
  #  md5.reset
  #  md5 << "message"
  #  md5.hexdigest                        # => "78e73102..."
  class MD5 < Base
  end

  # A class for calculating message digests using RIPEMD-160
  # cryptographic hash function, designed by Hans Dobbertin, Antoon
  # Bosselaers, and Bart Preneel.
  #
  # RMD160 calculates a digest of 160 bits (20 bytes).
  #
  # == Examples
  #  require 'digest'
  #
  #  # Compute a complete digest
  #  Digest::RMD160.hexdigest 'abc'      #=> "8eb208f7..."
  #
  #  # Compute digest by chunks
  #  rmd160 = Digest::RMD160.new               # =>#<Digest::RMD160>
  #  rmd160.update "ab"
  #  rmd160 << "c"                           # alias for #update
  #  rmd160.hexdigest                        # => "8eb208f7..."
  #
  #  # Use the same object to compute another digest
  #  rmd160.reset
  #  rmd160 << "message"
  #  rmd160.hexdigest                        # => "1dddbe1b..."
  class RMD160 < Base
  end

  # A class for calculating message digests using the SHA-1 Secure Hash
  # Algorithm by NIST (the US' National Institute of Standards and
  # Technology), described in FIPS PUB 180-1.
  #
  # See Digest::Instance for digest API.
  #
  # SHA-1 calculates a digest of 160 bits (20 bytes).
  #
  # == Examples
  #  require 'digest'
  #
  #  # Compute a complete digest
  #  Digest::SHA1.hexdigest 'abc'      #=> "a9993e36..."
  #
  #  # Compute digest by chunks
  #  sha1 = Digest::SHA1.new               # =>#<Digest::SHA1>
  #  sha1.update "ab"
  #  sha1 << "c"                           # alias for #update
  #  sha1.hexdigest                        # => "a9993e36..."
  #
  #  # Use the same object to compute another digest
  #  sha1.reset
  #  sha1 << "message"
  #  sha1.hexdigest                        # => "6f9b9af3..."
  class SHA1 < Base
  end

  # Classes for calculating message digests using the SHA-256/384/512
  # Secure Hash Algorithm(s) by NIST (the US' National Institute of
  # Standards and Technology), described in FIPS PUB 180-2.
  #
  # See SHA2.
  class SHA256 < Base
  end

  # Classes for calculating message digests using the SHA-256/384/512
  # Secure Hash Algorithm(s) by NIST (the US' National Institute of
  # Standards and Technology), described in FIPS PUB 180-2.
  #
  # See SHA2.
  class SHA384 < Base
  end

  # Classes for calculating message digests using the SHA-256/384/512
  # Secure Hash Algorithm(s) by NIST (the US' National Institute of
  # Standards and Technology), described in FIPS PUB 180-2.
  #
  # See SHA2.
  class SHA512 < Base
  end
end
