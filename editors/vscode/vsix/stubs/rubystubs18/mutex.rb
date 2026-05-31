# frozen_string_literal: true

# Mutex implements a simple semaphore that can be used to coordinate access to
# shared data from multiple concurrent threads.
#
# Example:
#
#   require 'thread'
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
  # If the mutex is locked, unlocks the mutex, wakes one waiting thread, and
  # yields in a critical section.
  def exclusive_unlock; end

  # Attempts to grab the lock and waits if it isn't available.
  def lock; end

  # Returns +true+ if this lock is currently held by some thread.
  def locked?; end

  def marshal_dump; end

  # for marshalling mutexes and condvars
  def marshal_load(p1) end

  # Obtains a lock, runs the block, and releases the lock when the block
  # completes.  See the example under Mutex.
  def synchronize; end

  # Attempts to obtain the lock and returns immediately. Returns +true+ if the
  # lock was granted.
  def try_lock; end

  # Releases the lock. Returns +nil+ if ref wasn't locked.
  def unlock; end
end
