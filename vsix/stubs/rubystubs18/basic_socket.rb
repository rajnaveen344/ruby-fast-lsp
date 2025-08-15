# frozen_string_literal: true

class BasicSocket < IO
  def self.do_not_reverse_lookup; end

  def self.do_not_reverse_lookup=(p1) end

  def self.for_fd(p1) end

  def close_read; end

  def close_write; end

  def getpeername; end

  def getsockname; end

  # Gets a socket option. These are protocol and system specific, see your
  # local sytem documentation for details. The option is returned as
  # a String with the data being the binary value of the socket option.
  #
  # === Parameters
  # * +level+ is an integer, usually one of the SOL_ constants such as
  #   Socket::SOL_SOCKET, or a protocol level.
  # * +optname+ is an integer, usually one of the SO_ constants, such
  #   as Socket::SO_REUSEADDR.
  #
  # === Examples
  #
  # Some socket options are integers with boolean values, in this case
  # #getsockopt could be called like this:
  #   optval = sock.getsockopt(Socket::SOL_SOCKET,Socket::SO_REUSEADDR)
  #   optval = optval.unpack "i"
  #   reuseaddr = optval[0] == 0 ? false : true
  #
  # Some socket options are integers with numeric values, in this case
  # #getsockopt could be called like this:
  #   optval = sock.getsockopt(Socket::IPPROTO_IP, Socket::IP_TTL)
  #   ipttl = optval.unpack("i")[0]
  #
  # Option values may be structs. Decoding them can be complex as it involves
  # examining your system headers to determine the correct definition. An
  # example is a +struct linger+, which may be defined in your system headers
  # as:
  #   struct linger {
  #     int l_onoff;
  #     int l_linger;
  #   };
  #
  # In this case #getsockopt could be called like this:
  #   optval =  sock.getsockopt(Socket::SOL_SOCKET, Socket::SO_LINGER)
  #   onoff, linger = optval.unpack "ii"
  def getsockopt(level, optname) end

  def recv(*args) end

  # Receives up to _maxlen_ bytes from +socket+ using recvfrom(2) after
  # O_NONBLOCK is set for the underlying file descriptor.
  # _flags_ is zero or more of the +MSG_+ options.
  # The result, _mesg_, is the data received.
  #
  # When recvfrom(2) returns 0, Socket#recv_nonblock returns
  # an empty string as data.
  # The meaning depends on the socket: EOF on TCP, empty packet on UDP, etc.
  #
  # === Parameters
  # * +maxlen+ - the number of bytes to receive from the socket
  # * +flags+ - zero or more of the +MSG_+ options
  #
  # === Example
  #      serv = TCPServer.new("127.0.0.1", 0)
  #      af, port, host, addr = serv.addr
  #      c = TCPSocket.new(addr, port)
  #      s = serv.accept
  #      c.send "aaa", 0
  #      IO.select([s])
  #      p s.recv_nonblock(10) #=> "aaa"
  #
  # Refer to Socket#recvfrom for the exceptions that may be thrown if the call
  # to _recv_nonblock_ fails.
  #
  # BasicSocket#recv_nonblock may raise any error corresponding to recvfrom(2) failure,
  # including Errno::EAGAIN.
  #
  # === See
  # * Socket#recvfrom
  def recv_nonblock(*several_variants) end

  def send(p1, p2, p3 = v3) end

  # Sets a socket option. These are protocol and system specific, see your
  # local sytem documentation for details.
  #
  # === Parameters
  # * +level+ is an integer, usually one of the SOL_ constants such as
  #   Socket::SOL_SOCKET, or a protocol level.
  # * +optname+ is an integer, usually one of the SO_ constants, such
  #   as Socket::SO_REUSEADDR.
  # * +optval+ is the value of the option, it is passed to the underlying
  #   setsockopt() as a pointer to a certain number of bytes. How this is
  #   done depends on the type:
  #   - Fixnum: value is assigned to an int, and a pointer to the int is
  #     passed, with length of sizeof(int).
  #   - true or false: 1 or 0 (respectively) is assigned to an int, and the
  #     int is passed as for a Fixnum. Note that +false+ must be passed,
  #     not +nil+.
  #   - String: the string's data and length is passed to the socket.
  #
  # === Examples
  #
  # Some socket options are integers with boolean values, in this case
  # #setsockopt could be called like this:
  #   sock.setsockopt(Socket::SOL_SOCKET,Socket::SO_REUSEADDR, true)
  #
  # Some socket options are integers with numeric values, in this case
  # #setsockopt could be called like this:
  #   sock.setsockopt(Socket::IPPROTO_IP, Socket::IP_TTL, 255)
  #
  # Option values may be structs. Passing them can be complex as it involves
  # examining your system headers to determine the correct definition. An
  # example is an +ip_mreq+, which may be defined in your system headers as:
  #   struct ip_mreq {
  #     struct  in_addr imr_multiaddr;
  #     struct  in_addr imr_interface;
  #   };
  #
  # In this case #setsockopt could be called like this:
  #   optval =  IPAddr.new("224.0.0.251") + Socket::INADDR_ANY
  #   sock.setsockopt(Socket::IPPROTO_IP, Socket::IP_ADD_MEMBERSHIP, optval)
  def setsockopt(level, optname, optval) end

  def shutdown(p1 = v1) end
end
