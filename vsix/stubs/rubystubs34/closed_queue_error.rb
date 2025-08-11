# frozen_string_literal: true

# The exception class which will be raised when pushing into a closed
# Queue.  See Thread::Queue#close and Thread::SizedQueue#close.
class ClosedQueueError < StopIteration
end
