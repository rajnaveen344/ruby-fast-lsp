# frozen_string_literal: true

class SignalException < Exception
  #  Construct a new SignalException object.  +sig+ should be a known
  #  signal name, or a signal number.
  def initialize(sig) end
end
