# frozen_string_literal: true

# The <code>Process</code> module is a collection of methods used to
# manipulate processes.
module Process
  PRIO_PGRP = _
  PRIO_PROCESS = _
  PRIO_USER = _
  RLIMIT_AS = _
  RLIMIT_CORE = _
  RLIMIT_CPU = _
  RLIMIT_DATA = _
  RLIMIT_FSIZE = _
  RLIMIT_MEMLOCK = _
  RLIMIT_NOFILE = _
  RLIMIT_NPROC = _
  RLIMIT_RSS = _
  RLIMIT_SBSIZE = _
  RLIMIT_STACK = _
  RLIM_INFINITY = _
  RLIM_SAVED_CUR = _
  RLIM_SAVED_MAX = _
  WNOHANG = _
  WUNTRACED = _

  # Terminate execution immediately, effectively by calling
  # <code>Kernel.exit(1)</code>. If _msg_ is given, it is written
  # to STDERR prior to terminating.
  def self.abort(message = '') end

  # Some operating systems retain the status of terminated child
  # processes until the parent collects that status (normally using
  # some variant of <code>wait()</code>. If the parent never collects
  # this status, the child stays around as a <em>zombie</em> process.
  # <code>Process::detach</code> prevents this by setting up a
  # separate Ruby thread whose sole job is to reap the status of the
  # process _pid_ when it terminates. Use <code>detach</code>
  # only when you do not intent to explicitly wait for the child to
  # terminate.  <code>detach</code> only checks the status
  # periodically (currently once each second).
  #
  # The waiting thread returns the exit status of the detached process
  # when it terminates, so you can use <code>Thread#join</code> to
  # know the result.  If specified _pid_ is not a valid child process
  # ID, the thread returns +nil+ immediately.
  #
  # In this first example, we don't reap the first child process, so
  # it appears as a zombie in the process status display.
  #
  #    p1 = fork { sleep 0.1 }
  #    p2 = fork { sleep 0.2 }
  #    Process.waitpid(p2)
  #    sleep 2
  #    system("ps -ho pid,state -p #{p1}")
  #
  # <em>produces:</em>
  #
  #    27389 Z
  #
  # In the next example, <code>Process::detach</code> is used to reap
  # the child automatically.
  #
  #    p1 = fork { sleep 0.1 }
  #    p2 = fork { sleep 0.2 }
  #    Process.detach(p1)
  #    Process.waitpid(p2)
  #    sleep 2
  #    system("ps -ho pid,state -p #{p1}")
  #
  # <em>(produces no output)</em>
  def self.detach(pid) end

  # Returns the effective group ID for this process. Not available on
  # all platforms.
  #
  #    Process.egid   #=> 500
  def self.egid; end

  # Sets the effective group ID for this process. Not available on all
  # platforms.
  def self.egid=(fixnum) end

  # Returns the effective user ID for this process.
  #
  #    Process.euid   #=> 501
  def self.euid; end

  # Sets the effective user ID for this process. Not available on all
  # platforms.
  def self.euid=(integer) end

  # Replaces the current process by running the given external _command_.
  # If +exec+ is given a single argument, that argument is
  # taken as a line that is subject to shell expansion before being
  # executed. If multiple arguments are given, the second and subsequent
  # arguments are passed as parameters to _command_ with no shell
  # expansion. If the first argument is a two-element array, the first
  # element is the command to be executed, and the second argument is
  # used as the <code>argv[0]</code> value, which may show up in process
  # listings. In MSDOS environments, the command is executed in a
  # subshell; otherwise, one of the <code>exec(2)</code> system calls is
  # used, so the running command may inherit some of the environment of
  # the original program (including open file descriptors).
  #
  #    exec "echo *"       # echoes list of files in current directory
  #    # never get here
  #
  #    exec "echo", "*"    # echoes an asterisk
  #    # never get here
  def self.exec(command, *arg) end

  # Initiates the termination of the Ruby script by raising the
  # <code>SystemExit</code> exception. This exception may be caught. The
  # optional parameter is used to return a status code to the invoking
  # environment.
  #
  #    begin
  #      exit
  #      puts "never get here"
  #    rescue SystemExit
  #      puts "rescued a SystemExit exception"
  #    end
  #    puts "after begin block"
  #
  # <em>produces:</em>
  #
  #    rescued a SystemExit exception
  #    after begin block
  #
  # Just prior to termination, Ruby executes any <code>at_exit</code> functions
  # (see Kernel::at_exit) and runs any object finalizers (see
  # ObjectSpace::define_finalizer).
  #
  #    at_exit { puts "at_exit function" }
  #    ObjectSpace.define_finalizer("string",  proc { puts "in finalizer" })
  #    exit
  #
  # <em>produces:</em>
  #
  #    at_exit function
  #    in finalizer
  def self.exit(integer = 0) end

  # Exits the process immediately. No exit handlers are
  # run. <em>fixnum</em> is returned to the underlying system as the
  # exit status.
  #
  #    Process.exit!(0)
  def self.exit!(fixnum = -1) end

  # Creates a subprocess. If a block is specified, that block is run
  # in the subprocess, and the subprocess terminates with a status of
  # zero. Otherwise, the +fork+ call returns twice, once in
  # the parent, returning the process ID of the child, and once in
  # the child, returning _nil_. The child process can exit using
  # <code>Kernel.exit!</code> to avoid running any
  # <code>at_exit</code> functions. The parent process should
  # use <code>Process.wait</code> to collect the termination statuses
  # of its children or use <code>Process.detach</code> to register
  # disinterest in their status; otherwise, the operating system
  # may accumulate zombie processes.
  #
  # The thread calling fork is the only thread in the created child process.
  # fork doesn't copy other threads.
  def self.fork; end

  # Returns the process group ID for the given process id. Not
  # available on all platforms.
  #
  #    Process.getpgid(Process.ppid())   #=> 25527
  def self.getpgid(pid) end

  # Returns the process group ID for this process. Not available on
  # all platforms.
  #
  #    Process.getpgid(0)   #=> 25527
  #    Process.getpgrp      #=> 25527
  def self.getpgrp; end

  # Gets the scheduling priority for specified process, process group,
  # or user. <em>kind</em> indicates the kind of entity to find: one
  # of <code>Process::PRIO_PGRP</code>,
  # <code>Process::PRIO_USER</code>, or
  # <code>Process::PRIO_PROCESS</code>. _integer_ is an id
  # indicating the particular process, process group, or user (an id
  # of 0 means _current_). Lower priorities are more favorable
  # for scheduling. Not available on all platforms.
  #
  #    Process.getpriority(Process::PRIO_USER, 0)      #=> 19
  #    Process.getpriority(Process::PRIO_PROCESS, 0)   #=> 19
  def self.getpriority(kind, integer) end

  # Gets the resource limit of the process.
  # _cur_limit_ means current (soft) limit and
  # _max_limit_ means maximum (hard) limit.
  #
  # _resource_ indicates the kind of resource to limit:
  # such as <code>Process::RLIMIT_CORE</code>,
  # <code>Process::RLIMIT_CPU</code>, etc.
  # See Process.setrlimit for details.
  #
  # _cur_limit_ and _max_limit_ may be <code>Process::RLIM_INFINITY</code>,
  # <code>Process::RLIM_SAVED_MAX</code> or
  # <code>Process::RLIM_SAVED_CUR</code>.
  # See Process.setrlimit and the system getrlimit(2) manual for details.
  def self.getrlimit(resource) end

  # Returns the (real) group ID for this process.
  #
  #    Process.gid   #=> 500
  def self.gid; end

  # Sets the group ID for this process.
  def self.gid=(fixnum) end

  # Get an <code>Array</code> of the gids of groups in the
  # supplemental group access list for this process.
  #
  #    Process.groups   #=> [27, 6, 10, 11]
  def self.groups; end

  # Set the supplemental group access list to the given
  # <code>Array</code> of group IDs.
  #
  #    Process.groups   #=> [0, 1, 2, 3, 4, 6, 10, 11, 20, 26, 27]
  #    Process.groups = [27, 6, 10, 11]   #=> [27, 6, 10, 11]
  #    Process.groups   #=> [27, 6, 10, 11]
  def self.groups=(array) end

  # Initializes the supplemental group access list by reading the
  # system group database and using all groups of which the given user
  # is a member. The group with the specified <em>gid</em> is also
  # added to the list. Returns the resulting <code>Array</code> of the
  # gids of all the groups in the supplementary group access list. Not
  # available on all platforms.
  #
  #    Process.groups   #=> [0, 1, 2, 3, 4, 6, 10, 11, 20, 26, 27]
  #    Process.initgroups( "mgranger", 30 )   #=> [30, 6, 10, 11]
  #    Process.groups   #=> [30, 6, 10, 11]
  def self.initgroups(username, gid) end

  # Sends the given signal to the specified process id(s), or to the
  # current process if _pid_ is zero. _signal_ may be an
  # integer signal number or a POSIX signal name (either with or without
  # a +SIG+ prefix). If _signal_ is negative (or starts
  # with a minus sign), kills process groups instead of
  # processes. Not all signals are available on all platforms.
  #
  #    pid = fork do
  #       Signal.trap("HUP") { puts "Ouch!"; exit }
  #       # ... do some work ...
  #    end
  #    # ...
  #    Process.kill("HUP", pid)
  #    Process.wait
  #
  # <em>produces:</em>
  #
  #    Ouch!
  def self.kill(signal, pid, *args) end

  # Returns the maximum number of gids allowed in the supplemental
  # group access list.
  #
  #    Process.maxgroups   #=> 32
  def self.maxgroups; end

  # Sets the maximum number of gids allowed in the supplemental group
  # access list.
  def self.maxgroups=(fixnum) end

  # Returns the process id of this process. Not available on all
  # platforms.
  #
  #    Process.pid   #=> 27415
  def self.pid; end

  # Returns the process id of the parent of this process. Always
  # returns 0 on NT. Not available on all platforms.
  #
  #    puts "I am #{Process.pid}"
  #    Process.fork { puts "Dad is #{Process.ppid}" }
  #
  # <em>produces:</em>
  #
  #    I am 27417
  #    Dad is 27417
  def self.ppid; end

  # Sets the process group ID of _pid_ (0 indicates this
  # process) to <em>integer</em>. Not available on all platforms.
  def self.setpgid(pid, integer) end

  # Equivalent to <code>setpgid(0,0)</code>. Not available on all
  # platforms.
  def self.setpgrp; end

  # See <code>Process#getpriority</code>.
  #
  #    Process.setpriority(Process::PRIO_USER, 0, 19)      #=> 0
  #    Process.setpriority(Process::PRIO_PROCESS, 0, 19)   #=> 0
  #    Process.getpriority(Process::PRIO_USER, 0)          #=> 19
  #    Process.getpriority(Process::PRIO_PROCESS, 0)       #=> 19
  def self.setpriority(kind, integer, priority) end

  # Sets the resource limit of the process.
  # _cur_limit_ means current (soft) limit and
  # _max_limit_ means maximum (hard) limit.
  #
  # If _max_limit_ is not given, _cur_limit_ is used.
  #
  # _resource_ indicates the kind of resource to limit.
  # The list of resources are OS dependent.
  # Ruby may support following resources.
  #
  # [Process::RLIMIT_CORE] core size (bytes) (SUSv3)
  # [Process::RLIMIT_CPU] CPU time (seconds) (SUSv3)
  # [Process::RLIMIT_DATA] data segment (bytes) (SUSv3)
  # [Process::RLIMIT_FSIZE] file size (bytes) (SUSv3)
  # [Process::RLIMIT_NOFILE] file descriptors (number) (SUSv3)
  # [Process::RLIMIT_STACK] stack size (bytes) (SUSv3)
  # [Process::RLIMIT_AS] total available memory (bytes) (SUSv3, NetBSD, FreeBSD, OpenBSD but 4.4BSD-Lite)
  # [Process::RLIMIT_MEMLOCK] total size for mlock(2) (bytes) (4.4BSD, GNU/Linux)
  # [Process::RLIMIT_NPROC] number of processes for the user (number) (4.4BSD, GNU/Linux)
  # [Process::RLIMIT_RSS] resident memory size (bytes) (4.2BSD, GNU/Linux)
  # [Process::RLIMIT_SBSIZE] all socket buffers (bytes) (NetBSD, FreeBSD)
  #
  # Other <code>Process::RLIMIT_???</code> constants may be defined.
  #
  # _cur_limit_ and _max_limit_ may be <code>Process::RLIM_INFINITY</code>,
  # which means that the resource is not limited.
  # They may be <code>Process::RLIM_SAVED_MAX</code> or
  # <code>Process::RLIM_SAVED_CUR</code> too.
  # See system setrlimit(2) manual for details.
  def self.setrlimit(*several_variants) end

  # Establishes this process as a new session and process group
  # leader, with no controlling tty. Returns the session id. Not
  # available on all platforms.
  #
  #    Process.setsid   #=> 27422
  def self.setsid; end

  # Returns a <code>Tms</code> structure (see <code>Struct::Tms</code>
  # on page 388) that contains user and system CPU times for this
  # process.
  #
  #    t = Process.times
  #    [ t.utime, t.stime ]   #=> [0.0, 0.02]
  def self.times; end

  # Returns the (real) user ID of this process.
  #
  #    Process.uid   #=> 501
  def self.uid; end

  # Sets the (integer) user ID for this process. Not available on all
  # platforms.
  def self.uid=(integer) end

  # Waits for a child process to exit, returns its process id, and
  # sets <code>$?</code> to a <code>Process::Status</code> object
  # containing information on that process. Which child it waits on
  # depends on the value of _pid_:
  #
  # > 0::   Waits for the child whose process ID equals _pid_.
  #
  # 0::     Waits for any child whose process group ID equals that of the
  #         calling process.
  #
  # -1::    Waits for any child process (the default if no _pid_ is
  #         given).
  #
  # < -1::  Waits for any child whose process group ID equals the absolute
  #         value of _pid_.
  #
  # The _flags_ argument may be a logical or of the flag values
  # <code>Process::WNOHANG</code> (do not block if no child available)
  # or <code>Process::WUNTRACED</code> (return stopped children that
  # haven't been reported). Not all flags are available on all
  # platforms, but a flag value of zero will work on all platforms.
  #
  # Calling this method raises a <code>SystemError</code> if there are
  # no child processes. Not available on all platforms.
  #
  #    include Process
  #    fork { exit 99 }                 #=> 27429
  #    wait                             #=> 27429
  #    $?.exitstatus                    #=> 99
  #
  #    pid = fork { sleep 3 }           #=> 27440
  #    Time.now                         #=> Wed Apr 09 08:57:09 CDT 2003
  #    waitpid(pid, Process::WNOHANG)   #=> nil
  #    Time.now                         #=> Wed Apr 09 08:57:09 CDT 2003
  #    waitpid(pid, 0)                  #=> 27440
  #    Time.now                         #=> Wed Apr 09 08:57:12 CDT 2003
  def self.wait(*several_variants) end

  # Waits for a child process to exit (see Process::waitpid for exact
  # semantics) and returns an array containing the process id and the
  # exit status (a <code>Process::Status</code> object) of that
  # child. Raises a <code>SystemError</code> if there are no child
  # processes.
  #
  #    Process.fork { exit 99 }   #=> 27437
  #    pid, status = Process.wait2
  #    pid                        #=> 27437
  #    status.exitstatus          #=> 99
  def self.wait2(pid = -1, flags = 0) end

  # Waits for all children, returning an array of
  # _pid_/_status_ pairs (where _status_ is a
  # <code>Process::Status</code> object).
  #
  #    fork { sleep 0.2; exit 2 }   #=> 27432
  #    fork { sleep 0.1; exit 1 }   #=> 27433
  #    fork {            exit 0 }   #=> 27434
  #    p Process.waitall
  #
  # <em>produces</em>:
  #
  #    [[27434, #<Process::Status: pid=27434,exited(0)>],
  #     [27433, #<Process::Status: pid=27433,exited(1)>],
  #     [27432, #<Process::Status: pid=27432,exited(2)>]]
  def self.waitall; end

  # Waits for a child process to exit, returns its process id, and
  # sets <code>$?</code> to a <code>Process::Status</code> object
  # containing information on that process. Which child it waits on
  # depends on the value of _pid_:
  #
  # > 0::   Waits for the child whose process ID equals _pid_.
  #
  # 0::     Waits for any child whose process group ID equals that of the
  #         calling process.
  #
  # -1::    Waits for any child process (the default if no _pid_ is
  #         given).
  #
  # < -1::  Waits for any child whose process group ID equals the absolute
  #         value of _pid_.
  #
  # The _flags_ argument may be a logical or of the flag values
  # <code>Process::WNOHANG</code> (do not block if no child available)
  # or <code>Process::WUNTRACED</code> (return stopped children that
  # haven't been reported). Not all flags are available on all
  # platforms, but a flag value of zero will work on all platforms.
  #
  # Calling this method raises a <code>SystemError</code> if there are
  # no child processes. Not available on all platforms.
  #
  #    include Process
  #    fork { exit 99 }                 #=> 27429
  #    wait                             #=> 27429
  #    $?.exitstatus                    #=> 99
  #
  #    pid = fork { sleep 3 }           #=> 27440
  #    Time.now                         #=> Wed Apr 09 08:57:09 CDT 2003
  #    waitpid(pid, Process::WNOHANG)   #=> nil
  #    Time.now                         #=> Wed Apr 09 08:57:09 CDT 2003
  #    waitpid(pid, 0)                  #=> 27440
  #    Time.now                         #=> Wed Apr 09 08:57:12 CDT 2003
  def self.waitpid(pid = -1, flags = 0) end

  # Waits for a child process to exit (see Process::waitpid for exact
  # semantics) and returns an array containing the process id and the
  # exit status (a <code>Process::Status</code> object) of that
  # child. Raises a <code>SystemError</code> if there are no child
  # processes.
  #
  #    Process.fork { exit 99 }   #=> 27437
  #    pid, status = Process.wait2
  #    pid                        #=> 27437
  #    status.exitstatus          #=> 99
  def self.waitpid2(pid = -1, flags = 0) end

  private

  # Some operating systems retain the status of terminated child
  # processes until the parent collects that status (normally using
  # some variant of <code>wait()</code>. If the parent never collects
  # this status, the child stays around as a <em>zombie</em> process.
  # <code>Process::detach</code> prevents this by setting up a
  # separate Ruby thread whose sole job is to reap the status of the
  # process _pid_ when it terminates. Use <code>detach</code>
  # only when you do not intent to explicitly wait for the child to
  # terminate.  <code>detach</code> only checks the status
  # periodically (currently once each second).
  #
  # The waiting thread returns the exit status of the detached process
  # when it terminates, so you can use <code>Thread#join</code> to
  # know the result.  If specified _pid_ is not a valid child process
  # ID, the thread returns +nil+ immediately.
  #
  # In this first example, we don't reap the first child process, so
  # it appears as a zombie in the process status display.
  #
  #    p1 = fork { sleep 0.1 }
  #    p2 = fork { sleep 0.2 }
  #    Process.waitpid(p2)
  #    sleep 2
  #    system("ps -ho pid,state -p #{p1}")
  #
  # <em>produces:</em>
  #
  #    27389 Z
  #
  # In the next example, <code>Process::detach</code> is used to reap
  # the child automatically.
  #
  #    p1 = fork { sleep 0.1 }
  #    p2 = fork { sleep 0.2 }
  #    Process.detach(p1)
  #    Process.waitpid(p2)
  #    sleep 2
  #    system("ps -ho pid,state -p #{p1}")
  #
  # <em>(produces no output)</em>
  def detach(pid) end

  # Returns the effective group ID for this process. Not available on
  # all platforms.
  #
  #    Process.egid   #=> 500
  def egid; end

  # Sets the effective group ID for this process. Not available on all
  # platforms.
  def egid=(fixnum) end

  # Returns the effective user ID for this process.
  #
  #    Process.euid   #=> 501
  def euid; end

  # Sets the effective user ID for this process. Not available on all
  # platforms.
  def euid=(integer) end

  # Returns the process group ID for the given process id. Not
  # available on all platforms.
  #
  #    Process.getpgid(Process.ppid())   #=> 25527
  def getpgid(pid) end

  # Returns the process group ID for this process. Not available on
  # all platforms.
  #
  #    Process.getpgid(0)   #=> 25527
  #    Process.getpgrp      #=> 25527
  def getpgrp; end

  # Gets the scheduling priority for specified process, process group,
  # or user. <em>kind</em> indicates the kind of entity to find: one
  # of <code>Process::PRIO_PGRP</code>,
  # <code>Process::PRIO_USER</code>, or
  # <code>Process::PRIO_PROCESS</code>. _integer_ is an id
  # indicating the particular process, process group, or user (an id
  # of 0 means _current_). Lower priorities are more favorable
  # for scheduling. Not available on all platforms.
  #
  #    Process.getpriority(Process::PRIO_USER, 0)      #=> 19
  #    Process.getpriority(Process::PRIO_PROCESS, 0)   #=> 19
  def getpriority(kind, integer) end

  # Gets the resource limit of the process.
  # _cur_limit_ means current (soft) limit and
  # _max_limit_ means maximum (hard) limit.
  #
  # _resource_ indicates the kind of resource to limit:
  # such as <code>Process::RLIMIT_CORE</code>,
  # <code>Process::RLIMIT_CPU</code>, etc.
  # See Process.setrlimit for details.
  #
  # _cur_limit_ and _max_limit_ may be <code>Process::RLIM_INFINITY</code>,
  # <code>Process::RLIM_SAVED_MAX</code> or
  # <code>Process::RLIM_SAVED_CUR</code>.
  # See Process.setrlimit and the system getrlimit(2) manual for details.
  def getrlimit(resource) end

  # Returns the (real) group ID for this process.
  #
  #    Process.gid   #=> 500
  def gid; end

  # Sets the group ID for this process.
  def gid=(fixnum) end

  # Get an <code>Array</code> of the gids of groups in the
  # supplemental group access list for this process.
  #
  #    Process.groups   #=> [27, 6, 10, 11]
  def groups; end

  # Set the supplemental group access list to the given
  # <code>Array</code> of group IDs.
  #
  #    Process.groups   #=> [0, 1, 2, 3, 4, 6, 10, 11, 20, 26, 27]
  #    Process.groups = [27, 6, 10, 11]   #=> [27, 6, 10, 11]
  #    Process.groups   #=> [27, 6, 10, 11]
  def groups=(array) end

  # Initializes the supplemental group access list by reading the
  # system group database and using all groups of which the given user
  # is a member. The group with the specified <em>gid</em> is also
  # added to the list. Returns the resulting <code>Array</code> of the
  # gids of all the groups in the supplementary group access list. Not
  # available on all platforms.
  #
  #    Process.groups   #=> [0, 1, 2, 3, 4, 6, 10, 11, 20, 26, 27]
  #    Process.initgroups( "mgranger", 30 )   #=> [30, 6, 10, 11]
  #    Process.groups   #=> [30, 6, 10, 11]
  def initgroups(username, gid) end

  # Sends the given signal to the specified process id(s), or to the
  # current process if _pid_ is zero. _signal_ may be an
  # integer signal number or a POSIX signal name (either with or without
  # a +SIG+ prefix). If _signal_ is negative (or starts
  # with a minus sign), kills process groups instead of
  # processes. Not all signals are available on all platforms.
  #
  #    pid = fork do
  #       Signal.trap("HUP") { puts "Ouch!"; exit }
  #       # ... do some work ...
  #    end
  #    # ...
  #    Process.kill("HUP", pid)
  #    Process.wait
  #
  # <em>produces:</em>
  #
  #    Ouch!
  def kill(signal, pid, *args) end

  # Returns the maximum number of gids allowed in the supplemental
  # group access list.
  #
  #    Process.maxgroups   #=> 32
  def maxgroups; end

  # Sets the maximum number of gids allowed in the supplemental group
  # access list.
  def maxgroups=(fixnum) end

  # Returns the process id of this process. Not available on all
  # platforms.
  #
  #    Process.pid   #=> 27415
  def pid; end

  # Returns the process id of the parent of this process. Always
  # returns 0 on NT. Not available on all platforms.
  #
  #    puts "I am #{Process.pid}"
  #    Process.fork { puts "Dad is #{Process.ppid}" }
  #
  # <em>produces:</em>
  #
  #    I am 27417
  #    Dad is 27417
  def ppid; end

  # Sets the process group ID of _pid_ (0 indicates this
  # process) to <em>integer</em>. Not available on all platforms.
  def setpgid(pid, integer) end

  # Equivalent to <code>setpgid(0,0)</code>. Not available on all
  # platforms.
  def setpgrp; end

  # See <code>Process#getpriority</code>.
  #
  #    Process.setpriority(Process::PRIO_USER, 0, 19)      #=> 0
  #    Process.setpriority(Process::PRIO_PROCESS, 0, 19)   #=> 0
  #    Process.getpriority(Process::PRIO_USER, 0)          #=> 19
  #    Process.getpriority(Process::PRIO_PROCESS, 0)       #=> 19
  def setpriority(kind, integer, priority) end

  # Sets the resource limit of the process.
  # _cur_limit_ means current (soft) limit and
  # _max_limit_ means maximum (hard) limit.
  #
  # If _max_limit_ is not given, _cur_limit_ is used.
  #
  # _resource_ indicates the kind of resource to limit.
  # The list of resources are OS dependent.
  # Ruby may support following resources.
  #
  # [Process::RLIMIT_CORE] core size (bytes) (SUSv3)
  # [Process::RLIMIT_CPU] CPU time (seconds) (SUSv3)
  # [Process::RLIMIT_DATA] data segment (bytes) (SUSv3)
  # [Process::RLIMIT_FSIZE] file size (bytes) (SUSv3)
  # [Process::RLIMIT_NOFILE] file descriptors (number) (SUSv3)
  # [Process::RLIMIT_STACK] stack size (bytes) (SUSv3)
  # [Process::RLIMIT_AS] total available memory (bytes) (SUSv3, NetBSD, FreeBSD, OpenBSD but 4.4BSD-Lite)
  # [Process::RLIMIT_MEMLOCK] total size for mlock(2) (bytes) (4.4BSD, GNU/Linux)
  # [Process::RLIMIT_NPROC] number of processes for the user (number) (4.4BSD, GNU/Linux)
  # [Process::RLIMIT_RSS] resident memory size (bytes) (4.2BSD, GNU/Linux)
  # [Process::RLIMIT_SBSIZE] all socket buffers (bytes) (NetBSD, FreeBSD)
  #
  # Other <code>Process::RLIMIT_???</code> constants may be defined.
  #
  # _cur_limit_ and _max_limit_ may be <code>Process::RLIM_INFINITY</code>,
  # which means that the resource is not limited.
  # They may be <code>Process::RLIM_SAVED_MAX</code> or
  # <code>Process::RLIM_SAVED_CUR</code> too.
  # See system setrlimit(2) manual for details.
  def setrlimit(*several_variants) end

  # Establishes this process as a new session and process group
  # leader, with no controlling tty. Returns the session id. Not
  # available on all platforms.
  #
  #    Process.setsid   #=> 27422
  def setsid; end

  # Returns a <code>Tms</code> structure (see <code>Struct::Tms</code>
  # on page 388) that contains user and system CPU times for this
  # process.
  #
  #    t = Process.times
  #    [ t.utime, t.stime ]   #=> [0.0, 0.02]
  def times; end

  # Returns the (real) user ID of this process.
  #
  #    Process.uid   #=> 501
  def uid; end

  # Sets the (integer) user ID for this process. Not available on all
  # platforms.
  def uid=(integer) end

  # Waits for a child process to exit, returns its process id, and
  # sets <code>$?</code> to a <code>Process::Status</code> object
  # containing information on that process. Which child it waits on
  # depends on the value of _pid_:
  #
  # > 0::   Waits for the child whose process ID equals _pid_.
  #
  # 0::     Waits for any child whose process group ID equals that of the
  #         calling process.
  #
  # -1::    Waits for any child process (the default if no _pid_ is
  #         given).
  #
  # < -1::  Waits for any child whose process group ID equals the absolute
  #         value of _pid_.
  #
  # The _flags_ argument may be a logical or of the flag values
  # <code>Process::WNOHANG</code> (do not block if no child available)
  # or <code>Process::WUNTRACED</code> (return stopped children that
  # haven't been reported). Not all flags are available on all
  # platforms, but a flag value of zero will work on all platforms.
  #
  # Calling this method raises a <code>SystemError</code> if there are
  # no child processes. Not available on all platforms.
  #
  #    include Process
  #    fork { exit 99 }                 #=> 27429
  #    wait                             #=> 27429
  #    $?.exitstatus                    #=> 99
  #
  #    pid = fork { sleep 3 }           #=> 27440
  #    Time.now                         #=> Wed Apr 09 08:57:09 CDT 2003
  #    waitpid(pid, Process::WNOHANG)   #=> nil
  #    Time.now                         #=> Wed Apr 09 08:57:09 CDT 2003
  #    waitpid(pid, 0)                  #=> 27440
  #    Time.now                         #=> Wed Apr 09 08:57:12 CDT 2003
  def wait(*several_variants) end
  alias waitpid wait

  # Waits for a child process to exit (see Process::waitpid for exact
  # semantics) and returns an array containing the process id and the
  # exit status (a <code>Process::Status</code> object) of that
  # child. Raises a <code>SystemError</code> if there are no child
  # processes.
  #
  #    Process.fork { exit 99 }   #=> 27437
  #    pid, status = Process.wait2
  #    pid                        #=> 27437
  #    status.exitstatus          #=> 99
  def wait2(pid = -1, flags = 0) end
  alias waitpid2 wait2

  # Waits for all children, returning an array of
  # _pid_/_status_ pairs (where _status_ is a
  # <code>Process::Status</code> object).
  #
  #    fork { sleep 0.2; exit 2 }   #=> 27432
  #    fork { sleep 0.1; exit 1 }   #=> 27433
  #    fork {            exit 0 }   #=> 27434
  #    p Process.waitall
  #
  # <em>produces</em>:
  #
  #    [[27434, #<Process::Status: pid=27434,exited(0)>],
  #     [27433, #<Process::Status: pid=27433,exited(1)>],
  #     [27432, #<Process::Status: pid=27432,exited(2)>]]
  def waitall; end

  # The <code>Process::GID</code> module contains a collection of
  # module functions which can be used to portably get, set, and
  # switch the current process's real, effective, and saved group IDs.
  module GID
    # Change the current process's real and effective group ID to that
    # specified by _integer_. Returns the new group ID. Not
    # available on all platforms.
    #
    #    [Process.gid, Process.egid]          #=> [0, 0]
    #    Process::GID.change_privilege(33)    #=> 33
    #    [Process.gid, Process.egid]          #=> [33, 33]
    def self.change_privilege(integer) end

    # Returns the effective group ID for this process. Not available on
    # all platforms.
    #
    #    Process.egid   #=> 500
    def self.eid; end

    # Set the effective group ID, and if possible, the saved group ID of
    # the process to the given _integer_. Returns the new
    # effective group ID. Not available on all platforms.
    #
    #    [Process.gid, Process.egid]          #=> [0, 0]
    #    Process::GID.grant_privilege(31)     #=> 33
    #    [Process.gid, Process.egid]          #=> [0, 33]
    def self.grant_privilege(integer) end

    # Exchange real and effective group IDs and return the new effective
    # group ID. Not available on all platforms.
    #
    #    [Process.gid, Process.egid]   #=> [0, 33]
    #    Process::GID.re_exchange      #=> 0
    #    [Process.gid, Process.egid]   #=> [33, 0]
    def self.re_exchange; end

    # Returns +true+ if the real and effective group IDs of a
    # process may be exchanged on the current platform.
    def self.re_exchangeable?; end

    # Returns the (real) group ID for this process.
    #
    #    Process.gid   #=> 500
    def self.rid; end

    # Returns +true+ if the current platform has saved group
    # ID functionality.
    def self.sid_available?; end

    # Switch the effective and real group IDs of the current process. If
    # a <em>block</em> is given, the group IDs will be switched back
    # after the block is executed. Returns the new effective group ID if
    # called without a block, and the return value of the block if one
    # is given.
    def self.switch; end

    private

    # Change the current process's real and effective group ID to that
    # specified by _integer_. Returns the new group ID. Not
    # available on all platforms.
    #
    #    [Process.gid, Process.egid]          #=> [0, 0]
    #    Process::GID.change_privilege(33)    #=> 33
    #    [Process.gid, Process.egid]          #=> [33, 33]
    def change_privilege(integer) end

    # Returns the effective group ID for this process. Not available on
    # all platforms.
    #
    #    Process.egid   #=> 500
    def eid; end

    # Set the effective group ID, and if possible, the saved group ID of
    # the process to the given _integer_. Returns the new
    # effective group ID. Not available on all platforms.
    #
    #    [Process.gid, Process.egid]          #=> [0, 0]
    #    Process::GID.grant_privilege(31)     #=> 33
    #    [Process.gid, Process.egid]          #=> [0, 33]
    def grant_privilege(integer) end

    # Exchange real and effective group IDs and return the new effective
    # group ID. Not available on all platforms.
    #
    #    [Process.gid, Process.egid]   #=> [0, 33]
    #    Process::GID.re_exchange      #=> 0
    #    [Process.gid, Process.egid]   #=> [33, 0]
    def re_exchange; end

    # Returns +true+ if the real and effective group IDs of a
    # process may be exchanged on the current platform.
    def re_exchangeable?; end

    # Returns the (real) group ID for this process.
    #
    #    Process.gid   #=> 500
    def rid; end

    # Returns +true+ if the current platform has saved group
    # ID functionality.
    def sid_available?; end

    # Switch the effective and real group IDs of the current process. If
    # a <em>block</em> is given, the group IDs will be switched back
    # after the block is executed. Returns the new effective group ID if
    # called without a block, and the return value of the block if one
    # is given.
    def switch; end
  end

  # <code>Process::Status</code> encapsulates the information on the
  # status of a running or terminated system process. The built-in
  # variable <code>$?</code> is either +nil+ or a
  # <code>Process::Status</code> object.
  #
  #    fork { exit 99 }   #=> 26557
  #    Process.wait       #=> 26557
  #    $?.class           #=> Process::Status
  #    $?.to_i            #=> 25344
  #    $? >> 8            #=> 99
  #    $?.stopped?        #=> false
  #    $?.exited?         #=> true
  #    $?.exitstatus      #=> 99
  #
  # Posix systems record information on processes using a 16-bit
  # integer.  The lower bits record the process status (stopped,
  # exited, signaled) and the upper bits possibly contain additional
  # information (for example the program's return code in the case of
  # exited processes). Pre Ruby 1.8, these bits were exposed directly
  # to the Ruby program. Ruby now encapsulates these in a
  # <code>Process::Status</code> object. To maximize compatibility,
  # however, these objects retain a bit-oriented interface. In the
  # descriptions that follow, when we talk about the integer value of
  # _stat_, we're referring to this 16 bit value.
  class Status
    # Logical AND of the bits in _stat_ with <em>num</em>.
    #
    #    fork { exit 0x37 }
    #    Process.wait
    #    sprintf('%04x', $?.to_i)       #=> "3700"
    #    sprintf('%04x', $? & 0x1e00)   #=> "1600"
    def &(other) end

    # Returns +true+ if the integer value of _stat_
    # equals <em>other</em>.
    def ==(other) end

    # Shift the bits in _stat_ right <em>num</em> places.
    #
    #    fork { exit 99 }   #=> 26563
    #    Process.wait       #=> 26563
    #    $?.to_i            #=> 25344
    #    $? >> 8            #=> 99
    def >>(other) end

    # Returns +true+ if _stat_ generated a coredump
    # when it terminated. Not available on all platforms.
    def coredump?; end

    # Returns +true+ if _stat_ exited normally (for
    # example using an <code>exit()</code> call or finishing the
    # program).
    def exited?; end

    # Returns the least significant eight bits of the return code of
    # _stat_. Only available if <code>exited?</code> is
    # +true+.
    #
    #    fork { }           #=> 26572
    #    Process.wait       #=> 26572
    #    $?.exited?         #=> true
    #    $?.exitstatus      #=> 0
    #
    #    fork { exit 99 }   #=> 26573
    #    Process.wait       #=> 26573
    #    $?.exited?         #=> true
    #    $?.exitstatus      #=> 99
    def exitstatus; end

    # Override the inspection method.
    def inspect; end

    # Returns the process ID that this status object represents.
    #
    #    fork { exit }   #=> 26569
    #    Process.wait    #=> 26569
    #    $?.pid          #=> 26569
    def pid; end

    # Returns +true+ if _stat_ terminated because of
    # an uncaught signal.
    def signaled?; end

    # Returns +true+ if this process is stopped. This is only
    # returned if the corresponding <code>wait</code> call had the
    # <code>WUNTRACED</code> flag set.
    def stopped?; end

    # Returns the number of the signal that caused _stat_ to stop
    # (or +nil+ if self is not stopped).
    def stopsig; end

    # Returns +true+ if _stat_ is successful, +false+ if not.
    # Returns +nil+ if <code>exited?</code> is not +true+.
    def success?; end

    # Returns the number of the signal that caused _stat_ to
    # terminate (or +nil+ if self was not terminated by an
    # uncaught signal).
    def termsig; end

    # Returns the bits in _stat_ as a <code>Fixnum</code>. Poking
    # around in these bits is platform dependent.
    #
    #    fork { exit 0xab }         #=> 26566
    #    Process.wait               #=> 26566
    #    sprintf('%04x', $?.to_i)   #=> "ab00"
    def to_i; end
    alias to_int to_i

    # Equivalent to _stat_<code>.to_i.to_s</code>.
    def to_s; end
  end

  # The <code>Process::Sys</code> module contains UID and GID
  # functions which provide direct bindings to the system calls of the
  # same names instead of the more-portable versions of the same
  # functionality found in the <code>Process</code>,
  # <code>Process::UID</code>, and <code>Process::GID</code> modules.
  module Sys
    # Returns the effective group ID for this process. Not available on
    # all platforms.
    #
    #    Process.egid   #=> 500
    def self.getegid; end

    # Returns the effective user ID for this process.
    #
    #    Process.euid   #=> 501
    def self.geteuid; end

    # Returns the (real) group ID for this process.
    #
    #    Process.gid   #=> 500
    def self.getgid; end

    # Returns the (real) user ID of this process.
    #
    #    Process.uid   #=> 501
    def self.getuid; end

    # Returns +true+ if the process was created as a result
    # of an execve(2) system call which had either of the setuid or
    # setgid bits set (and extra privileges were given as a result) or
    # if it has changed any of its real, effective or saved user or
    # group IDs since it began execution.
    def self.issetugid; end

    # Set the effective group ID of the calling process to
    # _integer_.  Not available on all platforms.
    def self.setegid(integer) end

    # Set the effective user ID of the calling process to
    # _integer_.  Not available on all platforms.
    def self.seteuid(integer) end

    # Set the group ID of the current process to _integer_. Not
    # available on all platforms.
    def self.setgid(integer) end

    # Sets the (integer) real and/or effective group IDs of the current
    # process to <em>rid</em> and <em>eid</em>, respectively. A value of
    # <code>-1</code> for either means to leave that ID unchanged. Not
    # available on all platforms.
    def self.setregid(rid, eid) end

    # Sets the (integer) real, effective, and saved user IDs of the
    # current process to <em>rid</em>, <em>eid</em>, and <em>sid</em>
    # respectively. A value of <code>-1</code> for any value means to
    # leave that ID unchanged. Not available on all platforms.
    def self.setresgid(rid, eid, sid) end

    # Sets the (integer) real, effective, and saved user IDs of the
    # current process to _rid_, _eid_, and _sid_ respectively. A
    # value of <code>-1</code> for any value means to
    # leave that ID unchanged. Not available on all platforms.
    def self.setresuid(rid, eid, sid) end

    # Sets the (integer) real and/or effective user IDs of the current
    # process to _rid_ and _eid_, respectively. A value of
    # <code>-1</code> for either means to leave that ID unchanged. Not
    # available on all platforms.
    def self.setreuid(rid, eid) end

    # Set the real group ID of the calling process to _integer_.
    # Not available on all platforms.
    def self.setrgid(integer) end

    # Set the real user ID of the calling process to _integer_.
    # Not available on all platforms.
    def self.setruid(integer) end

    # Set the user ID of the current process to _integer_. Not
    # available on all platforms.
    def self.setuid(integer) end

    private

    # Returns the effective group ID for this process. Not available on
    # all platforms.
    #
    #    Process.egid   #=> 500
    def getegid; end

    # Returns the effective user ID for this process.
    #
    #    Process.euid   #=> 501
    def geteuid; end

    # Returns the (real) group ID for this process.
    #
    #    Process.gid   #=> 500
    def getgid; end

    # Returns the (real) user ID of this process.
    #
    #    Process.uid   #=> 501
    def getuid; end

    # Returns +true+ if the process was created as a result
    # of an execve(2) system call which had either of the setuid or
    # setgid bits set (and extra privileges were given as a result) or
    # if it has changed any of its real, effective or saved user or
    # group IDs since it began execution.
    def issetugid; end

    # Set the effective group ID of the calling process to
    # _integer_.  Not available on all platforms.
    def setegid(integer) end

    # Set the effective user ID of the calling process to
    # _integer_.  Not available on all platforms.
    def seteuid(integer) end

    # Set the group ID of the current process to _integer_. Not
    # available on all platforms.
    def setgid(integer) end

    # Sets the (integer) real and/or effective group IDs of the current
    # process to <em>rid</em> and <em>eid</em>, respectively. A value of
    # <code>-1</code> for either means to leave that ID unchanged. Not
    # available on all platforms.
    def setregid(rid, eid) end

    # Sets the (integer) real, effective, and saved user IDs of the
    # current process to <em>rid</em>, <em>eid</em>, and <em>sid</em>
    # respectively. A value of <code>-1</code> for any value means to
    # leave that ID unchanged. Not available on all platforms.
    def setresgid(rid, eid, sid) end

    # Sets the (integer) real, effective, and saved user IDs of the
    # current process to _rid_, _eid_, and _sid_ respectively. A
    # value of <code>-1</code> for any value means to
    # leave that ID unchanged. Not available on all platforms.
    def setresuid(rid, eid, sid) end

    # Sets the (integer) real and/or effective user IDs of the current
    # process to _rid_ and _eid_, respectively. A value of
    # <code>-1</code> for either means to leave that ID unchanged. Not
    # available on all platforms.
    def setreuid(rid, eid) end

    # Set the real group ID of the calling process to _integer_.
    # Not available on all platforms.
    def setrgid(integer) end

    # Set the real user ID of the calling process to _integer_.
    # Not available on all platforms.
    def setruid(integer) end

    # Set the user ID of the current process to _integer_. Not
    # available on all platforms.
    def setuid(integer) end
  end

  # The <code>Process::UID</code> module contains a collection of
  # module functions which can be used to portably get, set, and
  # switch the current process's real, effective, and saved user IDs.
  module UID
    # Change the current process's real and effective user ID to that
    # specified by _integer_. Returns the new user ID. Not
    # available on all platforms.
    #
    #    [Process.uid, Process.euid]          #=> [0, 0]
    #    Process::UID.change_privilege(31)    #=> 31
    #    [Process.uid, Process.euid]          #=> [31, 31]
    def self.change_privilege(integer) end

    # Returns the effective user ID for this process.
    #
    #    Process.euid   #=> 501
    def self.eid; end

    # Set the effective user ID, and if possible, the saved user ID of
    # the process to the given _integer_. Returns the new
    # effective user ID. Not available on all platforms.
    #
    #    [Process.uid, Process.euid]          #=> [0, 0]
    #    Process::UID.grant_privilege(31)     #=> 31
    #    [Process.uid, Process.euid]          #=> [0, 31]
    def self.grant_privilege(integer) end

    # Exchange real and effective user IDs and return the new effective
    # user ID. Not available on all platforms.
    #
    #    [Process.uid, Process.euid]   #=> [0, 31]
    #    Process::UID.re_exchange      #=> 0
    #    [Process.uid, Process.euid]   #=> [31, 0]
    def self.re_exchange; end

    # Returns +true+ if the real and effective user IDs of a
    # process may be exchanged on the current platform.
    def self.re_exchangeable?; end

    # Returns the (real) user ID of this process.
    #
    #    Process.uid   #=> 501
    def self.rid; end

    # Returns +true+ if the current platform has saved user
    # ID functionality.
    def self.sid_available?; end

    # Switch the effective and real user IDs of the current process. If
    # a <em>block</em> is given, the user IDs will be switched back
    # after the block is executed. Returns the new effective user ID if
    # called without a block, and the return value of the block if one
    # is given.
    def self.switch; end

    private

    # Change the current process's real and effective user ID to that
    # specified by _integer_. Returns the new user ID. Not
    # available on all platforms.
    #
    #    [Process.uid, Process.euid]          #=> [0, 0]
    #    Process::UID.change_privilege(31)    #=> 31
    #    [Process.uid, Process.euid]          #=> [31, 31]
    def change_privilege(integer) end

    # Returns the effective user ID for this process.
    #
    #    Process.euid   #=> 501
    def eid; end

    # Set the effective user ID, and if possible, the saved user ID of
    # the process to the given _integer_. Returns the new
    # effective user ID. Not available on all platforms.
    #
    #    [Process.uid, Process.euid]          #=> [0, 0]
    #    Process::UID.grant_privilege(31)     #=> 31
    #    [Process.uid, Process.euid]          #=> [0, 31]
    def grant_privilege(integer) end

    # Exchange real and effective user IDs and return the new effective
    # user ID. Not available on all platforms.
    #
    #    [Process.uid, Process.euid]   #=> [0, 31]
    #    Process::UID.re_exchange      #=> 0
    #    [Process.uid, Process.euid]   #=> [31, 0]
    def re_exchange; end

    # Returns +true+ if the real and effective user IDs of a
    # process may be exchanged on the current platform.
    def re_exchangeable?; end

    # Returns the (real) user ID of this process.
    #
    #    Process.uid   #=> 501
    def rid; end

    # Returns +true+ if the current platform has saved user
    # ID functionality.
    def sid_available?; end

    # Switch the effective and real user IDs of the current process. If
    # a <em>block</em> is given, the user IDs will be switched back
    # after the block is executed. Returns the new effective user ID if
    # called without a block, and the return value of the block if one
    # is given.
    def switch; end
  end
end
