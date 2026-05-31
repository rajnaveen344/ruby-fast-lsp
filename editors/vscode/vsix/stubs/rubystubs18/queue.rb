# frozen_string_literal: true

# This class provides a way to synchronize communication between threads.
#
# Example:
#
#   require 'thread'
#
#   queue = Queue.new
#
#   producer = Thread.new do
#     5.times do |i|
#       sleep rand(i) # simulate expense
#       queue << i
#       puts "#{i} produced"
#     end
#   end
#
#   consumer = Thread.new do
#     5.times do |i|
#       value = queue.pop
#       sleep rand(i/2) # simulate expense
#       puts "consumed #{value}"
#     end
#   end
#
#   consumer.join
class Queue
  # Removes all objects from the queue.
  def clear; end

  # Returns +true+ if the queue is empty.
  def empty?; end

  # Returns the length of the queue.
  def length; end

  def marshal_dump; end

  def marshal_load(p1) end

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
