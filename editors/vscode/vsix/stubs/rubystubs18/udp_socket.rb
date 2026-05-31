# frozen_string_literal: true

class UDPSocket < IPSocket
  def initialize(p1 = v1) end

  def bind(p1, p2) end

  def connect(p1, p2) end

  # Receives up to _maxlen_ bytes from +udpsocket+ using recvfrom(2) after
  # O_NONBLOCK is set for the underlying file descriptor.
  # _flags_ is zero or more of the +MSG_+ options.
  # The first element of the results, _mesg_, is the data received.
  # The second element, _sender_inet_addr_, is an array to represent the sender address.
  #
  # When recvfrom(2) returns 0,
  # Socket#recvfrom_nonblock returns an empty string as data.
  # It means an empty packet.
  #
  # === Parameters
  # * +maxlen+ - the number of bytes to receive from the socket
  # * +flags+ - zero or more of the +MSG_+ options
  #
  # === Example
  #      require 'socket'
  #      s1 = UDPSocket.new
  #      s1.bind("127.0.0.1", 0)
  #      s2 = UDPSocket.new
  #      s2.bind("127.0.0.1", 0)
  #      s2.connect(*s1.addr.values_at(3,1))
  #      s1.connect(*s2.addr.values_at(3,1))
  #      s1.send "aaa", 0
  #      IO.select([s2])
  #      p s2.recvfrom_nonblock(10)  #=> ["aaa", ["AF_INET", 33302, "localhost.localdomain", "127.0.0.1"]]
  #
  # Refer to Socket#recvfrom for the exceptions that may be thrown if the call
  # to _recvfrom_nonblock_ fails.
  #
  # UDPSocket#recvfrom_nonblock may raise any error corresponding to recvfrom(2) failure,
  # including Errno::EAGAIN.
  #
  # === See
  # * Socket#recvfrom
  def recvfrom_nonblock(*several_variants) end

  def send(p1, p2, p3, p4) end
end
