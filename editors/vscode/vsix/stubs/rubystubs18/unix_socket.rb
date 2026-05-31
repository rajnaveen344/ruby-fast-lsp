# frozen_string_literal: true

class UNIXSocket < BasicSocket
  def self.pair(p1 = v1, p2 = v2) end

  def self.socketpair(p1 = v1, p2 = v2) end

  def initialize(p1) end

  def addr; end

  def path; end

  def peeraddr; end

  def recv_io(p1 = v1, p2 = v2) end

  def recvfrom(*args) end

  def send_io(p1) end
end
