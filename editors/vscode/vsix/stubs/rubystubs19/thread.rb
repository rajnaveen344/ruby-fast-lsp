# frozen_string_literal: true

# ::Thread
class Thread
  MUTEX_FOR_THREAD_EXCLUSIVE = _

  # Returns the thread debug level.  Available only if compiled with
  # THREAD_DEBUG=-1.
  def self.DEBUG; end

  # Sets the thread debug level.  Available only if compiled with
  # THREAD_DEBUG=-1.
  def self.DEBUG=(num) end

  # Returns the status of the global ``abort on exception'' condition.  The
  # default is <code>false</code>. When set to <code>true</code>, or if the
  # global <code>$DEBUG</code> flag is <code>true</code> (perhaps because the
  # command line option <code>-d</code> was specified) all threads will abort
  # (the process will <code>exit(0)</code>) if an exception is raised in any
  # thread. See also <code>Thread::abort_on_exception=</code>.
  def self.abort_on_exception; end

  # When set to <code>true</code>, all threads will abort if an exception is
  # raised. Returns the new state.
  #
  #    Thread.abort_on_exception = true
  #    t1 = Thread.new do
  #      puts  "In new thread"
  #      raise "Exception from thread"
  #    end
  #    sleep(1)
  #    puts "not reached"
  #
  # <em>produces:</em>
  #
  #    In new thread
  #    prog.rb:4: Exception from thread (RuntimeError)
  #     from prog.rb:2:in `initialize'
  #     from prog.rb:2:in `new'
  #     from prog.rb:2
  def self.abort_on_exception=(boolean) end

  # Returns the currently executing thread.
  #
  #    Thread.current   #=> #<Thread:0x401bdf4c run>
  def self.current; end

  # Wraps a block in Thread.critical, restoring the original value
  # upon exit from the critical section, and returns the value of the
  # block.
  def self.exclusive; end

  # Terminates the currently running thread and schedules another thread to be
  # run. If this thread is already marked to be killed, <code>exit</code>
  # returns the <code>Thread</code>. If this is the main thread, or the last
  # thread, exit the process.
  def self.exit; end

  # Basically the same as <code>Thread::new</code>. However, if class
  # <code>Thread</code> is subclassed, then calling <code>start</code> in that
  # subclass will not invoke the subclass's <code>initialize</code> method.
  def self.fork(*args) end

  # Causes the given <em>thread</em> to exit (see <code>Thread::exit</code>).
  #
  #    count = 0
  #    a = Thread.new { loop { count += 1 } }
  #    sleep(0.1)       #=> 0
  #    Thread.kill(a)   #=> #<Thread:0x401b3d30 dead>
  #    count            #=> 93947
  #    a.alive?         #=> false
  def self.kill(thread) end

  # Returns an array of <code>Thread</code> objects for all threads that are
  # either runnable or stopped.
  #
  #    Thread.new { sleep(200) }
  #    Thread.new { 1000000.times {|i| i*i } }
  #    Thread.new { Thread.stop }
  #    Thread.list.each {|t| p t}
  #
  # <em>produces:</em>
  #
  #    #<Thread:0x401b3e84 sleep>
  #    #<Thread:0x401b3f38 run>
  #    #<Thread:0x401b3fb0 sleep>
  #    #<Thread:0x401bdf4c run>
  def self.list; end

  # Returns the main thread.
  def self.main; end

  # Give the thread scheduler a hint to pass execution to another thread.
  # A running thread may or may not switch, it depends on OS and processor.
  def self.pass; end

  # Basically the same as <code>Thread::new</code>. However, if class
  # <code>Thread</code> is subclassed, then calling <code>start</code> in that
  # subclass will not invoke the subclass's <code>initialize</code> method.
  def self.start(*args) end

  # Stops execution of the current thread, putting it into a ``sleep'' state,
  # and schedules execution of another thread.
  #
  #    a = Thread.new { print "a"; Thread.stop; print "c" }
  #    sleep 0.1 while a.status!='sleep'
  #    print "b"
  #    a.run
  #    a.join
  #
  # <em>produces:</em>
  #
  #    abc
  def self.stop; end

  # Attribute Reference---Returns the value of a thread-local variable, using
  # either a symbol or a string name. If the specified variable does not exist,
  # returns <code>nil</code>.
  #
  #    [
  #      Thread.new { Thread.current["name"] = "A" },
  #      Thread.new { Thread.current[:name]  = "B" },
  #      Thread.new { Thread.current["name"] = "C" }
  #    ].each do |th|
  #      th.join
  #      puts "#{th.inspect}: #{th[:name]}"
  #    end
  #
  # <em>produces:</em>
  #
  #    #<Thread:0x00000002a54220 dead>: A
  #    #<Thread:0x00000002a541a8 dead>: B
  #    #<Thread:0x00000002a54130 dead>: C
  def [](sym) end

  # Attribute Assignment---Sets or creates the value of a thread-local variable,
  # using either a symbol or a string. See also <code>Thread#[]</code>.
  def []=(sym, obj) end

  # Returns the status of the thread-local ``abort on exception'' condition for
  # <i>thr</i>. The default is <code>false</code>. See also
  # <code>Thread::abort_on_exception=</code>.
  def abort_on_exception; end

  # When set to <code>true</code>, causes all threads (including the main
  # program) to abort if an exception is raised in <i>thr</i>. The process will
  # effectively <code>exit(0)</code>.
  def abort_on_exception=(boolean) end

  # Adds _proc_ as a handler for tracing.
  # See <code>Thread#set_trace_func</code> and +set_trace_func+.
  def add_trace_func(proc) end

  # Returns <code>true</code> if <i>thr</i> is running or sleeping.
  #
  #    thr = Thread.new { }
  #    thr.join                #=> #<Thread:0x401b3fb0 dead>
  #    Thread.current.alive?   #=> true
  #    thr.alive?              #=> false
  def alive?; end

  # Returns the current back trace of the _thr_.
  def backtrace; end

  # Returns the <code>ThreadGroup</code> which contains <i>thr</i>, or nil if
  # the thread is not a member of any group.
  #
  #    Thread.main.group   #=> #<ThreadGroup:0x4029d914>
  def group; end

  # Dump the name, id, and status of _thr_ to a string.
  def inspect; end

  # The calling thread will suspend execution and run <i>thr</i>. Does not
  # return until <i>thr</i> exits or until <i>limit</i> seconds have passed. If
  # the time limit expires, <code>nil</code> will be returned, otherwise
  # <i>thr</i> is returned.
  #
  # Any threads not joined will be killed when the main program exits.  If
  # <i>thr</i> had previously raised an exception and the
  # <code>abort_on_exception</code> and <code>$DEBUG</code> flags are not set
  # (so the exception has not yet been processed) it will be processed at this
  # time.
  #
  #    a = Thread.new { print "a"; sleep(10); print "b"; print "c" }
  #    x = Thread.new { print "x"; Thread.pass; print "y"; print "z" }
  #    x.join # Let x thread finish, a will be killed on exit.
  #
  # <em>produces:</em>
  #
  #    axyz
  #
  # The following example illustrates the <i>limit</i> parameter.
  #
  #    y = Thread.new { 4.times { sleep 0.1; puts 'tick... ' }}
  #    puts "Waiting" until y.join(0.15)
  #
  # <em>produces:</em>
  #
  #    tick...
  #    Waiting
  #    tick...
  #    Waitingtick...
  #
  #    tick...
  def join(*several_variants) end

  # Returns <code>true</code> if the given string (or symbol) exists as a
  # thread-local variable.
  #
  #    me = Thread.current
  #    me[:oliver] = "a"
  #    me.key?(:oliver)    #=> true
  #    me.key?(:stanley)   #=> false
  def key?(sym) end

  # Returns an an array of the names of the thread-local variables (as Symbols).
  #
  #    thr = Thread.new do
  #      Thread.current[:cat] = 'meow'
  #      Thread.current["dog"] = 'woof'
  #    end
  #    thr.join   #=> #<Thread:0x401b3f10 dead>
  #    thr.keys   #=> [:dog, :cat]
  def keys; end

  # Terminates <i>thr</i> and schedules another thread to be run. If this thread
  # is already marked to be killed, <code>exit</code> returns the
  # <code>Thread</code>. If this is the main thread, or the last thread, exits
  # the process.
  def kill; end
  alias terminate kill
  alias exit kill

  # Returns the priority of <i>thr</i>. Default is inherited from the
  # current thread which creating the new thread, or zero for the
  # initial main thread; higher-priority thread will run more frequently
  # than lower-priority threads (but lower-priority threads can also run).
  #
  # This is just hint for Ruby thread scheduler.  It may be ignored on some
  # platform.
  #
  #    Thread.current.priority   #=> 0
  def priority; end

  # Sets the priority of <i>thr</i> to <i>integer</i>. Higher-priority threads
  # will run more frequently than lower-priority threads (but lower-priority
  # threads can also run).
  #
  # This is just hint for Ruby thread scheduler.  It may be ignored on some
  # platform.
  #
  #    count1 = count2 = 0
  #    a = Thread.new do
  #          loop { count1 += 1 }
  #        end
  #    a.priority = -1
  #
  #    b = Thread.new do
  #          loop { count2 += 1 }
  #        end
  #    b.priority = -2
  #    sleep 1   #=> 1
  #    count1    #=> 622504
  #    count2    #=> 5832
  def priority=(integer) end

  # Raises an exception (see <code>Kernel::raise</code>) from <i>thr</i>. The
  # caller does not have to be <i>thr</i>.
  #
  #    Thread.abort_on_exception = true
  #    a = Thread.new { sleep(200) }
  #    a.raise("Gotcha")
  #
  # <em>produces:</em>
  #
  #    prog.rb:3: Gotcha (RuntimeError)
  #     from prog.rb:2:in `initialize'
  #     from prog.rb:2:in `new'
  #     from prog.rb:2
  def raise(*several_variants) end

  # Wakes up <i>thr</i>, making it eligible for scheduling.
  #
  #    a = Thread.new { puts "a"; Thread.stop; puts "c" }
  #    sleep 0.1 while a.status!='sleep'
  #    puts "Got here"
  #    a.run
  #    a.join
  #
  # <em>produces:</em>
  #
  #    a
  #    Got here
  #    c
  def run; end

  # Returns the safe level in effect for <i>thr</i>. Setting thread-local safe
  # levels can help when implementing sandboxes which run insecure code.
  #
  #    thr = Thread.new { $SAFE = 3; sleep }
  #    Thread.current.safe_level   #=> 0
  #    thr.safe_level              #=> 3
  def safe_level; end

  # Establishes _proc_ on _thr_ as the handler for tracing, or
  # disables tracing if the parameter is +nil+.
  # See +set_trace_func+.
  def set_trace_func(*several_variants) end

  # Returns the status of <i>thr</i>: ``<code>sleep</code>'' if <i>thr</i> is
  # sleeping or waiting on I/O, ``<code>run</code>'' if <i>thr</i> is executing,
  # ``<code>aborting</code>'' if <i>thr</i> is aborting, <code>false</code> if
  # <i>thr</i> terminated normally, and <code>nil</code> if <i>thr</i>
  # terminated with an exception.
  #
  #    a = Thread.new { raise("die now") }
  #    b = Thread.new { Thread.stop }
  #    c = Thread.new { Thread.exit }
  #    d = Thread.new { sleep }
  #    d.kill                  #=> #<Thread:0x401b3678 aborting>
  #    a.status                #=> nil
  #    b.status                #=> "sleep"
  #    c.status                #=> false
  #    d.status                #=> "aborting"
  #    Thread.current.status   #=> "run"
  def status; end

  # Returns <code>true</code> if <i>thr</i> is dead or sleeping.
  #
  #    a = Thread.new { Thread.stop }
  #    b = Thread.current
  #    a.stop?   #=> true
  #    b.stop?   #=> false
  def stop?; end

  # Waits for <i>thr</i> to complete (via <code>Thread#join</code>) and returns
  # its value.
  #
  #    a = Thread.new { 2 + 2 }
  #    a.value   #=> 4
  def value; end

  # Marks <i>thr</i> as eligible for scheduling (it may still remain blocked on
  # I/O, however). Does not invoke the scheduler (see <code>Thread#run</code>).
  #
  #    c = Thread.new { Thread.stop; puts "hey!" }
  #    sleep 0.1 while c.status!='sleep'
  #    c.wakeup
  #    c.join
  #
  # <em>produces:</em>
  #
  #    hey!
  def wakeup; end
end
