# frozen_string_literal: true

# The Etc module provides access to information typically stored in
# files in the /etc directory on Unix systems.
#
# The information accessible consists of the information found in the
# /etc/passwd and /etc/group files, plus information about the system's
# temporary directory (/tmp) and configuration directory (/etc).
#
# The Etc module provides a more reliable way to access information about
# the logged in user than environment variables such as +$USER+.
#
# == Example:
#
#     require 'etc'
#
#     login = Etc.getlogin
#     info = Etc.getpwnam(login)
#     username = info.gecos.split(/,/).first
#     puts "Hello #{username}, I see your login name is #{login}"
#
# Note that the methods provided by this module are not always secure.
# It should be used for informational purposes, and not for security.
#
# All operations defined in this module are class methods, so that you can
# include the Etc module into your class.
module Etc
  # Group
  #
  # Group is a Struct that is only available when compiled with +HAVE_GETGRENT+.
  #
  # The struct contains the following members:
  #
  # name::
  #      contains the name of the group as a String.
  # passwd::
  #      contains the encrypted password as a String. An 'x' is
  #      returned if password access to the group is not available; an empty
  #      string is returned if no password is needed to obtain membership of
  #      the group.
  #
  #      Must be compiled with +HAVE_STRUCT_GROUP_GR_PASSWD+.
  # gid::
  #      contains the group's numeric ID as an integer.
  # mem::
  #      is an Array of Strings containing the short login names of the
  #      members of the group.
  Group = _
  # Passwd
  #
  # Passwd is a Struct that contains the following members:
  #
  # name::
  #      contains the short login name of the user as a String.
  # passwd::
  #      contains the encrypted password of the user as a String.
  #      an 'x' is returned if shadow passwords are in use. An '*' is returned
  #      if the user cannot log in using a password.
  # uid::
  #      contains the integer user ID (uid) of the user.
  # gid::
  #      contains the integer group ID (gid) of the user's primary group.
  # dir::
  #      contains the path to the home directory of the user as a String.
  # shell::
  #      contains the path to the login shell of the user as a String.
  #
  # === The following members below are optional, and must be compiled with special flags:
  #
  # gecos::
  #     contains a longer String description of the user, such as
  #     a full name. Some Unix systems provide structured information in the
  #     gecos field, but this is system-dependent.
  #     must be compiled with +HAVE_STRUCT_PASSWD_PW_GECOS+
  # change::
  #     password change time(integer) must be compiled with +HAVE_STRUCT_PASSWD_PW_CHANGE+
  # quota::
  #     quota value(integer) must be compiled with +HAVE_STRUCT_PASSWD_PW_QUOTA+
  # age::
  #     password age(integer) must be compiled with +HAVE_STRUCT_PASSWD_PW_AGE+
  # class::
  #     user access class(string) must be compiled with +HAVE_STRUCT_PASSWD_PW_CLASS+
  # comment::
  #     comment(string) must be compiled with +HAVE_STRUCT_PASSWD_PW_COMMENT+
  # expire::
  #      account expiration time(integer) must be compiled with +HAVE_STRUCT_PASSWD_PW_EXPIRE+
  Passwd = _

  # Returns system configuration variable using confstr().
  #
  # _name_ should be a constant under <code>Etc</code> which begins with <code>CS_</code>.
  #
  # The return value is a string or nil.
  # nil means no configuration-defined value.  (confstr() returns 0 but errno is not set.)
  #
  #   Etc.confstr(Etc::CS_PATH) #=> "/bin:/usr/bin"
  #
  #   # GNU/Linux
  #   Etc.confstr(Etc::CS_GNU_LIBC_VERSION) #=> "glibc 2.18"
  #   Etc.confstr(Etc::CS_GNU_LIBPTHREAD_VERSION) #=> "NPTL 2.18"
  def self.confstr(p1) end

  # Ends the process of scanning through the /etc/group file begun by
  # ::getgrent, and closes the file.
  def self.endgrent; end

  # Ends the process of scanning through the /etc/passwd file begun with
  # ::getpwent, and closes the file.
  def self.endpwent; end

  # Returns an entry from the /etc/group file.
  #
  # The first time it is called it opens the file and returns the first entry;
  # each successive call returns the next entry, or +nil+ if the end of the file
  # has been reached.
  #
  # To close the file when processing is complete, call ::endgrent.
  #
  # Each entry is returned as a Group struct
  def self.getgrent; end

  # Returns information about the group with specified integer +group_id+,
  # as found in /etc/group.
  #
  # The information is returned as a Group struct.
  #
  # See the unix manpage for <code>getgrgid(3)</code> for more detail.
  #
  # === Example:
  #
  #      Etc.getgrgid(100)
  #      #=> #<struct Etc::Group name="users", passwd="x", gid=100, mem=["meta", "root"]>
  def self.getgrgid(group_id) end

  # Returns information about the group with specified +name+, as found in
  # /etc/group.
  #
  # The information is returned as a Group struct.
  #
  # See the unix manpage for <code>getgrnam(3)</code> for more detail.
  #
  # === Example:
  #
  #      Etc.getgrnam('users')
  #      #=> #<struct Etc::Group name="users", passwd="x", gid=100, mem=["meta", "root"]>
  def self.getgrnam(name) end

  # Returns the short user name of the currently logged in user.
  # Unfortunately, it is often rather easy to fool ::getlogin.
  #
  # Avoid ::getlogin for security-related purposes.
  #
  # If ::getlogin fails, try ::getpwuid.
  #
  # See the unix manpage for <code>getpwuid(3)</code> for more detail.
  #
  # e.g.
  #   Etc.getlogin -> 'guest'
  def self.getlogin; end

  # Returns an entry from the /etc/passwd file.
  #
  # The first time it is called it opens the file and returns the first entry;
  # each successive call returns the next entry, or +nil+ if the end of the file
  # has been reached.
  #
  # To close the file when processing is complete, call ::endpwent.
  #
  # Each entry is returned as a Passwd struct.
  def self.getpwent; end

  # Returns the /etc/passwd information for the user with specified login
  # +name+.
  #
  # The information is returned as a Passwd struct.
  #
  # See the unix manpage for <code>getpwnam(3)</code> for more detail.
  #
  # === Example:
  #
  #      Etc.getpwnam('root')
  #      #=> #<struct Etc::Passwd name="root", passwd="x", uid=0, gid=0, gecos="root",dir="/root", shell="/bin/bash">
  def self.getpwnam(name) end

  # Returns the /etc/passwd information for the user with the given integer +uid+.
  #
  # The information is returned as a Passwd struct.
  #
  # If +uid+ is omitted, the value from <code>Passwd[:uid]</code> is returned
  # instead.
  #
  # See the unix manpage for <code>getpwuid(3)</code> for more detail.
  #
  # === Example:
  #
  #      Etc.getpwuid(0)
  #      #=> #<struct Etc::Passwd name="root", passwd="x", uid=0, gid=0, gecos="root",dir="/root", shell="/bin/bash">
  def self.getpwuid(uid) end

  # Provides a convenient Ruby iterator which executes a block for each entry
  # in the /etc/group file.
  #
  # The code block is passed an Group struct.
  #
  # See ::getgrent above for details.
  #
  # Example:
  #
  #     require 'etc'
  #
  #     Etc.group {|g|
  #       puts g.name + ": " + g.mem.join(', ')
  #     }
  def self.group; end

  # Returns the number of online processors.
  #
  # The result is intended as the number of processes to
  # use all available processors.
  #
  # This method is implemented using:
  # - sched_getaffinity(): Linux
  # - sysconf(_SC_NPROCESSORS_ONLN): GNU/Linux, NetBSD, FreeBSD, OpenBSD, DragonFly BSD, OpenIndiana, Mac OS X, AIX
  #
  # Example:
  #
  #   require 'etc'
  #   p Etc.nprocessors #=> 4
  #
  # The result might be smaller number than physical cpus especially when ruby
  # process is bound to specific cpus. This is intended for getting better
  # parallel processing.
  #
  # Example: (Linux)
  #
  #   linux$ taskset 0x3 ./ruby -retc -e "p Etc.nprocessors"  #=> 2
  def self.nprocessors; end

  # Provides a convenient Ruby iterator which executes a block for each entry
  # in the /etc/passwd file.
  #
  # The code block is passed an Passwd struct.
  #
  # See ::getpwent above for details.
  #
  # Example:
  #
  #     require 'etc'
  #
  #     Etc.passwd {|u|
  #       puts u.name + " = " + u.gecos
  #     }
  def self.passwd; end

  # Resets the process of reading the /etc/group file, so that the next call
  # to ::getgrent will return the first entry again.
  def self.setgrent; end

  # Resets the process of reading the /etc/passwd file, so that the next call
  # to ::getpwent will return the first entry again.
  def self.setpwent; end

  # Returns system configuration variable using sysconf().
  #
  # _name_ should be a constant under <code>Etc</code> which begins with <code>SC_</code>.
  #
  # The return value is an integer or nil.
  # nil means indefinite limit.  (sysconf() returns -1 but errno is not set.)
  #
  #   Etc.sysconf(Etc::SC_ARG_MAX) #=> 2097152
  #   Etc.sysconf(Etc::SC_LOGIN_NAME_MAX) #=> 256
  def self.sysconf(p1) end

  # Returns system configuration directory.
  #
  # This is typically "/etc", but is modified by the prefix used when Ruby was
  # compiled. For example, if Ruby is built and installed in /usr/local,
  # returns "/usr/local/etc" on other platforms than Windows.
  # On Windows, this always returns the directory provided by the system.
  def self.sysconfdir; end

  # Returns system temporary directory; typically "/tmp".
  def self.systmpdir; end

  # Returns the system information obtained by uname system call.
  #
  # The return value is a hash which has 5 keys at least:
  #   :sysname, :nodename, :release, :version, :machine
  #
  # Example:
  #
  #   require 'etc'
  #   require 'pp'
  #
  #   pp Etc.uname
  #   #=> {:sysname=>"Linux",
  #   #    :nodename=>"boron",
  #   #    :release=>"2.6.18-6-xen-686",
  #   #    :version=>"#1 SMP Thu Nov 5 19:54:42 UTC 2009",
  #   #    :machine=>"i686"}
  def self.uname; end

  private

  # Returns system configuration variable using confstr().
  #
  # _name_ should be a constant under <code>Etc</code> which begins with <code>CS_</code>.
  #
  # The return value is a string or nil.
  # nil means no configuration-defined value.  (confstr() returns 0 but errno is not set.)
  #
  #   Etc.confstr(Etc::CS_PATH) #=> "/bin:/usr/bin"
  #
  #   # GNU/Linux
  #   Etc.confstr(Etc::CS_GNU_LIBC_VERSION) #=> "glibc 2.18"
  #   Etc.confstr(Etc::CS_GNU_LIBPTHREAD_VERSION) #=> "NPTL 2.18"
  def confstr(p1) end

  # Ends the process of scanning through the /etc/group file begun by
  # ::getgrent, and closes the file.
  def endgrent; end

  # Ends the process of scanning through the /etc/passwd file begun with
  # ::getpwent, and closes the file.
  def endpwent; end

  # Returns an entry from the /etc/group file.
  #
  # The first time it is called it opens the file and returns the first entry;
  # each successive call returns the next entry, or +nil+ if the end of the file
  # has been reached.
  #
  # To close the file when processing is complete, call ::endgrent.
  #
  # Each entry is returned as a Group struct
  def getgrent; end

  # Returns information about the group with specified integer +group_id+,
  # as found in /etc/group.
  #
  # The information is returned as a Group struct.
  #
  # See the unix manpage for <code>getgrgid(3)</code> for more detail.
  #
  # === Example:
  #
  #      Etc.getgrgid(100)
  #      #=> #<struct Etc::Group name="users", passwd="x", gid=100, mem=["meta", "root"]>
  def getgrgid(group_id) end

  # Returns information about the group with specified +name+, as found in
  # /etc/group.
  #
  # The information is returned as a Group struct.
  #
  # See the unix manpage for <code>getgrnam(3)</code> for more detail.
  #
  # === Example:
  #
  #      Etc.getgrnam('users')
  #      #=> #<struct Etc::Group name="users", passwd="x", gid=100, mem=["meta", "root"]>
  def getgrnam(name) end

  # Returns the short user name of the currently logged in user.
  # Unfortunately, it is often rather easy to fool ::getlogin.
  #
  # Avoid ::getlogin for security-related purposes.
  #
  # If ::getlogin fails, try ::getpwuid.
  #
  # See the unix manpage for <code>getpwuid(3)</code> for more detail.
  #
  # e.g.
  #   Etc.getlogin -> 'guest'
  def getlogin; end

  # Returns an entry from the /etc/passwd file.
  #
  # The first time it is called it opens the file and returns the first entry;
  # each successive call returns the next entry, or +nil+ if the end of the file
  # has been reached.
  #
  # To close the file when processing is complete, call ::endpwent.
  #
  # Each entry is returned as a Passwd struct.
  def getpwent; end

  # Returns the /etc/passwd information for the user with specified login
  # +name+.
  #
  # The information is returned as a Passwd struct.
  #
  # See the unix manpage for <code>getpwnam(3)</code> for more detail.
  #
  # === Example:
  #
  #      Etc.getpwnam('root')
  #      #=> #<struct Etc::Passwd name="root", passwd="x", uid=0, gid=0, gecos="root",dir="/root", shell="/bin/bash">
  def getpwnam(name) end

  # Returns the /etc/passwd information for the user with the given integer +uid+.
  #
  # The information is returned as a Passwd struct.
  #
  # If +uid+ is omitted, the value from <code>Passwd[:uid]</code> is returned
  # instead.
  #
  # See the unix manpage for <code>getpwuid(3)</code> for more detail.
  #
  # === Example:
  #
  #      Etc.getpwuid(0)
  #      #=> #<struct Etc::Passwd name="root", passwd="x", uid=0, gid=0, gecos="root",dir="/root", shell="/bin/bash">
  def getpwuid(uid) end

  # Provides a convenient Ruby iterator which executes a block for each entry
  # in the /etc/group file.
  #
  # The code block is passed an Group struct.
  #
  # See ::getgrent above for details.
  #
  # Example:
  #
  #     require 'etc'
  #
  #     Etc.group {|g|
  #       puts g.name + ": " + g.mem.join(', ')
  #     }
  def group; end

  # Returns the number of online processors.
  #
  # The result is intended as the number of processes to
  # use all available processors.
  #
  # This method is implemented using:
  # - sched_getaffinity(): Linux
  # - sysconf(_SC_NPROCESSORS_ONLN): GNU/Linux, NetBSD, FreeBSD, OpenBSD, DragonFly BSD, OpenIndiana, Mac OS X, AIX
  #
  # Example:
  #
  #   require 'etc'
  #   p Etc.nprocessors #=> 4
  #
  # The result might be smaller number than physical cpus especially when ruby
  # process is bound to specific cpus. This is intended for getting better
  # parallel processing.
  #
  # Example: (Linux)
  #
  #   linux$ taskset 0x3 ./ruby -retc -e "p Etc.nprocessors"  #=> 2
  def nprocessors; end

  # Provides a convenient Ruby iterator which executes a block for each entry
  # in the /etc/passwd file.
  #
  # The code block is passed an Passwd struct.
  #
  # See ::getpwent above for details.
  #
  # Example:
  #
  #     require 'etc'
  #
  #     Etc.passwd {|u|
  #       puts u.name + " = " + u.gecos
  #     }
  def passwd; end

  # Resets the process of reading the /etc/group file, so that the next call
  # to ::getgrent will return the first entry again.
  def setgrent; end

  # Resets the process of reading the /etc/passwd file, so that the next call
  # to ::getpwent will return the first entry again.
  def setpwent; end

  # Returns system configuration variable using sysconf().
  #
  # _name_ should be a constant under <code>Etc</code> which begins with <code>SC_</code>.
  #
  # The return value is an integer or nil.
  # nil means indefinite limit.  (sysconf() returns -1 but errno is not set.)
  #
  #   Etc.sysconf(Etc::SC_ARG_MAX) #=> 2097152
  #   Etc.sysconf(Etc::SC_LOGIN_NAME_MAX) #=> 256
  def sysconf(p1) end

  # Returns system configuration directory.
  #
  # This is typically "/etc", but is modified by the prefix used when Ruby was
  # compiled. For example, if Ruby is built and installed in /usr/local,
  # returns "/usr/local/etc" on other platforms than Windows.
  # On Windows, this always returns the directory provided by the system.
  def sysconfdir; end

  # Returns system temporary directory; typically "/tmp".
  def systmpdir; end

  # Returns the system information obtained by uname system call.
  #
  # The return value is a hash which has 5 keys at least:
  #   :sysname, :nodename, :release, :version, :machine
  #
  # Example:
  #
  #   require 'etc'
  #   require 'pp'
  #
  #   pp Etc.uname
  #   #=> {:sysname=>"Linux",
  #   #    :nodename=>"boron",
  #   #    :release=>"2.6.18-6-xen-686",
  #   #    :version=>"#1 SMP Thu Nov 5 19:54:42 UTC 2009",
  #   #    :machine=>"i686"}
  def uname; end
end
