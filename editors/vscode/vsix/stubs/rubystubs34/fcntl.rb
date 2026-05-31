# frozen_string_literal: true

# Fcntl loads the constants defined in the system's <fcntl.h> C header
# file, and used with both the fcntl(2) and open(2) POSIX system calls.
#
# To perform a fcntl(2) operation, use IO::fcntl.
#
# To perform an open(2) operation, use IO::sysopen.
#
# The set of operations and constants available depends upon specific
# operating system.  Some values listed below may not be supported on your
# system.
#
# See your fcntl(2) man page for complete details.
#
# Open /tmp/tempfile as a write-only file that is created if it doesn't
# exist:
#
#   require 'fcntl'
#
#   fd = IO.sysopen('/tmp/tempfile',
#                   Fcntl::O_WRONLY | Fcntl::O_EXCL | Fcntl::O_CREAT)
#   f = IO.open(fd)
#   f.syswrite("TEMP DATA")
#   f.close
#
# Get the flags on file +s+:
#
#   m = s.fcntl(Fcntl::F_GETFL, 0)
#
# Set the non-blocking flag on +f+ in addition to the existing flags in +m+.
#
#   f.fcntl(Fcntl::F_SETFL, Fcntl::O_NONBLOCK|m)
module Fcntl
  # the value of the close-on-exec flag.
  FD_CLOEXEC = _
  # It is a FreeBSD specific constant and equivalent
  # to dup2 call.
  F_DUP2FD = _
  # It is a FreeBSD specific constant and acts
  # similarly as F_DUP2FD but set the FD_CLOEXEC
  # flag in addition.
  F_DUP2FD_CLOEXEC = _
  # Duplicate a file descriptor to the minimum unused file descriptor
  # greater than or equal to the argument.
  #
  # The close-on-exec flag of the duplicated file descriptor is set.
  # (Ruby uses F_DUPFD_CLOEXEC internally if available to avoid race
  # condition.  F_SETFD is used if F_DUPFD_CLOEXEC is not available.)
  F_DUPFD = _
  # Read the close-on-exec flag of a file descriptor.
  F_GETFD = _
  # Get the file descriptor flags.  This will be one or more of the O_*
  # flags.
  F_GETFL = _
  # Determine whether a given region of a file is locked.  This uses one of
  # the F_*LK flags.
  F_GETLK = _
  # Return (as the function result) the capacity of the pipe referred to by fd.
  F_GETPIPE_SZ = _
  # Read lock for a region of a file
  F_RDLCK = _
  # Set the close-on-exec flag of a file descriptor.
  F_SETFD = _
  # Set the file descriptor flags.  This will be one or more of the O_*
  # flags.
  F_SETFL = _
  # Acquire a lock on a region of a file.  This uses one of the F_*LCK
  # flags.
  F_SETLK = _
  # Acquire a lock on a region of a file, waiting if necessary.  This uses
  # one of the F_*LCK flags
  F_SETLKW = _
  # Change the capacity of the pipe referred to by fd to be at least arg bytes.
  F_SETPIPE_SZ = _
  # Remove lock for a region of a file
  F_UNLCK = _
  # Write lock for a region of a file
  F_WRLCK = _
  # Mask to extract the read/write flags
  O_ACCMODE = _
  # Open the file in append mode
  O_APPEND = _
  # Create the file if it doesn't exist
  O_CREAT = _
  # Used with O_CREAT, fail if the file exists
  O_EXCL = _
  # Open the file in non-blocking mode
  O_NDELAY = _
  # Open TTY without it becoming the controlling TTY
  O_NOCTTY = _
  # Open the file in non-blocking mode
  O_NONBLOCK = _
  # Open the file in read-only mode
  O_RDONLY = _
  # Open the file in read-write mode
  O_RDWR = _
  # Truncate the file on open
  O_TRUNC = _
  # Open the file in write-only mode.
  O_WRONLY = _
  # The version string.
  VERSION = _
end
