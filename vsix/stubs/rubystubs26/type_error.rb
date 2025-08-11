# frozen_string_literal: true

# Raised when encountering an object that is not of the expected type.
#
#    [1, 2, 3].first("two")
#
# <em>raises the exception:</em>
#
#    TypeError: no implicit conversion of String into Integer
class TypeError < StandardError
end
