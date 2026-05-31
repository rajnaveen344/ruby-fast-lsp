# frozen_string_literal: true

# Fibers are primitives for implementing light weight cooperative
# concurrency in Ruby. Basically they are a means of creating code blocks
# that can be paused and resumed, much like threads. The main difference
# is that they are never preempted and that the scheduling must be done by
# the programmer and not the VM.
#
# As opposed to other stackless light weight concurrency models, each fiber
# comes with a stack.  This enables the fiber to be paused from deeply
# nested function calls within the fiber block.  See the ruby(1)
# manpage to configure the size of the fiber stack(s).
#
# When a fiber is created it will not run automatically. Rather it must
# be explicitly asked to run using the Fiber#resume method.
# The code running inside the fiber can give up control by calling
# Fiber.yield in which case it yields control back to caller (the
# caller of the Fiber#resume).
#
# Upon yielding or termination the Fiber returns the value of the last
# executed expression
#
# For instance:
#
#   fiber = Fiber.new do
#     Fiber.yield 1
#     2
#   end
#
#   puts fiber.resume
#   puts fiber.resume
#   puts fiber.resume
#
# <em>produces</em>
#
#   1
#   2
#   FiberError: dead fiber called
#
# The Fiber#resume method accepts an arbitrary number of parameters,
# if it is the first call to #resume then they will be passed as
# block arguments. Otherwise they will be the return value of the
# call to Fiber.yield
#
# Example:
#
#   fiber = Fiber.new do |first|
#     second = Fiber.yield first + 2
#   end
#
#   puts fiber.resume 10
#   puts fiber.resume 1_000_000
#   puts fiber.resume "The fiber will be dead before I can cause trouble"
#
# <em>produces</em>
#
#   12
#   1000000
#   FiberError: dead fiber called
#
# == Non-blocking Fibers
#
# Since Ruby 3.0, the concept of <em>non-blocking fiber</em> was introduced.
# Non-blocking fiber, when reaching any potentially blocking operation (like
# sleep, wait for another process, wait for I/O data to be ready), instead
# of just freezing itself and all execution in the thread, yields control
# to other fibers, and allows the <em>scheduler</em> to handle waiting and waking
# (resuming) the fiber when it can proceed.
#
# For Fiber to behave as non-blocking, it should be created in Fiber.new with
# <tt>blocking: false</tt> (which is the default now), and Fiber.scheduler
# should be set with Fiber.set_scheduler. If Fiber.scheduler is not set in
# the current thread, blocking and non-blocking fiber's behavior is identical.
#
# Ruby doesn't provide a scheduler class: it is expected to be implemented by
# the user and correspond to Fiber::SchedulerInterface.
#
# There is also Fiber.schedule method, which is expected to immediately perform
# passed block in a non-blocking manner (but its actual implementation is up to
# the scheduler).
class Fiber
  # Returns +false+ if the current fiber is non-blocking.
  # Fiber is non-blocking if it was created via passing <tt>blocking: false</tt>
  # to Fiber.new, or via Fiber.schedule.
  #
  # If the current Fiber is blocking, the method, unlike usual
  # predicate methods, returns a *number* of blocking fibers currently
  # running (TBD: always 1?).
  #
  # Note, that even if the method returns +false+, Fiber behaves differently
  # only if Fiber.scheduler is set in the current thread.
  #
  # See the "Non-blocking fibers" section in class docs for details.
  def self.blocking?; end

  # Returns the current fiber. You need to <code>require 'fiber'</code>
  # before using this method. If you are not running in the context of
  # a fiber this method will return the root fiber.
  def self.current; end

  # The method is <em>expected</em> to immediately run the provided block of code in a
  # separate non-blocking fiber.
  #
  #    puts "Go to sleep!"
  #
  #    Fiber.set_scheduler(MyScheduler.new)
  #
  #    Fiber.schedule do
  #      puts "Going to sleep"
  #      sleep(1)
  #      puts "I slept well"
  #    end
  #
  #    puts "Wakey-wakey, sleepyhead"
  #
  # Assuming MyScheduler is properly implemented, this program will produce:
  #
  #    Go to sleep!
  #    Going to sleep
  #    Wakey-wakey, sleepyhead
  #    ...1 sec pause here...
  #    I slept well
  #
  # ...e.g. on the first blocking operation inside the Fiber (<tt>sleep(1)</tt>),
  # the control is yielded at the outside code (main fiber), and <em>at the end
  # of the execution</em>, the scheduler takes care of properly resuming all the
  # blocked fibers.
  #
  # Note that the behavior described above is how the method is <em>expected</em>
  # to behave, actual behavior is up to the current scheduler's implementation of
  # Fiber::SchedulerInterface#fiber method. Ruby doesn't enforce this method to
  # behave in any particular way.
  #
  # If the scheduler is not set, the method raises
  # <tt>RuntimeError (No scheduler is available!)</tt>.
  def self.schedule; end

  # Fiber scheduler, set in the current thread with Fiber.set_scheduler. If the scheduler
  # is +nil+ (which is the default), non-blocking fibers behavior is the same as blocking.
  # (see "Non-blocking fibers" section in class docs for details about the scheduler concept).
  def self.scheduler; end

  # Sets Fiber scheduler for the current thread. If the scheduler is set, non-blocking
  # fibers (created by Fiber.new with <tt>blocking: false</tt>, or by Fiber.schedule)
  # call that scheduler's hook methods on potentially blocking operations, and the current
  # thread will call scheduler's +close+ method on finalization (allowing the scheduler to
  # properly manage all non-finished fibers).
  #
  # +scheduler+ can be an object of any class corresponding to Fiber::SchedulerInterface. Its
  # implementation is up to the user.
  #
  # See also the "Non-blocking fibers" section in class docs.
  def self.set_scheduler(scheduler) end

  # Yields control back to the context that resumed the fiber, passing
  # along any arguments that were passed to it. The fiber will resume
  # processing at this point when #resume is called next.
  # Any arguments passed to the next #resume will be the value that
  # this Fiber.yield expression evaluates to.
  def self.yield(*args) end

  # Creates new Fiber. Initially, fiber is not running, but can be resumed with
  # #resume. Arguments to the first #resume call would be passed to the block:
  #
  #     f = Fiber.new do |initial|
  #        current = initial
  #        loop do
  #          puts "current: #{current.inspect}"
  #          current = Fiber.yield
  #        end
  #     end
  #     f.resume(100)     # prints: current: 100
  #     f.resume(1, 2, 3) # prints: current: [1, 2, 3]
  #     f.resume          # prints: current: nil
  #     # ... and so on ...
  #
  # if <tt>blocking: false</tt> is passed to the <tt>Fiber.new</tt>, _and_ current thread
  # has Fiber.scheduler defined, the Fiber becomes non-blocking (see "Non-blocking
  # fibers" section in class docs).
  def initialize(blocking: false) end

  # Returns true if the fiber can still be resumed (or transferred
  # to). After finishing execution of the fiber block this method will
  # always return false. You need to <code>require 'fiber'</code>
  # before using this method.
  def alive?; end

  # Returns the current execution stack of the fiber. +start+, +count+ and +end+ allow
  # to select only parts of the backtrace.
  #
  #    def level3
  #      Fiber.yield
  #    end
  #
  #    def level2
  #      level3
  #    end
  #
  #    def level1
  #      level2
  #    end
  #
  #    f = Fiber.new { level1 }
  #
  #    # It is empty before the fiber started
  #    f.backtrace
  #    #=> []
  #
  #    f.resume
  #
  #    f.backtrace
  #    #=> ["test.rb:2:in `yield'", "test.rb:2:in `level3'", "test.rb:6:in `level2'", "test.rb:10:in `level1'", "test.rb:13:in `block in <main>'"]
  #    p f.backtrace(1) # start from the item 1
  #    #=> ["test.rb:2:in `level3'", "test.rb:6:in `level2'", "test.rb:10:in `level1'", "test.rb:13:in `block in <main>'"]
  #    p f.backtrace(2, 2) # start from item 2, take 2
  #    #=> ["test.rb:6:in `level2'", "test.rb:10:in `level1'"]
  #    p f.backtrace(1..3) # take items from 1 to 3
  #    #=> ["test.rb:2:in `level3'", "test.rb:6:in `level2'", "test.rb:10:in `level1'"]
  #
  #    f.resume
  #
  #    # It is nil after the fiber is finished
  #    f.backtrace
  #    #=> nil
  def backtrace(...) end

  # Like #backtrace, but returns each line of the execution stack as a
  # Thread::Backtrace::Location. Accepts the same arguments as #backtrace.
  #
  #   f = Fiber.new { Fiber.yield }
  #   f.resume
  #   loc = f.backtrace_locations.first
  #   loc.label  #=> "yield"
  #   loc.path   #=> "test.rb"
  #   loc.lineno #=> 1
  def backtrace_locations(...) end

  # Returns +true+ if +fiber+ is blocking and +false+ otherwise.
  # Fiber is non-blocking if it was created via passing <tt>blocking: false</tt>
  # to Fiber.new, or via Fiber.schedule.
  #
  # Note, that even if the method returns +false+, Fiber behaves differently
  # only if Fiber.scheduler is set in the current thread.
  #
  # See the "Non-blocking fibers" section in class docs for details.
  def blocking?; end

  # Raises an exception in the fiber at the point at which the last
  # +Fiber.yield+ was called. If the fiber has not been started or has
  # already run to completion, raises +FiberError+. If the fiber is
  # yielding, it is resumed. If it is transferring, it is transferred into.
  # But if it is resuming, raises +FiberError+.
  #
  # With no arguments, raises a +RuntimeError+. With a single +String+
  # argument, raises a +RuntimeError+ with the string as a message.  Otherwise,
  # the first parameter should be the name of an +Exception+ class (or an
  # object that returns an +Exception+ object when sent an +exception+
  # message). The optional second parameter sets the message associated with
  # the exception, and the third parameter is an array of callback information.
  # Exceptions are caught by the +rescue+ clause of <code>begin...end</code>
  # blocks.
  def raise(...) end

  # Resumes the fiber from the point at which the last Fiber.yield was
  # called, or starts running it if it is the first call to
  # #resume. Arguments passed to resume will be the value of the
  # Fiber.yield expression or will be passed as block parameters to
  # the fiber's block if this is the first #resume.
  #
  # Alternatively, when resume is called it evaluates to the arguments passed
  # to the next Fiber.yield statement inside the fiber's block
  # or to the block value if it runs to completion without any
  # Fiber.yield
  def resume(*args) end

  # Returns fiber information string.
  def to_s; end
  alias inspect to_s

  # Transfer control to another fiber, resuming it from where it last
  # stopped or starting it if it was not resumed before. The calling
  # fiber will be suspended much like in a call to
  # Fiber.yield. You need to <code>require 'fiber'</code>
  # before using this method.
  #
  # The fiber which receives the transfer call is treats it much like
  # a resume call. Arguments passed to transfer are treated like those
  # passed to resume.
  #
  # The two style of control passing to and from fiber (one is #resume and
  # Fiber::yield, another is #transfer to and from fiber) can't be freely
  # mixed.
  #
  # * If the Fiber's lifecycle had started with transfer, it will never
  #   be able to yield or be resumed control passing, only
  #   finish or transfer back. (It still can resume other fibers that
  #   are allowed to be resumed.)
  # * If the Fiber's lifecycle had started with resume, it can yield
  #   or transfer to another Fiber, but can receive control back only
  #   the way compatible with the way it was given away: if it had
  #   transferred, it only can be transferred back, and if it had
  #   yielded, it only can be resumed back. After that, it again can
  #   transfer or yield.
  #
  # If those rules are broken FiberError is raised.
  #
  # For an individual Fiber design, yield/resume is more easy to use
  # style (the Fiber just gives away control, it doesn't need to think
  # about who the control is given to), while transfer is more flexible
  # for complex cases, allowing to build arbitrary graphs of Fibers
  # dependent on each other.
  #
  # Example:
  #
  #    require 'fiber'
  #
  #    manager = nil # For local var to be visible inside worker block
  #
  #    # This fiber would be started with transfer
  #    # It can't yield, and can't be resumed
  #    worker = Fiber.new { |work|
  #      puts "Worker: starts"
  #      puts "Worker: Performed #{work.inspect}, transferring back"
  #      # Fiber.yield     # this would raise FiberError: attempt to yield on a not resumed fiber
  #      # manager.resume  # this would raise FiberError: attempt to resume a resumed fiber (double resume)
  #      manager.transfer(work.capitalize)
  #    }
  #
  #    # This fiber would be started with resume
  #    # It can yield or transfer, and can be transferred
  #    # back or resumed
  #    manager = Fiber.new {
  #      puts "Manager: starts"
  #      puts "Manager: transferring 'something' to worker"
  #      result = worker.transfer('something')
  #      puts "Manager: worker returned #{result.inspect}"
  #      # worker.resume    # this would raise FiberError: attempt to resume a transferring fiber
  #      Fiber.yield        # this is OK, the fiber transferred from and to, now it can yield
  #      puts "Manager: finished"
  #    }
  #
  #    puts "Starting the manager"
  #    manager.resume
  #    puts "Resuming the manager"
  #    # manager.transfer  # this would raise FiberError: attempt to transfer to a yielding fiber
  #    manager.resume
  #
  # <em>produces</em>
  #
  #    Starting the manager
  #    Manager: starts
  #    Manager: transferring 'something' to worker
  #    Worker: starts
  #    Worker: Performed "something", transferring back
  #    Manager: worker returned "Something"
  #    Resuming the manager
  #    Manager: finished
  def transfer(*args) end

  # This is not an existing class, but documentation of the interface that Scheduler
  # object should comply in order to be used as Fiber.scheduler and handle non-blocking
  # fibers. See also the "Non-blocking fibers" section in Fiber class docs for explanations
  # of some concepts.
  #
  # Scheduler's behavior and usage are expected to be as follows:
  #
  # * When the execution in the non-blocking Fiber reaches some blocking operation (like
  #   sleep, wait for a process, or a non-ready I/O), it calls some of the scheduler's
  #   hook methods, listed below.
  # * Scheduler somehow registers what the current fiber is waited for, and yields control
  #   to other fibers with Fiber.yield (so the fiber would be suspended while expecting its
  #   wait to end, and other fibers in the same thread can perform)
  # * At the end of the current thread execution, the scheduler's method #close is called
  # * The scheduler runs into a wait loop, checking all the blocked fibers (which it has
  #   registered on hook calls) and resuming them when the awaited resource is ready (I/O
  #   ready, sleep time passed).
  #
  # A typical implementation would probably rely for this closing loop on a gem like
  # EventMachine[https://github.com/eventmachine/eventmachine] or
  # Async[https://github.com/socketry/async].
  #
  # This way concurrent execution will be achieved in a way that is transparent for every
  # individual Fiber's code.
  #
  # Hook methods are:
  #
  # * #io_wait
  # * #process_wait
  # * #kernel_sleep
  # * #block and #unblock
  # * (the list is expanded as Ruby developers make more methods having non-blocking calls)
  #
  # When not specified otherwise, the hook implementations are mandatory: if they are not
  # implemented, the methods trying to call hook will fail. To provide backward compatibility,
  # in the future hooks will be optional (if they are not implemented, due to the scheduler
  # being created for the older Ruby version, the code which needs this hook will not fail,
  # and will just behave in a blocking fashion).
  #
  # It is also strongly suggested that the scheduler implement the #fiber method, which is
  # delegated to by Fiber.schedule.
  #
  # Sample _toy_ implementation of the scheduler can be found in Ruby's code, in
  # <tt>test/fiber/scheduler.rb</tt>
  class SchedulerInterface
    # Invoked by methods like Thread.join, and by Mutex, to signify that current
    # Fiber is blocked till further notice (e.g. #unblock) or till +timeout+ will
    # pass.
    #
    # +blocker+ is what we are waiting on, informational only (for debugging and
    # logging). There are no guarantees about its value.
    #
    # Expected to return boolean, specifying whether the blocking operation was
    # successful or not.
    def block(blocker, timeout = nil) end

    # Called when the current thread exits. The scheduler is expected to implement this
    # method in order to allow all waiting fibers to finalize their execution.
    #
    # The suggested pattern is to implement the main event loop in the #close method.
    def close; end

    # Implementation of the Fiber.schedule. The method is <em>expected</em> to immediately
    # run passed block of code in a separate non-blocking fiber, and to return that Fiber.
    #
    # Minimal suggested implementation is:
    #
    #    def fiber(&block)
    #      Fiber.new(blocking: false, &block).tap(&:resume)
    #    end
    def fiber(&block) end

    # Invoked by IO#wait, IO#wait_readable, IO#wait_writable to ask whether the
    # specified descriptor is ready for specified events within
    # the specified +timeout+.
    #
    # +events+ is a bit mask of <tt>IO::READABLE</tt>, <tt>IO::WRITABLE</tt>, and
    # <tt>IO::PRIORITY</tt>.
    #
    # Suggested implementation should register which Fiber is waiting for which
    # resources and immediately calling Fiber.yield to pass control to other
    # fibers. Then, in the #close method, the scheduler might dispatch all the
    # I/O resources to fibers waiting for it.
    #
    # Expected to return the subset of events that are ready immediately.
    def io_wait(io, events, timeout) end

    # Invoked by Kernel#sleep and Mutex#sleep and is expected to provide
    # an implementation of sleeping in a non-blocking way. Implementation might
    # register the current fiber in some list of "what fiber waits till what
    # moment", call Fiber.yield to pass control, and then in #close resume
    # the fibers whose wait period have ended.
    def kernel_sleep(duration = nil) end

    # Invoked by Process::Status.wait in order to wait for a specified process.
    # See that method description for arguments description.
    #
    # Suggested minimal implementation:
    #
    #     Thread.new do
    #       Process::Status.wait(pid, flags)
    #     end.value
    #
    # This hook is optional: if it is not present in the current scheduler,
    # Process::Status.wait will behave as a blocking method.
    #
    # Expected to returns a Process::Status instance.
    def process_wait(pid, flags) end

    # Invoked to wake up Fiber previously blocked with #block (for example, Mutex#lock
    # calls #block and Mutex#unlock calls #unblock). The scheduler should use
    # the +fiber+ parameter to understand which fiber is unblocked.
    #
    # +blocker+ is what was awaited for, but it is informational only (for debugging
    # and logging), and it is not guaranteed to be the same value as the +blocker+ for
    # #block.
    def unblock(blocker, fiber) end
  end
end
