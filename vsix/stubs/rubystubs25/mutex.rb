# frozen_string_literal: true

# Mutex implements a simple semaphore that can be used to coordinate access to
# shared data from multiple concurrent threads.
#
# Example:
#
#   semaphore = Mutex.new
#
#   a = Thread.new {
#     semaphore.synchronize {
#       # access shared resource
#     }
#   }
#
#   b = Thread.new {
#     semaphore.synchronize {
#       # access shared resource
#     }
#   }
class Mutex
  # Creates a new Mutex
  def initialize; end

  # Attempts to grab the lock and waits if it isn't available.
  # Raises +ThreadError+ if +mutex+ was locked by the current thread.
  def lock; end

  # Returns +true+ if this lock is currently held by some thread.
  def locked?; end

  # Returns +true+ if this lock is currently held by current thread.
  def owned?; end

  # Releases the lock and sleeps +timeout+ seconds if it is given and
  # non-nil or forever.  Raises +ThreadError+ if +mutex+ wasn't locked by
  # the current thread.
  #
  # When the thread is next woken up, it will attempt to reacquire
  # the lock.
  #
  # Note that this method can wakeup without explicit Thread#wakeup call.
  # For example, receiving signal and so on.
  def sleep(timeout = nil) end

  # Obtains a lock, runs the block, and releases the lock when the block
  # completes.  See the example under +Mutex+.
  def synchronize; end

  # Attempts to obtain the lock and returns immediately. Returns +true+ if the
  # lock was granted.
  def try_lock; end

  # Releases the lock.
  # Raises +ThreadError+ if +mutex+ wasn't locked by the current thread.
  def unlock; end
end
