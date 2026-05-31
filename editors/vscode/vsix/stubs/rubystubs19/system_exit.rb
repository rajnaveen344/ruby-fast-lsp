# frozen_string_literal: true

# Raised by +exit+ to initiate the termination of the script.
class SystemExit < Exception
  # Create a new +SystemExit+ exception with the given status.
  def initialize(status = 0) end

  # Return the status value associated with this system exit.
  def status; end

  # Returns +true+ if exiting successful, +false+ if not.
  def success?; end
end
