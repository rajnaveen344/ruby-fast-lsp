# frozen_string_literal: true

# \Module +Process+ represents a process in the underlying operating system.
# Its methods support management of the current process and its child processes.
#
# == \Process Creation
#
# Each of the following methods executes a given command in a new process or subshell,
# or multiple commands in new processes and/or subshells.
# The choice of process or subshell depends on the form of the command;
# see {Argument command_line or exe_path}[rdoc-ref:Process@Argument+command_line+or+exe_path].
#
# - Process.spawn, Kernel#spawn: Executes the command;
#   returns the new pid without waiting for completion.
# - Process.exec: Replaces the current process by executing the command.
#
# In addition:
#
# - \Method Kernel#system executes a given command-line (string) in a subshell;
#   returns +true+, +false+, or +nil+.
# - \Method Kernel#` executes a given command-line (string) in a subshell;
#   returns its $stdout string.
# - \Module Open3 supports creating child processes
#   with access to their $stdin, $stdout, and $stderr streams.
#
# === Execution Environment
#
# Optional leading argument +env+ is a hash of name/value pairs,
# where each name is a string and each value is a string or +nil+;
# each name/value pair is added to ENV in the new process.
#
#   Process.spawn(                'ruby -e "p ENV[\"Foo\"]"')
#   Process.spawn({'Foo' => '0'}, 'ruby -e "p ENV[\"Foo\"]"')
#
# Output:
#
#   "0"
#
# The effect is usually similar to that of calling ENV#update with argument +env+,
# where each named environment variable is created or updated
# (if the value is non-+nil+),
# or deleted (if the value is +nil+).
#
# However, some modifications to the calling process may remain
# if the new process fails.
# For example, hard resource limits are not restored.
#
# === Argument +command_line+ or +exe_path+
#
# The required string argument is one of the following:
#
# - +command_line+ if it begins with a shell reserved word or special built-in,
#   or if it contains one or more meta characters.
# - +exe_path+ otherwise.
#
# <b>Argument +command_line+</b>
#
# \String argument +command_line+ is a command line to be passed to a shell;
# it must begin with a shell reserved word, begin with a special built-in,
# or contain meta characters:
#
#   system('if true; then echo "Foo"; fi')          # => true  # Shell reserved word.
#   system('echo')                                  # => true  # Built-in.
#   system('date > /tmp/date.tmp')                  # => true  # Contains meta character.
#   system('date > /nop/date.tmp')                  # => false
#   system('date > /nop/date.tmp', exception: true) # Raises RuntimeError.
#
# The command line may also contain arguments and options for the command:
#
#   system('echo "Foo"') # => true
#
# Output:
#
#   Foo
#
# See {Execution Shell}[rdoc-ref:Process@Execution+Shell] for details about the shell.
#
# <b>Argument +exe_path+</b>
#
# Argument +exe_path+ is one of the following:
#
# - The string path to an executable to be called.
# - A 2-element array containing the path to an executable to be called,
#   and the string to be used as the name of the executing process.
#
# Example:
#
#   system('/usr/bin/date') # => true # Path to date on Unix-style system.
#   system('foo')           # => nil  # Command failed.
#
# Output:
#
#   Mon Aug 28 11:43:10 AM CDT 2023
#
# === Execution Options
#
# Optional trailing argument +options+ is a hash of execution options.
#
# ==== Working Directory (+:chdir+)
#
# By default, the working directory for the new process is the same as
# that of the current process:
#
#   Dir.chdir('/var')
#   Process.spawn('ruby -e "puts Dir.pwd"')
#
# Output:
#
#   /var
#
# Use option +:chdir+ to set the working directory for the new process:
#
#   Process.spawn('ruby -e "puts Dir.pwd"', {chdir: '/tmp'})
#
# Output:
#
#   /tmp
#
# The working directory of the current process is not changed:
#
#   Dir.pwd # => "/var"
#
# ==== \File Redirection (\File Descriptor)
#
# Use execution options for file redirection in the new process.
#
# The key for such an option may be an integer file descriptor (fd),
# specifying a source,
# or an array of fds, specifying multiple sources.
#
# An integer source fd may be specified as:
#
# - _n_: Specifies file descriptor _n_.
#
# There are these shorthand symbols for fds:
#
# - +:in+: Specifies file descriptor 0 (STDIN).
# - +:out+: Specifies file descriptor 1 (STDOUT).
# - +:err+: Specifies file descriptor 2 (STDERR).
#
# The value given with a source is one of:
#
# - _n_:
#   Redirects to fd _n_ in the parent process.
# - +filepath+:
#   Redirects from or to the file at +filepath+ via <tt>open(filepath, mode, 0644)</tt>,
#   where +mode+ is <tt>'r'</tt> for source +:in+,
#   or <tt>'w'</tt> for source +:out+ or +:err+.
# - <tt>[filepath]</tt>:
#   Redirects from the file at +filepath+ via <tt>open(filepath, 'r', 0644)</tt>.
# - <tt>[filepath, mode]</tt>:
#   Redirects from or to the file at +filepath+ via <tt>open(filepath, mode, 0644)</tt>.
# - <tt>[filepath, mode, perm]</tt>:
#   Redirects from or to the file at +filepath+ via <tt>open(filepath, mode, perm)</tt>.
# - <tt>[:child, fd]</tt>:
#   Redirects to the redirected +fd+.
# - +:close+: Closes the file descriptor in child process.
#
# See {Access Modes}[rdoc-ref:File@Access+Modes]
# and {File Permissions}[rdoc-ref:File@File+Permissions].
#
# ==== Environment Variables (+:unsetenv_others+)
#
# By default, the new process inherits environment variables
# from the parent process;
# use execution option key +:unsetenv_others+ with value +true+
# to clear environment variables in the new process.
#
# Any changes specified by execution option +env+ are made after the new process
# inherits or clears its environment variables;
# see {Execution Environment}[rdoc-ref:Process@Execution+Environment].
#
# ==== \File-Creation Access (+:umask+)
#
# Use execution option +:umask+ to set the file-creation access
# for the new process;
# see {Access Modes}[rdoc-ref:File@Access+Modes]:
#
#   command = 'ruby -e "puts sprintf(\"0%o\", File.umask)"'
#   options = {:umask => 0644}
#   Process.spawn(command, options)
#
# Output:
#
#   0644
#
# ==== \Process Groups (+:pgroup+ and +:new_pgroup+)
#
# By default, the new process belongs to the same
# {process group}[https://en.wikipedia.org/wiki/Process_group]
# as the parent process.
#
# To specify a different process group.
# use execution option +:pgroup+ with one of the following values:
#
# - +true+: Create a new process group for the new process.
# - _pgid_: Create the new process in the process group
#   whose id is _pgid_.
#
# On Windows only, use execution option +:new_pgroup+ with value +true+
# to create a new process group for the new process.
#
# ==== Resource Limits
#
# Use execution options to set resource limits.
#
# The keys for these options are symbols of the form
# <tt>:rlimit_<i>resource_name</i></tt>,
# where _resource_name_ is the downcased form of one of the string
# resource names described at method Process.setrlimit.
# For example, key +:rlimit_cpu+ corresponds to resource limit <tt>'CPU'</tt>.
#
# The value for such as key is one of:
#
# - An integer, specifying both the current and maximum limits.
# - A 2-element array of integers, specifying the current and maximum limits.
#
# ==== \File Descriptor Inheritance
#
# By default, the new process inherits file descriptors from the parent process.
#
# Use execution option <tt>:close_others => true</tt> to modify that inheritance
# by closing non-standard fds (3 and greater) that are not otherwise redirected.
#
# === Execution Shell
#
# On a Unix-like system, the shell invoked is <tt>/bin/sh</tt>;
# otherwise the shell invoked is determined by environment variable
# <tt>ENV['RUBYSHELL']</tt>, if defined, or <tt>ENV['COMSPEC']</tt> otherwise.
#
# Except for the +COMSPEC+ case,
# the entire string +command_line+ is passed as an argument
# to {shell option -c}[https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/utilities/sh.html].
#
# The shell performs normal shell expansion on the command line:
#
#   spawn('echo C*') # => 799139
#   Process.wait     # => 799139
#
# Output:
#
#   CONTRIBUTING.md COPYING COPYING.ja
#
# == What's Here
#
# === Current-Process Getters
#
# - ::argv0: Returns the process name as a frozen string.
# - ::egid: Returns the effective group ID.
# - ::euid: Returns the effective user ID.
# - ::getpgrp: Return the process group ID.
# - ::getrlimit: Returns the resource limit.
# - ::gid: Returns the (real) group ID.
# - ::pid: Returns the process ID.
# - ::ppid: Returns the process ID of the parent process.
# - ::uid: Returns the (real) user ID.
#
# === Current-Process Setters
#
# - ::egid=: Sets the effective group ID.
# - ::euid=: Sets the effective user ID.
# - ::gid=: Sets the (real) group ID.
# - ::setproctitle: Sets the process title.
# - ::setpgrp: Sets the process group ID of the process to zero.
# - ::setrlimit: Sets a resource limit.
# - ::setsid: Establishes the process as a new session and process group leader,
#   with no controlling tty.
# - ::uid=: Sets the user ID.
#
# === Current-Process Execution
#
# - ::abort: Immediately terminates the process.
# - ::daemon: Detaches the process from its controlling terminal
#   and continues running it in the background as system daemon.
# - ::exec: Replaces the process by running a given external command.
# - ::exit: Initiates process termination by raising exception SystemExit
#   (which may be caught).
# - ::exit!: Immediately exits the process.
# - ::warmup: Notifies the Ruby virtual machine that the boot sequence
#   for the application is completed,
#   and that the VM may begin optimizing the application.
#
# === Child Processes
#
# - ::detach: Guards against a child process becoming a zombie.
# - ::fork: Creates a child process.
# - ::kill: Sends a given signal to processes.
# - ::spawn: Creates a child process.
# - ::wait, ::waitpid: Waits for a child process to exit; returns its process ID.
# - ::wait2, ::waitpid2: Waits for a child process to exit; returns its process ID and status.
# - ::waitall: Waits for all child processes to exit;
#   returns their process IDs and statuses.
#
# === \Process Groups
#
# - ::getpgid: Returns the process group ID for a process.
# - ::getpriority: Returns the scheduling priority
#   for a process, process group, or user.
# - ::getsid: Returns the session ID for a process.
# - ::groups: Returns an array of the group IDs
#   in the supplemental group access list for this process.
# - ::groups=: Sets the supplemental group access list
#   to the given array of group IDs.
# - ::initgroups: Initializes the supplemental group access list.
# - ::last_status: Returns the status of the last executed child process
#   in the current thread.
# - ::maxgroups: Returns the maximum number of group IDs allowed
#   in the supplemental group access list.
# - ::maxgroups=: Sets the maximum number of group IDs allowed
#   in the supplemental group access list.
# - ::setpgid: Sets the process group ID of a process.
# - ::setpriority: Sets the scheduling priority
#   for a process, process group, or user.
#
# === Timing
#
# - ::clock_getres: Returns the resolution of a system clock.
# - ::clock_gettime: Returns the time from a system clock.
# - ::times: Returns a Process::Tms object containing times
#   for the current process and its child processes.
module Process
  # see Process.clock_gettime
  CLOCK_BOOTTIME = _
  # see Process.clock_gettime
  CLOCK_BOOTTIME_ALARM = _
  # see Process.clock_gettime
  CLOCK_MONOTONIC = _
  # see Process.clock_gettime
  CLOCK_MONOTONIC_COARSE = _
  # see Process.clock_gettime
  CLOCK_MONOTONIC_FAST = _
  # see Process.clock_gettime
  CLOCK_MONOTONIC_PRECISE = _
  # see Process.clock_gettime
  CLOCK_MONOTONIC_RAW = _
  # see Process.clock_gettime
  CLOCK_MONOTONIC_RAW_APPROX = _
  # see Process.clock_gettime
  CLOCK_PROCESS_CPUTIME_ID = _
  # see Process.clock_gettime
  CLOCK_PROF = _
  # see Process.clock_gettime
  CLOCK_REALTIME = _
  # see Process.clock_gettime
  CLOCK_REALTIME_ALARM = _
  # see Process.clock_gettime
  CLOCK_REALTIME_COARSE = _
  # see Process.clock_gettime
  CLOCK_REALTIME_FAST = _
  # see Process.clock_gettime
  CLOCK_REALTIME_PRECISE = _
  # see Process.clock_gettime
  CLOCK_SECOND = _
  # see Process.clock_gettime
  CLOCK_TAI = _
  # see Process.clock_gettime
  CLOCK_THREAD_CPUTIME_ID = _
  # see Process.clock_gettime
  CLOCK_UPTIME = _
  # see Process.clock_gettime
  CLOCK_UPTIME_FAST = _
  # see Process.clock_gettime
  CLOCK_UPTIME_PRECISE = _
  # see Process.clock_gettime
  CLOCK_UPTIME_RAW = _
  # see Process.clock_gettime
  CLOCK_UPTIME_RAW_APPROX = _
  # see Process.clock_gettime
  CLOCK_VIRTUAL = _
  # see Process.setpriority
  PRIO_PGRP = _
  # see Process.setpriority
  PRIO_PROCESS = _
  # see Process.setpriority
  PRIO_USER = _
  # Maximum size of the process's virtual memory (address space) in bytes.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_AS = _
  # Maximum size of the core file.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_CORE = _
  # CPU time limit in seconds.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_CPU = _
  # Maximum size of the process's data segment.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_DATA = _
  # Maximum size of files that the process may create.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_FSIZE = _
  # Maximum number of bytes of memory that may be locked into RAM.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_MEMLOCK = _
  # Specifies the limit on the number of bytes that can be allocated
  # for POSIX message queues for the real user ID of the calling process.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_MSGQUEUE = _
  # Specifies a ceiling to which the process's nice value can be raised.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_NICE = _
  # Specifies a value one greater than the maximum file descriptor
  # number that can be opened by this process.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_NOFILE = _
  # The maximum number of processes that can be created for the
  # real user ID of the calling process.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_NPROC = _
  # The maximum number of pseudo-terminals that can be created for the
  # real user ID of the calling process.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_NPTS = _
  # Specifies the limit (in pages) of the process's resident set.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_RSS = _
  # Specifies a ceiling on the real-time priority that may be set for this process.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_RTPRIO = _
  # Specifies limit on CPU time this process scheduled under a real-time
  # scheduling policy can consume.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_RTTIME = _
  # Maximum size of the socket buffer.
  RLIMIT_SBSIZE = _
  # Specifies a limit on the number of signals that may be queued for
  # the real user ID of the calling process.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_SIGPENDING = _
  # Maximum size of the stack, in bytes.
  #
  # see the system getrlimit(2) manual for details.
  RLIMIT_STACK = _
  # see Process.setrlimit
  RLIM_INFINITY = _
  # see Process.setrlimit
  RLIM_SAVED_CUR = _
  # see Process.setrlimit
  RLIM_SAVED_MAX = _
  # see Process.wait
  WNOHANG = _
  # see Process.wait
  WUNTRACED = _

  # An internal API for fork. Do not call this method directly.
  # Currently, this is called via Kernel#fork, Process.fork, and
  # IO.popen with <tt>"-"</tt>.
  #
  # This method is not for casual code but for application monitoring
  # libraries. You can add custom code before and after fork events
  # by overriding this method.
  #
  # Note: Process.daemon may be implemented using fork(2) BUT does not go
  # through this method.
  # Thus, depending on your reason to hook into this method, you
  # may also want to hook into that one.
  # See {this issue}[https://bugs.ruby-lang.org/issues/18911] for a
  # more detailed discussion of this.
  def self._fork; end

  # Terminates execution immediately, effectively by calling
  # <tt>Kernel.exit(false)</tt>.
  #
  # If string argument +msg+ is given,
  # it is written to STDERR prior to termination;
  # otherwise, if an exception was raised,
  # prints its message and backtrace.
  def self.abort(...) end

  # Returns the name of the script being executed.  The value is not
  # affected by assigning a new value to $0.
  #
  # This method first appeared in Ruby 2.1 to serve as a global
  # variable free means to get the script name.
  def self.argv0; end

  # Returns a clock resolution as determined by POSIX function
  # {clock_getres()}[https://man7.org/linux/man-pages/man3/clock_getres.3.html]:
  #
  #   Process.clock_getres(:CLOCK_REALTIME) # => 1.0e-09
  #
  # See Process.clock_gettime for the values of +clock_id+ and +unit+.
  #
  # Examples:
  #
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :float_microsecond) # => 0.001
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :float_millisecond) # => 1.0e-06
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :float_second)      # => 1.0e-09
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :microsecond)       # => 0
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :millisecond)       # => 0
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :nanosecond)        # => 1
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :second)            # => 0
  #
  # In addition to the values for +unit+ supported in Process.clock_gettime,
  # this method supports +:hertz+, the integer number of clock ticks per second
  # (which is the reciprocal of +:float_second+):
  #
  #   Process.clock_getres(:TIMES_BASED_CLOCK_PROCESS_CPUTIME_ID, :hertz)        # => 100.0
  #   Process.clock_getres(:TIMES_BASED_CLOCK_PROCESS_CPUTIME_ID, :float_second) # => 0.01
  #
  # <b>Accuracy</b>:
  # Note that the returned resolution may be inaccurate on some platforms
  # due to underlying bugs.
  # Inaccurate resolutions have been reported for various clocks including
  # +:CLOCK_MONOTONIC+ and +:CLOCK_MONOTONIC_RAW+
  # on Linux, macOS, BSD or AIX platforms, when using ARM processors,
  # or when using virtualization.
  def self.clock_getres(clock_id, unit = :float_second) end

  # Returns a clock time as determined by POSIX function
  # {clock_gettime()}[https://man7.org/linux/man-pages/man3/clock_gettime.3.html]:
  #
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID) # => 198.650379677
  #
  # Argument +clock_id+ should be a symbol or a constant that specifies
  # the clock whose time is to be returned;
  # see below.
  #
  # Optional argument +unit+ should be a symbol that specifies
  # the unit to be used in the returned clock time;
  # see below.
  #
  # <b>Argument +clock_id+</b>
  #
  # Argument +clock_id+ specifies the clock whose time is to be returned;
  # it may be a constant such as <tt>Process::CLOCK_REALTIME</tt>,
  # or a symbol shorthand such as +:CLOCK_REALTIME+.
  #
  # The supported clocks depend on the underlying operating system;
  # this method supports the following clocks on the indicated platforms
  # (raises Errno::EINVAL if called with an unsupported clock):
  #
  # - +:CLOCK_BOOTTIME+: Linux 2.6.39.
  # - +:CLOCK_BOOTTIME_ALARM+: Linux 3.0.
  # - +:CLOCK_MONOTONIC+: SUSv3 to 4, Linux 2.5.63, FreeBSD 3.0, NetBSD 2.0, OpenBSD 3.4, macOS 10.12, Windows-2000.
  # - +:CLOCK_MONOTONIC_COARSE+: Linux 2.6.32.
  # - +:CLOCK_MONOTONIC_FAST+: FreeBSD 8.1.
  # - +:CLOCK_MONOTONIC_PRECISE+: FreeBSD 8.1.
  # - +:CLOCK_MONOTONIC_RAW+: Linux 2.6.28, macOS 10.12.
  # - +:CLOCK_MONOTONIC_RAW_APPROX+: macOS 10.12.
  # - +:CLOCK_PROCESS_CPUTIME_ID+: SUSv3 to 4, Linux 2.5.63, FreeBSD 9.3, OpenBSD 5.4, macOS 10.12.
  # - +:CLOCK_PROF+: FreeBSD 3.0, OpenBSD 2.1.
  # - +:CLOCK_REALTIME+: SUSv2 to 4, Linux 2.5.63, FreeBSD 3.0, NetBSD 2.0, OpenBSD 2.1, macOS 10.12, Windows-8/Server-2012.
  #   Time.now is recommended over +:CLOCK_REALTIME:.
  # - +:CLOCK_REALTIME_ALARM+: Linux 3.0.
  # - +:CLOCK_REALTIME_COARSE+: Linux 2.6.32.
  # - +:CLOCK_REALTIME_FAST+: FreeBSD 8.1.
  # - +:CLOCK_REALTIME_PRECISE+: FreeBSD 8.1.
  # - +:CLOCK_SECOND+: FreeBSD 8.1.
  # - +:CLOCK_TAI+: Linux 3.10.
  # - +:CLOCK_THREAD_CPUTIME_ID+: SUSv3 to 4, Linux 2.5.63, FreeBSD 7.1, OpenBSD 5.4, macOS 10.12.
  # - +:CLOCK_UPTIME+: FreeBSD 7.0, OpenBSD 5.5.
  # - +:CLOCK_UPTIME_FAST+: FreeBSD 8.1.
  # - +:CLOCK_UPTIME_PRECISE+: FreeBSD 8.1.
  # - +:CLOCK_UPTIME_RAW+: macOS 10.12.
  # - +:CLOCK_UPTIME_RAW_APPROX+: macOS 10.12.
  # - +:CLOCK_VIRTUAL+: FreeBSD 3.0, OpenBSD 2.1.
  #
  # Note that SUS stands for Single Unix Specification.
  # SUS contains POSIX and clock_gettime is defined in the POSIX part.
  # SUS defines +:CLOCK_REALTIME+ as mandatory but
  # +:CLOCK_MONOTONIC+, +:CLOCK_PROCESS_CPUTIME_ID+,
  # and +:CLOCK_THREAD_CPUTIME_ID+ are optional.
  #
  # Certain emulations are used when the given +clock_id+
  # is not supported directly:
  #
  # - Emulations for +:CLOCK_REALTIME+:
  #
  #   - +:GETTIMEOFDAY_BASED_CLOCK_REALTIME+:
  #     Use gettimeofday() defined by SUS (deprecated in SUSv4).
  #     The resolution is 1 microsecond.
  #   - +:TIME_BASED_CLOCK_REALTIME+:
  #     Use time() defined by ISO C.
  #     The resolution is 1 second.
  #
  # - Emulations for +:CLOCK_MONOTONIC+:
  #
  #   - +:MACH_ABSOLUTE_TIME_BASED_CLOCK_MONOTONIC+:
  #     Use mach_absolute_time(), available on Darwin.
  #     The resolution is CPU dependent.
  #   - +:TIMES_BASED_CLOCK_MONOTONIC+:
  #     Use the result value of times() defined by POSIX, thus:
  #     >>>
  #       Upon successful completion, times() shall return the elapsed real time,
  #       in clock ticks, since an arbitrary point in the past
  #       (for example, system start-up time).
  #
  #     For example, GNU/Linux returns a value based on jiffies and it is monotonic.
  #     However, 4.4BSD uses gettimeofday() and it is not monotonic.
  #     (FreeBSD uses +:CLOCK_MONOTONIC+ instead, though.)
  #
  #     The resolution is the clock tick.
  #     "getconf CLK_TCK" command shows the clock ticks per second.
  #     (The clock ticks-per-second is defined by HZ macro in older systems.)
  #     If it is 100 and clock_t is 32 bits integer type,
  #     the resolution is 10 millisecond and cannot represent over 497 days.
  #
  # - Emulations for +:CLOCK_PROCESS_CPUTIME_ID+:
  #
  #   - +:GETRUSAGE_BASED_CLOCK_PROCESS_CPUTIME_ID+:
  #     Use getrusage() defined by SUS.
  #     getrusage() is used with RUSAGE_SELF to obtain the time only for
  #     the calling process (excluding the time for child processes).
  #     The result is addition of user time (ru_utime) and system time (ru_stime).
  #     The resolution is 1 microsecond.
  #   - +:TIMES_BASED_CLOCK_PROCESS_CPUTIME_ID+:
  #     Use times() defined by POSIX.
  #     The result is addition of user time (tms_utime) and system time (tms_stime).
  #     tms_cutime and tms_cstime are ignored to exclude the time for child processes.
  #     The resolution is the clock tick.
  #     "getconf CLK_TCK" command shows the clock ticks per second.
  #     (The clock ticks per second is defined by HZ macro in older systems.)
  #     If it is 100, the resolution is 10 millisecond.
  #   - +:CLOCK_BASED_CLOCK_PROCESS_CPUTIME_ID+:
  #     Use clock() defined by ISO C.
  #     The resolution is <tt>1/CLOCKS_PER_SEC</tt>.
  #     +CLOCKS_PER_SEC+ is the C-level macro defined by time.h.
  #     SUS defines +CLOCKS_PER_SEC+ as 1000000;
  #     other systems may define it differently.
  #     If +CLOCKS_PER_SEC+ is 1000000 (as in SUS),
  #     the resolution is 1 microsecond.
  #     If +CLOCKS_PER_SEC+ is 1000000 and clock_t is a 32-bit integer type,
  #     it cannot represent over 72 minutes.
  #
  # <b>Argument +unit+</b>
  #
  # Optional argument +unit+ (default +:float_second+)
  # specifies the unit for the returned value.
  #
  # - +:float_microsecond+: Number of microseconds as a float.
  # - +:float_millisecond+: Number of milliseconds as a float.
  # - +:float_second+: Number of seconds as a float.
  # - +:microsecond+: Number of microseconds as an integer.
  # - +:millisecond+: Number of milliseconds as an integer.
  # - +:nanosecond+: Number of nanoseconds as an integer.
  # - +::second+: Number of seconds as an integer.
  #
  # Examples:
  #
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :float_microsecond)
  #   # => 203605054.825
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :float_millisecond)
  #   # => 203643.696848
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :float_second)
  #   # => 203.762181929
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :microsecond)
  #   # => 204123212
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :millisecond)
  #   # => 204298
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :nanosecond)
  #   # => 204602286036
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :second)
  #   # => 204
  #
  # The underlying function, clock_gettime(), returns a number of nanoseconds.
  # Float object (IEEE 754 double) is not enough to represent
  # the return value for +:CLOCK_REALTIME+.
  # If the exact nanoseconds value is required, use +:nanosecond+ as the +unit+.
  #
  # The origin (time zero) of the returned value is system-dependent,
  # and may be, for example, system start up time,
  # process start up time, the Epoch, etc.
  #
  # The origin in +:CLOCK_REALTIME+ is defined as the Epoch:
  # <tt>1970-01-01 00:00:00 UTC</tt>;
  # some systems count leap seconds and others don't,
  # so the result may vary across systems.
  def self.clock_gettime(clock_id, unit = :float_second) end

  # Detaches the current process from its controlling terminal
  # and runs it in the background as system daemon;
  # returns zero.
  #
  # By default:
  #
  # - Changes the current working directory to the root directory.
  # - Redirects $stdin, $stdout, and $stderr to the null device.
  #
  # If optional argument +nochdir+ is +true+,
  # does not change the current working directory.
  #
  # If optional argument +noclose+ is +true+,
  # does not redirect $stdin, $stdout, or $stderr.
  def self.daemon(nochdir = nil, noclose = nil) end

  # Avoids the potential for a child process to become a
  # {zombie process}[https://en.wikipedia.org/wiki/Zombie_process].
  # Process.detach prevents this by setting up a separate Ruby thread
  # whose sole job is to reap the status of the process _pid_ when it terminates.
  #
  # This method is needed only when the parent process will never wait
  # for the child process.
  #
  # This example does not reap the second child process;
  # that process appears as a zombie in the process status (+ps+) output:
  #
  #   pid = Process.spawn('ruby', '-e', 'exit 13') # => 312691
  #   sleep(1)
  #   # Find zombies.
  #   system("ps -ho pid,state -p #{pid}")
  #
  # Output:
  #
  #    312716 Z
  #
  # This example also does not reap the second child process,
  # but it does detach the process so that it does not become a zombie:
  #
  #   pid = Process.spawn('ruby', '-e', 'exit 13') # => 313213
  #   thread = Process.detach(pid)
  #   sleep(1)
  #   # => #<Process::Waiter:0x00007f038f48b838 run>
  #   system("ps -ho pid,state -p #{pid}")        # Finds no zombies.
  #
  # The waiting thread can return the pid of the detached child process:
  #
  #   thread.join.pid                       # => 313262
  def self.detach(pid) end

  # Returns the effective group ID for the current process:
  #
  #   Process.egid # => 500
  #
  # Not available on all platforms.
  def self.egid; end

  # Sets the effective group ID for the current process.
  #
  # Not available on all platforms.
  def self.egid=(new_egid) end

  # Returns the effective user ID for the current process.
  #
  #   Process.euid # => 501
  def self.euid; end

  # Sets the effective user ID for the current process.
  #
  # Not available on all platforms.
  def self.euid=(new_euid) end

  # Replaces the current process by doing one of the following:
  #
  # - Passing string +command_line+ to the shell.
  # - Invoking the executable at +exe_path+.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # The new process is created using the
  # {exec system call}[https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/functions/execve.html];
  # it may inherit some of its environment from the calling program
  # (possibly including open file descriptors).
  #
  # Argument +env+, if given, is a hash that affects +ENV+ for the new process;
  # see {Execution Environment}[rdoc-ref:Process@Execution+Environment].
  #
  # Argument +options+ is a hash of options for the new process;
  # see {Execution Options}[rdoc-ref:Process@Execution+Options].
  #
  # The first required argument is one of the following:
  #
  # - +command_line+ if it is a string,
  #   and if it begins with a shell reserved word or special built-in,
  #   or if it contains one or more meta characters.
  # - +exe_path+ otherwise.
  #
  # <b>Argument +command_line+</b>
  #
  # \String argument +command_line+ is a command line to be passed to a shell;
  # it must begin with a shell reserved word, begin with a special built-in,
  # or contain meta characters:
  #
  #   exec('if true; then echo "Foo"; fi') # Shell reserved word.
  #   exec('echo')                         # Built-in.
  #   exec('date > date.tmp')              # Contains meta character.
  #
  # The command line may also contain arguments and options for the command:
  #
  #   exec('echo "Foo"')
  #
  # Output:
  #
  #   Foo
  #
  # See {Execution Shell}[rdoc-ref:Process@Execution+Shell] for details about the shell.
  #
  # Raises an exception if the new process could not execute.
  #
  # <b>Argument +exe_path+</b>
  #
  # Argument +exe_path+ is one of the following:
  #
  # - The string path to an executable to be called.
  # - A 2-element array containing the path to an executable
  #   and the string to be used as the name of the executing process.
  #
  # Example:
  #
  #   exec('/usr/bin/date')
  #
  # Output:
  #
  #   Sat Aug 26 09:38:00 AM CDT 2023
  #
  # Ruby invokes the executable directly, with no shell and no shell expansion:
  #
  #   exec('doesnt_exist') # Raises Errno::ENOENT
  #
  # If one or more +args+ is given, each is an argument or option
  # to be passed to the executable:
  #
  #   exec('echo', 'C*')
  #   exec('echo', 'hello', 'world')
  #
  # Output:
  #
  #   C*
  #   hello world
  #
  # Raises an exception if the new process could not execute.
  def self.exec(...) end

  # Initiates termination of the Ruby script by raising SystemExit;
  # the exception may be caught.
  # Returns exit status +status+ to the underlying operating system.
  #
  # Values +true+ and +false+ for argument +status+
  # indicate, respectively, success and failure;
  # The meanings of integer values are system-dependent.
  #
  # Example:
  #
  #   begin
  #     exit
  #     puts 'Never get here.'
  #   rescue SystemExit
  #     puts 'Rescued a SystemExit exception.'
  #   end
  #   puts 'After begin block.'
  #
  # Output:
  #
  #   Rescued a SystemExit exception.
  #   After begin block.
  #
  # Just prior to final termination,
  # Ruby executes any at-exit procedures (see Kernel::at_exit)
  # and any object finalizers (see ObjectSpace::define_finalizer).
  #
  # Example:
  #
  #   at_exit { puts 'In at_exit function.' }
  #   ObjectSpace.define_finalizer('string', proc { puts 'In finalizer.' })
  #   exit
  #
  # Output:
  #
  #    In at_exit function.
  #    In finalizer.
  def self.exit(status = true) end

  # Exits the process immediately; no exit handlers are called.
  # Returns exit status +status+ to the underlying operating system.
  #
  #    Process.exit!(true)
  #
  # Values +true+ and +false+ for argument +status+
  # indicate, respectively, success and failure;
  # The meanings of integer values are system-dependent.
  def self.exit!(status = false) end

  # Creates a child process.
  #
  # With a block given, runs the block in the child process;
  # on block exit, the child terminates with a status of zero:
  #
  #   puts "Before the fork: #{Process.pid}"
  #   fork do
  #     puts "In the child process: #{Process.pid}"
  #   end                   # => 382141
  #   puts "After the fork: #{Process.pid}"
  #
  # Output:
  #
  #   Before the fork: 420496
  #   After the fork: 420496
  #   In the child process: 420520
  #
  # With no block given, the +fork+ call returns twice:
  #
  # - Once in the parent process, returning the pid of the child process.
  # - Once in the child process, returning +nil+.
  #
  # Example:
  #
  #   puts "This is the first line before the fork (pid #{Process.pid})"
  #   puts fork
  #   puts "This is the second line after the fork (pid #{Process.pid})"
  #
  # Output:
  #
  #   This is the first line before the fork (pid 420199)
  #   420223
  #   This is the second line after the fork (pid 420199)
  #
  #   This is the second line after the fork (pid 420223)
  #
  # In either case, the child process may exit using
  # Kernel.exit! to avoid the call to Kernel#at_exit.
  #
  # To avoid zombie processes, the parent process should call either:
  #
  # - Process.wait, to collect the termination statuses of its children.
  # - Process.detach, to register disinterest in their status.
  #
  # The thread calling +fork+ is the only thread in the created child process;
  # +fork+ doesn't copy other threads.
  #
  # Note that method +fork+ is available on some platforms,
  # but not on others:
  #
  #   Process.respond_to?(:fork) # => true # Would be false on some.
  #
  # If not, you may use ::spawn instead of +fork+.
  def self.fork; end

  #  Returns the process group ID for the given process ID +pid+:
  #
  #    Process.getpgid(Process.ppid) # => 25527
  #
  # Not available on all platforms.
  def self.getpgid(pid) end

  # Returns the process group ID for the current process:
  #
  #   Process.getpgid(0) # => 25527
  #   Process.getpgrp    # => 25527
  def self.getpgrp; end

  # Returns the scheduling priority for specified process, process group,
  # or user.
  #
  # Argument +kind+ is one of:
  #
  # - Process::PRIO_PROCESS: return priority for process.
  # - Process::PRIO_PGRP: return priority for process group.
  # - Process::PRIO_USER: return priority for user.
  #
  # Argument +id+ is the ID for the process, process group, or user;
  # zero specified the current ID for +kind+.
  #
  # Examples:
  #
  #   Process.getpriority(Process::PRIO_USER, 0)    # => 19
  #   Process.getpriority(Process::PRIO_PROCESS, 0) # => 19
  #
  # Not available on all platforms.
  def self.getpriority(kind, id) end

  # Returns a 2-element array of the current (soft) limit
  # and maximum (hard) limit for the given +resource+.
  #
  # Argument +resource+ specifies the resource whose limits are to be returned;
  # see Process.setrlimit.
  #
  # Each of the returned values +cur_limit+ and +max_limit+ is an integer;
  # see Process.setrlimit.
  #
  # Example:
  #
  #   Process.getrlimit(:CORE) # => [0, 18446744073709551615]
  #
  # See Process.setrlimit.
  #
  # Not available on all platforms.
  def self.getrlimit(resource) end

  # Returns the session ID of the given process ID +pid+,
  # or of the current process if not given:
  #
  #   Process.getsid                # => 27422
  #   Process.getsid(0)             # => 27422
  #   Process.getsid(Process.pid()) # => 27422
  #
  # Not available on all platforms.
  def self.getsid(pid = nil) end

  # Returns the (real) group ID for the current process:
  #
  #   Process.gid # => 1000
  def self.gid; end

  # Sets the group ID for the current process to +new_gid+:
  #
  #   Process.gid = 1000 # => 1000
  def self.gid=(new_gid) end

  # Returns an array of the group IDs
  # in the supplemental group access list for the current process:
  #
  #   Process.groups # => [4, 24, 27, 30, 46, 122, 135, 136, 1000]
  #
  # These properties of the returned array are system-dependent:
  #
  # - Whether (and how) the array is sorted.
  # - Whether the array includes effective group IDs.
  # - Whether the array includes duplicate group IDs.
  # - Whether the array size exceeds the value of Process.maxgroups.
  #
  # Use this call to get a sorted and unique array:
  #
  #   Process.groups.uniq.sort
  def self.groups; end

  # Sets the supplemental group access list to the given
  # array of group IDs.
  #
  #   Process.groups                     # => [0, 1, 2, 3, 4, 6, 10, 11, 20, 26, 27]
  #   Process.groups = [27, 6, 10, 11]   # => [27, 6, 10, 11]
  #   Process.groups                     # => [27, 6, 10, 11]
  def self.groups=(new_groups) end

  # Sets the supplemental group access list;
  # the new list includes:
  #
  # - The group IDs of those groups to which the user given by +username+ belongs.
  # - The group ID +gid+.
  #
  # Example:
  #
  #    Process.groups                # => [0, 1, 2, 3, 4, 6, 10, 11, 20, 26, 27]
  #    Process.initgroups('me', 30)  # => [30, 6, 10, 11]
  #    Process.groups                # => [30, 6, 10, 11]
  #
  # Not available on all platforms.
  def self.initgroups(username, gid) end

  # Sends a signal to each process specified by +ids+
  # (which must specify at least one ID);
  # returns the count of signals sent.
  #
  # For each given +id+, if +id+ is:
  #
  # - Positive, sends the signal to the process whose process ID is +id+.
  # - Zero, send the signal to all processes in the current process group.
  # - Negative, sends the signal to a system-dependent collection of processes.
  #
  # Argument +signal+ specifies the signal to be sent;
  # the argument may be:
  #
  # - An integer signal number: e.g., +-29+, +0+, +29+.
  # - A signal name (string), with or without leading <tt>'SIG'</tt>,
  #   and with or without a further prefixed minus sign (<tt>'-'</tt>):
  #   e.g.:
  #
  #   - <tt>'SIGPOLL'</tt>.
  #   - <tt>'POLL'</tt>,
  #   - <tt>'-SIGPOLL'</tt>.
  #   - <tt>'-POLL'</tt>.
  #
  # - A signal symbol, with or without leading <tt>'SIG'</tt>,
  #   and with or without a further prefixed minus sign (<tt>'-'</tt>):
  #   e.g.:
  #
  #   - +:SIGPOLL+.
  #   - +:POLL+.
  #   - <tt>:'-SIGPOLL'</tt>.
  #   - <tt>:'-POLL'</tt>.
  #
  # If +signal+ is:
  #
  # - A non-negative integer, or a signal name or symbol
  #   without prefixed <tt>'-'</tt>,
  #   each process with process ID +id+ is signalled.
  # - A negative integer, or a signal name or symbol
  #   with prefixed <tt>'-'</tt>,
  #   each process group with group ID +id+ is signalled.
  #
  # Use method Signal.list to see which signals are supported
  # by Ruby on the underlying platform;
  # the method returns a hash of the string names
  # and non-negative integer values of the supported signals.
  # The size and content of the returned hash varies widely
  # among platforms.
  #
  # Additionally, signal +0+ is useful to determine if the process exists.
  #
  # Example:
  #
  #   pid = fork do
  #     Signal.trap('HUP') { puts 'Ouch!'; exit }
  #     # ... do some work ...
  #   end
  #   # ...
  #   Process.kill('HUP', pid)
  #   Process.wait
  #
  # Output:
  #
  #    Ouch!
  #
  # Exceptions:
  #
  # - Raises Errno::EINVAL or RangeError if +signal+ is an integer
  #   but invalid.
  # - Raises ArgumentError if +signal+ is a string or symbol
  #   but invalid.
  # - Raises Errno::ESRCH or RangeError if one of +ids+ is invalid.
  # - Raises Errno::EPERM if needed permissions are not in force.
  #
  # In the last two cases, signals may have been sent to some processes.
  def self.kill(signal, *ids) end

  # Returns a Process::Status object representing the most recently exited
  # child process in the current thread, or +nil+ if none:
  #
  #   Process.spawn('ruby', '-e', 'exit 13')
  #   Process.wait
  #   Process.last_status # => #<Process::Status: pid 14396 exit 13>
  #
  #   Process.spawn('ruby', '-e', 'exit 14')
  #   Process.wait
  #   Process.last_status # => #<Process::Status: pid 4692 exit 14>
  #
  #   Process.spawn('ruby', '-e', 'exit 15')
  #   # 'exit 15' has not been reaped by #wait.
  #   Process.last_status # => #<Process::Status: pid 4692 exit 14>
  #   Process.wait
  #   Process.last_status # => #<Process::Status: pid 1380 exit 15>
  def self.last_status; end

  # Returns the maximum number of group IDs allowed
  # in the supplemental group access list:
  #
  #   Process.maxgroups # => 32
  def self.maxgroups; end

  # Sets the maximum number of group IDs allowed
  # in the supplemental group access list.
  def self.maxgroups=(new_max) end

  # Returns the process ID of the current process:
  #
  #   Process.pid # => 15668
  def self.pid; end

  # Returns the process ID of the parent of the current process:
  #
  #   puts "Pid is #{Process.pid}."
  #   fork { puts "Parent pid is #{Process.ppid}." }
  #
  # Output:
  #
  #   Pid is 271290.
  #   Parent pid is 271290.
  #
  # May not return a trustworthy value on certain platforms.
  def self.ppid; end

  # Sets the process group ID for the process given by process ID +pid+
  # to +pgid+.
  #
  # Not available on all platforms.
  def self.setpgid(pid, pgid) end

  # Equivalent to <tt>setpgid(0, 0)</tt>.
  #
  # Not available on all platforms.
  def self.setpgrp; end

  # See Process.getpriority.
  #
  # Examples:
  #
  #   Process.setpriority(Process::PRIO_USER, 0, 19)    # => 0
  #   Process.setpriority(Process::PRIO_PROCESS, 0, 19) # => 0
  #   Process.getpriority(Process::PRIO_USER, 0)        # => 19
  #   Process.getpriority(Process::PRIO_PROCESS, 0)     # => 19
  #
  # Not available on all platforms.
  def self.setpriority(kind, integer, priority) end

  # Sets the process title that appears on the ps(1) command.  Not
  # necessarily effective on all platforms.  No exception will be
  # raised regardless of the result, nor will NotImplementedError be
  # raised even if the platform does not support the feature.
  #
  # Calling this method does not affect the value of $0.
  #
  #    Process.setproctitle('myapp: worker #%d' % worker_id)
  #
  # This method first appeared in Ruby 2.1 to serve as a global
  # variable free means to change the process title.
  def self.setproctitle(string) end

  # Sets limits for the current process for the given +resource+
  # to +cur_limit+ (soft limit) and +max_limit+ (hard limit);
  # returns +nil+.
  #
  # Argument +resource+ specifies the resource whose limits are to be set;
  # the argument may be given as a symbol, as a string, or as a constant
  # beginning with <tt>Process::RLIMIT_</tt>
  # (e.g., +:CORE+, <tt>'CORE'</tt>, or <tt>Process::RLIMIT_CORE</tt>.
  #
  # The resources available and supported are system-dependent,
  # and may include (here expressed as symbols):
  #
  # - +:AS+: Total available memory (bytes) (SUSv3, NetBSD, FreeBSD, OpenBSD except 4.4BSD-Lite).
  # - +:CORE+: Core size (bytes) (SUSv3).
  # - +:CPU+: CPU time (seconds) (SUSv3).
  # - +:DATA+: Data segment (bytes) (SUSv3).
  # - +:FSIZE+: File size (bytes) (SUSv3).
  # - +:MEMLOCK+: Total size for mlock(2) (bytes) (4.4BSD, GNU/Linux).
  # - +:MSGQUEUE+: Allocation for POSIX message queues (bytes) (GNU/Linux).
  # - +:NICE+: Ceiling on process's nice(2) value (number) (GNU/Linux).
  # - +:NOFILE+: File descriptors (number) (SUSv3).
  # - +:NPROC+: Number of processes for the user (number) (4.4BSD, GNU/Linux).
  # - +:NPTS+: Number of pseudo terminals (number) (FreeBSD).
  # - +:RSS+: Resident memory size (bytes) (4.2BSD, GNU/Linux).
  # - +:RTPRIO+: Ceiling on the process's real-time priority (number) (GNU/Linux).
  # - +:RTTIME+: CPU time for real-time process (us) (GNU/Linux).
  # - +:SBSIZE+: All socket buffers (bytes) (NetBSD, FreeBSD).
  # - +:SIGPENDING+: Number of queued signals allowed (signals) (GNU/Linux).
  # - +:STACK+: Stack size (bytes) (SUSv3).
  #
  # Arguments +cur_limit+ and +max_limit+ may be:
  #
  # - Integers (+max_limit+ should not be smaller than +cur_limit+).
  # - Symbol +:SAVED_MAX+, string <tt>'SAVED_MAX'</tt>,
  #   or constant <tt>Process::RLIM_SAVED_MAX</tt>: saved maximum limit.
  # - Symbol +:SAVED_CUR+, string <tt>'SAVED_CUR'</tt>,
  #   or constant <tt>Process::RLIM_SAVED_CUR</tt>: saved current limit.
  # - Symbol +:INFINITY+, string <tt>'INFINITY'</tt>,
  #   or constant <tt>Process::RLIM_INFINITY</tt>: no limit on resource.
  #
  # This example raises the soft limit of core size to
  # the hard limit to try to make core dump possible:
  #
  #   Process.setrlimit(:CORE, Process.getrlimit(:CORE)[1])
  #
  # Not available on all platforms.
  def self.setrlimit(resource, cur_limit, max_limit = cur_limit) end

  # Establishes the current process as a new session and process group leader,
  # with no controlling tty;
  # returns the session ID:
  #
  #   Process.setsid # => 27422
  #
  # Not available on all platforms.
  def self.setsid; end

  # Creates a new child process by doing one of the following
  # in that process:
  #
  # - Passing string +command_line+ to the shell.
  # - Invoking the executable at +exe_path+.
  #
  # This method has potential security vulnerabilities if called with untrusted input;
  # see {Command Injection}[rdoc-ref:command_injection.rdoc].
  #
  # Returns the process ID (pid) of the new process,
  # without waiting for it to complete.
  #
  # To avoid zombie processes, the parent process should call either:
  #
  # - Process.wait, to collect the termination statuses of its children.
  # - Process.detach, to register disinterest in their status.
  #
  # The new process is created using the
  # {exec system call}[https://pubs.opengroup.org/onlinepubs/9699919799.2018edition/functions/execve.html];
  # it may inherit some of its environment from the calling program
  # (possibly including open file descriptors).
  #
  # Argument +env+, if given, is a hash that affects +ENV+ for the new process;
  # see {Execution Environment}[rdoc-ref:Process@Execution+Environment].
  #
  # Argument +options+ is a hash of options for the new process;
  # see {Execution Options}[rdoc-ref:Process@Execution+Options].
  #
  # The first required argument is one of the following:
  #
  # - +command_line+ if it is a string,
  #   and if it begins with a shell reserved word or special built-in,
  #   or if it contains one or more meta characters.
  # - +exe_path+ otherwise.
  #
  # <b>Argument +command_line+</b>
  #
  # \String argument +command_line+ is a command line to be passed to a shell;
  # it must begin with a shell reserved word, begin with a special built-in,
  # or contain meta characters:
  #
  #   spawn('if true; then echo "Foo"; fi') # => 798847 # Shell reserved word.
  #   Process.wait                          # => 798847
  #   spawn('echo')                         # => 798848 # Built-in.
  #   Process.wait                          # => 798848
  #   spawn('date > /tmp/date.tmp')         # => 798879 # Contains meta character.
  #   Process.wait                          # => 798849
  #   spawn('date > /nop/date.tmp')         # => 798882 # Issues error message.
  #   Process.wait                          # => 798882
  #
  # The command line may also contain arguments and options for the command:
  #
  #   spawn('echo "Foo"') # => 799031
  #   Process.wait        # => 799031
  #
  # Output:
  #
  #   Foo
  #
  # See {Execution Shell}[rdoc-ref:Process@Execution+Shell] for details about the shell.
  #
  # Raises an exception if the new process could not execute.
  #
  # <b>Argument +exe_path+</b>
  #
  # Argument +exe_path+ is one of the following:
  #
  # - The string path to an executable to be called:
  #
  #     spawn('/usr/bin/date') # Path to date on Unix-style system.
  #     Process.wait
  #
  #   Output:
  #
  #     Thu Aug 31 10:06:48 AM CDT 2023
  #
  # - A 2-element array containing the path to an executable
  #   and the string to be used as the name of the executing process:
  #
  #     pid = spawn(['sleep', 'Hello!'], '1') # 2-element array.
  #     p `ps -p #{pid} -o command=`
  #
  #   Output:
  #
  #     "Hello! 1\n"
  #
  # Ruby invokes the executable directly, with no shell and no shell expansion.
  #
  # If one or more +args+ is given, each is an argument or option
  # to be passed to the executable:
  #
  #   spawn('echo', 'C*')             # => 799392
  #   Process.wait                    # => 799392
  #   spawn('echo', 'hello', 'world') # => 799393
  #   Process.wait                    # => 799393
  #
  # Output:
  #
  #   C*
  #   hello world
  #
  # Raises an exception if the new process could not execute.
  def self.spawn(...) end

  # Returns a Process::Tms structure that contains user and system CPU times
  # for the current process, and for its children processes:
  #
  #   Process.times
  #   # => #<struct Process::Tms utime=55.122118, stime=35.533068, cutime=0.0, cstime=0.002846>
  #
  # The precision is platform-defined.
  def self.times; end

  # Returns the (real) user ID of the current process.
  #
  #   Process.uid # => 1000
  def self.uid; end

  # Sets the (user) user ID for the current process to +new_uid+:
  #
  #   Process.uid = 1000 # => 1000
  #
  # Not available on all platforms.
  def self.uid=(new_uid) end

  # Waits for a suitable child process to exit, returns its process ID,
  # and sets <tt>$?</tt> to a Process::Status object
  # containing information on that process.
  # Which child it waits for depends on the value of the given +pid+:
  #
  # - Positive integer: Waits for the child process whose process ID is +pid+:
  #
  #     pid0 = Process.spawn('ruby', '-e', 'exit 13') # => 230866
  #     pid1 = Process.spawn('ruby', '-e', 'exit 14') # => 230891
  #     Process.wait(pid0)                            # => 230866
  #     $?                                            # => #<Process::Status: pid 230866 exit 13>
  #     Process.wait(pid1)                            # => 230891
  #     $?                                            # => #<Process::Status: pid 230891 exit 14>
  #     Process.wait(pid0)                            # Raises Errno::ECHILD
  #
  # - <tt>0</tt>: Waits for any child process whose group ID
  #   is the same as that of the current process:
  #
  #     parent_pgpid = Process.getpgid(Process.pid)
  #     puts "Parent process group ID is #{parent_pgpid}."
  #     child0_pid = fork do
  #       puts "Child 0 pid is #{Process.pid}"
  #       child0_pgid = Process.getpgid(Process.pid)
  #       puts "Child 0 process group ID is #{child0_pgid} (same as parent's)."
  #     end
  #     child1_pid = fork do
  #       puts "Child 1 pid is #{Process.pid}"
  #       Process.setpgid(0, Process.pid)
  #       child1_pgid = Process.getpgid(Process.pid)
  #       puts "Child 1 process group ID is #{child1_pgid} (different from parent's)."
  #     end
  #     retrieved_pid = Process.wait(0)
  #     puts "Process.wait(0) returned pid #{retrieved_pid}, which is child 0 pid."
  #     begin
  #       Process.wait(0)
  #     rescue Errno::ECHILD => x
  #       puts "Raised #{x.class}, because child 1 process group ID differs from parent process group ID."
  #     end
  #
  #   Output:
  #
  #     Parent process group ID is 225764.
  #     Child 0 pid is 225788
  #     Child 0 process group ID is 225764 (same as parent's).
  #     Child 1 pid is 225789
  #     Child 1 process group ID is 225789 (different from parent's).
  #     Process.wait(0) returned pid 225788, which is child 0 pid.
  #     Raised Errno::ECHILD, because child 1 process group ID differs from parent process group ID.
  #
  # - <tt>-1</tt> (default): Waits for any child process:
  #
  #     parent_pgpid = Process.getpgid(Process.pid)
  #     puts "Parent process group ID is #{parent_pgpid}."
  #     child0_pid = fork do
  #       puts "Child 0 pid is #{Process.pid}"
  #       child0_pgid = Process.getpgid(Process.pid)
  #       puts "Child 0 process group ID is #{child0_pgid} (same as parent's)."
  #     end
  #     child1_pid = fork do
  #       puts "Child 1 pid is #{Process.pid}"
  #       Process.setpgid(0, Process.pid)
  #       child1_pgid = Process.getpgid(Process.pid)
  #       puts "Child 1 process group ID is #{child1_pgid} (different from parent's)."
  #       sleep 3 # To force child 1 to exit later than child 0 exit.
  #     end
  #     child_pids = [child0_pid, child1_pid]
  #     retrieved_pid = Process.wait(-1)
  #     puts child_pids.include?(retrieved_pid)
  #     retrieved_pid = Process.wait(-1)
  #     puts child_pids.include?(retrieved_pid)
  #
  #   Output:
  #
  #     Parent process group ID is 228736.
  #     Child 0 pid is 228758
  #     Child 0 process group ID is 228736 (same as parent's).
  #     Child 1 pid is 228759
  #     Child 1 process group ID is 228759 (different from parent's).
  #     true
  #     true
  #
  # - Less than <tt>-1</tt>: Waits for any child whose process group ID is <tt>-pid</tt>:
  #
  #     parent_pgpid = Process.getpgid(Process.pid)
  #     puts "Parent process group ID is #{parent_pgpid}."
  #     child0_pid = fork do
  #       puts "Child 0 pid is #{Process.pid}"
  #       child0_pgid = Process.getpgid(Process.pid)
  #       puts "Child 0 process group ID is #{child0_pgid} (same as parent's)."
  #     end
  #     child1_pid = fork do
  #       puts "Child 1 pid is #{Process.pid}"
  #       Process.setpgid(0, Process.pid)
  #       child1_pgid = Process.getpgid(Process.pid)
  #       puts "Child 1 process group ID is #{child1_pgid} (different from parent's)."
  #     end
  #     sleep 1
  #     retrieved_pid = Process.wait(-child1_pid)
  #     puts "Process.wait(-child1_pid) returned pid #{retrieved_pid}, which is child 1 pid."
  #     begin
  #       Process.wait(-child1_pid)
  #     rescue Errno::ECHILD => x
  #       puts "Raised #{x.class}, because there's no longer a child with process group id #{child1_pid}."
  #     end
  #
  #   Output:
  #
  #     Parent process group ID is 230083.
  #     Child 0 pid is 230108
  #     Child 0 process group ID is 230083 (same as parent's).
  #     Child 1 pid is 230109
  #     Child 1 process group ID is 230109 (different from parent's).
  #     Process.wait(-child1_pid) returned pid 230109, which is child 1 pid.
  #     Raised Errno::ECHILD, because there's no longer a child with process group id 230109.
  #
  # Argument +flags+ should be given as one of the following constants,
  # or as the logical OR of both:
  #
  # - Process::WNOHANG: Does not block if no child process is available.
  # - Process:WUNTRACED: May return a stopped child process, even if not yet reported.
  #
  # Not all flags are available on all platforms.
  #
  # Raises Errno::ECHILD if there is no suitable child process.
  #
  # Not available on all platforms.
  #
  # Process.waitpid is an alias for Process.wait.
  def self.wait(pid = -1, flags = 0) end

  # Like Process.waitpid, but returns an array
  # containing the child process +pid+ and Process::Status +status+:
  #
  #   pid = Process.spawn('ruby', '-e', 'exit 13') # => 309581
  #   Process.wait2(pid)
  #   # => [309581, #<Process::Status: pid 309581 exit 13>]
  #
  # Process.waitpid2 is an alias for Process.waitpid.
  def self.wait2(pid = -1, flags = 0) end

  # Waits for all children, returns an array of 2-element arrays;
  # each subarray contains the integer pid and Process::Status status
  # for one of the reaped child processes:
  #
  #   pid0 = Process.spawn('ruby', '-e', 'exit 13') # => 325470
  #   pid1 = Process.spawn('ruby', '-e', 'exit 14') # => 325495
  #   Process.waitall
  #   # => [[325470, #<Process::Status: pid 325470 exit 13>], [325495, #<Process::Status: pid 325495 exit 14>]]
  def self.waitall; end

  # Waits for a suitable child process to exit, returns its process ID,
  # and sets <tt>$?</tt> to a Process::Status object
  # containing information on that process.
  # Which child it waits for depends on the value of the given +pid+:
  #
  # - Positive integer: Waits for the child process whose process ID is +pid+:
  #
  #     pid0 = Process.spawn('ruby', '-e', 'exit 13') # => 230866
  #     pid1 = Process.spawn('ruby', '-e', 'exit 14') # => 230891
  #     Process.wait(pid0)                            # => 230866
  #     $?                                            # => #<Process::Status: pid 230866 exit 13>
  #     Process.wait(pid1)                            # => 230891
  #     $?                                            # => #<Process::Status: pid 230891 exit 14>
  #     Process.wait(pid0)                            # Raises Errno::ECHILD
  #
  # - <tt>0</tt>: Waits for any child process whose group ID
  #   is the same as that of the current process:
  #
  #     parent_pgpid = Process.getpgid(Process.pid)
  #     puts "Parent process group ID is #{parent_pgpid}."
  #     child0_pid = fork do
  #       puts "Child 0 pid is #{Process.pid}"
  #       child0_pgid = Process.getpgid(Process.pid)
  #       puts "Child 0 process group ID is #{child0_pgid} (same as parent's)."
  #     end
  #     child1_pid = fork do
  #       puts "Child 1 pid is #{Process.pid}"
  #       Process.setpgid(0, Process.pid)
  #       child1_pgid = Process.getpgid(Process.pid)
  #       puts "Child 1 process group ID is #{child1_pgid} (different from parent's)."
  #     end
  #     retrieved_pid = Process.wait(0)
  #     puts "Process.wait(0) returned pid #{retrieved_pid}, which is child 0 pid."
  #     begin
  #       Process.wait(0)
  #     rescue Errno::ECHILD => x
  #       puts "Raised #{x.class}, because child 1 process group ID differs from parent process group ID."
  #     end
  #
  #   Output:
  #
  #     Parent process group ID is 225764.
  #     Child 0 pid is 225788
  #     Child 0 process group ID is 225764 (same as parent's).
  #     Child 1 pid is 225789
  #     Child 1 process group ID is 225789 (different from parent's).
  #     Process.wait(0) returned pid 225788, which is child 0 pid.
  #     Raised Errno::ECHILD, because child 1 process group ID differs from parent process group ID.
  #
  # - <tt>-1</tt> (default): Waits for any child process:
  #
  #     parent_pgpid = Process.getpgid(Process.pid)
  #     puts "Parent process group ID is #{parent_pgpid}."
  #     child0_pid = fork do
  #       puts "Child 0 pid is #{Process.pid}"
  #       child0_pgid = Process.getpgid(Process.pid)
  #       puts "Child 0 process group ID is #{child0_pgid} (same as parent's)."
  #     end
  #     child1_pid = fork do
  #       puts "Child 1 pid is #{Process.pid}"
  #       Process.setpgid(0, Process.pid)
  #       child1_pgid = Process.getpgid(Process.pid)
  #       puts "Child 1 process group ID is #{child1_pgid} (different from parent's)."
  #       sleep 3 # To force child 1 to exit later than child 0 exit.
  #     end
  #     child_pids = [child0_pid, child1_pid]
  #     retrieved_pid = Process.wait(-1)
  #     puts child_pids.include?(retrieved_pid)
  #     retrieved_pid = Process.wait(-1)
  #     puts child_pids.include?(retrieved_pid)
  #
  #   Output:
  #
  #     Parent process group ID is 228736.
  #     Child 0 pid is 228758
  #     Child 0 process group ID is 228736 (same as parent's).
  #     Child 1 pid is 228759
  #     Child 1 process group ID is 228759 (different from parent's).
  #     true
  #     true
  #
  # - Less than <tt>-1</tt>: Waits for any child whose process group ID is <tt>-pid</tt>:
  #
  #     parent_pgpid = Process.getpgid(Process.pid)
  #     puts "Parent process group ID is #{parent_pgpid}."
  #     child0_pid = fork do
  #       puts "Child 0 pid is #{Process.pid}"
  #       child0_pgid = Process.getpgid(Process.pid)
  #       puts "Child 0 process group ID is #{child0_pgid} (same as parent's)."
  #     end
  #     child1_pid = fork do
  #       puts "Child 1 pid is #{Process.pid}"
  #       Process.setpgid(0, Process.pid)
  #       child1_pgid = Process.getpgid(Process.pid)
  #       puts "Child 1 process group ID is #{child1_pgid} (different from parent's)."
  #     end
  #     sleep 1
  #     retrieved_pid = Process.wait(-child1_pid)
  #     puts "Process.wait(-child1_pid) returned pid #{retrieved_pid}, which is child 1 pid."
  #     begin
  #       Process.wait(-child1_pid)
  #     rescue Errno::ECHILD => x
  #       puts "Raised #{x.class}, because there's no longer a child with process group id #{child1_pid}."
  #     end
  #
  #   Output:
  #
  #     Parent process group ID is 230083.
  #     Child 0 pid is 230108
  #     Child 0 process group ID is 230083 (same as parent's).
  #     Child 1 pid is 230109
  #     Child 1 process group ID is 230109 (different from parent's).
  #     Process.wait(-child1_pid) returned pid 230109, which is child 1 pid.
  #     Raised Errno::ECHILD, because there's no longer a child with process group id 230109.
  #
  # Argument +flags+ should be given as one of the following constants,
  # or as the logical OR of both:
  #
  # - Process::WNOHANG: Does not block if no child process is available.
  # - Process:WUNTRACED: May return a stopped child process, even if not yet reported.
  #
  # Not all flags are available on all platforms.
  #
  # Raises Errno::ECHILD if there is no suitable child process.
  #
  # Not available on all platforms.
  #
  # Process.waitpid is an alias for Process.wait.
  def self.waitpid(*args) end

  # Like Process.waitpid, but returns an array
  # containing the child process +pid+ and Process::Status +status+:
  #
  #   pid = Process.spawn('ruby', '-e', 'exit 13') # => 309581
  #   Process.wait2(pid)
  #   # => [309581, #<Process::Status: pid 309581 exit 13>]
  #
  # Process.waitpid2 is an alias for Process.waitpid.
  def self.waitpid2(*args) end

  # Notify the Ruby virtual machine that the boot sequence is finished,
  # and that now is a good time to optimize the application. This is useful
  # for long running applications.
  #
  # This method is expected to be called at the end of the application boot.
  # If the application is deployed using a pre-forking model, +Process.warmup+
  # should be called in the original process before the first fork.
  #
  # The actual optimizations performed are entirely implementation specific
  # and may change in the future without notice.
  #
  # On CRuby, +Process.warmup+:
  #
  # * Performs a major GC.
  # * Compacts the heap.
  # * Promotes all surviving objects to the old generation.
  # * Precomputes the coderange of all strings.
  # * Frees all empty heap pages and increments the allocatable pages counter
  #   by the number of pages freed.
  # * Invoke +malloc_trim+ if available to free empty malloc pages.
  def self.warmup; end

  private

  # Returns the name of the script being executed.  The value is not
  # affected by assigning a new value to $0.
  #
  # This method first appeared in Ruby 2.1 to serve as a global
  # variable free means to get the script name.
  def argv0; end

  # Returns a clock resolution as determined by POSIX function
  # {clock_getres()}[https://man7.org/linux/man-pages/man3/clock_getres.3.html]:
  #
  #   Process.clock_getres(:CLOCK_REALTIME) # => 1.0e-09
  #
  # See Process.clock_gettime for the values of +clock_id+ and +unit+.
  #
  # Examples:
  #
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :float_microsecond) # => 0.001
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :float_millisecond) # => 1.0e-06
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :float_second)      # => 1.0e-09
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :microsecond)       # => 0
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :millisecond)       # => 0
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :nanosecond)        # => 1
  #   Process.clock_getres(:CLOCK_PROCESS_CPUTIME_ID, :second)            # => 0
  #
  # In addition to the values for +unit+ supported in Process.clock_gettime,
  # this method supports +:hertz+, the integer number of clock ticks per second
  # (which is the reciprocal of +:float_second+):
  #
  #   Process.clock_getres(:TIMES_BASED_CLOCK_PROCESS_CPUTIME_ID, :hertz)        # => 100.0
  #   Process.clock_getres(:TIMES_BASED_CLOCK_PROCESS_CPUTIME_ID, :float_second) # => 0.01
  #
  # <b>Accuracy</b>:
  # Note that the returned resolution may be inaccurate on some platforms
  # due to underlying bugs.
  # Inaccurate resolutions have been reported for various clocks including
  # +:CLOCK_MONOTONIC+ and +:CLOCK_MONOTONIC_RAW+
  # on Linux, macOS, BSD or AIX platforms, when using ARM processors,
  # or when using virtualization.
  def clock_getres(clock_id, unit = :float_second) end

  # Returns a clock time as determined by POSIX function
  # {clock_gettime()}[https://man7.org/linux/man-pages/man3/clock_gettime.3.html]:
  #
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID) # => 198.650379677
  #
  # Argument +clock_id+ should be a symbol or a constant that specifies
  # the clock whose time is to be returned;
  # see below.
  #
  # Optional argument +unit+ should be a symbol that specifies
  # the unit to be used in the returned clock time;
  # see below.
  #
  # <b>Argument +clock_id+</b>
  #
  # Argument +clock_id+ specifies the clock whose time is to be returned;
  # it may be a constant such as <tt>Process::CLOCK_REALTIME</tt>,
  # or a symbol shorthand such as +:CLOCK_REALTIME+.
  #
  # The supported clocks depend on the underlying operating system;
  # this method supports the following clocks on the indicated platforms
  # (raises Errno::EINVAL if called with an unsupported clock):
  #
  # - +:CLOCK_BOOTTIME+: Linux 2.6.39.
  # - +:CLOCK_BOOTTIME_ALARM+: Linux 3.0.
  # - +:CLOCK_MONOTONIC+: SUSv3 to 4, Linux 2.5.63, FreeBSD 3.0, NetBSD 2.0, OpenBSD 3.4, macOS 10.12, Windows-2000.
  # - +:CLOCK_MONOTONIC_COARSE+: Linux 2.6.32.
  # - +:CLOCK_MONOTONIC_FAST+: FreeBSD 8.1.
  # - +:CLOCK_MONOTONIC_PRECISE+: FreeBSD 8.1.
  # - +:CLOCK_MONOTONIC_RAW+: Linux 2.6.28, macOS 10.12.
  # - +:CLOCK_MONOTONIC_RAW_APPROX+: macOS 10.12.
  # - +:CLOCK_PROCESS_CPUTIME_ID+: SUSv3 to 4, Linux 2.5.63, FreeBSD 9.3, OpenBSD 5.4, macOS 10.12.
  # - +:CLOCK_PROF+: FreeBSD 3.0, OpenBSD 2.1.
  # - +:CLOCK_REALTIME+: SUSv2 to 4, Linux 2.5.63, FreeBSD 3.0, NetBSD 2.0, OpenBSD 2.1, macOS 10.12, Windows-8/Server-2012.
  #   Time.now is recommended over +:CLOCK_REALTIME:.
  # - +:CLOCK_REALTIME_ALARM+: Linux 3.0.
  # - +:CLOCK_REALTIME_COARSE+: Linux 2.6.32.
  # - +:CLOCK_REALTIME_FAST+: FreeBSD 8.1.
  # - +:CLOCK_REALTIME_PRECISE+: FreeBSD 8.1.
  # - +:CLOCK_SECOND+: FreeBSD 8.1.
  # - +:CLOCK_TAI+: Linux 3.10.
  # - +:CLOCK_THREAD_CPUTIME_ID+: SUSv3 to 4, Linux 2.5.63, FreeBSD 7.1, OpenBSD 5.4, macOS 10.12.
  # - +:CLOCK_UPTIME+: FreeBSD 7.0, OpenBSD 5.5.
  # - +:CLOCK_UPTIME_FAST+: FreeBSD 8.1.
  # - +:CLOCK_UPTIME_PRECISE+: FreeBSD 8.1.
  # - +:CLOCK_UPTIME_RAW+: macOS 10.12.
  # - +:CLOCK_UPTIME_RAW_APPROX+: macOS 10.12.
  # - +:CLOCK_VIRTUAL+: FreeBSD 3.0, OpenBSD 2.1.
  #
  # Note that SUS stands for Single Unix Specification.
  # SUS contains POSIX and clock_gettime is defined in the POSIX part.
  # SUS defines +:CLOCK_REALTIME+ as mandatory but
  # +:CLOCK_MONOTONIC+, +:CLOCK_PROCESS_CPUTIME_ID+,
  # and +:CLOCK_THREAD_CPUTIME_ID+ are optional.
  #
  # Certain emulations are used when the given +clock_id+
  # is not supported directly:
  #
  # - Emulations for +:CLOCK_REALTIME+:
  #
  #   - +:GETTIMEOFDAY_BASED_CLOCK_REALTIME+:
  #     Use gettimeofday() defined by SUS (deprecated in SUSv4).
  #     The resolution is 1 microsecond.
  #   - +:TIME_BASED_CLOCK_REALTIME+:
  #     Use time() defined by ISO C.
  #     The resolution is 1 second.
  #
  # - Emulations for +:CLOCK_MONOTONIC+:
  #
  #   - +:MACH_ABSOLUTE_TIME_BASED_CLOCK_MONOTONIC+:
  #     Use mach_absolute_time(), available on Darwin.
  #     The resolution is CPU dependent.
  #   - +:TIMES_BASED_CLOCK_MONOTONIC+:
  #     Use the result value of times() defined by POSIX, thus:
  #     >>>
  #       Upon successful completion, times() shall return the elapsed real time,
  #       in clock ticks, since an arbitrary point in the past
  #       (for example, system start-up time).
  #
  #     For example, GNU/Linux returns a value based on jiffies and it is monotonic.
  #     However, 4.4BSD uses gettimeofday() and it is not monotonic.
  #     (FreeBSD uses +:CLOCK_MONOTONIC+ instead, though.)
  #
  #     The resolution is the clock tick.
  #     "getconf CLK_TCK" command shows the clock ticks per second.
  #     (The clock ticks-per-second is defined by HZ macro in older systems.)
  #     If it is 100 and clock_t is 32 bits integer type,
  #     the resolution is 10 millisecond and cannot represent over 497 days.
  #
  # - Emulations for +:CLOCK_PROCESS_CPUTIME_ID+:
  #
  #   - +:GETRUSAGE_BASED_CLOCK_PROCESS_CPUTIME_ID+:
  #     Use getrusage() defined by SUS.
  #     getrusage() is used with RUSAGE_SELF to obtain the time only for
  #     the calling process (excluding the time for child processes).
  #     The result is addition of user time (ru_utime) and system time (ru_stime).
  #     The resolution is 1 microsecond.
  #   - +:TIMES_BASED_CLOCK_PROCESS_CPUTIME_ID+:
  #     Use times() defined by POSIX.
  #     The result is addition of user time (tms_utime) and system time (tms_stime).
  #     tms_cutime and tms_cstime are ignored to exclude the time for child processes.
  #     The resolution is the clock tick.
  #     "getconf CLK_TCK" command shows the clock ticks per second.
  #     (The clock ticks per second is defined by HZ macro in older systems.)
  #     If it is 100, the resolution is 10 millisecond.
  #   - +:CLOCK_BASED_CLOCK_PROCESS_CPUTIME_ID+:
  #     Use clock() defined by ISO C.
  #     The resolution is <tt>1/CLOCKS_PER_SEC</tt>.
  #     +CLOCKS_PER_SEC+ is the C-level macro defined by time.h.
  #     SUS defines +CLOCKS_PER_SEC+ as 1000000;
  #     other systems may define it differently.
  #     If +CLOCKS_PER_SEC+ is 1000000 (as in SUS),
  #     the resolution is 1 microsecond.
  #     If +CLOCKS_PER_SEC+ is 1000000 and clock_t is a 32-bit integer type,
  #     it cannot represent over 72 minutes.
  #
  # <b>Argument +unit+</b>
  #
  # Optional argument +unit+ (default +:float_second+)
  # specifies the unit for the returned value.
  #
  # - +:float_microsecond+: Number of microseconds as a float.
  # - +:float_millisecond+: Number of milliseconds as a float.
  # - +:float_second+: Number of seconds as a float.
  # - +:microsecond+: Number of microseconds as an integer.
  # - +:millisecond+: Number of milliseconds as an integer.
  # - +:nanosecond+: Number of nanoseconds as an integer.
  # - +::second+: Number of seconds as an integer.
  #
  # Examples:
  #
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :float_microsecond)
  #   # => 203605054.825
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :float_millisecond)
  #   # => 203643.696848
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :float_second)
  #   # => 203.762181929
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :microsecond)
  #   # => 204123212
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :millisecond)
  #   # => 204298
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :nanosecond)
  #   # => 204602286036
  #   Process.clock_gettime(:CLOCK_PROCESS_CPUTIME_ID, :second)
  #   # => 204
  #
  # The underlying function, clock_gettime(), returns a number of nanoseconds.
  # Float object (IEEE 754 double) is not enough to represent
  # the return value for +:CLOCK_REALTIME+.
  # If the exact nanoseconds value is required, use +:nanosecond+ as the +unit+.
  #
  # The origin (time zero) of the returned value is system-dependent,
  # and may be, for example, system start up time,
  # process start up time, the Epoch, etc.
  #
  # The origin in +:CLOCK_REALTIME+ is defined as the Epoch:
  # <tt>1970-01-01 00:00:00 UTC</tt>;
  # some systems count leap seconds and others don't,
  # so the result may vary across systems.
  def clock_gettime(clock_id, unit = :float_second) end

  # Detaches the current process from its controlling terminal
  # and runs it in the background as system daemon;
  # returns zero.
  #
  # By default:
  #
  # - Changes the current working directory to the root directory.
  # - Redirects $stdin, $stdout, and $stderr to the null device.
  #
  # If optional argument +nochdir+ is +true+,
  # does not change the current working directory.
  #
  # If optional argument +noclose+ is +true+,
  # does not redirect $stdin, $stdout, or $stderr.
  def daemon(nochdir = nil, noclose = nil) end

  # Avoids the potential for a child process to become a
  # {zombie process}[https://en.wikipedia.org/wiki/Zombie_process].
  # Process.detach prevents this by setting up a separate Ruby thread
  # whose sole job is to reap the status of the process _pid_ when it terminates.
  #
  # This method is needed only when the parent process will never wait
  # for the child process.
  #
  # This example does not reap the second child process;
  # that process appears as a zombie in the process status (+ps+) output:
  #
  #   pid = Process.spawn('ruby', '-e', 'exit 13') # => 312691
  #   sleep(1)
  #   # Find zombies.
  #   system("ps -ho pid,state -p #{pid}")
  #
  # Output:
  #
  #    312716 Z
  #
  # This example also does not reap the second child process,
  # but it does detach the process so that it does not become a zombie:
  #
  #   pid = Process.spawn('ruby', '-e', 'exit 13') # => 313213
  #   thread = Process.detach(pid)
  #   sleep(1)
  #   # => #<Process::Waiter:0x00007f038f48b838 run>
  #   system("ps -ho pid,state -p #{pid}")        # Finds no zombies.
  #
  # The waiting thread can return the pid of the detached child process:
  #
  #   thread.join.pid                       # => 313262
  def detach(pid) end

  # Returns the effective group ID for the current process:
  #
  #   Process.egid # => 500
  #
  # Not available on all platforms.
  def egid; end

  # Sets the effective group ID for the current process.
  #
  # Not available on all platforms.
  def egid=(new_egid) end

  # Returns the effective user ID for the current process.
  #
  #   Process.euid # => 501
  def euid; end

  # Sets the effective user ID for the current process.
  #
  # Not available on all platforms.
  def euid=(new_euid) end

  #  Returns the process group ID for the given process ID +pid+:
  #
  #    Process.getpgid(Process.ppid) # => 25527
  #
  # Not available on all platforms.
  def getpgid(pid) end

  # Returns the process group ID for the current process:
  #
  #   Process.getpgid(0) # => 25527
  #   Process.getpgrp    # => 25527
  def getpgrp; end

  # Returns the scheduling priority for specified process, process group,
  # or user.
  #
  # Argument +kind+ is one of:
  #
  # - Process::PRIO_PROCESS: return priority for process.
  # - Process::PRIO_PGRP: return priority for process group.
  # - Process::PRIO_USER: return priority for user.
  #
  # Argument +id+ is the ID for the process, process group, or user;
  # zero specified the current ID for +kind+.
  #
  # Examples:
  #
  #   Process.getpriority(Process::PRIO_USER, 0)    # => 19
  #   Process.getpriority(Process::PRIO_PROCESS, 0) # => 19
  #
  # Not available on all platforms.
  def getpriority(kind, id) end

  # Returns a 2-element array of the current (soft) limit
  # and maximum (hard) limit for the given +resource+.
  #
  # Argument +resource+ specifies the resource whose limits are to be returned;
  # see Process.setrlimit.
  #
  # Each of the returned values +cur_limit+ and +max_limit+ is an integer;
  # see Process.setrlimit.
  #
  # Example:
  #
  #   Process.getrlimit(:CORE) # => [0, 18446744073709551615]
  #
  # See Process.setrlimit.
  #
  # Not available on all platforms.
  def getrlimit(resource) end

  # Returns the session ID of the given process ID +pid+,
  # or of the current process if not given:
  #
  #   Process.getsid                # => 27422
  #   Process.getsid(0)             # => 27422
  #   Process.getsid(Process.pid()) # => 27422
  #
  # Not available on all platforms.
  def getsid(pid = nil) end

  # Returns the (real) group ID for the current process:
  #
  #   Process.gid # => 1000
  def gid; end

  # Sets the group ID for the current process to +new_gid+:
  #
  #   Process.gid = 1000 # => 1000
  def gid=(new_gid) end

  # Returns an array of the group IDs
  # in the supplemental group access list for the current process:
  #
  #   Process.groups # => [4, 24, 27, 30, 46, 122, 135, 136, 1000]
  #
  # These properties of the returned array are system-dependent:
  #
  # - Whether (and how) the array is sorted.
  # - Whether the array includes effective group IDs.
  # - Whether the array includes duplicate group IDs.
  # - Whether the array size exceeds the value of Process.maxgroups.
  #
  # Use this call to get a sorted and unique array:
  #
  #   Process.groups.uniq.sort
  def groups; end

  # Sets the supplemental group access list to the given
  # array of group IDs.
  #
  #   Process.groups                     # => [0, 1, 2, 3, 4, 6, 10, 11, 20, 26, 27]
  #   Process.groups = [27, 6, 10, 11]   # => [27, 6, 10, 11]
  #   Process.groups                     # => [27, 6, 10, 11]
  def groups=(new_groups) end

  # Sets the supplemental group access list;
  # the new list includes:
  #
  # - The group IDs of those groups to which the user given by +username+ belongs.
  # - The group ID +gid+.
  #
  # Example:
  #
  #    Process.groups                # => [0, 1, 2, 3, 4, 6, 10, 11, 20, 26, 27]
  #    Process.initgroups('me', 30)  # => [30, 6, 10, 11]
  #    Process.groups                # => [30, 6, 10, 11]
  #
  # Not available on all platforms.
  def initgroups(username, gid) end

  # Sends a signal to each process specified by +ids+
  # (which must specify at least one ID);
  # returns the count of signals sent.
  #
  # For each given +id+, if +id+ is:
  #
  # - Positive, sends the signal to the process whose process ID is +id+.
  # - Zero, send the signal to all processes in the current process group.
  # - Negative, sends the signal to a system-dependent collection of processes.
  #
  # Argument +signal+ specifies the signal to be sent;
  # the argument may be:
  #
  # - An integer signal number: e.g., +-29+, +0+, +29+.
  # - A signal name (string), with or without leading <tt>'SIG'</tt>,
  #   and with or without a further prefixed minus sign (<tt>'-'</tt>):
  #   e.g.:
  #
  #   - <tt>'SIGPOLL'</tt>.
  #   - <tt>'POLL'</tt>,
  #   - <tt>'-SIGPOLL'</tt>.
  #   - <tt>'-POLL'</tt>.
  #
  # - A signal symbol, with or without leading <tt>'SIG'</tt>,
  #   and with or without a further prefixed minus sign (<tt>'-'</tt>):
  #   e.g.:
  #
  #   - +:SIGPOLL+.
  #   - +:POLL+.
  #   - <tt>:'-SIGPOLL'</tt>.
  #   - <tt>:'-POLL'</tt>.
  #
  # If +signal+ is:
  #
  # - A non-negative integer, or a signal name or symbol
  #   without prefixed <tt>'-'</tt>,
  #   each process with process ID +id+ is signalled.
  # - A negative integer, or a signal name or symbol
  #   with prefixed <tt>'-'</tt>,
  #   each process group with group ID +id+ is signalled.
  #
  # Use method Signal.list to see which signals are supported
  # by Ruby on the underlying platform;
  # the method returns a hash of the string names
  # and non-negative integer values of the supported signals.
  # The size and content of the returned hash varies widely
  # among platforms.
  #
  # Additionally, signal +0+ is useful to determine if the process exists.
  #
  # Example:
  #
  #   pid = fork do
  #     Signal.trap('HUP') { puts 'Ouch!'; exit }
  #     # ... do some work ...
  #   end
  #   # ...
  #   Process.kill('HUP', pid)
  #   Process.wait
  #
  # Output:
  #
  #    Ouch!
  #
  # Exceptions:
  #
  # - Raises Errno::EINVAL or RangeError if +signal+ is an integer
  #   but invalid.
  # - Raises ArgumentError if +signal+ is a string or symbol
  #   but invalid.
  # - Raises Errno::ESRCH or RangeError if one of +ids+ is invalid.
  # - Raises Errno::EPERM if needed permissions are not in force.
  #
  # In the last two cases, signals may have been sent to some processes.
  def kill(signal, *ids) end

  # Returns the maximum number of group IDs allowed
  # in the supplemental group access list:
  #
  #   Process.maxgroups # => 32
  def maxgroups; end

  # Sets the maximum number of group IDs allowed
  # in the supplemental group access list.
  def maxgroups=(new_max) end

  # Returns the process ID of the current process:
  #
  #   Process.pid # => 15668
  def pid; end

  # Returns the process ID of the parent of the current process:
  #
  #   puts "Pid is #{Process.pid}."
  #   fork { puts "Parent pid is #{Process.ppid}." }
  #
  # Output:
  #
  #   Pid is 271290.
  #   Parent pid is 271290.
  #
  # May not return a trustworthy value on certain platforms.
  def ppid; end

  # Sets the process group ID for the process given by process ID +pid+
  # to +pgid+.
  #
  # Not available on all platforms.
  def setpgid(pid, pgid) end

  # Equivalent to <tt>setpgid(0, 0)</tt>.
  #
  # Not available on all platforms.
  def setpgrp; end

  # See Process.getpriority.
  #
  # Examples:
  #
  #   Process.setpriority(Process::PRIO_USER, 0, 19)    # => 0
  #   Process.setpriority(Process::PRIO_PROCESS, 0, 19) # => 0
  #   Process.getpriority(Process::PRIO_USER, 0)        # => 19
  #   Process.getpriority(Process::PRIO_PROCESS, 0)     # => 19
  #
  # Not available on all platforms.
  def setpriority(kind, integer, priority) end

  # Sets the process title that appears on the ps(1) command.  Not
  # necessarily effective on all platforms.  No exception will be
  # raised regardless of the result, nor will NotImplementedError be
  # raised even if the platform does not support the feature.
  #
  # Calling this method does not affect the value of $0.
  #
  #    Process.setproctitle('myapp: worker #%d' % worker_id)
  #
  # This method first appeared in Ruby 2.1 to serve as a global
  # variable free means to change the process title.
  def setproctitle(string) end

  # Sets limits for the current process for the given +resource+
  # to +cur_limit+ (soft limit) and +max_limit+ (hard limit);
  # returns +nil+.
  #
  # Argument +resource+ specifies the resource whose limits are to be set;
  # the argument may be given as a symbol, as a string, or as a constant
  # beginning with <tt>Process::RLIMIT_</tt>
  # (e.g., +:CORE+, <tt>'CORE'</tt>, or <tt>Process::RLIMIT_CORE</tt>.
  #
  # The resources available and supported are system-dependent,
  # and may include (here expressed as symbols):
  #
  # - +:AS+: Total available memory (bytes) (SUSv3, NetBSD, FreeBSD, OpenBSD except 4.4BSD-Lite).
  # - +:CORE+: Core size (bytes) (SUSv3).
  # - +:CPU+: CPU time (seconds) (SUSv3).
  # - +:DATA+: Data segment (bytes) (SUSv3).
  # - +:FSIZE+: File size (bytes) (SUSv3).
  # - +:MEMLOCK+: Total size for mlock(2) (bytes) (4.4BSD, GNU/Linux).
  # - +:MSGQUEUE+: Allocation for POSIX message queues (bytes) (GNU/Linux).
  # - +:NICE+: Ceiling on process's nice(2) value (number) (GNU/Linux).
  # - +:NOFILE+: File descriptors (number) (SUSv3).
  # - +:NPROC+: Number of processes for the user (number) (4.4BSD, GNU/Linux).
  # - +:NPTS+: Number of pseudo terminals (number) (FreeBSD).
  # - +:RSS+: Resident memory size (bytes) (4.2BSD, GNU/Linux).
  # - +:RTPRIO+: Ceiling on the process's real-time priority (number) (GNU/Linux).
  # - +:RTTIME+: CPU time for real-time process (us) (GNU/Linux).
  # - +:SBSIZE+: All socket buffers (bytes) (NetBSD, FreeBSD).
  # - +:SIGPENDING+: Number of queued signals allowed (signals) (GNU/Linux).
  # - +:STACK+: Stack size (bytes) (SUSv3).
  #
  # Arguments +cur_limit+ and +max_limit+ may be:
  #
  # - Integers (+max_limit+ should not be smaller than +cur_limit+).
  # - Symbol +:SAVED_MAX+, string <tt>'SAVED_MAX'</tt>,
  #   or constant <tt>Process::RLIM_SAVED_MAX</tt>: saved maximum limit.
  # - Symbol +:SAVED_CUR+, string <tt>'SAVED_CUR'</tt>,
  #   or constant <tt>Process::RLIM_SAVED_CUR</tt>: saved current limit.
  # - Symbol +:INFINITY+, string <tt>'INFINITY'</tt>,
  #   or constant <tt>Process::RLIM_INFINITY</tt>: no limit on resource.
  #
  # This example raises the soft limit of core size to
  # the hard limit to try to make core dump possible:
  #
  #   Process.setrlimit(:CORE, Process.getrlimit(:CORE)[1])
  #
  # Not available on all platforms.
  def setrlimit(resource, cur_limit, max_limit = cur_limit) end

  # Establishes the current process as a new session and process group leader,
  # with no controlling tty;
  # returns the session ID:
  #
  #   Process.setsid # => 27422
  #
  # Not available on all platforms.
  def setsid; end

  # Returns a Process::Tms structure that contains user and system CPU times
  # for the current process, and for its children processes:
  #
  #   Process.times
  #   # => #<struct Process::Tms utime=55.122118, stime=35.533068, cutime=0.0, cstime=0.002846>
  #
  # The precision is platform-defined.
  def times; end

  # Returns the (real) user ID of the current process.
  #
  #   Process.uid # => 1000
  def uid; end

  # Sets the (user) user ID for the current process to +new_uid+:
  #
  #   Process.uid = 1000 # => 1000
  #
  # Not available on all platforms.
  def uid=(new_uid) end

  # Waits for a suitable child process to exit, returns its process ID,
  # and sets <tt>$?</tt> to a Process::Status object
  # containing information on that process.
  # Which child it waits for depends on the value of the given +pid+:
  #
  # - Positive integer: Waits for the child process whose process ID is +pid+:
  #
  #     pid0 = Process.spawn('ruby', '-e', 'exit 13') # => 230866
  #     pid1 = Process.spawn('ruby', '-e', 'exit 14') # => 230891
  #     Process.wait(pid0)                            # => 230866
  #     $?                                            # => #<Process::Status: pid 230866 exit 13>
  #     Process.wait(pid1)                            # => 230891
  #     $?                                            # => #<Process::Status: pid 230891 exit 14>
  #     Process.wait(pid0)                            # Raises Errno::ECHILD
  #
  # - <tt>0</tt>: Waits for any child process whose group ID
  #   is the same as that of the current process:
  #
  #     parent_pgpid = Process.getpgid(Process.pid)
  #     puts "Parent process group ID is #{parent_pgpid}."
  #     child0_pid = fork do
  #       puts "Child 0 pid is #{Process.pid}"
  #       child0_pgid = Process.getpgid(Process.pid)
  #       puts "Child 0 process group ID is #{child0_pgid} (same as parent's)."
  #     end
  #     child1_pid = fork do
  #       puts "Child 1 pid is #{Process.pid}"
  #       Process.setpgid(0, Process.pid)
  #       child1_pgid = Process.getpgid(Process.pid)
  #       puts "Child 1 process group ID is #{child1_pgid} (different from parent's)."
  #     end
  #     retrieved_pid = Process.wait(0)
  #     puts "Process.wait(0) returned pid #{retrieved_pid}, which is child 0 pid."
  #     begin
  #       Process.wait(0)
  #     rescue Errno::ECHILD => x
  #       puts "Raised #{x.class}, because child 1 process group ID differs from parent process group ID."
  #     end
  #
  #   Output:
  #
  #     Parent process group ID is 225764.
  #     Child 0 pid is 225788
  #     Child 0 process group ID is 225764 (same as parent's).
  #     Child 1 pid is 225789
  #     Child 1 process group ID is 225789 (different from parent's).
  #     Process.wait(0) returned pid 225788, which is child 0 pid.
  #     Raised Errno::ECHILD, because child 1 process group ID differs from parent process group ID.
  #
  # - <tt>-1</tt> (default): Waits for any child process:
  #
  #     parent_pgpid = Process.getpgid(Process.pid)
  #     puts "Parent process group ID is #{parent_pgpid}."
  #     child0_pid = fork do
  #       puts "Child 0 pid is #{Process.pid}"
  #       child0_pgid = Process.getpgid(Process.pid)
  #       puts "Child 0 process group ID is #{child0_pgid} (same as parent's)."
  #     end
  #     child1_pid = fork do
  #       puts "Child 1 pid is #{Process.pid}"
  #       Process.setpgid(0, Process.pid)
  #       child1_pgid = Process.getpgid(Process.pid)
  #       puts "Child 1 process group ID is #{child1_pgid} (different from parent's)."
  #       sleep 3 # To force child 1 to exit later than child 0 exit.
  #     end
  #     child_pids = [child0_pid, child1_pid]
  #     retrieved_pid = Process.wait(-1)
  #     puts child_pids.include?(retrieved_pid)
  #     retrieved_pid = Process.wait(-1)
  #     puts child_pids.include?(retrieved_pid)
  #
  #   Output:
  #
  #     Parent process group ID is 228736.
  #     Child 0 pid is 228758
  #     Child 0 process group ID is 228736 (same as parent's).
  #     Child 1 pid is 228759
  #     Child 1 process group ID is 228759 (different from parent's).
  #     true
  #     true
  #
  # - Less than <tt>-1</tt>: Waits for any child whose process group ID is <tt>-pid</tt>:
  #
  #     parent_pgpid = Process.getpgid(Process.pid)
  #     puts "Parent process group ID is #{parent_pgpid}."
  #     child0_pid = fork do
  #       puts "Child 0 pid is #{Process.pid}"
  #       child0_pgid = Process.getpgid(Process.pid)
  #       puts "Child 0 process group ID is #{child0_pgid} (same as parent's)."
  #     end
  #     child1_pid = fork do
  #       puts "Child 1 pid is #{Process.pid}"
  #       Process.setpgid(0, Process.pid)
  #       child1_pgid = Process.getpgid(Process.pid)
  #       puts "Child 1 process group ID is #{child1_pgid} (different from parent's)."
  #     end
  #     sleep 1
  #     retrieved_pid = Process.wait(-child1_pid)
  #     puts "Process.wait(-child1_pid) returned pid #{retrieved_pid}, which is child 1 pid."
  #     begin
  #       Process.wait(-child1_pid)
  #     rescue Errno::ECHILD => x
  #       puts "Raised #{x.class}, because there's no longer a child with process group id #{child1_pid}."
  #     end
  #
  #   Output:
  #
  #     Parent process group ID is 230083.
  #     Child 0 pid is 230108
  #     Child 0 process group ID is 230083 (same as parent's).
  #     Child 1 pid is 230109
  #     Child 1 process group ID is 230109 (different from parent's).
  #     Process.wait(-child1_pid) returned pid 230109, which is child 1 pid.
  #     Raised Errno::ECHILD, because there's no longer a child with process group id 230109.
  #
  # Argument +flags+ should be given as one of the following constants,
  # or as the logical OR of both:
  #
  # - Process::WNOHANG: Does not block if no child process is available.
  # - Process:WUNTRACED: May return a stopped child process, even if not yet reported.
  #
  # Not all flags are available on all platforms.
  #
  # Raises Errno::ECHILD if there is no suitable child process.
  #
  # Not available on all platforms.
  #
  # Process.waitpid is an alias for Process.wait.
  def wait(pid = -1, flags = 0) end
  alias waitpid wait

  # Like Process.waitpid, but returns an array
  # containing the child process +pid+ and Process::Status +status+:
  #
  #   pid = Process.spawn('ruby', '-e', 'exit 13') # => 309581
  #   Process.wait2(pid)
  #   # => [309581, #<Process::Status: pid 309581 exit 13>]
  #
  # Process.waitpid2 is an alias for Process.waitpid.
  def wait2(pid = -1, flags = 0) end
  alias waitpid2 wait2

  # Waits for all children, returns an array of 2-element arrays;
  # each subarray contains the integer pid and Process::Status status
  # for one of the reaped child processes:
  #
  #   pid0 = Process.spawn('ruby', '-e', 'exit 13') # => 325470
  #   pid1 = Process.spawn('ruby', '-e', 'exit 14') # => 325495
  #   Process.waitall
  #   # => [[325470, #<Process::Status: pid 325470 exit 13>], [325495, #<Process::Status: pid 325495 exit 14>]]
  def waitall; end

  # Notify the Ruby virtual machine that the boot sequence is finished,
  # and that now is a good time to optimize the application. This is useful
  # for long running applications.
  #
  # This method is expected to be called at the end of the application boot.
  # If the application is deployed using a pre-forking model, +Process.warmup+
  # should be called in the original process before the first fork.
  #
  # The actual optimizations performed are entirely implementation specific
  # and may change in the future without notice.
  #
  # On CRuby, +Process.warmup+:
  #
  # * Performs a major GC.
  # * Compacts the heap.
  # * Promotes all surviving objects to the old generation.
  # * Precomputes the coderange of all strings.
  # * Frees all empty heap pages and increments the allocatable pages counter
  #   by the number of pages freed.
  # * Invoke +malloc_trim+ if available to free empty malloc pages.
  def warmup; end

  # The Process::GID module contains a collection of
  # module functions which can be used to portably get, set, and
  # switch the current process's real, effective, and saved group IDs.
  module GID
    # Change the current process's real and effective group ID to that
    # specified by _group_. Returns the new group ID. Not
    # available on all platforms.
    #
    #    [Process.gid, Process.egid]          #=> [0, 0]
    #    Process::GID.change_privilege(33)    #=> 33
    #    [Process.gid, Process.egid]          #=> [33, 33]
    def self.change_privilege(group) end

    # Returns the effective group ID for the current process:
    #
    #   Process.egid # => 500
    #
    # Not available on all platforms.
    def self.eid; end

    # Get the group ID by the _name_.
    # If the group is not found, +ArgumentError+ will be raised.
    #
    #    Process::GID.from_name("wheel") #=> 0
    #    Process::GID.from_name("nosuchgroup") #=> can't find group for nosuchgroup (ArgumentError)
    def self.from_name(name) end

    # Set the effective group ID, and if possible, the saved group ID of
    # the process to the given _group_. Returns the new
    # effective group ID. Not available on all platforms.
    #
    #    [Process.gid, Process.egid]          #=> [0, 0]
    #    Process::GID.grant_privilege(31)     #=> 33
    #    [Process.gid, Process.egid]          #=> [0, 33]
    def self.grant_privilege(group) end

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

    # Returns the (real) group ID for the current process:
    #
    #   Process.gid # => 1000
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
    # specified by _group_. Returns the new group ID. Not
    # available on all platforms.
    #
    #    [Process.gid, Process.egid]          #=> [0, 0]
    #    Process::GID.change_privilege(33)    #=> 33
    #    [Process.gid, Process.egid]          #=> [33, 33]
    def change_privilege(group) end

    # Returns the effective group ID for the current process:
    #
    #   Process.egid # => 500
    #
    # Not available on all platforms.
    def eid; end

    # Get the group ID by the _name_.
    # If the group is not found, +ArgumentError+ will be raised.
    #
    #    Process::GID.from_name("wheel") #=> 0
    #    Process::GID.from_name("nosuchgroup") #=> can't find group for nosuchgroup (ArgumentError)
    def from_name(name) end

    # Set the effective group ID, and if possible, the saved group ID of
    # the process to the given _group_. Returns the new
    # effective group ID. Not available on all platforms.
    #
    #    [Process.gid, Process.egid]          #=> [0, 0]
    #    Process::GID.grant_privilege(31)     #=> 33
    #    [Process.gid, Process.egid]          #=> [0, 33]
    def grant_privilege(group) end

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

    # Returns the (real) group ID for the current process:
    #
    #   Process.gid # => 1000
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

  # A Process::Status contains information about a system process.
  #
  # Thread-local variable <tt>$?</tt> is initially +nil+.
  # Some methods assign to it a Process::Status object
  # that represents a system process (either running or terminated):
  #
  #   `ruby -e "exit 99"`
  #   stat = $?       # => #<Process::Status: pid 1262862 exit 99>
  #   stat.class      # => Process::Status
  #   stat.to_i       # => 25344
  #   stat.stopped?   # => false
  #   stat.exited?    # => true
  #   stat.exitstatus # => 99
  class Status
    # Like Process.wait, but returns a Process::Status object
    # (instead of an integer pid or nil);
    # see Process.wait for the values of +pid+ and +flags+.
    #
    # If there are child processes,
    # waits for a child process to exit and returns a Process::Status object
    # containing information on that process;
    # sets thread-local variable <tt>$?</tt>:
    #
    #   Process.spawn('cat /nop') # => 1155880
    #   Process::Status.wait      # => #<Process::Status: pid 1155880 exit 1>
    #   $?                        # => #<Process::Status: pid 1155508 exit 1>
    #
    # If there is no child process,
    # returns an "empty" Process::Status object
    # that does not represent an actual process;
    # does not set thread-local variable <tt>$?</tt>:
    #
    #   Process::Status.wait # => #<Process::Status: pid -1 exit 0>
    #   $?                   # => #<Process::Status: pid 1155508 exit 1> # Unchanged.
    #
    # May invoke the scheduler hook Fiber::Scheduler#process_wait.
    #
    # Not available on all platforms.
    def self.wait(pid = -1, flags = 0) end

    # This method is deprecated as #to_i value is system-specific; use
    # predicate methods like #exited? or #stopped?, or getters like #exitstatus
    # or #stopsig.
    #
    # Returns the logical AND of the value of #to_i with +mask+:
    #
    #   `cat /nop`
    #   stat = $?                 # => #<Process::Status: pid 1155508 exit 1>
    #   sprintf('%x', stat.to_i)  # => "100"
    #   stat & 0x00               # => 0
    #
    # ArgumentError is raised if +mask+ is negative.
    def &(other) end

    # Returns whether the value of #to_i == +other+:
    #
    #   `cat /nop`
    #   stat = $?                # => #<Process::Status: pid 1170366 exit 1>
    #   sprintf('%x', stat.to_i) # => "100"
    #   stat == 0x100            # => true
    def ==(other) end

    # This method is deprecated as #to_i value is system-specific; use
    # predicate methods like #exited? or #stopped?, or getters like #exitstatus
    # or #stopsig.
    #
    # Returns the value of #to_i, shifted +places+ to the right:
    #
    #    `cat /nop`
    #    stat = $?                 # => #<Process::Status: pid 1155508 exit 1>
    #    stat.to_i                 # => 256
    #    stat >> 1                 # => 128
    #    stat >> 2                 # => 64
    #
    # ArgumentError is raised if +places+ is negative.
    def >>(other) end

    # Returns +true+ if the process generated a coredump
    # when it terminated, +false+ if not.
    #
    # Not available on all platforms.
    def coredump?; end

    # Returns +true+ if the process exited normally
    # (for example using an <code>exit()</code> call or finishing the
    # program), +false+ if not.
    def exited?; end

    # Returns the least significant eight bits of the return code
    # of the process if it has exited;
    # +nil+ otherwise:
    #
    #   `exit 99`
    #   $?.exitstatus # => 99
    def exitstatus; end

    # Returns a string representation of +self+:
    #
    #   system("false")
    #   $?.inspect # => "#<Process::Status: pid 1303494 exit 1>"
    def inspect; end

    # Returns the process ID of the process:
    #
    #   system("false")
    #   $?.pid # => 1247002
    def pid; end

    # Returns +true+ if the process terminated because of an uncaught signal,
    # +false+ otherwise.
    def signaled?; end

    # Returns +true+ if this process is stopped,
    # and if the corresponding #wait call had the Process::WUNTRACED flag set,
    # +false+ otherwise.
    def stopped?; end

    # Returns the number of the signal that caused the process to stop,
    # or +nil+ if the process is not stopped.
    def stopsig; end

    # Returns:
    #
    # - +true+ if the process has completed successfully and exited.
    # - +false+ if the process has completed unsuccessfully and exited.
    # - +nil+ if the process has not exited.
    def success?; end

    # Returns the number of the signal that caused the process to terminate
    # or +nil+ if the process was not terminated by an uncaught signal.
    def termsig; end

    # Returns the system-dependent integer status of +self+:
    #
    #   `cat /nop`
    #   $?.to_i # => 256
    def to_i; end

    # Returns a string representation of +self+:
    #
    #   `cat /nop`
    #   $?.to_s # => "pid 1262141 exit 1"
    def to_s; end
  end

  # The Process::Sys module contains UID and GID
  # functions which provide direct bindings to the system calls of the
  # same names instead of the more-portable versions of the same
  # functionality found in the Process,
  # Process::UID, and Process::GID modules.
  module Sys
    # Returns the effective group ID for the current process:
    #
    #   Process.egid # => 500
    #
    # Not available on all platforms.
    def self.getegid; end

    # Returns the effective user ID for the current process.
    #
    #   Process.euid # => 501
    def self.geteuid; end

    # Returns the (real) group ID for the current process:
    #
    #   Process.gid # => 1000
    def self.getgid; end

    # Returns the (real) user ID of the current process.
    #
    #   Process.uid # => 1000
    def self.getuid; end

    # Returns +true+ if the process was created as a result
    # of an execve(2) system call which had either of the setuid or
    # setgid bits set (and extra privileges were given as a result) or
    # if it has changed any of its real, effective or saved user or
    # group IDs since it began execution.
    def self.issetugid; end

    # Set the effective group ID of the calling process to
    # _group_.  Not available on all platforms.
    def self.setegid(group) end

    # Set the effective user ID of the calling process to
    # _user_.  Not available on all platforms.
    def self.seteuid(user) end

    # Set the group ID of the current process to _group_. Not
    # available on all platforms.
    def self.setgid(group) end

    # Sets the (group) real and/or effective group IDs of the current
    # process to <em>rid</em> and <em>eid</em>, respectively. A value of
    # <code>-1</code> for either means to leave that ID unchanged. Not
    # available on all platforms.
    def self.setregid(rid, eid) end

    # Sets the (group) real, effective, and saved user IDs of the
    # current process to <em>rid</em>, <em>eid</em>, and <em>sid</em>
    # respectively. A value of <code>-1</code> for any value means to
    # leave that ID unchanged. Not available on all platforms.
    def self.setresgid(rid, eid, sid) end

    # Sets the (user) real, effective, and saved user IDs of the
    # current process to _rid_, _eid_, and _sid_ respectively. A
    # value of <code>-1</code> for any value means to
    # leave that ID unchanged. Not available on all platforms.
    def self.setresuid(rid, eid, sid) end

    # Sets the (user) real and/or effective user IDs of the current
    # process to _rid_ and _eid_, respectively. A value of
    # <code>-1</code> for either means to leave that ID unchanged. Not
    # available on all platforms.
    def self.setreuid(rid, eid) end

    # Set the real group ID of the calling process to _group_.
    # Not available on all platforms.
    def self.setrgid(group) end

    # Set the real user ID of the calling process to _user_.
    # Not available on all platforms.
    def self.setruid(user) end

    # Set the user ID of the current process to _user_. Not
    # available on all platforms.
    def self.setuid(user) end

    private

    # Returns the effective group ID for the current process:
    #
    #   Process.egid # => 500
    #
    # Not available on all platforms.
    def getegid; end

    # Returns the effective user ID for the current process.
    #
    #   Process.euid # => 501
    def geteuid; end

    # Returns the (real) group ID for the current process:
    #
    #   Process.gid # => 1000
    def getgid; end

    # Returns the (real) user ID of the current process.
    #
    #   Process.uid # => 1000
    def getuid; end

    # Returns +true+ if the process was created as a result
    # of an execve(2) system call which had either of the setuid or
    # setgid bits set (and extra privileges were given as a result) or
    # if it has changed any of its real, effective or saved user or
    # group IDs since it began execution.
    def issetugid; end

    # Set the effective group ID of the calling process to
    # _group_.  Not available on all platforms.
    def setegid(group) end

    # Set the effective user ID of the calling process to
    # _user_.  Not available on all platforms.
    def seteuid(user) end

    # Set the group ID of the current process to _group_. Not
    # available on all platforms.
    def setgid(group) end

    # Sets the (group) real and/or effective group IDs of the current
    # process to <em>rid</em> and <em>eid</em>, respectively. A value of
    # <code>-1</code> for either means to leave that ID unchanged. Not
    # available on all platforms.
    def setregid(rid, eid) end

    # Sets the (group) real, effective, and saved user IDs of the
    # current process to <em>rid</em>, <em>eid</em>, and <em>sid</em>
    # respectively. A value of <code>-1</code> for any value means to
    # leave that ID unchanged. Not available on all platforms.
    def setresgid(rid, eid, sid) end

    # Sets the (user) real, effective, and saved user IDs of the
    # current process to _rid_, _eid_, and _sid_ respectively. A
    # value of <code>-1</code> for any value means to
    # leave that ID unchanged. Not available on all platforms.
    def setresuid(rid, eid, sid) end

    # Sets the (user) real and/or effective user IDs of the current
    # process to _rid_ and _eid_, respectively. A value of
    # <code>-1</code> for either means to leave that ID unchanged. Not
    # available on all platforms.
    def setreuid(rid, eid) end

    # Set the real group ID of the calling process to _group_.
    # Not available on all platforms.
    def setrgid(group) end

    # Set the real user ID of the calling process to _user_.
    # Not available on all platforms.
    def setruid(user) end

    # Set the user ID of the current process to _user_. Not
    # available on all platforms.
    def setuid(user) end
  end

  # Placeholder for rusage
  class Tms
  end

  # The Process::UID module contains a collection of
  # module functions which can be used to portably get, set, and
  # switch the current process's real, effective, and saved user IDs.
  module UID
    # Change the current process's real and effective user ID to that
    # specified by _user_. Returns the new user ID. Not
    # available on all platforms.
    #
    #    [Process.uid, Process.euid]          #=> [0, 0]
    #    Process::UID.change_privilege(31)    #=> 31
    #    [Process.uid, Process.euid]          #=> [31, 31]
    def self.change_privilege(user) end

    # Returns the effective user ID for the current process.
    #
    #   Process.euid # => 501
    def self.eid; end

    # Get the user ID by the _name_.
    # If the user is not found, +ArgumentError+ will be raised.
    #
    #    Process::UID.from_name("root") #=> 0
    #    Process::UID.from_name("nosuchuser") #=> can't find user for nosuchuser (ArgumentError)
    def self.from_name(name) end

    # Set the effective user ID, and if possible, the saved user ID of
    # the process to the given _user_. Returns the new
    # effective user ID. Not available on all platforms.
    #
    #    [Process.uid, Process.euid]          #=> [0, 0]
    #    Process::UID.grant_privilege(31)     #=> 31
    #    [Process.uid, Process.euid]          #=> [0, 31]
    def self.grant_privilege(user) end

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

    # Returns the (real) user ID of the current process.
    #
    #   Process.uid # => 1000
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
    # specified by _user_. Returns the new user ID. Not
    # available on all platforms.
    #
    #    [Process.uid, Process.euid]          #=> [0, 0]
    #    Process::UID.change_privilege(31)    #=> 31
    #    [Process.uid, Process.euid]          #=> [31, 31]
    def change_privilege(user) end

    # Returns the effective user ID for the current process.
    #
    #   Process.euid # => 501
    def eid; end

    # Get the user ID by the _name_.
    # If the user is not found, +ArgumentError+ will be raised.
    #
    #    Process::UID.from_name("root") #=> 0
    #    Process::UID.from_name("nosuchuser") #=> can't find user for nosuchuser (ArgumentError)
    def from_name(name) end

    # Set the effective user ID, and if possible, the saved user ID of
    # the process to the given _user_. Returns the new
    # effective user ID. Not available on all platforms.
    #
    #    [Process.uid, Process.euid]          #=> [0, 0]
    #    Process::UID.grant_privilege(31)     #=> 31
    #    [Process.uid, Process.euid]          #=> [0, 31]
    def grant_privilege(user) end

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

    # Returns the (real) user ID of the current process.
    #
    #   Process.uid # => 1000
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
