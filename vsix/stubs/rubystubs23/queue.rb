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
end
