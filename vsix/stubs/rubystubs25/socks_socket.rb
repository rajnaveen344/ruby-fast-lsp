# frozen_string_literal: true

# SOCKS is an Internet protocol that routes packets between a client and
# a server through a proxy server.  SOCKS5, if supported, additionally
# provides authentication so only authorized users may access a server.
class SOCKSSocket < TCPSocket
  # Opens a SOCKS connection to +host+ via the SOCKS server +serv+.
  def initialize(host, serv) end

  # Closes the SOCKS connection.
  def close; end
end
