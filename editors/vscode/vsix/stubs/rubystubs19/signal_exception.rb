# frozen_string_literal: true

# Raised when a signal is received.
#
#    begin
#      Process.kill('HUP',Process.pid)
#    rescue SignalException => e
#      puts "received Exception #{e}"
#    end
#
# <em>produces:</em>
#
#    received Exception SIGHUP
class SignalException < Exception
  #  Construct a new SignalException object.  +sig_name+ should be a known
  #  signal name.
  def initialize(*several_variants) end

  #  Returns a signal number.
  def signo; end
end
