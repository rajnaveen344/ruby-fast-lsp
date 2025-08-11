# frozen_string_literal: true

# ConditionVariable objects augment class Mutex. Using condition variables,
# it is possible to suspend while in the middle of a critical section until a
# resource becomes available.
#
# Example:
#
#   mutex = Mutex.new
#   resource = ConditionVariable.new
#
#   a = Thread.new {
#      mutex.synchronize {
#        # Thread 'a' now needs the resource
#        resource.wait(mutex)
#        # 'a' can now have the resource
#      }
#   }
#
#   b = Thread.new {
#      mutex.synchronize {
#        # Thread 'b' has finished using the resource
#        resource.signal
#      }
#   }
class ConditionVariable
  # Creates a new condition variable instance.
  def initialize; end

  # Wakes up all threads waiting for this lock.
  def broadcast; end

  # Wakes up the first thread in line waiting for this lock.
  def signal; end

  # Releases the lock held in +mutex+ and waits; reacquires the lock on wakeup.
  #
  # If +timeout+ is given, this method returns after +timeout+ seconds passed,
  # even if no other thread doesn't signal.
  def wait(mutex, timeout = nil) end
end
