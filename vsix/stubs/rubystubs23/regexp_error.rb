# frozen_string_literal: true

# Raised when given an invalid regexp expression.
#
#    Regexp.new("?")
#
# <em>raises the exception:</em>
#
#    RegexpError: target of repeat operator is not specified: /?/
class RegexpError < StandardError
end
