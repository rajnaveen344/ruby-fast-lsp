# frozen_string_literal: true

# TCPSocket represents a TCP/IP client socket.
#
# A simple client may look like:
#
#   require 'socket'
#
#   s = TCPSocket.new 'localhost', 2000
#
#   while line = s.gets # Read lines from socket
#     puts line         # and print them
#   end
#
#   s.close             # close socket when done
class TCPSocket < IPSocket
  # Lookups host information by _hostname_.
  #
  #   TCPSocket.gethostbyname("localhost")
  #   #=> ["localhost", ["hal"], 2, "127.0.0.1"]
  def self.gethostbyname(hostname) end

  # Opens a TCP connection to +remote_host+ on +remote_port+.  If +local_host+
  # and +local_port+ are specified, then those parameters are used on the local
  # end to establish the connection.
  def initialize(remote_host, remote_port, local_host = nil, local_port = nil) end
end
