# frozen_string_literal: true

# Raised when an invalid operation is attempted on a thread.
#
# For example, when no other thread has been started:
#
#    Thread.stop
#
# <em>raises the exception:</em>
#
#    ThreadError: stopping only thread
class ThreadError < StandardError
end
