# frozen_string_literal: true

# When an operating system encounters an error,
# it typically reports the error as an integer error code:
#
#   $ ls nosuch.txt
#   ls: cannot access 'nosuch.txt': No such file or directory
#   $ echo $? # Code for last error.
#   2
#
# When the Ruby interpreter interacts with the operating system
# and receives such an error code (e.g., +2+),
# it maps the code to a particular Ruby exception class (e.g., +Errno::ENOENT+):
#
#   File.open('nosuch.txt')
#   # => No such file or directory @ rb_sysopen - nosuch.txt (Errno::ENOENT)
#
# Each such class is:
#
# - A nested class in this module, +Errno+.
# - A subclass of class SystemCallError.
# - Associated with an error code.
#
# Thus:
#
#   Errno::ENOENT.superclass # => SystemCallError
#   Errno::ENOENT::Errno     # => 2
#
# The names of nested classes are returned by method +Errno.constants+:
#
#   Errno.constants.size         # => 158
#   Errno.constants.sort.take(5) # => [:E2BIG, :EACCES, :EADDRINUSE, :EADDRNOTAVAIL, :EADV]
#
# As seen above, the error code associated with each class
# is available as the value of a constant;
# the value for a particular class may vary among operating systems.
# If the class is not needed for the particular operating system,
# the value is zero:
#
#   Errno::ENOENT::Errno      # => 2
#   Errno::ENOTCAPABLE::Errno # => 0
module Errno
  # "Argument list too long" error
  E2BIG = _
  # "Permission denied" error
  EACCES = _
  # "Address already in use" error
  EADDRINUSE = _
  # "Address not available" error
  EADDRNOTAVAIL = _
  # "Advertise error" error
  EADV = _
  # "Address family not supported" error
  EAFNOSUPPORT = _
  # "Resource temporarily unavailable, try again (may be the same value as EWOULDBLOCK)" error
  EAGAIN = _
  # "Connection already in progress" error
  EALREADY = _
  # "Authentication error" error
  EAUTH = _
  # "Bad CPU type in executable" error
  EBADARCH = _
  # "Bad exchange" error
  EBADE = _
  # "Bad executable" error
  EBADEXEC = _
  # "Bad file descriptor" error
  EBADF = _
  # "File descriptor in bad state" error
  EBADFD = _
  # "Malformed Macho file" error
  EBADMACHO = _
  # "Bad message" error
  EBADMSG = _
  # "Invalid request descriptor" error
  EBADR = _
  # "RPC struct is bad" error
  EBADRPC = _
  # "Invalid request code" error
  EBADRQC = _
  # "Invalid slot" error
  EBADSLT = _
  # "Bad font file format" error
  EBFONT = _
  # "Device or resource busy" error
  EBUSY = _
  # "Operation canceled" error
  ECANCELED = _
  # "Not permitted in capability mode" error
  ECAPMODE = _
  # "No child processes" error
  ECHILD = _
  # "Channel number out of range" error
  ECHRNG = _
  # "Communication error on send" error
  ECOMM = _
  # "Connection aborted" error
  ECONNABORTED = _
  # "Connection refused" error
  ECONNREFUSED = _
  # "Connection reset" error
  ECONNRESET = _
  # "Resource deadlock avoided" error
  EDEADLK = _
  # "File locking deadlock error" error
  EDEADLOCK = _
  # "Destination address required" error
  EDESTADDRREQ = _
  # "Device error; e.g., printer paper out" error
  EDEVERR = _
  # "Mathematics argument out of domain of function" error
  EDOM = _
  # "Improper function use" error
  EDOOFUS = _
  # "RFS specific error" error
  EDOTDOT = _
  # "Disk quota exceeded" error
  EDQUOT = _
  # "File exists" error
  EEXIST = _
  # "Bad address" error
  EFAULT = _
  # "File too large" error
  EFBIG = _
  # "Invalid file type or format" error
  EFTYPE = _
  # "Host is down" error
  EHOSTDOWN = _
  # "Host is unreachable" error
  EHOSTUNREACH = _
  # "Memory page has hardware error" error
  EHWPOISON = _
  # "Identifier removed" error
  EIDRM = _
  # "Invalid or incomplete multibyte or wide character" error
  EILSEQ = _
  # "Operation in progress" error
  EINPROGRESS = _
  # "Interrupted function call" error
  EINTR = _
  # "Invalid argument" error
  EINVAL = _
  # "Input/output error" error
  EIO = _
  # "IPsec processing failure" error
  EIPSEC = _
  # "Socket is connected" error
  EISCONN = _
  # "Is a directory" error
  EISDIR = _
  # "Is a named file type" error
  EISNAM = _
  # "Key has expired" error
  EKEYEXPIRED = _
  # "Key was rejected by service" error
  EKEYREJECTED = _
  # "Key has been revoked" error
  EKEYREVOKED = _
  # "Level 2 halted" error
  EL2HLT = _
  # "Level 2 not synchronized" error
  EL2NSYNC = _
  # "Level 3 halted" error
  EL3HLT = _
  # "Level 3 reset" error
  EL3RST = _
  # "Largest errno value" error
  ELAST = _
  # "Cannot access a needed shared library" error
  ELIBACC = _
  # "Accessing a corrupted shared library" error
  ELIBBAD = _
  # "Cannot exec a shared library directly" error
  ELIBEXEC = _
  # "Attempting to link in too many shared libraries" error
  ELIBMAX = _
  # ".lib section in a.out corrupted" error
  ELIBSCN = _
  # "Link number out of range" error
  ELNRNG = _
  # "Too many levels of symbolic links" error
  ELOOP = _
  # "Wrong medium type" error
  EMEDIUMTYPE = _
  # "Too many open files" error
  EMFILE = _
  # "Too many links" error
  EMLINK = _
  # "Message too long" error
  EMSGSIZE = _
  # "Multihop attempted" error
  EMULTIHOP = _
  # "Filename too long" error
  ENAMETOOLONG = _
  # "No XENIX semaphores available" error
  ENAVAIL = _
  # "Need authenticator" error
  ENEEDAUTH = _
  # "Network is down" error
  ENETDOWN = _
  # "Connection aborted by network" error
  ENETRESET = _
  # "Network unreachable" error
  ENETUNREACH = _
  # "Too many open files in system" error
  ENFILE = _
  # "No anode" error
  ENOANO = _
  # "Attribute not found" error
  ENOATTR = _
  # "No buffer space available" error
  ENOBUFS = _
  # "No CSI structure available" error
  ENOCSI = _
  # "No data available" error
  ENODATA = _
  # "No such device" error
  ENODEV = _
  # "No such file or directory" error
  ENOENT = _
  # "Exec format error" error
  ENOEXEC = _
  # "Required key not available" error
  ENOKEY = _
  # "No locks available" error
  ENOLCK = _
  # "Link has been severed" error
  ENOLINK = _
  # "No medium found" error
  ENOMEDIUM = _
  # "Not enough space/cannot allocate memory" error
  ENOMEM = _
  # "No message of the desired type" error
  ENOMSG = _
  # "Machine is not on the network" error
  ENONET = _
  # "Package not installed" error
  ENOPKG = _
  # "No such policy" error
  ENOPOLICY = _
  # "Protocol not available" error
  ENOPROTOOPT = _
  # "No space left on device" error
  ENOSPC = _
  # "No STREAM resources" error
  ENOSR = _
  # "Not a STREAM" error
  ENOSTR = _
  # "Functionality not implemented" error
  ENOSYS = _
  # "Block device required" error
  ENOTBLK = _
  # "Capabilities insufficient" error
  ENOTCAPABLE = _
  # "The socket is not connected" error
  ENOTCONN = _
  # "Not a directory" error
  ENOTDIR = _
  # "Directory not empty" error
  ENOTEMPTY = _
  # "Not a XENIX named type file" error
  ENOTNAM = _
  # "State not recoverable" error
  ENOTRECOVERABLE = _
  # "Not a socket" error
  ENOTSOCK = _
  # "Operation not supported" error
  ENOTSUP = _
  # "Inappropriate I/O control operation" error
  ENOTTY = _
  # "Name not unique on network" error
  ENOTUNIQ = _
  # "No such device or address" error
  ENXIO = _
  # "Operation not supported on socket" error
  EOPNOTSUPP = _
  # "Value too large to be stored in data type" error
  EOVERFLOW = _
  # "Owner died" error
  EOWNERDEAD = _
  # "Operation not permitted" error
  EPERM = _
  # "Protocol family not supported" error
  EPFNOSUPPORT = _
  # "Broken pipe" error
  EPIPE = _
  # "Too many processes" error
  EPROCLIM = _
  # "Bad procedure for program" error
  EPROCUNAVAIL = _
  # "Program version wrong" error
  EPROGMISMATCH = _
  # "RPC program isn't available" error
  EPROGUNAVAIL = _
  # "Protocol error" error
  EPROTO = _
  # "Protocol not supported" error
  EPROTONOSUPPORT = _
  # "Protocol wrong type for socket" error
  EPROTOTYPE = _
  # "Device power is off" error
  EPWROFF = _
  # "Interface output queue is full" error
  EQFULL = _
  # "Result too large" error
  ERANGE = _
  # "Remote address changed" error
  EREMCHG = _
  # "Object is remote" error
  EREMOTE = _
  # "Remote I/O error" error
  EREMOTEIO = _
  # "Interrupted system call should be restarted" error
  ERESTART = _
  # "Operation not possible due to RF-kill" error
  ERFKILL = _
  # "Read-only file system" error
  EROFS = _
  # "RPC version wrong" error
  ERPCMISMATCH = _
  # "Shared library version mismatch" error
  ESHLIBVERS = _
  # "Cannot send after transport endpoint shutdown" error
  ESHUTDOWN = _
  # "Socket type not supported" error
  ESOCKTNOSUPPORT = _
  # "Illegal seek" error
  ESPIPE = _
  # "No such process" error
  ESRCH = _
  # "Server mount error" error
  ESRMNT = _
  # "Stale file handle" error
  ESTALE = _
  # "Streams pipe error" error
  ESTRPIPE = _
  # "Timer expired" error
  ETIME = _
  # "Connection timed out" error
  ETIMEDOUT = _
  # cannot splice" error
  ETOOMANYREFS = _
  # "Text file busy" error
  ETXTBSY = _
  # "Structure needs cleaning" error
  EUCLEAN = _
  # "Protocol driver not attached" error
  EUNATCH = _
  # "Too many users" error
  EUSERS = _
  # "Operation would block" error
  EWOULDBLOCK = _
  # "Invalid cross-device link" error
  EXDEV = _
  # "Exchange full" error
  EXFULL = _
  # No error
  NOERROR = _
end
