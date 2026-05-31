# frozen_string_literal: true

# Raised when the specified key is not found. It is a subclass of
# IndexError.
#
#    h = {"foo" => :bar}
#    h.fetch("foo") #=> :bar
#    h.fetch("baz") #=> KeyError: key not found: "baz"
class KeyError < IndexError
  # Construct a new +KeyError+ exception with the given message,
  # receiver and key.
  def initialize(message = nil, receiver: nil, key: nil) end

  # Return the key caused this KeyError exception.
  def key; end

  # Return the receiver associated with this KeyError exception.
  def receiver; end
end
