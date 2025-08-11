# frozen_string_literal: true

# The exception class which will be raised when pushing into a closed
# Queue.  See Queue#close and SizedQueue#close.
class ClosedQueueError < StopIteration
end
