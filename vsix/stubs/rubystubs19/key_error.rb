# frozen_string_literal: true

# Raised when the specified key is not found. It is a subclass of
# IndexError.
#
#    h = {"foo" => :bar}
#    h.fetch("foo") #=> :bar
#    h.fetch("baz") #=> KeyError: key not found: "baz"
class KeyError < IndexError
end
