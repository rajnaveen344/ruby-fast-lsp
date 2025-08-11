# frozen_string_literal: true

# ConditionVariable objects augment class Mutex. Using condition variables,
# it is possible to suspend while in the middle of a critical section until a
# resource becomes available.
#
# Example:
#
#   require 'thread'
#
#   mutex = Mutex.new
#   resource = ConditionVariable.new
#
#   a = Thread.new {
#     mutex.synchronize {
#       # Thread 'a' now needs the resource
#       resource.wait(mutex)
#       # 'a' can now have the resource
#     }
#   }
#
#   b = Thread.new {
#     mutex.synchronize {
#       # Thread 'b' has finished using the resource
#       resource.signal
#     }
#   }
class ConditionVariable
  # Wakes up all threads waiting for this condition.
  def broadcast; end

  def marshal_dump; end

  # for marshalling mutexes and condvars
  def marshal_load(p1) end

  # Wakes up the first thread in line waiting for this condition.
  def signal; end

  # Releases the lock held in +mutex+ and waits; reacquires the lock on wakeup.
  def wait; end
end
