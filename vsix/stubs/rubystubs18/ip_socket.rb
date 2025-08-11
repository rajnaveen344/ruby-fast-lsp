# frozen_string_literal: true

# IPSocket is the parent of TCPSocket and UDPSocket and implements
# functionality common to them.
#
# A number of APIs in IPSocket, Socket, and their descendants return an
# address as an array. The members of that array are:
# - address family: A string like "AF_INET" or "AF_INET6" if it is one of the
#   commonly used families, the string "unknown:#" (where `#' is the address
#   family number) if it is not one of the common ones.  The strings map to
#   the Socket::AF_* constants.
# - port: The port number.
# - name: Either the canonical name from looking the address up in the DNS, or
#   the address in presentation format
# - address: The address in presentation format (a dotted decimal string for
#   IPv4, a hex string for IPv6).
#
# The address and port can be used directly to create sockets and to bind or
# connect them to the address.
class IPSocket < BasicSocket
  def self.getaddress(p1) end

  def addr; end

  def peeraddr; end

  def recvfrom(*args) end
end
