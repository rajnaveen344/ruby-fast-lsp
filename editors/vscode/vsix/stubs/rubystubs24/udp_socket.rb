# frozen_string_literal: true

# UDPSocket represents a UDP/IP socket.
class UDPSocket < IPSocket
  # Creates a new UDPSocket object.
  #
  # _address_family_ should be an integer, a string or a symbol:
  # Socket::AF_INET, "AF_INET", :INET, etc.
  #
  #   require 'socket'
  #
  #   UDPSocket.new                   #=> #<UDPSocket:fd 3>
  #   UDPSocket.new(Socket::AF_INET6) #=> #<UDPSocket:fd 4>
  def initialize(*address_family) end

  # Binds _udpsocket_ to _host_:_port_.
  #
  #   u1 = UDPSocket.new
  #   u1.bind("127.0.0.1", 4913)
  #   u1.send "message-to-self", 0, "127.0.0.1", 4913
  #   p u1.recvfrom(10) #=> ["message-to", ["AF_INET", 4913, "localhost", "127.0.0.1"]]
  def bind(host, port) end

  # Connects _udpsocket_ to _host_:_port_.
  #
  # This makes possible to send without destination address.
  #
  #   u1 = UDPSocket.new
  #   u1.bind("127.0.0.1", 4913)
  #   u2 = UDPSocket.new
  #   u2.connect("127.0.0.1", 4913)
  #   u2.send "uuuu", 0
  #   p u1.recvfrom(10) #=> ["uuuu", ["AF_INET", 33230, "localhost", "127.0.0.1"]]
  def connect(host, port) end

  # Sends _mesg_ via _udpsocket_.
  #
  # _flags_ should be a bitwise OR of Socket::MSG_* constants.
  #
  #   u1 = UDPSocket.new
  #   u1.bind("127.0.0.1", 4913)
  #
  #   u2 = UDPSocket.new
  #   u2.send "hi", 0, "127.0.0.1", 4913
  #
  #   mesg, addr = u1.recvfrom(10)
  #   u1.send mesg, 0, addr[3], addr[1]
  #
  #   p u2.recv(100) #=> "hi"
  def send(*several_variants) end
end
