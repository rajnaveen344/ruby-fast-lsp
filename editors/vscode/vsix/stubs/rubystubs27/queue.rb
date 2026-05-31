# frozen_string_literal: true

# The Queue class implements multi-producer, multi-consumer queues.
# It is especially useful in threaded programming when information
# must be exchanged safely between multiple threads. The Queue class
# implements all the required locking semantics.
#
# The class implements FIFO type of queue. In a FIFO queue, the first
# tasks added are the first retrieved.
#
# Example:
#
#     queue = Queue.new
#
#     producer = Thread.new do
#       5.times do |i|
#          sleep rand(i) # simulate expense
#          queue << i
#          puts "#{i} produced"
#       end
#     end
#
#     consumer = Thread.new do
#       5.times do |i|
#          value = queue.pop
#          sleep rand(i/2) # simulate expense
#          puts "consumed #{value}"
#       end
#     end
#
#     consumer.join
class Queue
  # Creates a new queue instance.
  def initialize; end

  # Removes all objects from the queue.
  def clear; end

  # Closes the queue. A closed queue cannot be re-opened.
  #
  # After the call to close completes, the following are true:
  #
  # - +closed?+ will return true
  #
  # - +close+ will be ignored.
  #
  # - calling enq/push/<< will raise a +ClosedQueueError+.
  #
  # - when +empty?+ is false, calling deq/pop/shift will return an object
  #   from the queue as usual.
  # - when +empty?+ is true, deq(false) will not suspend the thread and will return nil.
  #   deq(true) will raise a +ThreadError+.
  #
  # ClosedQueueError is inherited from StopIteration, so that you can break loop block.
  #
  #  Example:
  #
  #      q = Queue.new
  #      Thread.new{
  #        while e = q.deq # wait for nil to break loop
  #          # ...
  #        end
  #      }
  #      q.close
  def close; end

  # Returns +true+ if the queue is closed.
  def closed?; end

  # Returns +true+ if the queue is empty.
  def empty?; end

  # Returns the length of the queue.
  def length; end
  alias size length

  # Returns the number of threads waiting on the queue.
  def num_waiting; end

  # Retrieves data from the queue.
  #
  # If the queue is empty, the calling thread is suspended until data is pushed
  # onto the queue. If +non_block+ is true, the thread isn't suspended, and
  # +ThreadError+ is raised.
  def pop(non_block = false) end
  alias deq pop
  alias shift pop

  # Pushes the given +object+ to the queue.
  def push(object) end
  alias enq push
  alias << push
end
