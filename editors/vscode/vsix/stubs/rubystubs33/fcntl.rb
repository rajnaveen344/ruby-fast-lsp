# frozen_string_literal: true

module Fcntl
  # FD_CLOEXEC
  #
  # the value of the close-on-exec flag.
  FD_CLOEXEC = _
  # F_DUP2FD
  #
  # It is a FreeBSD specific constant and equivalent
  # to dup2 call.
  F_DUP2FD = _
  # F_DUP2FD_CLOEXEC
  #
  # It is a FreeBSD specific constant and acts
  # similarly as F_DUP2FD but set the FD_CLOEXEC
  # flag in addition.
  F_DUP2FD_CLOEXEC = _
  # F_DUPFD
  #
  # Duplicate a file descriptor to the minimum unused file descriptor
  # greater than or equal to the argument.
  #
  # The close-on-exec flag of the duplicated file descriptor is set.
  # (Ruby uses F_DUPFD_CLOEXEC internally if available to avoid race
  # condition.  F_SETFD is used if F_DUPFD_CLOEXEC is not available.)
  F_DUPFD = _
  # F_GETFD
  #
  # Read the close-on-exec flag of a file descriptor.
  F_GETFD = _
  # F_GETFL
  #
  # Get the file descriptor flags.  This will be one or more of the O_*
  # flags.
  F_GETFL = _
  # F_GETLK
  #
  # Determine whether a given region of a file is locked.  This uses one of
  # the F_*LK flags.
  F_GETLK = _
  # F_GETPIPE_SZ
  #
  # Return (as the function result) the capacity of the pipe referred to by fd.
  F_GETPIPE_SZ = _
  # F_RDLCK
  #
  # Read lock for a region of a file
  F_RDLCK = _
  # F_SETFD
  #
  # Set the close-on-exec flag of a file descriptor.
  F_SETFD = _
  # F_SETFL
  #
  # Set the file descriptor flags.  This will be one or more of the O_*
  # flags.
  F_SETFL = _
  # F_SETLK
  #
  # Acquire a lock on a region of a file.  This uses one of the F_*LCK
  # flags.
  F_SETLK = _
  # F_SETLKW
  #
  # Acquire a lock on a region of a file, waiting if necessary.  This uses
  # one of the F_*LCK flags
  F_SETLKW = _
  # F_SETPIPE_SZ
  #
  # Change the capacity of the pipe referred to by fd to be at least arg bytes.
  F_SETPIPE_SZ = _
  # F_UNLCK
  #
  # Remove lock for a region of a file
  F_UNLCK = _
  # F_WRLCK
  #
  # Write lock for a region of a file
  F_WRLCK = _
  # O_ACCMODE
  #
  # Mask to extract the read/write flags
  O_ACCMODE = _
  # O_APPEND
  #
  # Open the file in append mode
  O_APPEND = _
  # O_CREAT
  #
  # Create the file if it doesn't exist
  O_CREAT = _
  # O_EXCL
  #
  # Used with O_CREAT, fail if the file exists
  O_EXCL = _
  # O_NDELAY
  #
  # Open the file in non-blocking mode
  O_NDELAY = _
  # O_NOCTTY
  #
  # Open TTY without it becoming the controlling TTY
  O_NOCTTY = _
  # O_NONBLOCK
  #
  # Open the file in non-blocking mode
  O_NONBLOCK = _
  # O_RDONLY
  #
  # Open the file in read-only mode
  O_RDONLY = _
  # O_RDWR
  #
  # Open the file in read-write mode
  O_RDWR = _
  # O_TRUNC
  #
  # Truncate the file on open
  O_TRUNC = _
  # O_WRONLY
  #
  # Open the file in write-only mode.
  O_WRONLY = _
  VERSION = _
end
