# frozen_string_literal: true

class NoMatchingPatternKeyError < NoMatchingPatternError
  # Construct a new +NoMatchingPatternKeyError+ exception with the given message,
  # matchee and key.
  def initialize(message = nil, matchee: nil, key: nil) end

  # Return the key caused this NoMatchingPatternKeyError exception.
  def key; end

  # Return the matchee associated with this NoMatchingPatternKeyError exception.
  def matchee; end
end
