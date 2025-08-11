# frozen_string_literal: true

class Random
  DEFAULT = _

  # Returns arbitrary value for seed.
  def self.new_seed; end

  #    Alias of _Random::DEFAULT.rand_.
  def self.rand(*several_variants) end

  # Seeds the pseudorandom number generator to the value of
  # <i>number</i>. If <i>number</i> is omitted,
  # seeds the generator using a combination of the time, the
  # process id, and a sequence number. (This is also the behavior if
  # <code>Kernel::rand</code> is called without previously calling
  # <code>srand</code>, but without the sequence.) By setting the seed
  # to a known value, scripts can be made deterministic during testing.
  # The previous seed value is returned. Also see <code>Kernel::rand</code>.
  def self.srand(number = 0) end

  # Creates new Mersenne Twister based pseudorandom number generator with
  # seed.  When the argument seed is omitted, the generator is initialized
  # with Random.new_seed.
  #
  # The argument seed is used to ensure repeatable sequences of random numbers
  # between different runs of the program.
  #
  #     prng = Random.new(1234)
  #     [ prng.rand, prng.rand ]   #=> [0.191519450378892, 0.622108771039832]
  #     [ prng.integer(10), prng.integer(1000) ]  #=> [4, 664]
  #     prng = Random.new(1234)
  #     [ prng.rand, prng.rand ]   #=> [0.191519450378892, 0.622108771039832]
  def initialize(*seed) end

  # Returns true if the generators' states equal.
  def ==(other) end

  # Returns a random binary string.  The argument size specified the length of
  # the result string.
  def bytes(size) end

  # When the argument is an +Integer+ or a +Bignum+, it returns a
  # random integer greater than or equal to zero and less than the
  # argument.  Unlike Random.rand, when the argument is a negative
  # integer or zero, it raises an ArgumentError.
  #
  # When the argument is a +Float+, it returns a random floating point
  # number between 0.0 and _max_, including 0.0 and excluding _max_.
  #
  # When the argument _limit_ is a +Range+, it returns a random
  # number where range.member?(number) == true.
  #     prng.rand(5..9)  #=> one of [5, 6, 7, 8, 9]
  #     prng.rand(5...9) #=> one of [5, 6, 7, 8]
  #     prng.rand(5.0..9.0) #=> between 5.0 and 9.0, including 9.0
  #     prng.rand(5.0...9.0) #=> between 5.0 and 9.0, excluding 9.0
  #
  # +begin+/+end+ of the range have to have subtract and add methods.
  #
  # Otherwise, it raises an ArgumentError.
  def rand(*several_variants) end

  # Returns the seed of the generator.
  def seed; end
end
