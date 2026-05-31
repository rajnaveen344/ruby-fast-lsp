# frozen_string_literal: true

# Random provides an interface to Ruby's pseudo-random number generator, or
# PRNG.  The PRNG produces a deterministic sequence of bits which approximate
# true randomness. The sequence may be represented by integers, floats, or
# binary strings.
#
# The generator may be initialized with either a system-generated or
# user-supplied seed value by using Random.srand.
#
# The class method Random.rand provides the base functionality of Kernel.rand
# along with better handling of floating point values. These are both
# interfaces to Random::DEFAULT, the Ruby system PRNG.
#
# Random.new will create a new PRNG with a state independent of
# Random::DEFAULT, allowing multiple generators with different seed values or
# sequence positions to exist simultaneously. Random objects can be
# marshaled, allowing sequences to be saved and resumed.
#
# PRNGs are currently implemented as a modified Mersenne Twister with a period
# of 2**19937-1.
class Random
  include Random::Formatter

  # The default Pseudorandom number generator.  Used by class
  # methods of Random.
  DEFAULT = _

  # Returns a random binary string.
  # The argument +size+ specifies the length of the returned string.
  def self.bytes(size) end

  # Returns an arbitrary seed value. This is used by Random.new
  # when no seed value is specified as an argument.
  #
  #   Random.new_seed  #=> 115032730400174366788466674494640623225
  def self.new_seed; end

  # Alias of Random::DEFAULT.rand.
  def self.rand(...) end

  # Seeds the system pseudo-random number generator, Random::DEFAULT, with
  # +number+.  The previous seed value is returned.
  #
  # If +number+ is omitted, seeds the generator using a source of entropy
  # provided by the operating system, if available (/dev/urandom on Unix systems
  # or the RSA cryptographic provider on Windows), which is then combined with
  # the time, the process id, and a sequence number.
  #
  # srand may be used to ensure repeatable sequences of pseudo-random numbers
  # between different runs of the program. By setting the seed to a known value,
  # programs can be made deterministic during testing.
  #
  #   srand 1234               # => 268519324636777531569100071560086917274
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  #   [ rand(10), rand(1000) ] # => [4, 664]
  #   srand 1234               # => 1234
  #   [ rand, rand ]           # => [0.1915194503788923, 0.6221087710398319]
  def self.srand(number = Random.new_seed) end

  # Returns a string, using platform providing features.
  # Returned value is expected to be a cryptographically secure
  # pseudo-random number in binary form.
  # This method raises a RuntimeError if the feature provided by platform
  # failed to prepare the result.
  #
  # In 2017, Linux manpage random(7) writes that "no cryptographic
  # primitive available today can hope to promise more than 256 bits of
  # security".  So it might be questionable to pass size > 32 to this
  # method.
  #
  #   Random.urandom(8)  #=> "\x78\x41\xBA\xAF\x7D\xEA\xD8\xEA"
  def self.urandom(size) end

  # Creates a new PRNG using +seed+ to set the initial state. If +seed+ is
  # omitted, the generator is initialized with Random.new_seed.
  #
  # See Random.srand for more information on the use of seed values.
  def initialize(seed = Random.new_seed) end

  # Returns true if the two generators have the same internal state, otherwise
  # false.  Equivalent generators will return the same sequence of
  # pseudo-random numbers.  Two generators will generally have the same state
  # only if they were initialized with the same seed
  #
  #   Random.new == Random.new             # => false
  #   Random.new(1234) == Random.new(1234) # => true
  #
  # and have the same invocation history.
  #
  #   prng1 = Random.new(1234)
  #   prng2 = Random.new(1234)
  #   prng1 == prng2 # => true
  #
  #   prng1.rand     # => 0.1915194503788923
  #   prng1 == prng2 # => false
  #
  #   prng2.rand     # => 0.1915194503788923
  #   prng1 == prng2 # => true
  def ==(other) end

  # Returns a random binary string containing +size+ bytes.
  #
  #   random_string = Random.new.bytes(10) # => "\xD7:R\xAB?\x83\xCE\xFAkO"
  #   random_string.size                   # => 10
  def bytes(size) end

  # When +max+ is an Integer, +rand+ returns a random integer greater than
  # or equal to zero and less than +max+. Unlike Kernel.rand, when +max+
  # is a negative integer or zero, +rand+ raises an ArgumentError.
  #
  #   prng = Random.new
  #   prng.rand(100)       # => 42
  #
  # When +max+ is a Float, +rand+ returns a random floating point number
  # between 0.0 and +max+, including 0.0 and excluding +max+.
  #
  #   prng.rand(1.5)       # => 1.4600282860034115
  #
  # When +max+ is a Range, +rand+ returns a random number where
  # range.member?(number) == true.
  #
  #   prng.rand(5..9)      # => one of [5, 6, 7, 8, 9]
  #   prng.rand(5...9)     # => one of [5, 6, 7, 8]
  #   prng.rand(5.0..9.0)  # => between 5.0 and 9.0, including 9.0
  #   prng.rand(5.0...9.0) # => between 5.0 and 9.0, excluding 9.0
  #
  # Both the beginning and ending values of the range must respond to subtract
  # (<tt>-</tt>) and add (<tt>+</tt>)methods, or rand will raise an
  # ArgumentError.
  def rand(...) end

  # Returns the seed value used to initialize the generator. This may be used to
  # initialize another generator with the same state at a later time, causing it
  # to produce the same sequence of numbers.
  #
  #   prng1 = Random.new(1234)
  #   prng1.seed       #=> 1234
  #   prng1.rand(100)  #=> 47
  #
  #   prng2 = Random.new(prng1.seed)
  #   prng2.rand(100)  #=> 47
  def seed; end

  # Format raw random number as Random does
  module Formatter
    # Generates formatted random number from raw random bytes.
    # See Random#rand.
    def random_number(...) end
    alias rand random_number
  end
end
