# frozen_string_literal: true

# A generic error class raised when an invalid operation is attempted.
# Kernel#raise will raise a RuntimeError if no Exception class is
# specified.
#
#    raise "ouch"
#
# <em>raises the exception:</em>
#
#    RuntimeError: ouch
class RuntimeError < StandardError
end
