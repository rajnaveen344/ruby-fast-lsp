# frozen_string_literal: true

# This class represents queues of specified size capacity.  The push operation
# may be blocked if the capacity is full.
#
# See Queue for an example of how a SizedQueue works.
class SizedQueue < Queue
  # Creates a new Mutex
  def initialize; end

  # Returns the maximum size of the queue.
  def max; end

  # Sets the maximum size of the queue.
  def max=(size) end

  # Returns the number of threads waiting on the queue.
  def num_waiting; end

  #   call_seq: pop(non_block=false)
  #
  # Retrieves data from the queue.  If the queue is empty, the calling thread is
  # suspended until data is pushed onto the queue.  If +non_block+ is true, the
  # thread isn't suspended, and an exception is raised.
  def pop(*args) end

  # Pushes +obj+ to the queue.
  def push(obj) end
end
