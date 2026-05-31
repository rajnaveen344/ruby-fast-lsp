# frozen_string_literal: true

class NameError < StandardError
  # Construct a new NameError exception. If given the <i>name</i>
  # parameter may subsequently be examined using the <code>NameError.name</code>
  # method.
  def initialize(*args) end

  # Return the name associated with this NameError exception.
  def name; end

  # Produce a nicely-formated string representing the +NameError+.
  def to_s; end
end
