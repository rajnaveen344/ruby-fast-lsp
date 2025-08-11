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
# The concept of <em>non-blocking fiber</em> was introduced in Ruby 3.0.
# A non-blocking fiber, when reaching a operation that would normally block
# the fiber (like <code>sleep</code>, or wait for another process or I/O)
# will yield control to other fibers and allow the <em>scheduler</em> to
# handle blocking and waking up (resuming) this fiber when it can proceed.
#
# For a Fiber to behave as non-blocking, it need to be created in Fiber.new with
# <tt>blocking: false</tt> (which is the default), and Fiber.scheduler
# should be set with Fiber.set_scheduler. If Fiber.scheduler is not set in
# the current thread, blocking and non-blocking fibers' behavior is identical.
#
# Ruby doesn't provide a scheduler class: it is expected to be implemented by
# the user and correspond to Fiber::Scheduler.
#
# There is also Fiber.schedule method, which is expected to immediately perform
# the given block in a non-blocking manner. Its actual implementation is up to
# the scheduler.
class Fiber
  # Returns the value of the fiber storage variable identified by +key+.
  #
  # The +key+ must be a symbol, and the value is set by Fiber#[]= or
  # Fiber#store.
  #
  # See also Fiber::[]=.
  def self.[](key) end

  # Assign +value+ to the fiber storage variable identified by +key+.
  # The variable is created if it doesn't exist.
  #
  # +key+ must be a Symbol, otherwise a TypeError is raised.
  #
  # See also Fiber::[].
  def self.[]=(key, value) end

  # Forces the fiber to be blocking for the duration of the block. Returns the
  # result of the block.
  #
  # See the "Non-blocking fibers" section in class docs for details.
  def self.blocking; end

  # Returns +false+ if the current fiber is non-blocking.
  # Fiber is non-blocking if it was created via passing <tt>blocking: false</tt>
  # to Fiber.new, or via Fiber.schedule.
  #
  # If the current Fiber is blocking, the method returns 1.
  # Future developments may allow for situations where larger integers
  # could be returned.
  #
  # Note that, even if the method returns +false+, Fiber behaves differently
  # only if Fiber.scheduler is set in the current thread.
  #
  # See the "Non-blocking fibers" section in class docs for details.
  def self.blocking?; end

  # Returns the current fiber. If you are not running in the context of
  # a fiber this method will return the root fiber.
  def self.current; end

  # Returns the Fiber scheduler, that was last set for the current thread with Fiber.set_scheduler
  # if and only if the current fiber is non-blocking.
  def self.current_scheduler; end

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
  # the control is yielded to the outside code (main fiber), and <em>at the end
  # of that execution</em>, the scheduler takes care of properly resuming all the
  # blocked fibers.
  #
  # Note that the behavior described above is how the method is <em>expected</em>
  # to behave, actual behavior is up to the current scheduler's implementation of
  # Fiber::Scheduler#fiber method. Ruby doesn't enforce this method to
  # behave in any particular way.
  #
  # If the scheduler is not set, the method raises
  # <tt>RuntimeError (No scheduler is available!)</tt>.
  def self.schedule; end

  # Returns the Fiber scheduler, that was last set for the current thread with Fiber.set_scheduler.
  # Returns +nil+ if no scheduler is set (which is the default), and non-blocking fibers'
  # behavior is the same as blocking.
  # (see "Non-blocking fibers" section in class docs for details about the scheduler concept).
  def self.scheduler; end

  # Sets the Fiber scheduler for the current thread. If the scheduler is set, non-blocking
  # fibers (created by Fiber.new with <tt>blocking: false</tt>, or by Fiber.schedule)
  # call that scheduler's hook methods on potentially blocking operations, and the current
  # thread will call scheduler's +close+ method on finalization (allowing the scheduler to
  # properly manage all non-finished fibers).
  #
  # +scheduler+ can be an object of any class corresponding to Fiber::Scheduler. Its
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

  # Creates new Fiber. Initially, the fiber is not running and can be resumed
  # with #resume. Arguments to the first #resume call will be passed to the
  # block:
  #
  #   f = Fiber.new do |initial|
  #      current = initial
  #      loop do
  #        puts "current: #{current.inspect}"
  #        current = Fiber.yield
  #      end
  #   end
  #   f.resume(100)     # prints: current: 100
  #   f.resume(1, 2, 3) # prints: current: [1, 2, 3]
  #   f.resume          # prints: current: nil
  #   # ... and so on ...
  #
  # If <tt>blocking: false</tt> is passed to <tt>Fiber.new</tt>, _and_ current
  # thread has a Fiber.scheduler defined, the Fiber becomes non-blocking (see
  # "Non-blocking Fibers" section in class docs).
  #
  # If the <tt>storage</tt> is unspecified, the default is to inherit a copy of
  # the storage from the current fiber. This is the same as specifying
  # <tt>storage: true</tt>.
  #
  #   Fiber[:x] = 1
  #   Fiber.new do
  #     Fiber[:x] # => 1
  #     Fiber[:x] = 2
  #   end.resume
  #   Fiber[:x] # => 1
  #
  # If the given <tt>storage</tt> is <tt>nil</tt>, this function will lazy
  # initialize the internal storage, which starts as an empty hash.
  #
  #   Fiber[:x] = "Hello World"
  #   Fiber.new(storage: nil) do
  #     Fiber[:x] # nil
  #   end
  #
  # Otherwise, the given <tt>storage</tt> is used as the new fiber's storage,
  # and it must be an instance of Hash.
  #
  # Explicitly using <tt>storage: true</tt> is currently experimental and may
  # change in the future.
  def initialize(blocking: false, storage: true) end

  # Returns true if the fiber can still be resumed (or transferred
  # to). After finishing execution of the fiber block this method will
  # always return +false+.
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
  # Note that, even if the method returns +false+, the fiber behaves differently
  # only if Fiber.scheduler is set in the current thread.
  #
  # See the "Non-blocking fibers" section in class docs for details.
  def blocking?; end

  # Terminates the fiber by raising an uncatchable exception.
  # It only terminates the given fiber and no other fiber, returning +nil+ to
  # another fiber if that fiber was calling #resume or #transfer.
  #
  # <tt>Fiber#kill</tt> only interrupts another fiber when it is in Fiber.yield.
  # If called on the current fiber then it raises that exception at the <tt>Fiber#kill</tt> call site.
  #
  # If the fiber has not been started, transition directly to the terminated state.
  #
  # If the fiber is already terminated, does nothing.
  #
  # Raises FiberError if called on a fiber belonging to another thread.
  def kill; end

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
  #
  # Raises +FiberError+ if called on a Fiber belonging to another +Thread+.
  #
  # See Kernel#raise for more information.
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

  # Returns a copy of the storage hash for the fiber. The method can only be called on the
  # Fiber.current.
  def storage; end

  # Sets the storage hash for the fiber. This feature is experimental
  # and may change in the future. The method can only be called on the
  # Fiber.current.
  #
  # You should be careful about using this method as you may inadvertently clear
  # important fiber-storage state. You should mostly prefer to assign specific
  # keys in the storage using Fiber::[]=.
  #
  # You can also use <tt>Fiber.new(storage: nil)</tt> to create a fiber with an empty
  # storage.
  #
  # Example:
  #
  #   while request = request_queue.pop
  #     # Reset the per-request state:
  #     Fiber.current.storage = nil
  #     handle_request(request)
  #   end
  def storage=(hash) end

  def to_s; end
  alias inspect to_s

  # Transfer control to another fiber, resuming it from where it last
  # stopped or starting it if it was not resumed before. The calling
  # fiber will be suspended much like in a call to
  # Fiber.yield.
  #
  # The fiber which receives the transfer call treats it much like
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
  # For an individual Fiber design, yield/resume is easier to use
  # (the Fiber just gives away control, it doesn't need to think
  # about who the control is given to), while transfer is more flexible
  # for complex cases, allowing to build arbitrary graphs of Fibers
  # dependent on each other.
  #
  # Example:
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
  # object should comply to in order to be used as argument to Fiber.scheduler and handle non-blocking
  # fibers. See also the "Non-blocking fibers" section in Fiber class docs for explanations
  # of some concepts.
  #
  # Scheduler's behavior and usage are expected to be as follows:
  #
  # * When the execution in the non-blocking Fiber reaches some blocking operation (like
  #   sleep, wait for a process, or a non-ready I/O), it calls some of the scheduler's
  #   hook methods, listed below.
  # * Scheduler somehow registers what the current fiber is waiting on, and yields control
  #   to other fibers with Fiber.yield (so the fiber would be suspended while expecting its
  #   wait to end, and other fibers in the same thread can perform)
  # * At the end of the current thread execution, the scheduler's method #scheduler_close is called
  # * The scheduler runs into a wait loop, checking all the blocked fibers (which it has
  #   registered on hook calls) and resuming them when the awaited resource is ready
  #   (e.g. I/O ready or sleep time elapsed).
  #
  # This way concurrent execution will be achieved transparently for every
  # individual Fiber's code.
  #
  # Scheduler implementations are provided by gems, like
  # Async[https://github.com/socketry/async].
  #
  # Hook methods are:
  #
  # * #io_wait, #io_read, #io_write, #io_pread, #io_pwrite, and #io_select, #io_close
  # * #process_wait
  # * #kernel_sleep
  # * #timeout_after
  # * #address_resolve
  # * #block and #unblock
  # * #blocking_operation_wait
  # * (the list is expanded as Ruby developers make more methods having non-blocking calls)
  #
  # When not specified otherwise, the hook implementations are mandatory: if they are not
  # implemented, the methods trying to call hook will fail. To provide backward compatibility,
  # in the future hooks will be optional (if they are not implemented, due to the scheduler
  # being created for the older Ruby version, the code which needs this hook will not fail,
  # and will just behave in a blocking fashion).
  #
  # It is also strongly recommended that the scheduler implements the #fiber method, which is
  # delegated to by Fiber.schedule.
  #
  # Sample _toy_ implementation of the scheduler can be found in Ruby's code, in
  # <tt>test/fiber/scheduler.rb</tt>
  class Scheduler
    # Invoked by any method that performs a non-reverse DNS lookup. The most
    # notable method is Addrinfo.getaddrinfo, but there are many other.
    #
    # The method is expected to return an array of strings corresponding to ip
    # addresses the +hostname+ is resolved to, or +nil+ if it can not be resolved.
    #
    # Fairly exhaustive list of all possible call-sites:
    #
    # - Addrinfo.getaddrinfo
    # - Addrinfo.tcp
    # - Addrinfo.udp
    # - Addrinfo.ip
    # - Addrinfo.new
    # - Addrinfo.marshal_load
    # - SOCKSSocket.new
    # - TCPServer.new
    # - TCPSocket.new
    # - IPSocket.getaddress
    # - TCPSocket.gethostbyname
    # - UDPSocket#connect
    # - UDPSocket#bind
    # - UDPSocket#send
    # - Socket.getaddrinfo
    # - Socket.gethostbyname
    # - Socket.pack_sockaddr_in
    # - Socket.sockaddr_in
    # - Socket.unpack_sockaddr_in
    def address_resolve(hostname) end

    # Invoked by methods like Thread.join, and by Mutex, to signify that current
    # Fiber is blocked until further notice (e.g. #unblock) or until +timeout+ has
    # elapsed.
    #
    # +blocker+ is what we are waiting on, informational only (for debugging and
    # logging). There are no guarantee about its value.
    #
    # Expected to return boolean, specifying whether the blocking operation was
    # successful or not.
    def block(blocker, timeout = nil) end

    # Invoked by Ruby's core methods to run a blocking operation in a non-blocking way.
    #
    # Minimal suggested implementation is:
    #
    #    def blocking_operation_wait(work)
    #      Thread.new(&work).join
    #    end
    def blocking_operation_wait(work) end

    # Called when the current thread exits. The scheduler is expected to implement this
    # method in order to allow all waiting fibers to finalize their execution.
    #
    # The suggested pattern is to implement the main event loop in the #close method.
    def close; end

    # Implementation of the Fiber.schedule. The method is <em>expected</em> to immediately
    # run the given block of code in a separate non-blocking fiber, and to return that Fiber.
    #
    # Minimal suggested implementation is:
    #
    #    def fiber(&block)
    #      fiber = Fiber.new(blocking: false, &block)
    #      fiber.resume
    #      fiber
    #    end
    def fiber(&) end

    # Invoked by IO#pread or IO::Buffer#pread to read +length+ bytes from +io+
    # at offset +from+ into a specified +buffer+ (see IO::Buffer) at the given
    # +offset+.
    #
    # This method is semantically the same as #io_read, but it allows to specify
    # the offset to read from and is often better for asynchronous IO on the same
    # file.
    #
    # The method should be considered _experimental_.
    def io_pread(io, buffer, from, length, offset) end

    # Invoked by IO#pwrite or IO::Buffer#pwrite to write +length+ bytes to +io+
    # at offset +from+ into a specified +buffer+ (see IO::Buffer) at the given
    # +offset+.
    #
    # This method is semantically the same as #io_write, but it allows to specify
    # the offset to write to and is often better for asynchronous IO on the same
    # file.
    #
    # The method should be considered _experimental_.
    def io_pwrite(io, buffer, from, length, offset) end

    # Invoked by IO#read or IO#Buffer.read to read +length+ bytes from +io+ into a
    # specified +buffer+ (see IO::Buffer) at the given +offset+.
    #
    # The +length+ argument is the "minimum length to be read". If the IO buffer
    # size is 8KiB, but the +length+ is +1024+ (1KiB), up to 8KiB might be read,
    # but at least 1KiB will be. Generally, the only case where less data than
    # +length+ will be read is if there is an error reading the data.
    #
    # Specifying a +length+ of 0 is valid and means try reading at least once and
    # return any available data.
    #
    # Suggested implementation should try to read from +io+ in a non-blocking
    # manner and call #io_wait if the +io+ is not ready (which will yield control
    # to other fibers).
    #
    # See IO::Buffer for an interface available to return data.
    #
    # Expected to return number of bytes read, or, in case of an error,
    # <tt>-errno</tt> (negated number corresponding to system's error code).
    #
    # The method should be considered _experimental_.
    def io_read(io, buffer, length, offset) end

    # Invoked by IO.select to ask whether the specified descriptors are ready for
    # specified events within the specified +timeout+.
    #
    # Expected to return the 3-tuple of Array of IOs that are ready.
    def io_select(readables, writables, exceptables, timeout) end

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

    # Invoked by IO#write or IO::Buffer#write to write +length+ bytes to +io+ from
    # from a specified +buffer+ (see IO::Buffer) at the given +offset+.
    #
    # The +length+ argument is the "minimum length to be written". If the IO
    # buffer size is 8KiB, but the +length+ specified is 1024 (1KiB), at most 8KiB
    # will be written, but at least 1KiB will be. Generally, the only case where
    # less data than +length+ will be written is if there is an error writing the
    # data.
    #
    # Specifying a +length+ of 0 is valid and means try writing at least once, as
    # much data as possible.
    #
    # Suggested implementation should try to write to +io+ in a non-blocking
    # manner and call #io_wait if the +io+ is not ready (which will yield control
    # to other fibers).
    #
    # See IO::Buffer for an interface available to get data from buffer
    # efficiently.
    #
    # Expected to return number of bytes written, or, in case of an error,
    # <tt>-errno</tt> (negated number corresponding to system's error code).
    #
    # The method should be considered _experimental_.
    def io_write(io, buffer, length, offset) end

    # Invoked by Kernel#sleep and Mutex#sleep and is expected to provide
    # an implementation of sleeping in a non-blocking way. Implementation might
    # register the current fiber in some list of "which fiber wait until what
    # moment", call Fiber.yield to pass control, and then in #close resume
    # the fibers whose wait period has elapsed.
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
    # Expected to return a Process::Status instance.
    def process_wait(pid, flags) end

    # Invoked by Timeout.timeout to execute the given +block+ within the given
    # +duration+. It can also be invoked directly by the scheduler or user code.
    #
    # Attempt to limit the execution time of a given +block+ to the given
    # +duration+ if possible. When a non-blocking operation causes the +block+'s
    # execution time to exceed the specified +duration+, that non-blocking
    # operation should be interrupted by raising the specified +exception_class+
    # constructed with the given +exception_arguments+.
    #
    # General execution timeouts are often considered risky. This implementation
    # will only interrupt non-blocking operations. This is by design because it's
    # expected that non-blocking operations can fail for a variety of
    # unpredictable reasons, so applications should already be robust in handling
    # these conditions and by implication timeouts.
    #
    # However, as a result of this design, if the +block+ does not invoke any
    # non-blocking operations, it will be impossible to interrupt it. If you
    # desire to provide predictable points for timeouts, consider adding
    # +sleep(0)+.
    #
    # If the block is executed successfully, its result will be returned.
    #
    # The exception will typically be raised using Fiber#raise.
    def timeout_after(duration, exception_class, *exception_arguments, &) end

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
