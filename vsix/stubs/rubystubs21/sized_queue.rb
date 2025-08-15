# frozen_string_literal: true

# This class represents queues of specified size capacity.  The push operation
# may be blocked if the capacity is full.
#
# See Queue for an example of how a SizedQueue works.
class SizedQueue
  # Creates a fixed-length queue with a maximum size of +max+.
  def initialize(max) end

  # Removes all objects from the queue.
  def clear; end

  # Returns the maximum size of the queue.
  def max; end

  # Sets the maximum size of the queue to the given +number+.
  def max=(number) end

  # Returns the number of threads waiting on the queue.
  def num_waiting; end

  # Retrieves data from the queue.
  #
  # If the queue is empty, the calling thread is suspended until data is pushed
  # onto the queue. If +non_block+ is true, the thread isn't suspended, and an
  # exception is raised.
  def pop(non_block = false) end
  alias deq pop
  alias shift pop

  # Pushes +object+ to the queue.
  #
  # If there is no space left in the queue, waits until space becomes available.
  def push(object) end
  alias enq push
  alias << push
end
