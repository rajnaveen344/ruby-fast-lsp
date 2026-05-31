# frozen_string_literal: true

# This class provides a way to synchronize communication between threads.
#
# Example:
#
#     require 'thread'
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
class Queue
  # Creates a new queue instance.
  def initialize; end

  # Removes all objects from the queue.
  def clear; end

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
  # onto the queue. If +non_block+ is true, the thread isn't suspended, and an
  # exception is raised.
  def pop(non_block = false) end
  alias deq pop
  alias shift pop

  # Pushes the given +object+ to the queue.
  def push(object) end
  alias enq push
  alias << push
end
