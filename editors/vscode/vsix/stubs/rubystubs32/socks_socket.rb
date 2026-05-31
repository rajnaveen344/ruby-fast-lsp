# frozen_string_literal: true

# SOCKS is an Internet protocol that routes packets between a client and
# a server through a proxy server.  SOCKS5, if supported, additionally
# provides authentication so only authorized users may access a server.
class SOCKSSocket < TCPSocket
  # Opens a SOCKS connection to +host+ via the SOCKS server.
  #
  # The SOCKS server configuration varies by implementation
  #
  # When using the Dante libsocks/libsocksd implementation it is configured as SOCKS_SERVER env var.
  #
  # See: https://manpages.debian.org/testing/dante-client/socksify.1.en.html for full env variable support.
  def initialize(host, port) end

  # Closes the SOCKS connection.
  def close; end
end
