# frozen_string_literal: true

# Raised when a signal is received.
#
#    begin
#      Process.kill('HUP',Process.pid)
#      sleep # wait for receiver to handle signal sent by Process.kill
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
  def initialize(...) end

  #  Returns a signal number.
  def signo; end
end
