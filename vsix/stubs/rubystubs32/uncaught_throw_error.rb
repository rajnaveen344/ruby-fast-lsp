# frozen_string_literal: true

# Raised when +throw+ is called with a _tag_ which does not have
# corresponding +catch+ block.
#
#    throw "foo", "bar"
#
# <em>raises the exception:</em>
#
#    UncaughtThrowError: uncaught throw "foo"
class UncaughtThrowError < ArgumentError
  # Document-class: UncaughtThrowError
  #
  # Raised when +throw+ is called with a _tag_ which does not have
  # corresponding +catch+ block.
  #
  #    throw "foo", "bar"
  #
  # <em>raises the exception:</em>
  #
  #    UncaughtThrowError: uncaught throw "foo"
  def initialize(*args) end

  # Return the tag object which was called for.
  def tag; end

  # Returns formatted message with the inspected tag.
  def to_s; end

  # Return the return value which was called for.
  def value; end
end
