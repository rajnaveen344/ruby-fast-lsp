# frozen_string_literal: true

# Fcntl loads the constants defined in the system's <fcntl.h> C header
# file, and used with both the fcntl(2) and open(2) POSIX system calls.
#
# Copyright (C) 1997-2001 Yukihiro Matsumoto
#
# Documented by mathew <meta@pobox.com>
#
# = Usage
#
# To perform a fcntl(2) operation, use IO::fcntl in the core classes.
#
# To perform an open(2) operation, use IO::sysopen.
#
# The set of operations and constants available depends upon specific OS
# platform. Some values listed below may not be supported on your system.
#
# The constants supported by Ruby for use with IO::fcntl are:
#
# - F_DUPFD - duplicate a close-on-exec file handle to a non-close-on-exec
#   file handle.
#
# - F_GETFD - read the close-on-exec flag of a file handle.
#
# - F_SETFD - set the close-on-exec flag of a file handle.
#
# - FD_CLOEXEC - the value of the close-on-exec flag.
#
# - F_GETFL - get file descriptor flags.
#
# - F_SETFL - set file descriptor flags.
#
# - O_APPEND, O_NONBLOCK, etc (see below) - file descriptor flag
#   values for the above.
#
# - F_GETLK - determine whether a given region of a file is locked.
#
# - F_SETLK - acquire a lock on a region of a file.
#
# - F_SETLKW - acquire a lock on a region of a file, waiting if necessary.
#
# - F_RDLCK, F_WRLCK, F_UNLCK - types of lock for the above.
#
# The constants supported by Ruby for use with IO::sysopen are:
#
# - O_APPEND - open file in append mode.
#
# - O_NOCTTY - open tty without it becoming controlling tty.
#
# - O_CREAT - create file if it doesn't exist.
#
# - O_EXCL - used with O_CREAT, fail if file exists.
#
# - O_TRUNC - truncate file on open.
#
# - O_NONBLOCK / O_NDELAY - open in non-blocking mode.
#
# - O_RDONLY - open read-only.
#
# - O_WRONLY - open write-only.
#
# - O_RDWR - open read-write.
#
# - O_ACCMODE - mask to extract read/write flags.
#
# Example:
#
#   require 'fcntl'
#
#   fd = IO::sysopen('/tmp/tempfile',
#        Fcntl::O_WRONLY | Fcntl::O_EXCL | Fcntl::O_CREAT)
#   f = IO.open(fd)
#   f.syswrite("TEMP DATA")
#   f.close
module Fcntl
  FD_CLOEXEC = _
  F_DUPFD = _
  F_GETFD = _
  F_GETFL = _
  F_GETLK = _
  F_RDLCK = _
  F_SETFD = _
  F_SETFL = _
  F_SETLK = _
  F_SETLKW = _
  F_UNLCK = _
  F_WRLCK = _
  O_ACCMODE = _
  O_APPEND = _
  O_CREAT = _
  O_EXCL = _
  O_NDELAY = _
  O_NOCTTY = _
  O_NONBLOCK = _
  O_RDONLY = _
  O_RDWR = _
  O_TRUNC = _
  O_WRONLY = _
end
