# frozen_string_literal: true

# Raised when the interrupt signal is received, typically because the
# user has pressed Control-C (on most posix platforms). As such, it is a
# subclass of +SignalException+.
#
#    begin
#      puts "Press ctrl-C when you get bored"
#      loop {}
#    rescue Interrupt => e
#      puts "Note: You will typically use Signal.trap instead."
#    end
#
# <em>produces:</em>
#
#    Press ctrl-C when you get bored
#
# <em>then waits until it is interrupted with Control-C and then prints:</em>
#
#    Note: You will typically use Signal.trap instead.
class Interrupt < SignalException
end
